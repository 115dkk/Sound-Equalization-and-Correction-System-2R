//! The response simulator over the signal-chain value: replays any stage of
//! the designed DSP on measured spectra.
//!
//! Upstream: `CorrectionEngine.stage_responses`, `input_response`,
//! `calibration._sub_out`, `xo_preview`, `sub_combine`, `stage_items`.

use crate::C;
use crate::input::Allpass;
use crate::measurement::ramp_phase;
use secs_dsp::response::allpass2;

/// One sub's BA row: upstream's `(gain dB, delay ms, polarity, all-passes)`.
#[derive(Debug, Clone, PartialEq)]
pub struct BaRow {
    /// Gain in dB.
    pub gain_db: f64,
    /// Delay in ms, internal axis.
    pub delay_ms: f64,
    /// Polarity, `1` or `-1`.
    pub polarity: i8,
    /// All-passes, in order.
    pub allpass: Vec<Allpass>,
}

/// One sub's output term, `_term`: `h · 10^(g/20) · polarity · delay · Π all-pass`
/// on the frequencies `f` (`h` and `f` have the same length: the design grid
/// or a subset of it). Each factor is a complex multiplication in NumPy's
/// order and formula (a real factor is the complex `(x, 0)`).
pub fn term(h: &[C], f: &[f64], row: &BaRow) -> Vec<C> {
    let gain = C::new(10f64.powf(row.gain_db / 20.0), 0.0);
    let polarity = C::new(f64::from(row.polarity), 0.0);
    let mut out: Vec<C> = h
        .iter()
        .zip(f)
        .map(|(&h, &f)| {
            let delay = C::from_polar(1.0, ramp_phase(f, row.delay_ms) * 0.001);
            h * gain * polarity * delay
        })
        .collect();
    for &(f0, q) in &row.allpass {
        for (v, a) in out.iter_mut().zip(allpass2(f, f0, q)) {
            *v *= a;
        }
    }
    out
}

/// `_sub_sum` at one position: the sum of the subs' terms in sub order,
/// starting, as upstream, from the integer 0 (so a zero first term is +0).
pub fn sub_sum(terms: &[(&[C], &BaRow)], f: &[f64]) -> Vec<C> {
    let mut sum = vec![C::new(0.0, 0.0); f.len()];
    for (h, row) in terms {
        for (s, t) in sum.iter_mut().zip(term(h, f, row)) {
            *s += t;
        }
    }
    sum
}

/// `h[idx]`: the bins of a design-grid spectrum at `idx`.
pub fn gather<T: Copy>(h: &[T], idx: &[usize]) -> Vec<T> {
    idx.iter().map(|&i| h[i]).collect()
}
