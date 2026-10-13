"""P09: RC arithmetic with recorded optimiser decisions and rebuild recipes."""
import inspect
import sys
import common
import numpy as np
from secs import calibration as cal, correction_engine as ce
from engine_cases import cases

fam = common.Family('engine_rc')
f = ce.freqs()
indices = np.unique(np.r_[np.flatnonzero((f >= 15) & (f <= 600)), np.arange(0, len(f), 257)]).astype(int)
original_correction = ce.CorrectionEngine._correction
original_fit, original_gd = ce.fit_peq, ce.fit_gd_cuts
original_nb, original_lm = ce.CorrectionEngine._no_boost_mask, ce.CorrectionEngine._long_mode_mask
original_compute = ce.CorrectionEngine.compute
active = None
records = {}
sub_rows = {}
original_sum = ce.CorrectionEngine._sub_sum


def ranges(mask):
    edges = np.diff(np.r_[False, mask, False].astype(int))
    return list(zip(np.flatnonzero(edges == 1).tolist(), np.flatnonzero(edges == -1).tolist()))


def sub_sum(self, params, idx, positions):
    if len(idx) == len(self.f):
        sub_rows[id(self)] = [(g, d, p, list(a)) for g, d, p, a in params]
    return original_sum(self, params, idx, positions)


def fit(*a, **kw):
    result = original_fit(*a, **kw)
    if active is not None:
        bound = inspect.signature(original_fit).bind(*a, **kw)
        bound.apply_defaults()
        args = dict(bound.arguments)
        key = active['key'] + '_fit' + str(len(active['fits']))
        for name in ('g', 'corr_db'):
            args[name] = fam.f64(key + '_' + name, args[name])
        active['fits'].append(dict(inputs=args, result=result))
    return result


def mask_wrapper(original, name):
    def wrapped(self, *a, **kw):
        result = original(self, *a, **kw)
        if active is not None:
            active[name] = ranges(result)
        return result
    return wrapped


def trace(frame, event, arg):
    if frame.f_code is original_compute.__code__:
        if event == 'line' and frame.f_lineno == 1457:
            v = frame.f_locals
            records[v['role']]['candidates'][-1]['gd_score'] = v['peak']
        return trace
    if frame.f_code is original_correction.__code__:
        if event == 'line' and frame.f_lineno == 1200:
            v = frame.f_locals
            active['peaks'].append((v['p'], int(round(v['t'] * ce.SR))))
        return trace
    if frame.f_code.co_name == 'realize' and frame.f_code.co_filename == original_correction.__code__.co_filename:
        if event == 'return':
            active['realizations'].append((float(frame.f_locals['frac']), float(arg[2]), float(arg[3])))
        return trace
    return None


def correction(self, role, comb, level_db):
    global active
    parent = sys._getframe(1).f_locals
    rec = records.setdefault(role, dict(recipe=dict(
        positions=list(comb), align_delay_ms=self.align_delay_ms[role], gain_db=parent['gain_db'],
        xo=parent['xo'], sub_rows=sub_rows.get(id(self), []), subs=list(self.subs),
        bus_gain=parent['bus_gain'], bus_peq=self.sub_bus_peq, bus_delay_ms=parent['bus_d'],
        bus_polarity=parent['bus_pol'], top_peq=self.top_peq.get(role, []),
        level_db=level_db, rc_hi=self.rc_hi), candidates=[]))
    key = run_name + '_' + role + '_' + str(len(rec['candidates']))
    active = dict(key=key, fraction=self.phase_frac, fits=[], peaks=[], realizations=[])
    before = len(self.notes)
    h, response = original_correction(self, role, comb, level_db)
    # Defer arrays until compute has chosen the candidate; losing candidates
    # need replay/decision evidence, not a second sampled transfer function.
    active.update(design_db=self.design_db[role][indices].copy(),
                  response=response[indices].copy(), fir=h.copy(),
                  peq=self.peq[role], strength=self.phase_strength[role], band=self.phase_band[role],
                  notes=self.notes[before:])
    # Touch-up calls share realize but are not trials of the strength ladder.
    n = 1
    if self.lift_ms > 0 and self.taps > 0:
        for trial in active['realizations'][1:]:
            n += 1
            if trial[0] == self.phase_strength[role]:
                break
            if n == 5:
                break
    active['ladder'] = active.pop('realizations')[:n]
    rec['candidates'].append(active)
    if 'combined' not in rec:
        rec['combined'] = fam.c128(key+'_combined', comb[self.main_position][indices])
    active = None
    return h, response


def gd(f, h, lo, hi, n_max=6, depth=3):
    role = sys._getframe(1).f_locals['role']
    result = original_gd(f, h, lo, hi, n_max, depth)
    key = run_name+'_'+role+'_gd'
    records[role]['gd'] = dict(lo=lo, hi=hi, n_max=n_max, depth=depth,
                              h=fam.c128(key, h[indices]), result=result)
    return result


def compute(self, *a, **kw):
    if run_name.endswith('_gd'):
        self.gd_cuts = True
    sys.settrace(trace)
    try:
        result = original_compute(self, *a, **kw)
    finally:
        sys.settrace(None)
    for role, rec in records.items():
        rec['chosen'] = getattr(self, 'phase_frac_used', {}).get(role)
        for candidate in rec['candidates']:
            key = candidate['key']
            chosen = rec['chosen'] is None or candidate['fraction'] == rec['chosen']
            h = candidate.pop('fir')
            full = chosen and role == 'L' and run_name in ('dolby', 'dolby_linear', 'dolby_ultra')
            peak = int(np.argmax(np.abs(h)))
            taps = np.arange(len(h))
            selected = taps if full else taps[(np.abs(taps - peak) <= 512) | (taps % 64 == 0)]
            candidate['fir'] = dict(full=full, length=len(h), peak=peak,
                                   values=fam.f64(key+'_fir', h[selected]),
                                   sum=float(np.sum(h)), sum_squares=float(np.sum(h * h)))
            design, response = candidate.pop('design_db'), candidate.pop('response')
            if chosen:
                candidate['design_db'] = fam.f64(key+'_design', design)
                candidate['response'] = fam.c128(key+'_response', response)
        rec['final_peq'] = self.peq[role]
        rec['final_response'] = fam.c128(run_name+'_'+role+'_final', self.chain[role]['F'][indices])
    return result


ce.CorrectionEngine._sub_sum = sub_sum
ce.CorrectionEngine._correction = correction
ce.CorrectionEngine._no_boost_mask = mask_wrapper(original_nb, 'no_boost')
ce.CorrectionEngine._long_mode_mask = mask_wrapper(original_lm, 'long_mode')
ce.CorrectionEngine.compute = compute
ce.fit_peq, ce.fit_gd_cuts = fit, gd
selected = ('dolby', 'dolby_zero', 'dolby_linear', 'dolby_ultra', 'room', 'room_filters')
for case in cases():
    if case.name not in selected:
        continue
    for gd_enabled in ([False, True] if case.name in ('dolby', 'room') else [False]):
        run_name = case.name + ('_gd' if gd_enabled else '')
        records = {}
        sub_rows = {}
        print('engine_rc:', run_name, flush=True)
        cal.compute(case.project, case.irs, case.settings)
        for role, rec in records.items():
            fam.save(run_name+'_'+role, dict(case=case.name, gd_cuts=gd_enabled,
                     recipe=rec.pop('recipe')), rec)
fam.finish()
assert sum(fam.written.values()) <= 6_000_000, sum(fam.written.values())
