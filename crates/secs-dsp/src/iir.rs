//! Adapted from Impulcifer-py313 `crates/impulcifer-dsp/src/filters.rs` (MIT, Copyright (c) 2024- 115dkk).
//! See docs/THIRD_PARTY.md for attribution and licenses.
//! Butterworth design and zero-state filtering with conventional denominator signs.
use crate::{DspError, fft::Complex64, invalid, numeric::divide};
use std::f64::consts::PI;

/// Rows are [b0,b1,b2,a0,a1,a2], with a0 exactly one.
#[derive(Debug, Clone)]
pub struct Sos(pub Vec<[f64; 6]>);
#[derive(Debug, Clone, Copy)]
pub enum BType {
    Lowpass,
    Highpass,
    Bandpass,
}

/// Analog Butterworth prototype -> LP/HP -> prewarped bilinear -> nearest SOS.
/// https://raw.githubusercontent.com/scipy/scipy/v1.16.0/scipy/signal/_filter_design.py
/// Verified with SciPy 1.18.0. Nearest pairing consumes the poles nearest the
/// unit circle first, writes rows backwards, and puts gain in the first row.
/// Odd orders add origin roots; their zero and pole need not share a section.
/// Panics unless finite 0 < wn < 1, or if the order cannot fit in isize.
/// Order zero is the SciPy identity section.
fn butter_single(order: usize, wn: f64, btype: BType) -> Sos {
    assert!(
        wn.is_finite() && wn > 0.0 && wn < 1.0,
        "cutoff must be between zero and Nyquist"
    );
    let order_i = isize::try_from(order).expect("filter order overflow");
    if order == 0 {
        return Sos(vec![[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]]);
    }
    let warped = 4.0 * (PI * wn / 2.0).tan();
    let mut analog = Vec::with_capacity(order);
    let mut prototype_product = Complex64::new(1.0, 0.0);
    for i in 0..order {
        let m = -(order_i - 1) + 2 * i as isize;
        let prototype = -Complex64::from_polar(1.0, PI * m as f64 / (2.0 * order as f64));
        prototype_product *= -prototype;
        analog.push(match btype {
            BType::Lowpass => prototype * warped,
            BType::Highpass => divide(Complex64::new(warped, 0.), prototype),
            BType::Bandpass => unreachable!(),
        });
    }
    let mut denominator = Complex64::new(1.0, 0.0);
    for p in &analog {
        denominator *= 4.0 - p;
    }
    let gain = match btype {
        BType::Lowpass => warped.powf(order as f64) * (1.0 / denominator).re,
        BType::Bandpass => unreachable!(),
        BType::Highpass => {
            (1.0 / prototype_product).re
                * (Complex64::new(4.0_f64.powf(order as f64), 0.0) / denominator).re
        }
    };
    // One representative per conjugate pair, as in scipy._cplxreal.
    let mut poles: Vec<_> = analog
        .iter()
        .map(|p| divide(4.0 + p, 4.0 - p))
        .filter(|p| p.im >= 0.0)
        .collect();
    poles.sort_by(|a, b| a.re.total_cmp(&b.re).then(a.im.total_cmp(&b.im)));
    let zero = match btype {
        BType::Lowpass => -1.0,
        BType::Highpass => 1.0,
        BType::Bandpass => unreachable!(),
    };
    let mut zeros = vec![zero; order];
    if order % 2 == 1 {
        poles.push(Complex64::new(0.0, 0.0));
        zeros.push(0.0);
    }
    let mut rows = vec![[0.0; 6]; order.div_ceil(2)];
    for row in rows.iter_mut().rev() {
        let index = poles
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (1.0 - a.norm()).abs().total_cmp(&(1.0 - b.norm()).abs()))
            .unwrap()
            .0;
        let p1 = poles.remove(index);
        let p2 = if p1.im == 0.0 {
            let index = poles
                .iter()
                .enumerate()
                .filter(|(_, p)| p.im == 0.0)
                .min_by(|(_, a), (_, b)| (1.0 - a.norm()).abs().total_cmp(&(1.0 - b.norm()).abs()))
                .unwrap()
                .0;
            poles.remove(index)
        } else {
            p1.conj()
        };
        let mut take_zero = || {
            let index = zeros
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| (p1 - **a).norm().total_cmp(&(p1 - **b).norm()))
                .unwrap()
                .0;
            zeros.remove(index)
        };
        let z1 = take_zero();
        let z2 = take_zero();
        *row = [1.0, -(z1 + z2), z1 * z2, 1.0, -(p1 + p2).re, (p1 * p2).re];
    }
    for v in &mut rows[0][..3] {
        *v *= gain;
    }
    Sos(rows)
}

/// Butterworth band-pass, normalized edges (1 is Nyquist), SciPy nearest SOS.
/// The analog low-pass prototype is transformed before the bilinear map.
fn butter_bandpass(order: usize, low: f64, high: f64) -> Sos {
    assert!(
        order > 0 && low.is_finite() && high.is_finite() && 0. < low && low < high && high < 1.
    );
    let lo = 4. * (PI * low / 2.).tan();
    let hi = 4. * (PI * high / 2.).tan();
    let bw = hi - lo;
    let wo = (lo * hi).sqrt();
    let mut analog = Vec::with_capacity(2 * order);
    let mut roots = Vec::with_capacity(order);
    for i in 0..order {
        let m = -(order as isize - 1) + 2 * i as isize;
        roots.push(-Complex64::from_polar(1., PI * m as f64 / (2. * order as f64)) * bw / 2.);
    }
    for sign in [1., -1.] {
        for p in &roots {
            analog.push(*p + sign * (*p * *p - wo * wo).sqrt());
        }
    }
    let mut denom = Complex64::new(1., 0.);
    for p in &analog {
        denom *= 4. - p;
    }
    let gain = bw.powi(order as i32) * (Complex64::new(4_f64.powi(order as i32), 0.) / denom).re;
    let mut poles: Vec<_> = analog
        .iter()
        .map(|p| divide(4. + p, 4. - p))
        .filter(|p| p.im > 0.)
        .collect();
    poles.sort_by(|a, b| a.re.total_cmp(&b.re).then(a.im.total_cmp(&b.im)));
    let mut zeros = vec![1.; order];
    zeros.extend(vec![-1.; order]);
    let mut rows = vec![[0.; 6]; order];
    for row in rows.iter_mut().rev() {
        let k = poles
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (1. - a.norm()).abs().total_cmp(&(1. - b.norm()).abs()))
            .unwrap()
            .0;
        let p = poles.remove(k);
        let mut take = || {
            let k = zeros
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| (p - **a).norm().total_cmp(&(p - **b).norm()))
                .unwrap()
                .0;
            zeros.remove(k)
        };
        let z1 = take();
        let z2 = take();
        *row = [1., -(z1 + z2), z1 * z2, 1., -2. * p.re, p.norm_sqr()];
    }
    for b in &mut rows[0][..3] {
        *b *= gain;
    }
    Sos(rows)
}

/// Direct form II transposed, sections in the supplied order, fresh zero state.
/// Errors for empty samples or a0 != 1. Nonfinite samples and coefficients
/// propagate arithmetically, including NaN numerator coefficients.
pub fn sosfilt(sos: &Sos, x: &[f64]) -> Result<Vec<f64>, DspError> {
    if x.is_empty() || sos.0.is_empty() || sos.0.iter().any(|s| s[3] != 1.0) {
        return Err(invalid("empty samples or non-normalized SOS"));
    }
    let mut output = x.to_vec();
    // Process four sections together: independent section states let the CPU
    // overlap their feedback arithmetic, without changing per-section ordering.
    let (groups, remainder) = sos.0.as_chunks::<4>();
    for group in groups {
        let mut state = [[0.0; 2]; 4];
        for sample in &mut output {
            let mut value = *sample;
            for (s, z) in group.iter().zip(&mut state) {
                let y = s[0] * value + z[0];
                z[0] = s[1] * value - s[4] * y + z[1];
                z[1] = s[2] * value - s[5] * y;
                value = y;
            }
            *sample = value;
        }
    }
    for s in remainder {
        let mut z1 = 0.0;
        let mut z2 = 0.0;
        for x in &mut output {
            let y = s[0] * *x + z1;
            z1 = s[1] * *x - s[4] * y + z2;
            z2 = s[2] * *x - s[5] * y;
            *x = y;
        }
    }
    Ok(output)
}

/// Digital Butterworth SOS with explicit sample rate, matching SciPy's
/// lowpass/highpass/bandpass types used upstream. Invalid cutoffs return Err.
/// Orders too large to represent or allocate are programmer errors.
pub fn butter(order: usize, cutoff: &[f64], kind: BType, fs: f64) -> Result<Sos, DspError> {
    let n = if matches!(kind, BType::Bandpass) {
        2
    } else {
        1
    };
    if cutoff.len() != n
        || !fs.is_finite()
        || fs <= 0.
        || cutoff
            .iter()
            .any(|x| !x.is_finite() || *x <= 0. || *x >= fs / 2.)
        || n == 2 && cutoff[0] >= cutoff[1]
    {
        return Err(invalid("invalid Butterworth cutoff/sample rate"));
    }
    if order == 0 {
        return Ok(Sos(vec![[1., 0., 0., 1., 0., 0.]]));
    }
    let wn: Vec<_> = cutoff.iter().map(|x| x / (fs / 2.)).collect();
    Ok(if n == 2 {
        butter_bandpass(order, wn[0], wn[1])
    } else {
        butter_single(order, wn[0], kind)
    })
}

/// Evaluate SOS on supplied frequencies in Hz (including DC and Nyquist).
pub fn sosfreqz(sos: &Sos, f: &[f64], fs: f64) -> Result<Vec<Complex64>, DspError> {
    if sos.0.is_empty() || sos.0.iter().any(|s| s[3] != 1.) {
        return Err(invalid("invalid SOS for frequency response"));
    }
    let mut out = vec![Complex64::new(1., 0.); f.len()];
    for s in &sos.0 {
        for (o, &f) in out.iter_mut().zip(f) {
            let w = 2. * PI * f / fs;
            let z = Complex64::from_polar(1., -w);
            let b = (s[2] * z + s[1]) * z + s[0];
            let a = (s[5] * z + s[4]) * z + s[3];
            *o *= divide(b, a);
        }
    }
    Ok(out)
}
