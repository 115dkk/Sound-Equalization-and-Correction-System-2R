//! Position weights, target sampling, band levels and Schroeder frequency.
//! Upstream: `_weights`, `_freq_weights`, `_band_level`, `schroeder_hz`, `_TargetFn`.

use crate::grid::{BINS, SR_F, design_freqs};
use crate::measurement::Spectra;
use crate::{EngineConfig, MeasurementSet};
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
            strict: schroeder_hz(set, config),
            lenient: (config.rc_hi_basis == secs_types::calibration::RcHiBasis::Lenient)
                .then(|| lenient_schroeder_hz(set, config))
                .flatten(),
        })
    }
}

pub fn schroeder_hz(set: &MeasurementSet, config: &EngineConfig) -> Option<f64> {
    let volume = config.room_volume_m3?;
    if volume == 0.0 {
        return None;
    }
    let mut t60s = Vec::new();
    for role in &config.mains {
        let Some(m) = set.get(config.main_position, role) else {
            continue;
        };
        if m.samples.is_empty() {
            continue;
        }
        let ir: Vec<_> = (0..m.samples.len()).map(|i| m.samples.at(i)).collect();
        for fc in [500.0, 1000.0] {
            let sos = iir::butter(
                4,
                &[fc / 2.0_f64.sqrt(), fc * 2.0_f64.sqrt()],
                BType::Bandpass,
                SR_F,
            )
            .ok()?;
            let y = iir::sosfilt(&sos, &ir).ok()?;
            let mut e = vec![0.0; y.len()];
            let mut sum = 0.0;
            for (e, y) in e.iter_mut().zip(&y).rev() {
                sum += y * y;
                *e = sum;
            }
            let origin = e[0] + 1e-30;
            let db: Vec<_> = e
                .iter()
                .map(|e| 10.0 * (e / origin + 1e-30).log10())
                .collect();
            let i5 = db.iter().position(|v| *v < -5.0).unwrap_or(0);
            let i25 = db.iter().position(|v| *v < -25.0).unwrap_or(0);
            if i25 > i5 && i5 > 0 {
                t60s.push(3.0 * (i25 - i5) as f64 / SR_F);
            }
        }
    }
    if t60s.is_empty() {
        None
    } else {
        Some(2000.0 * (median(t60s) / volume).sqrt())
    }
}

/// Nominal base-10 third-octave centres used by ADR 0021.
pub(crate) const RC_BAND_CENTRES: [f64; 14] = [
    100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 400.0, 500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0,
    2000.0,
];

/// Highest low-frequency band whose local Schroeder estimate reaches its centre.
/// Evidence is (band centre Hz, median T60 s, local Schroeder Hz).
pub fn lenient_schroeder_hz(
    set: &MeasurementSet,
    config: &EngineConfig,
) -> Option<(f64, f64, f64)> {
    let volume = config
        .room_volume_m3
        .filter(|v| v.is_finite() && *v > 0.0)?;
    let width = 2.0_f64.powf(1.0 / 6.0);
    let mut chosen = None;
    for fc in RC_BAND_CENTRES.into_iter().take(8) {
        let sos = iir::butter(4, &[fc / width, fc * width], BType::Bandpass, SR_F).ok()?;
        let mut t60s = Vec::new();
        for (position, _) in &set.positions {
            for role in &config.mains {
                let Some(m) = set.get(*position, role).filter(|m| !m.samples.is_empty()) else {
                    continue;
                };
                let ir: Vec<_> = (0..m.samples.len()).map(|i| m.samples.at(i)).collect();
                let y = iir::sosfilt(&sos, &ir).ok()?;
                let mut energy = vec![0.0; y.len()];
                let mut sum = 0.0;
                for (e, y) in energy.iter_mut().zip(&y).rev() {
                    sum += y * y;
                    *e = sum;
                }
                let origin = energy[0] + 1e-30;
                let db: Vec<_> = energy
                    .iter()
                    .map(|e| 10.0 * (e / origin + 1e-30).log10())
                    .collect();
                let i5 = db.iter().position(|v| *v < -5.0).unwrap_or(0);
                let i25 = db.iter().position(|v| *v < -25.0).unwrap_or(0);
                if i25 > i5 && i5 > 0 {
                    t60s.push(3.0 * (i25 - i5) as f64 / SR_F);
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
    chosen
}

pub(crate) fn schroeder_limit(strict: Option<f64>, band: Option<f64>, factor: u8) -> f64 {
    let value = match (strict, band) {
        (Some(s), Some(l)) => Some(s.max(l)),
        (s, l) => s.or(l),
    };
    let factor = f64::from(factor);
    value.map_or(500.0, |v| (v * factor).clamp(100.0, 500.0 * factor))
}
