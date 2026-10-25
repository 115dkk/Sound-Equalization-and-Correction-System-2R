#![forbid(unsafe_code)]
#![cfg(feature = "test-support")]

use secs_compute::Context;
use secs_engine::{
    CalibrationRequest, DesignRequest, ProjectView, Tuning, calibrate_with, design, engine_inputs,
    normalize_measurements,
    test_support::{Replay, load},
};
use secs_types::calibration::CalibrationSettings;

#[test]
fn calibration_resamples_44100_ir_without_changing_the_source() {
    use secs_engine::{Measurement, MeasurementSet, Samples, calibrate};
    use secs_types::calibration::SampleRatePolicy;
    let groups = vec![("front".into(), vec!["L".into()])];
    let target = secs_types::target::TargetCurve::default();
    let view = ProjectView {
        groups: &groups,
        project_xo: &[],
        target: &target,
    };
    let irs = MeasurementSet {
        positions: vec![(
            1,
            vec![(
                "L".into(),
                Measurement {
                    samples: Samples::F64(vec![1.0; 256].into()),
                    sample_rate: 44100,
                    t0: 0.125,
                },
            )],
        )],
    };
    let settings = CalibrationSettings {
        sample_rate_policy: SampleRatePolicy::Resample,
        ..Default::default()
    };
    let result = calibrate(
        &CalibrationRequest {
            view: &view,
            irs: &irs,
            settings: &settings,
            subs_only: true,
        },
        &Context::serial(),
    )
    .unwrap();
    assert_eq!(result.resampled, 1);
    assert_eq!(result.measurements.positions[0].1[0].1.sample_rate, 48000);
    assert_eq!(irs.positions[0].1[0].1.sample_rate, 44100);
    assert_eq!(irs.positions[0].1[0].1.t0.to_bits(), 0.125_f64.to_bits());
    assert!(result.design.mains.is_empty());
}

#[test]
fn cancelled_calibration_returns_before_resolving_invalid_inputs() {
    let target = secs_types::target::TargetCurve::default();
    let view = ProjectView {
        groups: &[],
        project_xo: &[],
        target: &target,
    };
    let irs = secs_engine::MeasurementSet { positions: vec![] };
    let settings = CalibrationSettings::default();
    let ctx = Context::serial();
    ctx.cancel().cancel();
    let request = CalibrationRequest {
        view: &view,
        irs: &irs,
        settings: &settings,
        subs_only: false,
    };
    assert!(matches!(
        secs_engine::calibrate(&request, &ctx),
        Err(secs_engine::EngineError::Cancelled)
    ));
    let fitters: Box<secs_engine::FitterFactory<'_>> =
        Box::new(|_| panic!("cancelled calibration must not create fitters"));
    assert!(matches!(
        calibrate_with(&request, &ctx, &*fitters),
        Err(secs_engine::EngineError::Cancelled)
    ));
}

#[test]
fn calibration_replay_matches_assembled_design_bit_for_bit() {
    let mut case = load("dolby_linear");
    // Resolution must retain its adapter note, but not the biamp measurement.
    let measurement = case.measurements.positions[0].1[0].1.clone();
    case.measurements.positions[0]
        .1
        .push(("L:LF".into(), measurement));
    let settings: CalibrationSettings = serde_json::from_value(case.settings).unwrap();
    let view = ProjectView {
        groups: &case.config.groups,
        project_xo: &case.config.xo,
        target: &case.config.target,
    };
    let ctx = Context::serial();
    let inputs = engine_inputs(&view, &case.measurements, &settings).unwrap();
    let measurements =
        normalize_measurements(&inputs.measurements, settings.sample_rate_policy, &ctx).unwrap();
    let replay = Replay::for_case("dolby_linear");
    let expected = design(
        &DesignRequest {
            measurements: &measurements,
            config: &inputs.config,
            tuning: &Tuning::default(),
            subs_only: false,
            adapter_notes: &inputs.notes,
        },
        &ctx,
        &*replay.factory(),
    )
    .unwrap();
    replay.finish();
    let replay = Replay::for_case("dolby_linear");
    let actual = calibrate_with(
        &CalibrationRequest {
            view: &view,
            irs: &case.measurements,
            settings: &settings,
            subs_only: false,
        },
        &ctx,
        &*replay.factory(),
    )
    .unwrap();
    replay.finish();
    assert_eq!(actual.resampled, 0);
    assert_eq!(actual.config.mains, inputs.config.mains);
    assert_eq!(
        actual.measurements.positions.len(),
        measurements.positions.len()
    );
    let actual = actual.design;
    assert_eq!(actual.chain, expected.chain);
    assert_eq!(actual.notes, expected.notes);
    assert_eq!(actual.adapter_notes, ["바이앰프 대역 측정 제외: L:LF"]);
    assert_eq!(actual.adapter_notes, expected.adapter_notes);
    assert_eq!(actual.ref_level.to_bits(), expected.ref_level.to_bits());
    assert_eq!(actual.rc_hi.to_bits(), expected.rc_hi.to_bits());
    assert_eq!(actual.mains.len(), expected.mains.len());
    for (a, b) in actual.mains.iter().zip(&expected.mains) {
        assert_eq!(a.role, b.role);
        for (a, b) in [
            (&a.before, &b.before),
            (&a.combined, &b.combined),
            (&a.after, &b.after),
        ] {
            assert_eq!(a.len(), b.len());
            assert!(
                a.iter()
                    .zip(b.iter())
                    .all(|(a, b)| (a.re.to_bits(), a.im.to_bits())
                        == (b.re.to_bits(), b.im.to_bits()))
            );
        }
    }
    assert_eq!(actual.scores.len(), expected.scores.len());
    for (a, b) in actual.scores.iter().zip(&expected.scores) {
        assert_eq!(a.role, b.role);
        assert_eq!(a.before, b.before);
        assert_eq!(a.after, b.after);
        for (a, b) in [
            (&a.spl_before, &b.spl_before),
            (&a.spl_after, &b.spl_after),
            (&a.gd_before, &b.gd_before),
            (&a.gd_after, &b.gd_after),
        ] {
            assert_eq!(a.len(), b.len());
            assert!(a.iter().zip(b).all(|(a, b)| a.to_bits() == b.to_bits()));
        }
    }
}
