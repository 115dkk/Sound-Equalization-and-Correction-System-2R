//! Adapted from Impulcifer-py313 `crates/impulcifer-dsp/src/stages/room_tuning2/primitives.rs`
//! (MIT, Copyright (c) 2024- 115dkk), corrected against SECS 2/SciPy.
//! See docs/THIRD_PARTY.md and ../THIRD_PARTY_LICENSES.txt.
//! SciPy firwin subset used by mimo.lowpass_fir: Hamming low-pass, DC scale.
use crate::{Complex64 as C, DspError, fft, invalid, numeric::sum};
use std::f64::consts::PI;

/// Persisted low-rate FIR recipes are tied to this interpolation and crop rule.
pub const LOWRATE_MATERIALIZATION_VERSION: u32 = 1;

/// Bound expansion of persisted low-rate recipes (about 21.8 s at 48 kHz).
/// Existing full-rate coefficient arrays are not subject to this recipe bound.
pub const MAX_LOWRATE_MATERIALIZED_TAPS: usize = 1 << 20;

/// Compatible output length, or `None` for an invalid or oversized recipe.
pub fn lowrate_len_48k(taps_low: usize, fs_low: f64) -> Option<usize> {
    if taps_low == 0 || !fs_low.is_finite() || fs_low <= 0.0 {
        return None;
    }
    let length = (taps_low as f64 * (48000.0 / fs_low)).ceil();
    (length.is_finite() && (1.0..=MAX_LOWRATE_MATERIALIZED_TAPS as f64).contains(&length))
        .then_some(length as usize)
}

pub fn firwin(taps: usize, cutoff: f64, fs: f64) -> Result<Vec<f64>, DspError> {
    let fc = cutoff / (0.5 * fs);
    if fc <= 0. || fc >= 1. {
        return Err(invalid("cutoff must lie between zero and Nyquist"));
    }
    if taps == 0 {
        return Err(invalid("firwin needs positive taps"));
    }
    let mut h: Vec<_> = (0..taps)
        .map(|i| {
            let m = i as f64 - 0.5 * (taps as f64 - 1.);
            let x = fc * m;
            let y = PI * if x == 0. { 1e-20 } else { x };
            let sinc = y.sin() / y;
            // scipy.windows.general_cosine uses linspace(-pi, pi, taps).
            let phase = if i == taps - 1 {
                PI
            } else {
                -PI + i as f64 * (2. * PI / (taps - 1) as f64)
            };
            let window = if taps == 1 {
                1.
            } else {
                0.54 + (1.0 - 0.54) * phase.cos()
            };
            (fc * sinc - 0.) * window
        })
        .collect();
    let scale = sum(&h);
    for v in &mut h {
        *v /= scale;
    }
    Ok(h)
}

/// Low-rate FIR response with the upstream linear 10% Nyquist edge ramp.
///
/// A 48 kHz real-FFT grid at an integer rate ratio visits a subset of a much
/// smaller FFT's bins: for `R = 48000 / fs_low`, `d = gcd(N, R)`, bin `k`
/// is bin `k * R / d` of a transform of length `N / d`. Other grids and
/// sample rates retain the direct DTFT, including its order of accumulation.
pub fn lowrate_response(taps: &[f64], fs_low: f64, frequencies: &[f64]) -> Vec<C> {
    if let Some((length, stride)) = lowrate_fft_grid(taps, fs_low, frequencies) {
        let mut padded = vec![0.0; length];
        padded[..taps.len()].copy_from_slice(taps);
        let spectrum = fft::rfft_owned(padded);
        let edge = fs_low / 2.0;
        return frequencies
            .iter()
            .enumerate()
            .map(|(k, &f)| {
                if f >= edge {
                    C::new(0.0, 0.0)
                } else {
                    spectrum[k * stride] * ((edge - f) / (0.1 * edge)).clamp(0.0, 1.0)
                }
            })
            .collect();
    }
    lowrate_response_direct(taps, fs_low, frequencies)
}

fn lowrate_fft_grid(taps: &[f64], fs_low: f64, frequencies: &[f64]) -> Option<(usize, usize)> {
    let n = frequencies.len().checked_sub(1)?.checked_mul(2)?;
    let ratio = 48000.0 / fs_low;
    if !n.is_power_of_two()
        || !ratio.is_finite()
        || !(1.0..=u32::MAX as f64).contains(&ratio)
        || ratio.fract() != 0.0
        || taps.iter().any(|v| !v.is_finite())
    {
        return None;
    }
    let step = 48000.0 / n as f64;
    if !frequencies
        .iter()
        .enumerate()
        .all(|(k, &f)| f == k as f64 * step)
    {
        return None;
    }
    let ratio = ratio as usize;
    let (mut a, mut b) = (n, ratio);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    let length = n / a;
    // Longer taps would need circular folding. Keep that unusual case on the
    // direct path rather than introduce another changed summation order.
    (taps.len() <= length).then_some((length, ratio / a))
}

fn lowrate_response_direct(taps: &[f64], fs_low: f64, frequencies: &[f64]) -> Vec<C> {
    let edge = fs_low / 2.0;
    frequencies
        .iter()
        .map(|&f| {
            if f >= edge {
                return C::new(0.0, 0.0);
            }
            let mut sum = C::new(0.0, 0.0);
            for (k, &tap) in taps.iter().enumerate() {
                let phase = (-2.0 * PI * (f * k as f64)) / fs_low;
                sum += C::from_polar(1.0, phase) * tap;
            }
            sum * ((edge - f) / (0.1 * edge)).clamp(0.0, 1.0)
        })
        .collect()
}

/// Interpolate to 48 kHz using the edge-tapered spectrum, then retain the
/// original low-rate duration. Cropping is part of the exported FIR contract.
/// Invalid or oversized recipes (see [`lowrate_len_48k`]) return an empty FIR.
pub fn lowrate_to_48k(taps_low: &[f64], fs_low: f64) -> Vec<f64> {
    let Some(n48) = lowrate_len_48k(taps_low.len(), fs_low) else {
        return Vec::new();
    };
    let Some(nfft) = n48
        .checked_mul(2)
        .and_then(usize::checked_next_power_of_two)
    else {
        return Vec::new();
    };
    let frequencies: Vec<_> = (0..=nfft / 2)
        .map(|k| k as f64 * (48000.0 / nfft as f64))
        .collect();
    let mut taps = fft::irfft(&lowrate_response(taps_low, fs_low, &frequencies), nfft);
    taps.truncate(n48);
    taps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taps(n: usize) -> Vec<f64> {
        (0..n)
            .map(|k| ((k * 37 % 113) as f64 - 56.0) / (k + 1) as f64)
            .collect()
    }

    #[test]
    fn lowrate_small_fft_matches_dtft_and_edge_ramp() {
        for (fs, n, length, stride) in [
            (600.0, 16384, 1024, 5),
            (600.0, 32768, 2048, 5),
            (600.0, 131072, 8192, 5),
            (1000.0, 4096, 256, 3),
        ] {
            let taps = taps(100);
            let frequencies: Vec<_> = (0..=n / 2)
                .map(|k| k as f64 * (48000.0 / n as f64))
                .collect();
            assert_eq!(
                lowrate_fft_grid(&taps, fs, &frequencies),
                Some((length, stride))
            );
            let got = lowrate_response(&taps, fs, &frequencies);
            let expected = lowrate_response_direct(&taps, fs, &frequencies);
            for ((a, b), f) in got.iter().zip(&expected).zip(frequencies) {
                assert!((*a - *b).norm() < 2e-11, "fs={fs}, n={n}, f={f}");
                if f >= fs / 2.0 {
                    assert_eq!(*a, C::new(0.0, 0.0));
                }
            }
        }
    }

    #[test]
    fn lowrate_nonstandard_grids_keep_direct_accumulation() {
        let taps = taps(100);
        for (fs, frequencies) in [
            (600.0, vec![-12.0, 0.0, 17.3, 269.9, 270.0, 299.9, 300.0]),
            (
                700.0,
                (0..=8192).map(|k| k as f64 * (48000.0 / 16384.0)).collect(),
            ),
            (600.0, vec![0.0, 12000.0, 24000.0]),
        ] {
            assert_eq!(lowrate_fft_grid(&taps, fs, &frequencies), None);
            assert_eq!(
                lowrate_response(&taps, fs, &frequencies),
                lowrate_response_direct(&taps, fs, &frequencies)
            );
        }
        let mut frequencies: Vec<_> = (0..=8192).map(|k| k as f64 * (48000.0 / 16384.0)).collect();
        frequencies[10] = f64::from_bits(frequencies[10].to_bits() + 1);
        assert_eq!(lowrate_fft_grid(&taps, 600.0, &frequencies), None);
    }

    #[test]
    fn lowrate_materialization_preserves_inverse_and_crop() {
        let taps = taps(100);
        let frequencies: Vec<_> = (0..=8192).map(|k| k as f64 * (48000.0 / 16384.0)).collect();
        let mut expected = fft::irfft(&lowrate_response_direct(&taps, 600.0, &frequencies), 16384);
        expected.truncate(8000);
        let got = lowrate_to_48k(&taps, 600.0);
        assert_eq!(got.len(), 8000);
        assert!(got.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-12));
        for fs in [0.0, -600.0, f64::INFINITY, f64::NAN, f64::MIN_POSITIVE, 1.0] {
            assert!(lowrate_to_48k(&taps, fs).is_empty());
        }
        assert!(lowrate_to_48k(&[], 600.0).is_empty());
        assert_eq!(lowrate_len_48k(100, 600.0), Some(8000));
        assert_eq!(lowrate_len_48k(usize::MAX, 600.0), None);
    }
}
