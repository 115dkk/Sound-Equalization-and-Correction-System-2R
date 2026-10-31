use std::sync::Arc;

use secs_dsp::{fft, response, smooth::Fraction};
use secs_types::{
    calibration::{CalibrationSettings, Mode},
    target::TargetCurve,
};

use crate::{
    C, EngineConfig, Measurement, MeasurementSet, Note, Samples, Tuning,
    calibration::{ProjectView, engine_inputs},
    fit,
    grid::{N_FFT, SR, design_freqs},
    rc::{self, RcInput},
    setup::Setup,
};

pub(super) fn config(h: &[C], mode: Mode) -> EngineConfig {
    let set = MeasurementSet {
        positions: [(
            1,
            vec![(
                "L".into(),
                Measurement {
                    samples: Samples::F64(fft::irfft(h, N_FFT).into()),
                    sample_rate: SR,
                    t0: 0.0,
                },
            )],
        )]
        .into_iter()
        .collect(),
    };
    let mut config = engine_inputs(
        &ProjectView {
            groups: &[("front".into(), vec!["L".into()])],
            project_xo: &[],
            target: &TargetCurve::new(vec![(10.0, 0.0), (20000.0, 0.0)]),
        },
        &set,
        &CalibrationSettings {
            mode,
            rc_hi: Some(1000.0),
            ..Default::default()
        },
    )
    .unwrap()
    .config;
    config.peq_bands = Some(4);
    config
}

pub(super) fn correct(h: Arc<[C]>, config: &EngineConfig, setup: &Setup) -> rc::RcOutput {
    rc::channel_with_cuts(
        &RcInput {
            bus_rolloff_hz: None,
            role: "L",
            comb: &[(1, h)],
            main_position: 1,
            level_db: 0.0,
            setup,
            config,
            tuning: &Tuning::default(),
            positions: &[1],
        },
        &mut fit::fit_peq,
        &mut |_, _, _| vec![],
        &|| false,
    )
    .unwrap()
    .out
}

fn synthetic(hz: f64, q: f64) -> Arc<[C]> {
    response::hp2(design_freqs(), 35.0)
        .iter()
        .zip(response::allpass2(design_freqs(), hz, q))
        .map(|(h, a)| h * a)
        .collect()
}

fn arrival(h: &[C], out: &rc::RcOutput, lo: f64, hi: f64) -> f64 {
    let corrected: Vec<_> = h.iter().zip(&out.response).map(|(h, r)| h * r).collect();
    let band: Vec<_> = (0..101)
        .map(|i| lo + (hi - lo) * i as f64 / 100.0)
        .collect();
    let hf: Vec<_> = (0..101).map(|i| 3000.0 + 30.0 * i as f64).collect();
    let gd = |g: &[f64]| {
        crate::decay::median(
            &response::gd_curve(&corrected, design_freqs(), g, Fraction::Scalar(24.0)).unwrap(),
        )
    };
    gd(&band) - gd(&hf)
}

pub(super) fn assert_bits(a: &rc::RcOutput, b: &rc::RcOutput) {
    let bits = |v: &[f64]| v.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
    assert_eq!(bits(&a.fir), bits(&b.fir));
    assert_eq!(bits(&a.design_db), bits(&b.design_db));
    let complex = |v: &[C]| {
        v.iter()
            .map(|h| (h.re.to_bits(), h.im.to_bits()))
            .collect::<Vec<_>>()
    };
    assert_eq!(complex(&a.response), complex(&b.response));
    let peq = |v: &[(f64, f64, f64)]| {
        v.iter()
            .map(|(f, q, g)| [f.to_bits(), q.to_bits(), g.to_bits()])
            .collect::<Vec<_>>()
    };
    assert_eq!(peq(&a.peq), peq(&b.peq));
    assert_eq!(a.phase_strength.to_bits(), b.phase_strength.to_bits());
    assert_eq!(
        a.phase_band.map(f64::to_bits),
        b.phase_band.map(f64::to_bits)
    );
    assert_eq!(a.notes, b.notes);
}

#[test]
fn lf_align_aligns_lf_total_gd_to_hf() {
    let h = synthetic(80.0, 1.0);
    let config = config(&h, Mode::Linear);
    let mut setup = Setup::new(&config);
    let off = correct(h.clone(), &config, &setup);
    setup.lf_align = true;
    let on = correct(h.clone(), &config, &setup);
    let before = arrival(&h, &off, 60.0, 100.0);
    let after = arrival(&h, &on, 60.0, 100.0);
    eprintln!(
        "LF arrival off={before:.6} ms on={after:.6} ms; {:?}",
        on.lf_align
    );
    assert!(after.abs() < 1.0, "{after}");
    assert!(before.abs() > after.abs(), "{before} -> {after}");
    let reported = on.lf_align.as_ref().unwrap().arrival_ms.unwrap();
    // A measured residual, not the unclamped request minus itself.
    assert!(reported.abs() > 1e-6 && reported.abs() < 1.0);
    assert!(on.notes.iter().any(
        |n| matches!(n, Note::LfAlign { arrival_ms: Some(value), .. }
        if value.to_bits() == reported.to_bits())
    ));
    assert_eq!(
        on.notes
            .iter()
            .filter(|n| matches!(n, Note::LfAlign { .. }))
            .count(),
        1
    );
}

#[test]
fn lf_align_common_arrival_across_mains() {
    let arrivals: Vec<_> = [(80.0, 1.0), (110.0, 0.8)]
        .into_iter()
        .map(|(hz, q)| {
            let h = synthetic(hz, q);
            let config = config(&h, Mode::Linear);
            let mut setup = Setup::new(&config);
            setup.lf_align = true;
            let out = correct(h.clone(), &config, &setup);
            arrival(&h, &out, 60.0, 150.0)
        })
        .collect();
    eprintln!("common LF arrivals: {arrivals:?} ms");
    assert!((arrivals[0] - arrivals[1]).abs() < 1.0);
}

#[test]
fn lf_align_pull_limit_holds() {
    for &f in design_freqs().iter().filter(|f| **f > 0.0) {
        for strength in [0.0, 0.3, 0.5, 0.7, 0.9] {
            let applied = rc::lf_pull(1.0, 0.03, strength, f, 1.0);
            assert!(applied <= 1.5 / f);
            assert!(applied <= 0.03 * strength);
            assert_eq!(rc::lf_pull(-1.0, 0.03, strength, f, 1.0), -0.005);
            // At a taper the contract blends the old and cycle bounds.
            for b in [0.5, 0.75] {
                let upper = (1.0 - b) * 0.03 * strength + b * (0.03 * strength).min(1.5 / f);
                assert_eq!(rc::lf_pull(1.0, 0.03, strength, f, b), upper);
            }
        }
    }
    assert_eq!(rc::lf_pull(1.0, 0.03, 0.9, 0.0, 1.0), 0.027);
}

#[test]
fn lf_align_zero_mode_and_off_are_unchanged() {
    let h = synthetic(80.0, 1.0);
    for mode in [Mode::Zero, Mode::Linear] {
        let mut config = config(&h, mode);
        let setup = Setup::new(&config);
        let off = correct(h.clone(), &config, &setup);
        config.lf_align = true;
        let mut on_setup = Setup::new(&config);
        if mode != Mode::Zero {
            on_setup.lf_align = false;
        }
        let on = correct(h.clone(), &config, &on_setup);
        assert_bits(&off, &on);
        assert!(on.lf_align.is_none());
    }
}

#[test]
fn lf_align_mixed_mode_reports_limited() {
    let h = synthetic(80.0, 1.0);
    let config = config(&h, Mode::Mixed);
    let mut setup = Setup::new(&config);
    setup.lf_align = true;
    let out = correct(h, &config, &setup);
    eprintln!("mixed LF outcome: {:?}", out.lf_align);
    assert!(
        out.notes
            .iter()
            .any(|n| matches!(n, Note::LfAlign { limited: true, .. }))
    );
}

#[test]
fn lf_align_empty_band_reports_limited_without_arrival() {
    let h = synthetic(80.0, 1.0);
    let config = config(&h, Mode::Linear);
    let mut setup = Setup::new(&config);
    setup.lf_align = true;
    setup.rc_lo = 200.0;
    let out = correct(h, &config, &setup);
    let outcome = out.lf_align.unwrap();
    assert!(outcome.limited);
    assert_eq!(outcome.arrival_ms, None);
}
