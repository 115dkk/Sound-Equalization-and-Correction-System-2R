#![forbid(unsafe_code)]

use secs_compute::Context;
use secs_engine::{CalibrationRequest, ProjectView, calibrate_with};
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
