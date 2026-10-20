#![forbid(unsafe_code)]
use std::{cell::Cell, f64::consts::PI};

use secs_dsp::{Complex64 as C, DspError, resample::IrResamplePlan};

fn run(input: &[f64], rate: u32) -> (IrResamplePlan, Vec<f64>) {
    let plan = IrResamplePlan::new(input.len(), rate, 48_000).unwrap();
    let output = plan
        .process(|n| input[n], || Ok::<_, DspError>(()))
        .unwrap();
    (plan, output)
}

fn response(samples: &[f64], hz: f64, rate: f64, leading: f64) -> C {
    samples
        .iter()
        .enumerate()
        .map(|(i, &x)| C::from_polar(x, -2.0 * PI * hz * (i as f64 - leading) / rate))
        .sum()
}

#[test]
fn full_arrays_match_independent_scipy_polyphase_oracle() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("data/resample_ir_scipy.json")).unwrap();
    assert_eq!(fixture["scipy"], "1.18.1");
    for case in fixture["cases"].as_array().unwrap() {
        let values = |key: &str| {
            case[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap())
                .collect::<Vec<_>>()
        };
        let (plan, got) = run(&values("input"), case["from_hz"].as_u64().unwrap() as u32);
        let expected = values("output");
        assert_eq!(
            plan.leading_output_samples(),
            case["leading"].as_u64().unwrap() as usize
        );
        assert_eq!(got.len(), expected.len());
        let error = got
            .iter()
            .zip(&expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f64, f64::max);
        assert!(error < 2e-12, "rate={} max abs={error:e}", case["from_hz"]);
    }
}

#[test]
fn first_last_and_fractional_grid_impulses_preserve_gain_and_physical_phase() {
    for rate in [44_100, 96_000] {
        for index in [0, 1, 17, 63] {
            let mut input = vec![0.0; 64];
            input[index] = 1.0;
            let (plan, output) = run(&input, rate);
            assert!(
                output.len() > (input.len() as f64 * 48_000.0 / f64::from(rate)).ceil() as usize
            );
            assert!((output.iter().sum::<f64>() - 1.0).abs() < 1e-6);
            for hz in [0.0, 20.0, 100.0, 1000.0, 10_000.0, 20_000.0] {
                let actual = response(&output, hz, 48_000.0, plan.leading_output_samples() as f64);
                let expected = C::from_polar(1.0, -2.0 * PI * hz * index as f64 / f64::from(rate));
                assert!(
                    (actual - expected).norm() < 1e-5,
                    "rate={rate} index={index} hz={hz} error={:e}",
                    (actual - expected).norm()
                );
            }
        }
    }
}

#[test]
fn zero_dc_and_zero_responses_do_not_depend_on_sum_normalization() {
    for rate in [44_100, 96_000] {
        let mut input = vec![0.0; 64];
        input[0] = 1.0;
        input[63] = -1.0;
        let (_, output) = run(&input, rate);
        assert!(output.iter().all(|v| v.is_finite()));
        assert!(output.iter().sum::<f64>().abs() < 1e-6);
        let scaled: Vec<_> = input.iter().map(|v| -2.0 * v).collect();
        let (_, other) = run(&scaled, rate);
        assert!(
            output
                .iter()
                .zip(other)
                .all(|(a, b)| (b + 2.0 * a).abs() < 1e-13)
        );
        let (_, zero) = run(&[0.0; 64], rate);
        assert!(zero.iter().all(|&v| v == 0.0));
    }
}

#[test]
fn downsampling_rejects_out_of_band_aliases() {
    let length = 8192;
    for hz in [30_000.0, 36_000.0, 45_000.0] {
        let input: Vec<_> = (0..length)
            .map(|n| {
                let window = 0.5 - 0.5 * (2.0 * PI * n as f64 / (length - 1) as f64).cos();
                window * (2.0 * PI * hz * n as f64 / 96_000.0).cos()
            })
            .collect();
        let (plan, output) = run(&input, 96_000);
        let aliased = 48_000.0 - hz;
        let relative = response(
            &output,
            aliased,
            48_000.0,
            plan.leading_output_samples() as f64,
        )
        .norm()
            / response(&input, hz, 96_000.0, 0.0).norm();
        assert!(relative < 1e-5, "{hz} Hz alias ratio={relative:e}");
    }
}

#[test]
fn upsampling_rejects_images_above_the_source_nyquist() {
    let length = 8192;
    let input: Vec<_> = (0..length)
        .map(|n| {
            let window = 0.5 - 0.5 * (2.0 * PI * n as f64 / (length - 1) as f64).cos();
            window * (2.0 * PI * 21_000.0 * n as f64 / 44_100.0).cos()
        })
        .collect();
    let (plan, output) = run(&input, 44_100);
    let image = response(
        &output,
        23_100.0,
        48_000.0,
        plan.leading_output_samples() as f64,
    )
    .norm()
        / response(&input, 21_000.0, 44_100.0, 0.0).norm();
    assert!(image < 1e-5, "image ratio={image:e}");
}

#[test]
fn common_rates_and_bounds_preserve_in_band_impulse_transfer() {
    for rate in [8_000, 11_025, 32_000, 88_200, 192_000, 384_000] {
        for index in [0, 63] {
            let mut input = vec![0.0; 64];
            input[index] = 1.0;
            let (plan, output) = run(&input, rate);
            assert!((output.iter().sum::<f64>() - 1.0).abs() < 1e-6, "{rate}");
            for fraction in [0.1, 0.25, 0.45] {
                let hz = f64::from(rate.min(48_000)) * fraction;
                let actual = response(&output, hz, 48_000.0, plan.leading_output_samples() as f64);
                let expected = C::from_polar(1.0, -2.0 * PI * hz * index as f64 / f64::from(rate));
                assert!(
                    (actual - expected).norm() < 1e-5,
                    "rate={rate} index={index} hz={hz} error={:e}",
                    (actual - expected).norm()
                );
            }
        }
    }
}

#[test]
fn identity_limits_and_extreme_ratios_are_checked_without_allocating_kernels() {
    let input = [-0.0, 0.0, f64::MIN_POSITIVE, 1.0, -0.25];
    let (plan, output) = run(&input, 48_000);
    assert_eq!(plan.leading_output_samples(), 0);
    assert_eq!(
        input.map(f64::to_bits).to_vec(),
        output.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
    );
    assert!(IrResamplePlan::new(0, 44_100, 48_000).is_err());
    assert!(IrResamplePlan::new(1, 7_999, 48_000).is_err());
    assert!(IrResamplePlan::new(1, 384_001, 48_000).is_err());
    assert!(IrResamplePlan::new(usize::MAX, 44_100, 48_000).is_err());
    assert!(IrResamplePlan::new(usize::MAX, 48_000, 48_000).is_err());
    for rate in [47_999, 383_999] {
        let plan = IrResamplePlan::new(1, rate, 48_000).unwrap();
        assert!(plan.kernel_bytes() > 64 * 1024 * 1024);
    }
}

#[test]
fn cancellation_is_checked_during_kernel_and_output_work() {
    for cancel_at in [10, 250] {
        let plan = IrResamplePlan::new(5000, 44_100, 48_000).unwrap();
        let checks = Cell::new(0);
        let reads = Cell::new(0);
        let result: Result<Vec<f64>, DspError> = plan.process(
            |_| {
                reads.set(reads.get() + 1);
                1.0
            },
            || {
                checks.set(checks.get() + 1);
                if checks.get() == cancel_at {
                    Err(DspError::InvalidArgument("cancel probe".into()))
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.unwrap_err().to_string().contains("cancel probe"));
        if cancel_at == 10 {
            assert_eq!(reads.get(), 0);
        } else {
            assert!(reads.get() > 0);
        }
    }
}
