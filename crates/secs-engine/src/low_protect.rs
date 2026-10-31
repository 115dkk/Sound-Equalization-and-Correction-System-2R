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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LimitOutcome {
    pub reduced: usize,
    pub cuts: usize,
}

fn db(h: C) -> f64 {
    let magnitude = magnitude(h);
    if magnitude.is_finite() {
        20.0 * magnitude.max(1e-30).log10()
    } else {
        f64::INFINITY
    }
}

fn worst(
    peq: &[Peq],
    fixed: &[C],
    rolloff: f64,
    floor: f64,
    eligible: Option<&[bool]>,
) -> Option<(usize, f64)> {
    let n = design_freqs().partition_point(|&f| f < rolloff);
    let mut worst = None;
    for (i, (p, &h)) in peq_response(&design_freqs()[..n], peq)
        .into_iter()
        .zip(fixed)
        .enumerate()
    {
        if design_freqs()[i] < floor || eligible.is_some_and(|mask| !mask[i]) {
            continue;
        }
        let excess = db(p * h) - 0.5;
        if excess > 0.0 && worst.is_none_or(|(_, previous)| excess > previous) {
            worst = Some((i, excess));
        }
    }
    worst
}

pub(crate) fn limited(peq: &[Peq], fixed: &[C], rolloff: f64) -> bool {
    worst(peq, fixed, rolloff, 10.0, None).is_none()
}

/// Reduce only the boost contributing most at the worst protected bin, then
/// add cuts for fixed-response leakage. Never restore the unprotected fit.
/// `fixed` is the FIR (and unmodified cuts), or unity for a PEQ-only set.
pub(crate) fn limit(peq: &mut Vec<Peq>, fixed: &[C], rolloff: f64) -> LimitOutcome {
    let mut outcome = LimitOutcome::default();
    let mut reduced = vec![false; peq.len()];
    for _ in 0..4 * peq.len() + 8 {
        let n = design_freqs().partition_point(|&f| f < rolloff);
        let mut eligible = vec![false; n];
        for band in peq.iter().filter(|band| band.2 > 0.0) {
            for (allow, h) in eligible
                .iter_mut()
                .zip(peq_response(&design_freqs()[..n], &[*band]))
            {
                *allow |= db(h) > 0.05;
            }
        }
        let Some((i, _)) = worst(peq, fixed, rolloff, 0.0, Some(&eligible)) else {
            break;
        };
        let f = [design_freqs()[i]];
        let mut candidate = None;
        for (band, &(_, _, gain)) in peq.iter().enumerate() {
            if gain > 0.0 {
                let contribution = db(peq_response(&f, &peq[band..=band])[0]);
                if contribution > 0.05
                    && candidate.is_none_or(|(_, previous)| contribution > previous)
                {
                    candidate = Some((band, contribution));
                }
            }
        }
        let Some((band, _)) = candidate else {
            break;
        };
        let (mut lo, mut hi) = (0.0, peq[band].2);
        peq[band].2 = 0.0;
        if db(peq_response(&f, peq)[0] * fixed[i]) <= 0.5 {
            for _ in 0..40 {
                let mid = (lo + hi) / 2.0;
                peq[band].2 = mid;
                if db(peq_response(&f, peq)[0] * fixed[i]) <= 0.5 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
        }
        peq[band].2 = lo;
        if !reduced[band] {
            reduced[band] = true;
            outcome.reduced += 1;
        }
    }
    for _ in 0..4 {
        let Some((i, excess)) = worst(peq, fixed, rolloff, 10.0, None) else {
            return outcome;
        };
        // DC/subsonic gain is corrected in the FIR, not with peaking cuts.
        if !excess.is_finite() {
            return outcome;
        }
        peq.push((design_freqs()[i], 1.4, -(excess + 0.1)));
        outcome.cuts += 1;
    }
    for _ in 0..8 {
        let Some((_, excess)) = worst(peq, fixed, rolloff, 10.0, None) else {
            break;
        };
        if !excess.is_finite() {
            break;
        }
        peq.last_mut().expect("protective cuts appended").2 -= excess;
    }
    outcome
}
