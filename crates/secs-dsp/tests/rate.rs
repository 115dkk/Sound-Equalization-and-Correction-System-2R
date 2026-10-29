#![forbid(unsafe_code)]

use secs_dsp::{Complex64 as C, fft, fir, minphase, rate::*};
use std::f64::consts::PI;

fn bits(values: &[f64]) -> Vec<u64> {
    values.iter().map(|v| v.to_bits()).collect()
}

// Engine recipe: cepstral minimum phase, frequency-dependent excess phase,
// inverse on the design grid, and the same final-10% half-Hann truncation.
fn designed(length: usize, mode: &str) -> Vec<f64> {
    let n = 1 << 17;
    let f: Vec<_> = (0..=n / 2)
        .map(|i| i as f64 * 48_000.0 / n as f64)
        .collect();
    let magnitude: Vec<_> = f
        .iter()
        .map(|&f| {
            let db = -5.0 * (-0.5 * ((f - 85.0) / 35.0).powi(2)).exp()
                + 2.0 * (-0.5 * ((f - 250.0) / 80.0).powi(2)).exp();
            10_f64.powf(db / 20.0)
        })
        .collect();
    let mut h = minphase::minimum_phase(&magnitude).unwrap();
    for (i, value) in h.iter_mut().enumerate() {
        *value = match mode {
            "linear" => C::from_polar(
                magnitude[i],
                -2.0 * PI * f[i] * (length / 2) as f64 / 48_000.0,
            ),
            "minimum" => *value,
            "mixed" => {
                *value
                    * C::from_polar(
                        1.0,
                        -2.0 * PI * f[i] * (length / 4) as f64 / 48_000.0
                            + 0.7 * (f[i] / 90.0) * (-(f[i] / 300.0).powi(2)).exp(),
                    )
            }
            _ => unreachable!(),
        };
    }
    let mut taps = fft::irfft(&h, n);
    taps.truncate(length);
    let tail = (length as f64 * 0.1) as usize;
    for (i, value) in taps[length - tail..].iter_mut().enumerate() {
        *value *= 0.5 * (1.0 + (PI * i as f64 / tail as f64).cos());
    }
    taps
}

#[derive(Default, Debug)]
struct Deviation {
    db: f64,
    db_hz: f64,
    degrees: f64,
    phase_hz: f64,
    out_ratio: f64,
    out_hz: f64,
}

impl Deviation {
    fn observe(&mut self, got: C, reference: C, f: f64, peak: f64) {
        assert!(peak.is_finite() && peak > 0.0);
        if reference.norm() < peak * 10_f64.powf(-30.0 / 20.0) {
            let error = (got - reference).norm() / peak;
            assert!(error.is_finite());
            if error > self.out_ratio {
                self.out_ratio = error;
                self.out_hz = f;
            }
            return;
        }
        let ratio = got / reference;
        let db = (20.0 * ratio.norm().log10()).abs();
        let phase = ratio.arg().to_degrees().abs();
        assert!(
            db.is_finite() && phase.is_finite(),
            "undefined response at {f} Hz"
        );
        if db > self.db {
            self.db = db;
            self.db_hz = f;
        }
        if phase > self.degrees {
            self.degrees = phase;
            self.phase_hz = f;
        }
    }
    fn passes(&self) -> bool {
        self.db <= 0.1 && self.degrees <= 1.0 && self.out_ratio <= 10_f64.powf(-50.0 / 20.0)
    }
}

// Integer-Hz bins are exact DTFT samples, computed by zero-padded FFTs.
// One second resolves the full 16384-tap design and its output versions.
fn spectrum(taps: &[f64], rate: u32) -> Vec<C> {
    let mut padded = taps.to_vec();
    assert!(padded.len() <= rate as usize);
    padded.resize(rate as usize, 0.0);
    fft::rfft(&padded)
}

#[test]
fn output_rate_fir_designs_meet_unshifted_tolerance() {
    let mut worst: [Deviation; 6] = std::array::from_fn(|_| Deviation::default());
    for length in [2048, 16384] {
        for mode in ["linear", "minimum", "mixed"] {
            let taps = designed(length, mode);
            let reference = spectrum(&taps, DESIGN_RATE);
            let peak = reference[20..=20000]
                .iter()
                .map(|h| h.norm())
                .fold(0.0_f64, f64::max);
            for (i, rate) in OUTPUT_RATES.into_iter().enumerate() {
                let converted = fir_to_rate(&taps, rate).unwrap();
                assert_eq!(converted.len(), taps_at(length, rate));
                assert_eq!(bits(&converted), bits(&fir_to_rate(&taps, rate).unwrap()));
                if rate == DESIGN_RATE {
                    assert_eq!(bits(&converted), bits(&taps));
                }
                let h = spectrum(&converted, rate);
                let mut local = Deviation::default();
                for f in 20..=20_000 {
                    local.observe(h[f], reference[f], f as f64, peak);
                    worst[i].observe(h[f], reference[f], f as f64, peak);
                }
                eprintln!("FIR {mode} {length} {rate}: {local:?}");
            }
        }
    }
    for (rate, error) in OUTPUT_RATES.into_iter().zip(&worst) {
        eprintln!("FIR worst {rate}: {error:?}");
    }
    assert!(
        worst.iter().all(Deviation::passes),
        "ADR 0017 FIR tolerance not met"
    );
}

#[test]
fn output_rate_mimo_recipe_meets_unshifted_tolerance() {
    let solved: Vec<_> = (0..70)
        .map(|i| (-i as f64 / 18.0).exp() * (2.0 * PI * 67.0 * i as f64 / 600.0).cos())
        .collect();
    let lp = fir::firwin(31, 120.0, 600.0).unwrap();
    let mut recipe = vec![0.0; 100];
    for (i, a) in solved.iter().enumerate() {
        for (j, b) in lp.iter().enumerate() {
            recipe[i + j] += a * b;
        }
    }
    let baseline = fir::lowrate_to_48k(&recipe, 600.0);
    let reference = spectrum(&baseline, DESIGN_RATE);
    let peak = reference[20..=20000]
        .iter()
        .map(|h| h.norm())
        .fold(0.0_f64, f64::max);
    let mut passed = true;
    for rate in OUTPUT_RATES {
        let taps = lowrate_to_rate(&recipe, 600.0, rate).unwrap();
        if rate == DESIGN_RATE {
            assert_eq!(bits(&taps), bits(&baseline));
        }
        assert_eq!(
            bits(&taps),
            bits(&lowrate_to_rate(&recipe, 600.0, rate).unwrap())
        );
        let h = spectrum(&taps, rate);
        let mut all = Deviation::default();
        for f in 20..=20_000 {
            all.observe(h[f], reference[f], f as f64, peak);
        }
        eprintln!(
            "MIMO {rate}: {all:?}; out-of-band error relative to peak={} dB at {} Hz",
            20.0 * all.out_ratio.log10(),
            all.out_hz
        );
        passed &= all.passes();
    }
    assert!(
        passed,
        "ADR 0017 low-rate tolerance not met over 20 Hz–20 kHz"
    );
}

#[test]
fn output_rate_lengths_delays_validation_and_identity() {
    for (rate, short, long) in [
        (44100, 1882, 15053),
        (48000, 2048, 16384),
        (88200, 3763, 30106),
        (96000, 4096, 32768),
        (176400, 7526, 60211),
        (192000, 8192, 65536),
    ] {
        assert!(is_output_rate(rate));
        assert_eq!(taps_at(2048, rate), short);
        assert_eq!(taps_at(16384, rate), long);
        assert_eq!(taps_at(0, rate), 0);
        assert_eq!(
            taps_at(1, rate),
            (f64::from(rate) / 48000.0).round().max(1.0) as usize
        );
        for (samples, expected) in [(0.5, 0), (1.5, 2), (2.5, 2), (-0.5, 0), (-1.5, -2)] {
            assert_eq!(
                delay_samples(samples * 1000.0 / f64::from(rate), rate),
                expected
            );
        }
    }
    for rate in [0, 8000, 48001, 384000] {
        assert!(!is_output_rate(rate));
        assert!(fir_to_rate(&[1.0], rate).is_err());
        assert!(lowrate_to_rate(&[1.0], 600.0, rate).is_err());
    }
    assert_eq!(taps_at(usize::MAX, 192000), usize::MAX);
    let taps = [1.0, -0.0, 0.3, -0.2];
    assert_eq!(bits(&fir_to_rate(&taps, 48000).unwrap()), bits(&taps));
    assert!(fir_to_rate(&[], 96000).unwrap().is_empty());
    assert!(fir_to_rate(&[f64::NAN], 48000).is_err());
    for fs in [
        0.0,
        -1.0,
        f64::NAN,
        f64::INFINITY,
        96000.0,
        f64::MIN_POSITIVE,
    ] {
        assert!(lowrate_to_rate(&taps, fs, 96000).is_err());
    }
    assert!(lowrate_to_rate(&[], 600.0, 96000).is_err());
    for rate in OUTPUT_RATES {
        let got = fir_response(&[0.0, 1.0], rate, &[20.0, 20000.0]);
        for (value, f) in got.into_iter().zip([20.0, 20000.0]) {
            assert!((value - C::from_polar(1.0, -2.0 * PI * f / f64::from(rate))).norm() < 1e-14);
        }
    }
}

#[test]
fn output_rate_conversion_is_identical_on_fresh_threads() {
    let taps = designed(2048, "mixed");
    for rate in OUTPUT_RATES {
        let expected = bits(&fir_to_rate(&taps, rate).unwrap());
        std::thread::scope(|scope| {
            let workers: Vec<_> = (0..2)
                .map(|_| scope.spawn(|| bits(&fir_to_rate(&taps, rate).unwrap())))
                .collect();
            for worker in workers {
                assert_eq!(expected, worker.join().unwrap());
            }
        });
    }
}

#[test]
fn output_rate_delay_quantization_stays_within_half_sample() {
    for rate in OUTPUT_RATES {
        for ms in [
            0.0,
            0.001,
            0.010416666666666666,
            0.03125,
            0.346666666666667,
            1.0,
            7.125,
            30.0,
            200.0,
            -0.346666666666667,
        ] {
            let seconds = delay_samples(ms, rate) as f64 / f64::from(rate);
            assert!(
                (seconds - ms / 1000.0).abs() <= 0.5 / f64::from(rate),
                "ms={ms}, rate={rate}"
            );
        }
    }
}
