//! Measured rolloff and realized-response protection (ADR 0022).

use std::sync::Arc;

use secs_dsp::{
    interp,
    response::{magnitude, peq_response},
    smooth::{Fraction, smooth_power},
};

use crate::{C, decay, grid::design_freqs, input::Peq, levels};

/// Position-weighted power, not the coherent sum across microphone positions.
/// User filter corners select the same passband-maximum rule as MIMO; MIMO's
/// existing range finder and its arithmetic remain untouched.
pub(crate) fn rolloff(
    by_pos: &[(u32, Arc<[C]>)],
    main_position: u32,
    sub_filters: Option<(f64, f64)>,
) -> Option<f64> {
    let f = design_freqs();
    let positions: Vec<_> = by_pos.iter().map(|(p, _)| *p).collect();
    let weights = levels::weights(&positions, main_position);
    let mut power = vec![0.0; f.len()];
    let mut total = 0.0;
    for ((_, h), w) in by_pos.iter().zip(weights) {
        total += w;
        for (p, &h) in power.iter_mut().zip(h.iter()) {
            let a = magnitude(h);
            *p += w * a * a;
        }
    }
    if total == 0.0 {
        return None;
    }
    for p in &mut power {
        *p /= total;
    }
    let db: Vec<_> = smooth_power(f, &power, Fraction::Scalar(3.0))
        .expect("design grid")
        .iter()
        .map(|p| 10.0 * (p + 1e-30).log10())
        .collect();
    let (lo, hi) = if sub_filters.is_some() {
        (40.0, 80.0)
    } else {
        (200.0, 1000.0)
    };
    let mut reference = decay::median(
        &f.iter()
            .zip(&db)
            .filter(|(f, _)| (lo..=hi).contains(*f))
            .map(|(_, &v)| v)
            .collect::<Vec<_>>(),
    );
    let mut start = lo;
    if let Some((hp, lp)) = sub_filters.filter(|&(hp, lp)| hp != 0.0 || lp != 0.0) {
        let lo = hp.max(15.0);
        let hi = (if lp != 0.0 { lp.min(80.0) } else { 80.0 }).max(lo * 1.5);
        let indices: Vec<_> = f
            .iter()
            .enumerate()
            .filter(|(_, f)| (lo..=hi).contains(f))
            .map(|(i, _)| i)
            .collect();
        if indices.is_empty() {
            return None;
        }
        let values: Vec<_> = indices.iter().map(|&i| db[i]).collect();
        let k = indices[decay::first_max(&values)];
        reference = db[k];
        start = f[k];
    }
    let threshold = reference - 10.0;
    // Include the exact measured-band edge and interpolate the first crossing.
    let mut upper = start;
    let mut upper_db = interp(start, f, &db);
    for lower in f
        .iter()
        .copied()
        .rev()
        .filter(|&v| v >= 20.0 && v < start)
        .chain(std::iter::once(20.0))
    {
        let lower_db = interp(lower, f, &db);
        if lower_db <= threshold {
            return Some(if upper_db > threshold {
                lower + (upper - lower) * (threshold - lower_db) / (upper_db - lower_db)
            } else {
                upper
            });
        }
        upper = lower;
        upper_db = lower_db;
    }
    None
}

pub(crate) fn clamp_centres(peq: &mut [Peq], rolloff: f64) {
    for (hz, _, gain) in peq {
        if *hz < rolloff {
            *gain = gain.min(0.0);
        }
    }
}

/// `fixed` is the FIR (and any unmodified cuts), or unity for a PEQ-only set.
/// False means even removing every eligible boost cannot meet +0.5 dB; in that
/// case preserve the original fit, as specified by the ADR. Exactly 40 steps
/// keep results deterministic and return the feasible side of the bracket.
pub(crate) fn limit(peq: &mut [Peq], fixed: &[C], rolloff: f64) -> bool {
    let n = design_freqs().partition_point(|&f| f < rolloff);
    let f = &design_freqs()[..n];
    let original = peq.to_vec();
    let scaled = |k: f64| -> Vec<Peq> {
        original
            .iter()
            .map(|&(hz, q, gain)| {
                (
                    hz,
                    q,
                    if gain > 0.0 && hz < 4.0 * rolloff {
                        gain * k
                    } else {
                        gain
                    },
                )
            })
            .collect()
    };
    let acceptable = |bands: &[Peq]| {
        peq_response(f, bands)
            .iter()
            .zip(fixed)
            .all(|(&p, &h)| 20.0 * magnitude(p * h).max(1e-30).log10() <= 0.5)
    };
    if acceptable(&original) {
        return true;
    }
    if !acceptable(&scaled(0.0)) {
        return false;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if acceptable(&scaled(mid)) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    peq.copy_from_slice(&scaled(lo));
    true
}
