use std::{f64::consts::PI, sync::Arc};

use secs_compute::Context;
use secs_types::{
    calibration::{CalibrationSettings, Mode, RcHiBasis},
    target::TargetCurve,
};

use crate::{
    C, DesignRequest, EngineConfig, EngineError, Measurement, MeasurementSet, Samples, Tuning,
    calibration::{ProjectView, engine_inputs},
    grid::{BINS, SR, SR_F, design_freqs},
    levels,
    rc::{self, RcInput},
    setup::Setup,
};

fn config(set: &MeasurementSet) -> EngineConfig {
    engine_inputs(
        &ProjectView {
            groups: &[("front".into(), vec!["L".into(), "R".into()])],
            project_xo: &[],
            target: &TargetCurve::new(vec![(10.0, 0.0), (20000.0, 0.0)]),
        },
        set,
        &CalibrationSettings {
            mode: Mode::Zero,
            room_volume_m3: Some(60.0),
            ..Default::default()
        },
    )
    .unwrap()
    .config
}

fn decay_ir(low_t60: f64, high_t60: f64) -> Measurement {
    let centres = levels::RC_BAND_CENTRES;
    let samples = (0..96_000)
        .map(|i| {
            let t = i as f64 / SR_F;
            centres
                .iter()
                .enumerate()
                .map(|(j, &f)| {
                    let t60 = if f <= 315.0 { low_t60 } else { high_t60 };
                    (-3.0 * 10.0_f64.ln() * t / t60).exp()
                        * (2.0 * PI * f * t + j as f64 * 0.7).sin()
                })
                .sum::<f64>()
        })
        .collect::<Vec<_>>();
    Measurement {
        samples: Samples::F64(samples.into()),
        sample_rate: SR,
        t0: 0.0,
    }
}

fn room(low: f64, high: f64, count: u32) -> MeasurementSet {
    let m = decay_ir(low, high);
    MeasurementSet {
        positions: (1..=count)
            .map(|p| (p, vec![("L".into(), m.clone()), ("R".into(), m.clone())]))
            .collect(),
    }
}

#[test]
fn rc_lenient_synthetic_decay_bands_follow_room_treatment() {
    for (low, high, expected_band, increases) in [
        (0.64, 0.64, 200.0, false),
        (0.8, 0.12, 200.0, true),
        (0.3, 0.6, 125.0, false),
    ] {
        let set = room(low, high, 1);
        let config = config(&set);
        let strict = levels::schroeder_hz(&set, &config).unwrap();
        let (band, t60, local) = levels::lenient_schroeder_hz(&set, &config).unwrap();
        assert_eq!(
            band, expected_band,
            "low={low} high={high} T60={t60} g={local} S={strict}"
        );
        assert!((t60 - low).abs() < 0.05, "T60={t60} expected {low}");
        assert!(local >= band);
        let lenient = levels::schroeder_limit(Some(strict), Some(band), 1);
        if increases {
            assert!(lenient > levels::schroeder_limit(Some(strict), None, 1));
        } else {
            assert!((lenient - strict).abs() < 5.0);
        }
    }
}

#[test]
fn rc_lenient_uses_every_main_and_position_without_reference_weighting() {
    let mut set = room(0.8, 0.12, 3);
    // Both reference-position mains decay quickly; the four measurements
    // elsewhere still determine the equal-weight median.
    set.positions[0].1[0].1 = decay_ir(0.12, 0.12);
    set.positions[0].1[1].1 = decay_ir(0.12, 0.12);
    let config = config(&set);
    let (band, t60, _) = levels::lenient_schroeder_hz(&set, &config).unwrap();
    assert_eq!(band, 200.0);
    assert!((t60 - 0.8).abs() < 0.05);
    let mut config = config;
    config.room_volume_m3 = None;
    assert_eq!(levels::lenient_schroeder_hz(&set, &config), None);
    config.room_volume_m3 = Some(100_000.0);
    assert_eq!(levels::lenient_schroeder_hz(&set, &config), None);
}

#[test]
fn rc_transition_multiplies_then_clamps_and_requires_volume() {
    for factor in [1, 2, 4] {
        let m = f64::from(factor);
        assert_eq!(
            levels::schroeder_limit(Some(154.0), None, factor),
            154.0 * m
        );
        assert_eq!(
            levels::schroeder_limit(Some(900.0), None, factor),
            500.0 * m
        );
        assert_eq!(levels::schroeder_limit(Some(1.0), None, factor), 100.0);
        assert_eq!(levels::schroeder_limit(None, None, factor), 500.0);
        assert_eq!(
            levels::schroeder_limit(Some(154.0), Some(200.0), factor),
            200.0 * m
        );
        assert_eq!(
            levels::schroeder_limit(None, Some(200.0), factor),
            200.0 * m
        );
        assert_eq!(
            levels::schroeder_limit(Some(200.0), Some(100.0), factor),
            200.0 * m
        );
    }
}

#[test]
fn rc_agreement_stops_at_first_non_improving_band_and_weights_positions() {
    let set = room(0.6, 0.6, 3);
    let config = config(&set);
    let setup = Setup::new(&config);
    let tuning = Tuning::default();
    let positions = [1, 2, 3];
    let response =
        |db: f64| -> Arc<[C]> { vec![C::new(10.0_f64.powf(db / 20.0), 0.0); BINS].into() };
    let comb = vec![(1, response(4.0)), (2, response(-4.0)), (3, response(-4.0))];
    let input = RcInput {
        role: "L",
        comb: &comb,
        main_position: 1,
        level_db: 0.0,
        setup: &setup,
        config: &config,
        tuning: &tuning,
        positions: &positions,
    };
    let width = 2.0_f64.powf(1.0 / 6.0);
    let correction: Vec<_> = design_freqs()
        .iter()
        .map(|&f| {
            // Positive again above 500 Hz must not restart the contiguous run.
            let db = if f <= 200.0 * width || f > 500.0 {
                -2.0
            } else {
                2.0
            };
            C::new(10.0_f64.powf(db / 20.0), 0.0)
        })
        .collect();
    assert_eq!(rc::agreement_limit(&input, &correction), 200.0 * width);
    let other_reference = RcInput {
        main_position: 2,
        ..input
    };
    assert_eq!(rc::agreement_limit(&other_reference, &correction), 100.0);
    let full = rc::agreement_limit(&input, &response(-2.0));
    assert_eq!(full, 2000.0 * width);
    // The 2 kHz clamp belongs after the median, not on each main.
    assert_eq!(
        levels::median(vec![1600.0 * width, full]).clamp(100.0, 2000.0),
        2000.0
    );
    assert_eq!(rc::agreement_limit(&input, &response(2.0)), 100.0);
    assert_eq!(rc::agreement_limit(&input, &response(0.0)), 100.0);
    assert_eq!(rc::agreement_limit(&other_reference, &response(0.0)), 100.0);
}

#[test]
fn rc_agreement_design_runs_two_passes_and_preserves_manual_override() {
    let set = room(0.6, 0.6, 3);
    let mut config = config(&set);
    config.rc_hi_basis = RcHiBasis::Agreement;
    config.room_volume_m3 = None;
    config.rc_hi_transition = 4; // Agreement ignores the multiplier.
    let tuning = Tuning::default();
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let fitters = |stage: crate::FitStage<'_>| -> Box<crate::fit::Fitter<'_>> {
        if matches!(stage, crate::FitStage::Rc(_)) {
            calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        Box::new(|_| vec![(160.0, 1.0, -3.0)])
    };
    let run = |config: &EngineConfig, set: &MeasurementSet| {
        crate::design(
            &DesignRequest {
                measurements: set,
                config,
                tuning: &tuning,
                subs_only: false,
                adapter_notes: &[],
            },
            &Context::serial(),
            &fitters,
        )
    };
    let auto = run(&config, &set).unwrap();
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 4);
    assert!((100.0..=2000.0).contains(&auto.rc_hi));
    assert!(auto.rc_auto);
    assert_eq!(auto.replay.config(), &config);
    assert!(auto.notes.iter().any(|n| matches!(
        n,
        crate::Note::RcLimit {
            basis: RcHiBasis::Agreement,
            positions: Some(3),
            auto: true,
            ..
        }
    )));
    config.rc_range.1 = Some(auto.rc_hi);
    let manual = run(&config, &set).unwrap();
    assert!(!manual.rc_auto);
    for (auto, manual) in auto.mains.iter().zip(manual.mains) {
        assert_eq!(auto.response, manual.response);
    }
    let short = room(0.6, 0.6, 2);
    config.rc_range.1 = None;
    assert!(
        matches!(run(&config, &short), Err(EngineError::Input(message))
        if message.contains("at least 3 measured positions"))
    );
    config.rc_range.1 = Some(900.0);
    assert_eq!(run(&config, &short).unwrap().rc_hi, 900.0);
}

#[test]
fn rc_schroeder_design_applies_basis_factor_and_evidence() {
    let set = room(0.8, 0.12, 1);
    let mut config = config(&set);
    let tuning = Tuning::default();
    let fitters = |_: crate::FitStage<'_>| -> Box<crate::fit::Fitter<'_>> { Box::new(|_| vec![]) };
    let run = |config: &EngineConfig| {
        crate::design(
            &DesignRequest {
                measurements: &set,
                config,
                tuning: &tuning,
                subs_only: false,
                adapter_notes: &[],
            },
            &Context::serial(),
            &fitters,
        )
        .unwrap()
    };
    let strict = levels::schroeder_hz(&set, &config).unwrap();
    for basis in [RcHiBasis::Strict, RcHiBasis::Lenient] {
        config.rc_hi_basis = basis;
        for factor in [1, 2, 4] {
            config.rc_hi_transition = factor;
            let design = run(&config);
            let value = if basis == RcHiBasis::Lenient {
                strict.max(200.0)
            } else {
                strict
            };
            assert_eq!(
                design.rc_hi,
                (value * f64::from(factor)).clamp(100.0, 500.0 * f64::from(factor))
            );
            let note = design
                .notes
                .iter()
                .find(|n| matches!(n, crate::Note::RcLimit { .. }))
                .unwrap();
            let crate::Note::RcLimit {
                basis: actual,
                factor: multiplier,
                band_hz,
                band_t60_s,
                schroeder_hz,
                ..
            } = note
            else {
                unreachable!()
            };
            assert_eq!(
                (*actual, *multiplier, *schroeder_hz),
                (basis, factor, Some(strict))
            );
            if basis == RcHiBasis::Lenient {
                assert_eq!(*band_hz, Some(200.0));
                assert!((band_t60_s.unwrap() - 0.8).abs() < 0.05);
            } else {
                assert_eq!(*band_hz, None);
            }
        }
        config.room_volume_m3 = None;
        assert_eq!(run(&config).rc_hi, 500.0);
        config.room_volume_m3 = Some(60.0);
    }
}
