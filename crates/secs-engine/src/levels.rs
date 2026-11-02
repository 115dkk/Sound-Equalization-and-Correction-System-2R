//! Position weights, target sampling, band levels and Schroeder frequency.
//! Upstream: `_weights`, `_freq_weights`, `_band_level`, `schroeder_hz`, `_TargetFn`.

use crate::grid::{BINS, SR_F, design_freqs};
use crate::measurement::Spectra;
use crate::{EngineConfig, EngineError, MeasurementSet, Samples};
use secs_dsp::iir::{self, BType};
use secs_types::target::TargetCurve;
use std::sync::Arc;

pub const MAIN_WEIGHT: f64 = 5.0;

pub fn weights(positions: &[u32], main_position: u32) -> Vec<f64> {
    positions
        .iter()
        .map(|&p| if p == main_position { MAIN_WEIGHT } else { 1.0 })
        .collect()
}

pub fn freq_weights(positions: &[u32], main_position: u32, main_weight_lo: f64) -> Vec<Arc<[f64]>> {
    let main: Arc<[f64]> = design_freqs()
        .iter()
        .map(|&f| {
            let t = ((f.max(1.0) / 200.0).log2() / (500.0_f64 / 200.0).log2()).clamp(0.0, 1.0);
            main_weight_lo + (MAIN_WEIGHT - main_weight_lo) * t
        })
        .collect();
    let other: Arc<[f64]> = vec![1.0; BINS].into();
    positions
        .iter()
        .map(|&p| {
            if p == main_position {
                main.clone()
            } else {
                other.clone()
            }
        })
        .collect()
}

pub fn target_db(target: &TargetCurve, f: &[f64]) -> Vec<f64> {
    target.sample(f)
}

pub(crate) fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() || values.iter().any(|v| v.is_nan()) {
        return f64::NAN;
    }
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

pub fn band_level(
    h: &Spectra,
    role: &str,
    weights: &[f64],
    target: &TargetCurve,
    lo: f64,
    hi: f64,
) -> f64 {
    if weights.len() != h.positions().len() {
        return f64::NAN;
    }
    let bins: Vec<_> = design_freqs()
        .iter()
        .enumerate()
        .filter(|(_, f)| **f >= lo && **f <= hi)
        .collect();
    let mut power = vec![0.0; bins.len()];
    let mut total = 0.0;
    for (&pos, &w) in h.positions().iter().zip(weights) {
        if let Some(spectrum) = h.get(pos, role) {
            if spectrum.len() != BINS {
                return f64::NAN;
            }
            total += w;
            for (p, &(i, _)) in power.iter_mut().zip(&bins) {
                // NumPy abs(complex) uses hypot, not re² + im².
                let magnitude = spectrum[i].re.hypot(spectrum[i].im);
                *p += w * (magnitude * magnitude);
            }
        }
    }
    let f: Vec<_> = bins.iter().map(|(_, f)| **f).collect();
    let target = target_db(target, &f);
    median(
        power
            .into_iter()
            .zip(target)
            .map(|(p, t)| 10.0 * (p / total + 1e-30).log10() - t)
            .collect(),
    )
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Schroeder {
    pub strict: Option<f64>,
    pub lenient: Option<(f64, f64, f64)>,
}

impl Schroeder {
    pub(crate) fn analyze(
        set: &MeasurementSet,
        config: &EngineConfig,
        ctx: &secs_compute::Context,
    ) -> Result<Self, crate::EngineError> {
        Self::analyze_until(set, config, ctx, &|| ctx.cancel().is_cancelled())
    }

    pub(crate) fn analyze_until(
        set: &MeasurementSet,
        config: &EngineConfig,
        ctx: &secs_compute::Context,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, crate::EngineError> {
        check_analysis(cancelled)?;
        if config.rc_range.1.is_some() || config.room_volume_m3.is_none_or(|v| v == 0.0) {
            return Ok(Self::default());
        }
        let lenient = config.rc_hi_basis == secs_types::calibration::RcHiBasis::Lenient;
        let longest = set
            .positions
            .iter()
            .filter(|(position, _)| lenient || *position == config.main_position)
            .flat_map(|(_, roles)| roles)
            .filter(|(role, _)| config.mains.contains(role))
            .map(|(_, m)| m.samples.len())
            .max()
            .unwrap_or(0);
        // Sequential analysis: input, filtered samples, reverse energy and dB.
        // This permit ends before design/XO reserve their own working arrays.
        let _permit = crate::memory::reserve(ctx, (longest as u64).saturating_mul(4 * 8), 0)?;
        Ok(Self {
            strict: strict_until(set, config, cancelled)?,
            lenient: if lenient {
                lenient_until(set, config, cancelled)?
            } else {
                None
            },
        })
    }
}

#[cfg(test)]
pub fn schroeder_hz(set: &MeasurementSet, config: &EngineConfig) -> Option<f64> {
    strict_until(set, config, &|| false).unwrap()
}

fn strict_until(
    set: &MeasurementSet,
    config: &EngineConfig,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<f64>, EngineError> {
    let Some(volume) = config.room_volume_m3.filter(|v| *v != 0.0) else {
        return Ok(None);
    };
    let mut t60s = Vec::new();
    for role in &config.mains {
        check_analysis(cancelled)?;
        let Some(m) = set
            .get(config.main_position, role)
            .filter(|m| !m.samples.is_empty())
        else {
            continue;
        };
        for fc in [500.0, 1000.0] {
            check_analysis(cancelled)?;
            let Ok(sos) = iir::butter(
                4,
                &[fc / 2.0_f64.sqrt(), fc * 2.0_f64.sqrt()],
                BType::Bandpass,
                SR_F,
            ) else {
                return Ok(None);
            };
            if let Some(t60) = band_t60(&m.samples, &sos, cancelled)? {
                t60s.push(t60);
            }
        }
    }
    Ok(if t60s.is_empty() {
        None
    } else {
        Some(2000.0 * (median(t60s) / volume).sqrt())
    })
}

/// Nominal base-10 third-octave centres used by ADR 0021.
pub(crate) const RC_BAND_CENTRES: [f64; 14] = [
    100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 400.0, 500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0,
    2000.0,
];

/// Highest low-frequency band whose local Schroeder estimate reaches its centre.
/// Evidence is (band centre Hz, median T60 s, local Schroeder Hz).
#[cfg(test)]
pub fn lenient_schroeder_hz(
    set: &MeasurementSet,
    config: &EngineConfig,
) -> Option<(f64, f64, f64)> {
    lenient_until(set, config, &|| false).unwrap()
}

fn lenient_until(
    set: &MeasurementSet,
    config: &EngineConfig,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<(f64, f64, f64)>, EngineError> {
    let Some(volume) = config.room_volume_m3.filter(|v| v.is_finite() && *v > 0.0) else {
        return Ok(None);
    };
    let width = 2.0_f64.powf(1.0 / 6.0);
    let mut chosen = None;
    for fc in RC_BAND_CENTRES.into_iter().take(8) {
        check_analysis(cancelled)?;
        let Ok(sos) = iir::butter(4, &[fc / width, fc * width], BType::Bandpass, SR_F) else {
            return Ok(None);
        };
        let mut t60s = Vec::new();
        for (position, _) in &set.positions {
            for role in &config.mains {
                check_analysis(cancelled)?;
                let Some(m) = set.get(*position, role).filter(|m| !m.samples.is_empty()) else {
                    continue;
                };
                if let Some(t60) = band_t60(&m.samples, &sos, cancelled)? {
                    t60s.push(t60);
                }
            }
        }
        if !t60s.is_empty() {
            let t60 = median(t60s);
            let g = 2000.0 * (t60 / volume).sqrt();
            if g >= fc {
                chosen = Some((fc, t60, g));
            }
        }
    }
    Ok(chosen)
}

const ANALYSIS_CHUNK: usize = 4096;

fn check_analysis(cancelled: &dyn Fn() -> bool) -> Result<(), EngineError> {
    if cancelled() {
        Err(EngineError::Cancelled)
    } else {
        Ok(())
    }
}

// Both estimates keep the same sample, SOS-section and reverse-sum order.
fn band_t60(
    samples: &Samples,
    sos: &iir::Sos,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<f64>, EngineError> {
    let mut ir = Vec::with_capacity(samples.len());
    for start in (0..samples.len()).step_by(ANALYSIS_CHUNK) {
        check_analysis(cancelled)?;
        ir.extend((start..(start + ANALYSIS_CHUNK).min(samples.len())).map(|i| samples.at(i)));
    }
    let y = filter_band(&ir, sos, cancelled)?;
    let mut energy = vec![0.0; y.len()];
    let mut sum = 0.0;
    for (i, (e, y)) in energy.iter_mut().zip(&y).rev().enumerate() {
        if i.is_multiple_of(ANALYSIS_CHUNK) {
            check_analysis(cancelled)?;
        }
        sum += y * y;
        *e = sum;
    }
    let origin = energy[0] + 1e-30;
    let mut db = Vec::with_capacity(energy.len());
    for chunk in energy.chunks(ANALYSIS_CHUNK) {
        check_analysis(cancelled)?;
        db.extend(chunk.iter().map(|e| 10.0 * (e / origin + 1e-30).log10()));
    }
    let crossing = |threshold| -> Result<usize, EngineError> {
        for (i, chunk) in db.chunks(ANALYSIS_CHUNK).enumerate() {
            check_analysis(cancelled)?;
            if let Some(j) = chunk.iter().position(|v| *v < threshold) {
                return Ok(i * ANALYSIS_CHUNK + j);
            }
        }
        Ok(0)
    };
    let i5 = crossing(-5.0)?;
    let i25 = crossing(-25.0)?;
    Ok((i25 > i5 && i5 > 0).then(|| 3.0 * (i25 - i5) as f64 / SR_F))
}

// Fourth-order bandpass has exactly four sections. This is sosfilt's grouped
// DF-II transposed arithmetic, with state retained across cancellation chunks.
pub(crate) fn filter_band(
    ir: &[f64],
    sos: &iir::Sos,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<f64>, EngineError> {
    assert_eq!(sos.0.len(), 4);
    let mut output = Vec::with_capacity(ir.len());
    let mut state = [[0.0; 2]; 4];
    for chunk in ir.chunks(ANALYSIS_CHUNK) {
        check_analysis(cancelled)?;
        for &sample in chunk {
            let mut value = sample;
            for (s, z) in sos.0.iter().zip(&mut state) {
                let y = s[0] * value + z[0];
                z[0] = s[1] * value - s[4] * y + z[1];
                z[1] = s[2] * value - s[5] * y;
                value = y;
            }
            output.push(value);
        }
    }
    Ok(output)
}

pub(crate) fn schroeder_limit(strict: Option<f64>, band: Option<f64>, factor: u8) -> f64 {
    let value = match (strict, band) {
        (Some(s), Some(l)) => Some(s.max(l)),
        (s, l) => s.or(l),
    };
    let factor = f64::from(factor);
    value.map_or(500.0, |v| (v * factor).clamp(100.0, 500.0 * factor))
}
