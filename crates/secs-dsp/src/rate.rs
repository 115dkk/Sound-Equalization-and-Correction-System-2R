//! Output-rate materialization of filters designed at 48 kHz (ADR 0017).

use crate::{Complex64 as C, DspError, fft, fir, invalid};
use std::f64::consts::PI;

pub const DESIGN_RATE: u32 = 48_000;
pub const OUTPUT_RATES: [u32; 6] = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000];

pub fn is_output_rate(rate: u32) -> bool {
    OUTPUT_RATES.contains(&rate)
}

/// Rounded output taps for a 48 kHz length, at least one for nonempty input.
/// Unrepresentable lengths saturate at `usize::MAX`.
pub fn taps_at(taps_48k: usize, rate: u32) -> usize {
    if taps_48k == 0 {
        return 0;
    }
    let scaled = (taps_48k as u128 * u128::from(rate) + 24_000) / 48_000;
    scaled.clamp(1, usize::MAX as u128) as usize
}

/// A delay in output samples, rounding ties to even, as at the design rate.
pub fn delay_samples(ms: f64, rate: u32) -> i64 {
    (ms * f64::from(rate) / 1000.0).round_ties_even() as i64
}

/// Exact DTFT, with accumulation in tap order (no frequency interpolation).
pub fn fir_response(taps: &[f64], rate: u32, frequencies: &[f64]) -> Vec<C> {
    frequencies
        .iter()
        .map(|&f| {
            let mut value = C::new(0.0, 0.0);
            for (n, &tap) in taps.iter().enumerate() {
                value += C::from_polar(1.0, -2.0 * PI * f * n as f64 / f64::from(rate)) * tap;
            }
            value
        })
        .collect()
}

fn check(taps: &[f64], rate: u32) -> Result<(), DspError> {
    if !is_output_rate(rate) {
        return Err(invalid("unsupported output sample rate"));
    }
    if taps.iter().any(|tap| !tap.is_finite()) {
        return Err(invalid("non-finite FIR coefficient"));
    }
    Ok(())
}

/// DTFT on a uniform frequency grid using the chirp convolution identity.
/// The convolution is linear (zero padded), not circular. All transforms and
/// products run on the calling thread; no parallel floating-point reduction.
fn response_grid(taps: &[f64], step: f64, bins: usize) -> Result<Vec<C>, DspError> {
    let length = taps
        .len()
        .checked_add(bins - 1)
        .and_then(usize::checked_next_power_of_two)
        .ok_or_else(|| invalid("FIR transform length overflow"))?;
    let angle = PI * step / f64::from(DESIGN_RATE);
    let chirp = |n: usize| C::from_polar(1.0, angle * n as f64 * n as f64);
    let mut a = vec![C::default(); length];
    let mut b = vec![C::default(); length];
    for (n, &tap) in taps.iter().enumerate() {
        a[n] = chirp(n).conj() * tap;
    }
    for (n, value) in b[..bins].iter_mut().enumerate() {
        *value = chirp(n);
    }
    for n in 1..taps.len() {
        b[length - n] = chirp(n);
    }
    let mut a = fft::fft(&a);
    for (value, kernel) in a.iter_mut().zip(fft::fft(&b)) {
        *value *= kernel;
    }
    let mut result = fft::ifft(&a);
    result.truncate(bins);
    for (n, value) in result.iter_mut().enumerate() {
        *value *= chirp(n).conj();
    }
    Ok(result)
}

/// Time of the first largest absolute coefficient, refined by a parabola
/// through its absolute-valued neighbours. Boundary and flat peaks stay at
/// the sample time. Normalization avoids overflow for large finite taps.
fn peak_time(taps: &[f64]) -> f64 {
    let mut peak = 0;
    for i in 1..taps.len() {
        if taps[i].abs() > taps[peak].abs() {
            peak = i;
        }
    }
    let mut offset = 0.0;
    let center = taps[peak].abs();
    if peak > 0 && peak + 1 < taps.len() && center > 0.0 {
        let left = taps[peak - 1].abs() / center;
        let right = taps[peak + 1].abs() / center;
        let curvature = left - 2.0 + right;
        if curvature != 0.0 {
            offset = (0.5 * (left - right) / curvature).clamp(-0.5, 0.5);
        }
    }
    (peak as f64 + offset) / f64::from(DESIGN_RATE)
}

/// Rebuild a 48 kHz FIR on the output FFT grid. Keep its exact response
/// below 20 kHz. From 20 kHz to min(23 kHz, output Nyquist), crossfade in
/// the complex domain with a raised cosine to a pure delay at the refined
/// absolute impulse peak, with the magnitude at 20 kHz and no phase offset.
/// Above the crossfade use that delay. Keep the original duration without
/// a second taper: the design already carries `rc::truncate`'s half-Hann.
///
/// The inverse period is at least 64 times the retained duration and at
/// least the engine's 2^17/48000 seconds, rounded up to a power of two.
/// The 48 kHz branch copies without transforming or tapering.
pub fn fir_to_rate(taps_48k: &[f64], rate: u32) -> Result<Vec<f64>, DspError> {
    check(taps_48k, rate)?;
    if rate == DESIGN_RATE || taps_48k.is_empty() {
        return Ok(taps_48k.to_vec());
    }
    let length = taps_at(taps_48k.len(), rate);
    let nfft = length
        .checked_mul(64)
        .map(|n| n.max(taps_at(1 << 17, rate)))
        .and_then(usize::checked_next_power_of_two)
        .ok_or_else(|| invalid("FIR transform length overflow"))?;
    rebuild(taps_48k, rate, length, nfft)
}

fn rebuild(taps_48k: &[f64], rate: u32, length: usize, nfft: usize) -> Result<Vec<f64>, DspError> {
    let step = f64::from(rate) / nfft as f64;
    let end = 23_000.0_f64.min(f64::from(rate) / 2.0);
    let bins = (end / step).floor() as usize + 1;
    let mut spectrum = response_grid(taps_48k, step, bins)?;
    let magnitude = fir_response(taps_48k, DESIGN_RATE, &[20_000.0])[0].norm();
    let time = peak_time(taps_48k);
    let extension = |f: f64| C::from_polar(magnitude, -2.0 * PI * f * time);
    for (k, value) in spectrum.iter_mut().enumerate() {
        let f = k as f64 * step;
        if f > 20_000.0 {
            let weight = 0.5 * (1.0 - (PI * (f - 20_000.0) / (end - 20_000.0)).cos());
            *value = *value * (1.0 - weight) + extension(f) * weight;
        }
    }
    spectrum.extend((bins..=nfft / 2).map(|k| extension(k as f64 * step)));
    let mut result = fft::irfft(&spectrum, nfft);
    result.truncate(length);
    if result.iter().any(|value| !value.is_finite()) {
        return Err(invalid("non-finite output FIR coefficient"));
    }
    Ok(result)
}

/// Expand the edge-tapered low-rate recipe directly, retaining its original
/// duration. At 48 kHz this calls the existing materializer unchanged.
/// The recipe's existing 48 kHz size bound applies at every output rate.
pub fn lowrate_to_rate(taps_low: &[f64], fs_low: f64, rate: u32) -> Result<Vec<f64>, DspError> {
    check(taps_low, rate)?;
    if fs_low > f64::from(DESIGN_RATE) || fir::lowrate_len_48k(taps_low.len(), fs_low).is_none() {
        return Err(invalid("invalid or oversized low-rate FIR recipe"));
    }
    if rate == DESIGN_RATE {
        return Ok(fir::lowrate_to_48k(taps_low, fs_low));
    }
    let length = (taps_low.len() as f64 * (f64::from(rate) / fs_low)).ceil() as usize;
    let nfft = length
        .checked_mul(2)
        .and_then(usize::checked_next_power_of_two)
        .ok_or_else(|| invalid("low-rate FIR transform length overflow"))?;
    let frequencies: Vec<_> = (0..=nfft / 2)
        .map(|k| k as f64 * (f64::from(rate) / nfft as f64))
        .collect();
    let mut result = fft::irfft(&fir::lowrate_response(taps_low, fs_low, &frequencies), nfft);
    result.truncate(length);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_rate_peak_time_uses_absolute_parabolic_peak() {
        for (taps, expected) in [
            (vec![1.0, 0.5, 0.0], 0.0),
            (vec![0.0, 0.5, 1.0], 2.0),
            (vec![0.0, -1.0, 0.5], 1.0 + 1.0 / 6.0),
            (vec![0.5, -1.0, 0.5], 1.0),
            (vec![0.0; 3], 0.0),
            (vec![0.0, 1e300, -5e299], 1.0 + 1.0 / 6.0),
        ] {
            assert!((peak_time(&taps) * 48000.0 - expected).abs() < 1e-14);
        }
    }

    #[test]
    fn output_rate_inverse_period_converges_without_time_shift() {
        // A noninteger output delay exercises the Nyquist discontinuity and
        // its slowly decaying inverse, rather than an integer impulse only.
        for length in [2048, 16384] {
            let mut taps = vec![0.0; length];
            taps[length / 4] = 1.0;
            for rate in OUTPUT_RATES.into_iter().filter(|&rate| rate != DESIGN_RATE) {
                let count = taps_at(length, rate);
                let nfft = (64 * count).max(taps_at(1 << 17, rate)).next_power_of_two();
                let normal = rebuild(&taps, rate, count, nfft).unwrap();
                let longer = rebuild(&taps, rate, count, 2 * nfft).unwrap();
                // Sum of absolute tap differences bounds the complex DTFT
                // error at every frequency, not just selected test bins.
                let bound: f64 = normal.iter().zip(&longer).map(|(a, b)| (a - b).abs()).sum();
                eprintln!(
                    "RATE PERIOD taps={length} rate={rate} FFT={nfft} vs {}: DTFT absolute bound={bound:e}",
                    2 * nfft
                );
                assert!(bound < 1e-4, "inverse period has not converged: {bound}");
            }
        }
    }

    #[test]
    fn chirp_grid_matches_direct_dtft() {
        let taps: Vec<_> = (0..127)
            .map(|i| ((i * 17 % 31) as f64 - 15.0) / 31.0)
            .collect();
        for step in [0.18310546875, 0.336456298828125, 13.7] {
            let f: Vec<_> = (0..1003).map(|k| k as f64 * step).collect();
            let got = response_grid(&taps, step, f.len()).unwrap();
            for (got, expected) in got.iter().zip(fir_response(&taps, DESIGN_RATE, &f)) {
                assert!((*got - expected).norm() < 1e-10);
            }
        }
    }
}
