// Adapted from Impulcifer-pip313 crates/impulcifer-dsp/src/estimator.rs, cac2217.
// MIT; original copyright and license in ../THIRD_PARTY_LICENSES.txt.
//! Fixed-sample exponential sweeps and Farina inverse deconvolution.
use std::cell::{Ref, RefCell};
use std::f64::consts::PI;

use secs_dsp::fft;

use crate::{AudioError, invalid};

pub const SWEEP_LENGTHS: [&str; 7] = ["64k", "128k", "256k", "512k", "1M", "2M", "4M"];

pub fn sweep_length_samples(length: &str) -> Result<usize, AudioError> {
    SWEEP_LENGTHS
        .iter()
        .position(|&s| s == length)
        .map(|i| 65_536 << i)
        .ok_or_else(|| invalid("unknown sweep length"))
}

#[derive(Clone, Debug)]
struct SpectrumCache {
    n: usize,
    signal: Vec<fft::Complex64>,
    inverse: Vec<fft::Complex64>,
}

#[derive(Clone, Debug)]
pub struct SweepEstimator {
    pub fs: u32,
    pub low: f64,
    pub high: f64,
    pub test_signal: Vec<f64>,
    pub inverse_filter: Vec<f64>,
    spectrum_cache: RefCell<Option<SpectrumCache>>,
}

impl SweepEstimator {
    /// Fixed sample count, peak dBFS, half-Hann fades in seconds. Unlike the
    /// Impulcifer octave-aligned constructor, the requested length is exact.
    pub fn new(
        fs: u32,
        low: f64,
        high: f64,
        length: usize,
        level_dbfs: f64,
        fade_seconds: f64,
    ) -> Result<Self, AudioError> {
        if fs == 0
            || !low.is_finite()
            || !high.is_finite()
            || low <= 0.0
            || high <= low
            || high >= f64::from(fs) / 2.0
            || !(16..=4_194_304).contains(&length)
            || !level_dbfs.is_finite()
            || !(-120.0..=0.0).contains(&level_dbfs)
            || !fade_seconds.is_finite()
            || fade_seconds < 0.0
            || fade_seconds * f64::from(fs) * 2.0 >= length as f64
        {
            return Err(invalid("invalid sweep frequency, length, level or fades"));
        }
        let logarithm = (high / low).ln();
        let scale = 2.0 * PI * low * length as f64 / f64::from(fs) / logarithm;
        let gain = 10_f64.powf(level_dbfs / 20.0);
        let fade = (fade_seconds * f64::from(fs)).round() as usize;
        let test_signal: Vec<f64> = (0..length)
            .map(|i| {
                let mut x = (scale * (i as f64 / length as f64 * logarithm).exp_m1()).sin() * gain;
                if fade > 0 {
                    let distance = i.min(length - 1 - i);
                    if distance < fade {
                        x *= 0.5 * (1.0 - (PI * distance as f64 / fade as f64).cos());
                    }
                }
                // The inverse describes the actual f32 transport, not an unplayed f64 sweep.
                f64::from(x as f32)
            })
            .collect();
        let n_octaves = logarithm / 2_f64.ln();
        let base = 2_f64.powf(n_octaves / length as f64);
        let mut inverse_filter: Vec<f64> = test_signal
            .iter()
            .rev()
            .enumerate()
            .map(|(i, x)| {
                x * base.powf(-(i as f64)) * n_octaves * 2_f64.ln() / (1.0 - 2_f64.powf(-n_octaves))
            })
            .collect();
        // Unit peak at zero delay, also valid for sweeps not reaching fs/2.
        let energy: f64 = inverse_filter
            .iter()
            .rev()
            .zip(&test_signal)
            .map(|(a, b)| a * b)
            .sum();
        if !energy.is_finite() || energy <= 0.0 {
            return Err(invalid("empty sweep energy"));
        }
        inverse_filter.iter_mut().for_each(|x| *x /= energy);
        Ok(Self {
            fs,
            low,
            high,
            test_signal,
            inverse_filter,
            spectrum_cache: RefCell::new(None),
        })
    }

    fn spectra(&self, n: usize) -> Ref<'_, SpectrumCache> {
        let needs_refresh = self
            .spectrum_cache
            .borrow()
            .as_ref()
            .is_none_or(|cached| cached.n != n);
        if needs_refresh {
            let signal = spectrum(&self.test_signal, n);
            let inverse = spectrum(&self.inverse_filter, n);
            self.spectrum_cache
                .replace(Some(SpectrumCache { n, signal, inverse }));
        }
        Ref::map(self.spectrum_cache.borrow(), |cached| {
            cached.as_ref().expect("spectrum cache was just filled")
        })
    }

    /// Linear Farina convolution with zero-delay at `test_signal.len()-1`.
    pub fn estimate_raw(&self, recording: &[f64]) -> Vec<f64> {
        if recording.is_empty() || self.inverse_filter.is_empty() {
            return Vec::new();
        }
        let length = recording.len() + self.inverse_filter.len() - 1;
        let n = length.next_power_of_two();
        let cached = self.spectra(n);
        let mut raw_spectrum = spectrum(recording, n);
        for (x, y) in raw_spectrum.iter_mut().zip(&cached.inverse) {
            *x *= y;
        }
        let mut raw = fft::irfft(&raw_spectrum, n);
        raw.truncate(length);
        raw
    }

    /// Remove the known sweep/inverse colour after Farina deconvolution.
    /// This retains the inverse's phase origin but avoids treating its band-edge
    /// rolloff as the room. A -180 dB power regularizer bounds unexcited bins.
    /// Returned sample zero is recording sample zero (not the inverse's delay).
    pub fn estimate(&self, recording: &[f64]) -> Result<Vec<f64>, AudioError> {
        self.estimate_with_noise(recording, 0.0)
    }

    /// Wiener floor from independently measured input-noise variance. Without
    /// it, unexcited out-of-band bins amplify even quiet ADC noise enormously.
    pub fn estimate_with_noise(
        &self,
        recording: &[f64],
        noise_power: f64,
    ) -> Result<Vec<f64>, AudioError> {
        self.estimate_with_noise_and_raw(recording, noise_power, false)
            .map(|(ir, _)| ir)
    }

    pub(crate) fn estimate_with_noise_and_raw(
        &self,
        recording: &[f64],
        noise_power: f64,
        include_raw: bool,
    ) -> Result<(Vec<f64>, Option<Vec<f64>>), AudioError> {
        if !noise_power.is_finite() || noise_power < 0.0 {
            return Err(invalid("invalid noise power"));
        }
        if recording.is_empty() || !recording.iter().all(|x| x.is_finite()) {
            return Err(invalid("empty or nonfinite recording"));
        }
        let raw_length = recording.len() + self.test_signal.len() - 1;
        let n = raw_length.next_power_of_two();
        let cached = self.spectra(n);
        let recorded = spectrum(recording, n);
        let floor = (cached
            .signal
            .iter()
            .map(|x| x.norm_sqr())
            .fold(0.0_f64, f64::max)
            * 1e-18)
            .max(noise_power * recording.len() as f64);
        let response: Vec<_> = recorded
            .iter()
            .zip(&cached.signal)
            .zip(&cached.inverse)
            .enumerate()
            .map(|(bin, ((y, x), i))| {
                let frequency = bin as f64 * f64::from(self.fs) / n as f64;
                // Smoothly discard unexcited ultrasonic bins. Keep the entire
                // requested band, with a 10% cosine transition outside it.
                let taper = if frequency <= self.high {
                    1.0
                } else {
                    let t = ((frequency - self.high) / (self.high * 0.1)).min(1.0);
                    0.5 * (1.0 + (PI * t).cos())
                };
                let reference = x * i;
                let denominator = reference.norm_sqr() + floor * i.norm_sqr();
                if denominator == 0.0 {
                    fft::Complex64::default()
                } else {
                    (y * i) * reference.conj() / denominator * taper
                }
            })
            .collect();
        let raw = include_raw.then(|| {
            let mut raw_spectrum = recorded;
            for (x, y) in raw_spectrum.iter_mut().zip(&cached.inverse) {
                *x *= y;
            }
            let mut raw = fft::irfft(&raw_spectrum, n);
            raw.truncate(raw_length);
            raw
        });
        let mut ir = fft::irfft(&response, n);
        ir.truncate(recording.len());
        Ok((ir, raw))
    }
}

fn spectrum(samples: &[f64], n: usize) -> Vec<fft::Complex64> {
    let mut padded = vec![0.0; n];
    padded[..samples.len()].copy_from_slice(samples);
    fft::rfft(&padded)
}

pub(crate) fn convolve(a: &[f64], b: &[f64]) -> Vec<f64> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let length = a.len() + b.len() - 1;
    let n = length.next_power_of_two();
    let mut aa = vec![0.0; n];
    let mut bb = vec![0.0; n];
    aa[..a.len()].copy_from_slice(a);
    bb[..b.len()].copy_from_slice(b);
    let mut spectrum = fft::rfft(&aa);
    for (x, y) in spectrum.iter_mut().zip(fft::rfft(&bb)) {
        *x *= y;
    }
    let mut out = fft::irfft(&spectrum, n);
    out.truncate(length);
    out
}
