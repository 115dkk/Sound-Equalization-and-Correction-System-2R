"""Reproduce the room_C fixed-point analytic-versus-central-difference check.

From the repository root, run:
    python docs/perf/resource-iir-derivative-check.py > /tmp/iir-derivatives.json

Requires NumPy. No optimizer is run; the recorded room_C parameters and sidecars
are read directly. The output is a numerical diagnostic, not a replacement for
an oracle fixture or a test tolerance. f64 central differences lose precision
when subtracting low-frequency unnormalized RBJ coefficient sums. The second
pass evaluates only section responses/derivatives with NumPy longdouble, then
stores the same f64 Jacobian and FFT products, to separate truncation error from
that roundoff. longdouble precision is platform-dependent (on some platforms it
is identical to f64); the recorded environment reports its machine epsilon.
BLAS thread counts are fixed before importing NumPy. NumPy/libm/platform changes
can move the final digits; compare the convergence pattern, not JSON bytes.
"""
import json
import os
import platform
from pathlib import Path

for name in ('OPENBLAS_NUM_THREADS', 'OMP_NUM_THREADS', 'MKL_NUM_THREADS', 'VECLIB_MAXIMUM_THREADS'):
    os.environ[name] = '1'

import numpy as np

ROOT = Path(__file__).resolve().parents[2] / 'oracle' / 'goldens' / 'engine_mimo_iir'
fixture = json.loads((ROOT / 'room_C.json').read_text())
pb = fixture['inputs']['problem']
reference = fixture['outputs']['reference']['joint']

def read(value):
    if isinstance(value, dict) and 'file' in value:
        return np.fromfile(ROOT / value['file'], dtype='<f8')
    return np.asarray(value, dtype=float)

def complex_array(value):
    return read(value['re']) + 1j * read(value['im'])

K, A, n, nf = pb['K'], pb['A'], pb['n'], pb['nf']
width = 2 + 3 * K + 2 * A
R = len(pb['roles'])
f = read(pb['fl'])
F = len(f)
X = read(pb['X']).reshape(-1, n)
P = len(X)
WL = read(pb['WL']).reshape(P, n)
WE = read(pb['WE']).reshape(P, n)
G = complex_array(pb['G']).reshape(P, R, F)
LP = complex_array(pb['LP'])
x = read(reference['x0']).reshape(R, width)
pols = [s['polarity'] for s in fixture['outputs']['reference']['seeds']]
z = np.exp(-2j * np.pi * f / 48000.0)

def peq(v):
    f0, q, gain = np.exp(v[0]), np.exp(v[1]), v[2]
    a = 10.0 ** (gain / 40.0)
    w = 2.0 * np.pi * f0 / 48000.0
    alpha = np.sin(w) / (2.0 * q)
    numerator = (1.0 + alpha*a) + (-2.0*np.cos(w))*z + (1.0-alpha*a)*z*z
    denominator = (1.0 + alpha/a) + (-2.0*np.cos(w))*z + (1.0-alpha/a)*z*z
    return numerator / denominator

def peq_log_derivatives(v):
    f0, q, gain = np.exp(v[0]), np.exp(v[1]), v[2]
    a = 10.0 ** (gain / 40.0)
    w = 2.0 * np.pi * f0 / 48000.0
    alpha = np.sin(w) / (2.0 * q)
    af = w * np.cos(w) / (2.0 * q)
    mf = 2.0 * w * np.sin(w)
    numerator = (1.0 + alpha*a) + (-2.0*np.cos(w))*z + (1.0-alpha*a)*z*z
    denominator = (1.0 + alpha/a) + (-2.0*np.cos(w))*z + (1.0-alpha/a)*z*z
    u = 1.0-z*z
    dg = np.log(10.0)/40.0
    return np.array([
        (af*a*u+mf*z)/numerator - (af/a*u+mf*z)/denominator,
        (-alpha*a*u)/numerator - (-alpha/a*u)/denominator,
        (dg*alpha*a*u)/numerator - (-dg*alpha/a*u)/denominator,
    ])

def ap(v):
    s = 1j*f / np.exp(v[0])
    q = np.exp(v[1])
    return (s*s-s/q+1.0)/(s*s+s/q+1.0)

def ap_log_derivatives(v):
    s = 1j*f / np.exp(v[0])
    term = s / np.exp(v[1])
    ss = s*s
    numerator, denominator = ss-term+1.0, ss+term+1.0
    return np.array([
        (-2.0*ss+term)/numerator - (-2.0*ss-term)/denominator,
        term/numerator - (-term)/denominator,
    ])

def parts(v, pol, step=None):
    response = (10.0 ** (v[0]/20.0))*pol*np.exp(-2j*np.pi*f*v[1]/1000.0)
    sections = [(2+3*k, 3, peq, peq_log_derivatives) for k in range(K)]
    sections += [(2+3*K+2*k, 2, ap, ap_log_derivatives) for k in range(A)]
    for start, size, func, derivative in sections:
        response *= func(v[start:start+size])
    derivatives = np.zeros((width, F), complex)
    derivatives[0] = response*np.log(10.0)/20.0
    derivatives[1] = response*(-2j*np.pi*f/1000.0)
    for start, size, func, derivative in sections:
        band = v[start:start+size].copy()
        if step is None:
            derivatives[start:start+size] = response[None,:] * derivative(band)
        else:
            factor = func(band)
            for a in range(size):
                up, down = band.copy(), band.copy()
                up[a] += step
                down[a] -= step
                derivatives[start+a] = (response*(func(up)-func(down))/(2.0*step))/factor
    return response, derivatives

band = (f >= 15.0) & (f <= pb['fsiso']*1.2)
gbar = np.mean(np.abs(G), axis=0)
gref = np.median(gbar[:,band], axis=1)
scale = np.sqrt(np.sum(X[0]**2))
weff = 0.05*np.sqrt(np.sum(X[0]**2)/max(np.sum(band),1))*np.where(band[None,:],gref[:,None]/(gbar+0.05*gref[:,None]),0.0)


def jacobian(step=None):
    evaluated = [parts(v, pol, step) for v,pol in zip(x,pols)]
    J = np.zeros((2*P*n+1+2*R*F+R*K, R*width))
    st = [np.fft.irfft(response*LP, nf)[:n] for response,derivative in evaluated]
    level = sum(np.sum(s*s) for s in st)/max(pb['direct'],1e-30)
    over = 10*np.log10(level+1e-30)-pb['level_db']
    for r, (_, derivative) in enumerate(evaluated):
        cols = slice(r*width,(r+1)*width)
        for p in range(P):
            time = np.fft.irfft(G[p,r][None,:]*derivative,nf,axis=1)[:,:n]
            J[2*p*n:(2*p+1)*n,cols] = (WL[p][None,:]*time).T
            J[(2*p+1)*n:(2*p+2)*n,cols] = (np.sqrt(10.0)*WE[p][None,:]*time).T
        start = 2*P*n+1+2*r*F
        effort = weff[r][None,:]*derivative
        J[start:start+F,cols] = effort.real.T
        J[start+F:start+2*F,cols] = effort.imag.T
        if over > 0:
            time = np.fft.irfft(derivative*LP[None,:],nf,axis=1)[:,:n]
            dl = 2.0*(time @ st[r])/max(pb['direct'],1e-30)
            J[2*P*n,cols] = scale*10.0/(np.log(10.0)*max(level,1e-30))*dl
        for k in range(K):
            J[2*P*n+1+2*R*F+r*K+k,r*width+4+3*k] = 0.05*scale/30.0
    return J

analytic = jacobian()
analytic_h = analytic.T @ analytic
oracle_h = read(reference['jtj']).reshape(R*width,R*width)
scale_h = np.max(np.abs(analytic_h))
scale_j = np.max(np.abs(analytic))
result = {
    'fixture': 'engine_mimo_iir/room_C',
    'environment': {
        'python': platform.python_version(),
        'numpy': np.__version__,
        'system': platform.system(),
        'machine': platform.machine(),
        'longdouble_epsilon': float(np.finfo(np.longdouble).eps),
        'blas_threads': 1,
    },
    'shape': list(analytic.shape),
    'analytic_vs_preserved_oracle_h_relative_max': float(np.max(np.abs(analytic_h-oracle_h))/np.max(np.abs(oracle_h))),
    'steps': [],
}
for step in [1e-4,1e-5,1e-6]:
    numerical = jacobian(step)
    hessian = numerical.T @ numerical
    error = np.abs(hessian-analytic_h)
    result['steps'].append({
        'central_difference_step': step,
        'jacobian_relative_max_vs_analytic': float(np.max(np.abs(numerical-analytic))/scale_j),
        'jtj_relative_max_vs_analytic': float(np.max(error)/scale_h),
        'jtj_relative_max_vs_preserved_oracle': float(np.max(np.abs(hessian-oracle_h))/np.max(np.abs(oracle_h))),
        'worst_jtj_index': list(map(int,np.unravel_index(np.argmax(error),error.shape))),
    })
# Repeat only coefficient/section differentiation in extended precision, then
# store the same f64 Jacobian/FFT products. This separates FD truncation from
# cancellation in the 48 kHz low-frequency RBJ coefficient sums.
f = f.astype(np.longdouble)
x = x.astype(np.longdouble)
z = np.exp(np.clongdouble(-2j) * np.longdouble(np.pi) * f / np.longdouble(48000.0))
analytic_extended = jacobian()
h_extended = analytic_extended.T @ analytic_extended
extended = {
    'section_scalar_epsilon': float(np.finfo(np.longdouble).eps),
    'analytic_extended_vs_f64_jtj_relative_max': float(np.max(np.abs(h_extended-analytic_h))/np.max(np.abs(h_extended))),
    'steps': [],
}
for step in [1e-4,1e-5,1e-6]:
    numerical = jacobian(step)
    hessian = numerical.T @ numerical
    error = np.abs(hessian-h_extended)
    extended['steps'].append({
        'central_difference_step': step,
        'jacobian_relative_max_vs_analytic': float(np.max(np.abs(numerical-analytic_extended))/np.max(np.abs(analytic_extended))),
        'jtj_relative_max_vs_analytic': float(np.max(error)/np.max(np.abs(h_extended))),
        'worst_jtj_index': list(map(int,np.unravel_index(np.argmax(error),error.shape))),
    })
result['extended_section_precision'] = extended
print(json.dumps(result,indent=2))
