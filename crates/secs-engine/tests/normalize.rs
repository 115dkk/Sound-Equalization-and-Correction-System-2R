#![forbid(unsafe_code)]
use secs_compute::{CancelToken, Context, MemoryBudget, NoProgress};
use secs_engine::{EngineError, Measurement, MeasurementSet, Samples, normalize_measurements};
use secs_types::calibration::SampleRatePolicy;
use std::sync::Arc;

fn context(bytes: u64) -> Context {
    Context::new(
        1,
        MemoryBudget::new(bytes),
        Arc::new(NoProgress),
        CancelToken::new(),
    )
}
fn set(samples: Samples, rate: u32, t0: f64) -> MeasurementSet {
    MeasurementSet {
        positions: vec![(
            3,
            vec![(
                "L".into(),
                Measurement {
                    samples,
                    sample_rate: rate,
                    t0,
                },
            )],
        )],
    }
}

#[test]
fn reject_and_48k_paths_preserve_sample_arcs_and_time_bits() {
    for rate in [44_100, 48_000] {
        let arc: Arc<[f32]> = vec![-0.0, 1.0, -0.5].into();
        let input = set(Samples::F32(arc.clone()), rate, -0.0);
        for policy in [SampleRatePolicy::Reject, SampleRatePolicy::Resample] {
            if rate != 48_000 && policy == SampleRatePolicy::Resample {
                continue;
            }
            let output = normalize_measurements(&input, policy, &context(1)).unwrap();
            let measurement = output.get(3, "L").unwrap();
            let Samples::F32(result) = &measurement.samples else {
                panic!("precision changed");
            };
            assert!(Arc::ptr_eq(result, &arc));
            assert_eq!(measurement.t0.to_bits(), (-0.0_f64).to_bits());
            assert_eq!(measurement.sample_rate, rate);
        }
    }
    let arc: Arc<[f64]> = vec![-0.0, f64::MIN_POSITIVE, 1.0].into();
    let input = set(Samples::F64(arc.clone()), 48_000, 0.125);
    let output = normalize_measurements(&input, SampleRatePolicy::Resample, &context(1)).unwrap();
    let Samples::F64(result) = &output.get(3, "L").unwrap().samples else {
        panic!("precision changed");
    };
    assert!(Arc::ptr_eq(result, &arc));
}

#[test]
fn mixed_rates_preserve_the_original_ir_and_align_the_same_physical_arrival() {
    let mut input = MeasurementSet::default();
    for (role, rate, at) in [("L", 44_100, 441), ("R", 96_000, 960), ("C", 48_000, 480)] {
        let mut samples = vec![0.0_f32; at + 200];
        samples[at] = 1.0;
        input.positions.push((
            input.positions.len() as u32 + 1,
            vec![(
                role.into(),
                Measurement {
                    samples: Samples::F32(samples.into()),
                    sample_rate: rate,
                    t0: 0.25,
                },
            )],
        ));
    }
    let output =
        normalize_measurements(&input, SampleRatePolicy::Resample, &context(1_000_000)).unwrap();
    for ((_, source), (_, converted)) in input.positions.iter().zip(&output.positions) {
        let source = &source[0].1;
        let converted = &converted[0].1;
        let peak = (0..converted.samples.len())
            .max_by(|&a, &b| {
                converted
                    .samples
                    .at(a)
                    .abs()
                    .total_cmp(&converted.samples.at(b).abs())
            })
            .unwrap();
        assert!((converted.t0 + peak as f64 / 48_000.0 - 0.26).abs() < 1e-12);
        assert_eq!(source.t0, 0.25);
        assert_eq!(converted.sample_rate, 48_000);
        if source.sample_rate != 48_000 {
            assert!(converted.t0 < source.t0);
            assert!(matches!(converted.samples, Samples::F64(_)));
            assert!(
                (0..source.samples.len())
                    .map(|i| source.samples.at(i))
                    .sum::<f64>()
                    == 1.0
            );
        } else {
            let (Samples::F32(a), Samples::F32(b)) = (&source.samples, &converted.samples) else {
                panic!("precision changed");
            };
            assert!(Arc::ptr_eq(a, b));
        }
    }
}

#[test]
fn unsupported_rates_invalid_data_and_large_ratio_budgets_fail_before_conversion() {
    for rate in [0, 7_999, 384_001] {
        let input = set(Samples::F64(vec![1.0].into()), rate, 0.0);
        assert!(
            matches!(normalize_measurements(&input, SampleRatePolicy::Resample, &context(1024)),
            Err(EngineError::UnsupportedResamplingRate { position: 3, role, actual_hz }) if role == "L" && actual_hz == rate)
        );
    }
    for rate in [47_999, 383_999] {
        let input = set(Samples::F64(vec![1.0].into()), rate, 0.0);
        let error =
            normalize_measurements(&input, SampleRatePolicy::Resample, &context(1024)).unwrap_err();
        assert!(matches!(error, EngineError::Input(_)));
        assert!(error.to_string().contains("memory budget"));
    }
    for (samples, t0) in [
        (vec![], 0.0),
        (vec![f64::NAN], 0.0),
        (vec![1.0], f64::INFINITY),
    ] {
        let input = set(Samples::F64(samples.into()), 44_100, t0);
        assert!(
            matches!(normalize_measurements(&input, SampleRatePolicy::Resample, &context(1_000_000)),
            Err(EngineError::InvalidMeasurement { position: 3, role }) if role == "L")
        );
    }
}

#[test]
fn cancelled_context_stops_before_any_input_copy() {
    let ctx = context(1_000_000);
    ctx.cancel().cancel();
    let input = set(Samples::F64(vec![1.0].into()), 96_000, 0.0);
    for policy in [SampleRatePolicy::Reject, SampleRatePolicy::Resample] {
        assert!(matches!(
            normalize_measurements(&input, policy, &ctx),
            Err(EngineError::Cancelled)
        ));
    }
}
