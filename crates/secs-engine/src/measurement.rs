//! Measurement preparation on the upstream internal time axis.
//! Upstream: `onset_index`, `windowed`, `_prepare`, `long_spectra`, `_delay`.

use crate::grid::{BINS, N_FFT, SR, SR_F, design_freqs};
use crate::setup::{Setup, SubFilter};
use crate::{C, EngineConfig, EngineError, MeasurementSet, Samples};
use secs_dsp::{fft, response};
use std::{f64::consts::PI, sync::Arc};

pub const WINDOW_MS: f64 = 500.0;

pub fn onset_index(ir: &Samples) -> usize {
    if ir.is_empty() {
        return 0;
    }
    let mut peak = 0;
    for i in 1..ir.len() {
        if ir.at(i).abs() > ir.at(peak).abs() {
            peak = i;
        }
    }
    let threshold = 0.1 * ir.at(peak).abs();
    (0..=peak)
        .find(|&i| ir.at(i).abs() >= threshold)
        .unwrap_or(peak)
}

fn window(ir: &Samples, start: usize, length: usize, taper: f64) -> (Vec<f64>, usize) {
    let a = start.saturating_sub(96).min(ir.len());
    let end = a.saturating_add(length).min(ir.len());
    let mut w: Vec<_> = (a..end).map(|i| ir.at(i)).collect();
    let k = (w.len() as f64 * taper) as usize;
    let from = w.len() - k;
    for (i, v) in w[from..].iter_mut().enumerate() {
        *v *= 0.5 * (1.0 + (PI * i as f64 / k as f64).cos());
    }
    (w, a)
}

pub fn windowed(ir: &Samples, start: usize) -> (Vec<f64>, usize) {
    window(ir, start, (WINDOW_MS / 1000.0 * SR_F) as usize, 0.2)
}

pub fn spectrum(x: &[f64]) -> Vec<C> {
    let mut padded = vec![0.0; N_FFT];
    let n = x.len().min(N_FFT);
    padded[..n].copy_from_slice(&x[..n]);
    fft::rfft(&padded)
}

/// The phase NumPy computes for `-2j * np.pi * f * t`: the imaginary part of
/// two complex multiplications, `0.0 + (-2π·f)·t` (the `0.0 +` is the real
/// part's contribution and turns a zero product into +0).
pub(crate) fn ramp_phase(f: f64, t: f64) -> f64 {
    0.0 + (-2.0 * PI * f) * t
}

/// `_delay`: `np.exp(-2j * np.pi * f * ms / 1000.0)`. NumPy divides the
/// complex array by 1000 as a multiplication by the reciprocal (its complex
/// division loop), so the phase is `ramp·0.001`, not `ramp/1000`; the real
/// part stays zero, so the exponential is `(cos θ, sin θ)`.
pub fn delay_response(ms: f64) -> Vec<C> {
    design_freqs()
        .iter()
        .map(|&f| C::from_polar(1.0, ramp_phase(f, ms) * 0.001))
        .collect()
}

pub fn sub_filter_response(filter: &SubFilter) -> Vec<C> {
    let mut out = vec![C::new(1.0, 0.0); BINS];
    for (hz, kind, typ) in [
        (filter.hp_hz, "HP", filter.hp_type),
        (filter.lp_hz, "LP", filter.lp_type),
    ] {
        if hz > 0.0 {
            // BandType admits only the five names accepted by this primitive.
            if let Ok(h) = response::band_filter(design_freqs(), hz, kind, typ.name()) {
                for (v, h) in out.iter_mut().zip(h) {
                    *v *= h;
                }
            }
        }
    }
    if !filter.peq.is_empty() {
        for (v, h) in out
            .iter_mut()
            .zip(response::peq_response(design_freqs(), &filter.peq))
        {
            *v *= h;
        }
    }
    out
}

/// Ordered cells; cloning copies only the Arc handles, never a spectrum.
#[derive(Debug, Clone, Default)]
pub struct Spectra {
    positions: Vec<u32>,
    cells: Vec<Vec<(String, Arc<[C]>)>>,
}

impl Spectra {
    pub fn positions(&self) -> &[u32] {
        &self.positions
    }
    pub fn get(&self, position: u32, role: &str) -> Option<&Arc<[C]>> {
        let p = self.positions.binary_search(&position).ok()?;
        self.cells[p]
            .iter()
            .find(|(r, _)| r == role)
            .map(|(_, h)| h)
    }
    pub fn roles_at(&self, position: u32) -> impl Iterator<Item = &str> {
        self.positions
            .binary_search(&position)
            .ok()
            .into_iter()
            .flat_map(|p| self.cells[p].iter().map(|(r, _)| r.as_str()))
    }
    pub fn set(&mut self, position: u32, role: &str, value: Arc<[C]>) {
        let p = self.ensure_position(position);
        if let Some((_, h)) = self.cells[p].iter_mut().find(|(r, _)| r == role) {
            *h = value;
        } else {
            self.cells[p].push((role.to_owned(), value));
        }
    }
    fn ensure_position(&mut self, position: u32) -> usize {
        match self.positions.binary_search(&position) {
            Ok(p) => p,
            Err(p) => {
                self.positions.insert(p, position);
                self.cells.insert(p, Vec::new());
                p
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Prepared {
    pub arrival_s: Vec<(String, f64)>,
    pub align_delay_ms: Vec<(String, f64)>,
    pub base_s: f64,
    pub h: Spectra,
    pub h0: Spectra,
}

fn validate(set: &MeasurementSet) -> Result<(), EngineError> {
    let mut previous = None;
    for (pos, roles) in &set.positions {
        if previous.is_some_and(|p| p >= *pos) {
            return Err(EngineError::Input(
                "positions must be strictly ascending".into(),
            ));
        }
        previous = Some(*pos);
        for (i, (role, m)) in roles.iter().enumerate() {
            if m.sample_rate != SR {
                return Err(EngineError::MeasurementSampleRate {
                    position: *pos,
                    role: role.clone(),
                    actual_hz: m.sample_rate,
                    expected_hz: SR,
                });
            }
            if m.samples.is_empty()
                || !m.t0.is_finite()
                || (0..m.samples.len()).any(|i| !m.samples.at(i).is_finite())
            {
                return Err(EngineError::InvalidMeasurement {
                    position: *pos,
                    role: role.clone(),
                });
            }
            if roles[..i].iter().any(|(r, _)| r == role) {
                return Err(EngineError::DuplicateMeasurement {
                    position: *pos,
                    role: role.clone(),
                });
            }
        }
    }
    Ok(())
}

fn build_spectra(
    set: &MeasurementSet,
    base: f64,
    filters: &[(String, SubFilter)],
    length: usize,
    taper: f64,
) -> Spectra {
    let filters: Vec<_> = filters
        .iter()
        .map(|(r, f)| (r, sub_filter_response(f)))
        .collect();
    let mut out = Spectra::default();
    for (pos, roles) in &set.positions {
        out.ensure_position(*pos);
        for (role, m) in roles {
            let (w, a) = window(&m.samples, onset_index(&m.samples), length, taper);
            let start_t = m.t0 + a as f64 / SR_F - base;
            let mut h = spectrum(&w);
            for (h, f) in h.iter_mut().zip(design_freqs()) {
                *h *= C::from_polar(1.0, ramp_phase(*f, start_t));
            }
            if let Some((_, filter)) = filters.iter().find(|(r, _)| *r == role) {
                for (h, f) in h.iter_mut().zip(filter) {
                    *h *= f;
                }
            }
            out.set(*pos, role, h.into());
        }
    }
    out
}

pub fn prepare(
    set: &MeasurementSet,
    config: &EngineConfig,
    setup: &Setup,
) -> Result<Prepared, EngineError> {
    validate(set)?;
    let main = set
        .positions
        .iter()
        .find(|(p, _)| *p == config.main_position)
        .ok_or(EngineError::MissingReferencePosition {
            position: config.main_position,
        })?;
    if !main.1.iter().any(|(r, _)| config.mains.contains(r)) {
        return Err(EngineError::MissingMainMeasurement {
            position: config.main_position,
        });
    }
    let arrival_s: Vec<_> = main
        .1
        .iter()
        .map(|(r, m)| (r.clone(), m.t0 + onset_index(&m.samples) as f64 / SR_F))
        .collect();
    let latest = arrival_s
        .iter()
        .filter(|(r, _)| config.mains.contains(r) || config.subs.contains(r))
        .map(|(_, a)| *a)
        .fold(f64::NEG_INFINITY, f64::max);
    let base_s = arrival_s
        .iter()
        .map(|(_, a)| *a)
        .fold(f64::INFINITY, f64::min);
    let align_delay_ms = arrival_s
        .iter()
        .map(|(r, a)| (r.clone(), 1000.0 * (latest - a)))
        .collect();
    let h = build_spectra(set, base_s, &setup.sub_filters, 24000, 0.2);
    // Upstream copies the dictionaries *after* applying fixed sub filters.
    let h0 = h.clone();
    Ok(Prepared {
        arrival_s,
        align_delay_ms,
        base_s,
        h,
        h0,
    })
}

/// Apply the same last multiplication as build_spectra, sharing untouched cells.
/// `raw` must be built without filters on the requested window and time axis.
pub(crate) fn filtered_spectra(raw: &Spectra, filters: &[(String, SubFilter)]) -> Spectra {
    let filters: Vec<_> = filters
        .iter()
        .map(|(r, f)| (r, sub_filter_response(f)))
        .collect();
    let mut out = raw.clone();
    for &position in raw.positions() {
        for role in raw.roles_at(position) {
            if let Some((_, filter)) = filters.iter().find(|(r, _)| r.as_str() == role) {
                let h = raw.get(position, role).expect("existing cell");
                out.set(
                    position,
                    role,
                    h.iter().zip(filter).map(|(h, f)| h * f).collect(),
                );
            }
        }
    }
    out
}

pub fn long_spectra(
    set: &MeasurementSet,
    prepared: &Prepared,
    sub_filters: &[(String, SubFilter)],
    window_ms: f64,
) -> Result<Spectra, EngineError> {
    validate(set)?;
    if !window_ms.is_finite() || window_ms <= 0.0 || !prepared.base_s.is_finite() {
        return Err(EngineError::Input(
            "invalid long window or spectral origin".into(),
        ));
    }
    let length = (window_ms / 1000.0 * SR_F) as usize;
    // Upstream's w[-0:] broadcasting fails for fewer than ten samples.
    if set.positions.iter().flat_map(|(_, rs)| rs).any(|(_, m)| {
        let a = onset_index(&m.samples).saturating_sub(96);
        m.samples.len().saturating_sub(a).min(length) < 10
    }) {
        return Err(EngineError::Input(
            "long window needs at least ten samples".into(),
        ));
    }
    Ok(build_spectra(
        set,
        prepared.base_s,
        sub_filters,
        length,
        0.1,
    ))
}
