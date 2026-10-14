#![forbid(unsafe_code)]
use secs_audio::estimator::SweepEstimator;
use secs_audio::session::SessionEvent;
use secs_audio::*;
use secs_dsp::fft;

fn impulse(n: usize, at: usize, gain: f64) -> Vec<f64> {
    let mut v = vec![0.0; n];
    v[at] = gain;
    v
}
fn onset(v: &[f32]) -> usize {
    let peak = v.iter().map(|x| x.abs()).fold(0.0_f32, f32::max);
    v.iter().position(|x| x.abs() >= peak * 0.1).unwrap()
}
fn band_error(measured: &MeasuredRole, expected: &[f64], latency: usize) -> f64 {
    let n = (expected.len() + measured.ir.len() + latency + 4096).next_power_of_two();
    let mut a = vec![0.0; n];
    let mut b = vec![0.0; n];
    let start = (measured.t0 * f64::from(measured.sample_rate)).round() as isize;
    for (i, &x) in measured.ir.iter().enumerate() {
        let j = start + i as isize;
        if j >= 0 && (j as usize) < n {
            a[j as usize] = f64::from(x);
        }
    }
    b[latency..latency + expected.len()].copy_from_slice(expected);
    let a = fft::rfft(&a);
    let b = fft::rfft(&b);
    let mut e = 0.0;
    let mut p = 0.0;
    for (i, (a, b)) in a.iter().zip(&b).enumerate() {
        let f = i as f64 * f64::from(measured.sample_rate) / n as f64;
        if (20.0..=20_000.0).contains(&f) {
            e += (a - b).norm_sqr();
            p += b.norm_sqr();
        }
    }
    10.0 * (e / p).log10()
}

#[test]
fn sim_round_trip_rates_lengths_latencies_and_relative_arrivals() {
    for (fs, length, latency) in [
        (44_100, "64k", 0),
        (48_000, "128k", 127),
        (96_000, "64k", 1001),
        (48_000, "256k", 4096),
    ] {
        let mut left = impulse(1000, 137, 0.7);
        left[500] = 0.12;
        let mut right = impulse(1000, 204, 0.4);
        right[700] = -0.07;
        let room = SimRoom::new(
            fs,
            vec![vec![left.clone()], vec![right.clone()]],
            latency,
            f64::NEG_INFINITY,
            3,
        )
        .unwrap();
        let mut plan = MeasurementPlan::new(
            room.endpoint(),
            room.endpoint(),
            vec![
                RoleOutput {
                    role: "L".into(),
                    channel: 1,
                },
                RoleOutput {
                    role: "R".into(),
                    channel: 2,
                },
            ],
        );
        plan.sample_rate = fs;
        plan.sweep_length = length.into();
        let got = measure_position(&room, &plan, 1, &CancelToken::new(), &mut |_| {}).unwrap();
        let arrivals: Vec<_> = got
            .roles
            .iter()
            .map(|r| r.t0 * f64::from(fs) + onset(&r.ir) as f64)
            .collect();
        assert!(
            (arrivals[1] - arrivals[0] - 67.0).abs() <= 1.0,
            "{arrivals:?}"
        );
        for (r, expected) in got.roles.iter().zip([&left, &right]) {
            let error = band_error(r, expected, latency);
            println!(
                "roundtrip fs={fs} length={length} latency={latency} role={} band_error_db={error:.3}",
                r.role
            );
            assert!(error < -60.0, "{error}");
            assert!(r.ir.len() >= 2 * fs as usize);
        }
    }
}

#[test]
fn loopback_recovers_absolute_latency() {
    for latency in [0, 17, 2000] {
        let room = SimRoom::new(
            48_000,
            vec![vec![impulse(600, 181, 0.5), vec![1.0]]],
            latency,
            f64::NEG_INFINITY,
            5,
        )
        .unwrap();
        let mut plan = MeasurementPlan::new(
            room.endpoint(),
            room.endpoint(),
            vec![RoleOutput {
                role: "L".into(),
                channel: 1,
            }],
        );
        plan.sweep_length = "64k".into();
        plan.loopback_input_channel = Some(2);
        let got = measure_position(&room, &plan, 1, &CancelToken::new(), &mut |_| {}).unwrap();
        assert_eq!(got.latency_samples, Some(latency));
        let r = &got.roles[0];
        // Absolute loopback uses the peak, not the engine's 10% onset (a
        // band-limited impulse has real pre-ringing before that peak).
        let peak =
            r.ir.iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))
                .unwrap()
                .0;
        assert!((r.t0 * 48_000.0 + peak as f64 - 181.0).abs() <= 1.0);
    }
}

#[test]
fn cancellation_during_position_returns_no_roles() {
    let room = SimRoom::new(48_000, vec![vec![vec![1.0]]], 0, f64::NEG_INFINITY, 1).unwrap();
    let mut plan = MeasurementPlan::new(
        room.endpoint(),
        room.endpoint(),
        vec![
            RoleOutput {
                role: "L".into(),
                channel: 1,
            },
            RoleOutput {
                role: "R".into(),
                channel: 1,
            },
        ],
    );
    plan.sweep_length = "64k".into();
    let cancel = CancelToken::new();
    let result = measure_position(&room, &plan, 1, &cancel, &mut |event| {
        if matches!(event, SessionEvent::OutputStarted { .. }) {
            cancel.cancel();
        }
    });
    assert!(matches!(result, Err(AudioError::Cancelled)));
    // The backend can be reused after cancellation (both handles were dropped).
    assert!(measure_position(&room, &plan, 1, &CancelToken::new(), &mut |_| {}).is_ok());
}

#[test]
fn dolby_golden_ir_round_trip() {
    let fixture = secs_oracle::Fixture::load("engine_inputs", "dolby");
    let roles = fixture.inputs()["measurements"][0][1].as_array().unwrap();
    let mut paths = Vec::new();
    for role in roles.iter().take(2) {
        let name = role[1]["samples"]["blob"].as_str().unwrap();
        let bytes = std::fs::read(secs_oracle::goldens().join("engine_inputs").join(name)).unwrap();
        let ir = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|x| f64::from(f32::from_le_bytes(*x)))
            .collect::<Vec<_>>();
        paths.push(vec![ir]);
    }
    let room = SimRoom::new(48_000, paths.clone(), 223, f64::NEG_INFINITY, 1).unwrap();
    let mut plan = MeasurementPlan::new(
        room.endpoint(),
        room.endpoint(),
        vec![
            RoleOutput {
                role: "L".into(),
                channel: 1,
            },
            RoleOutput {
                role: "R".into(),
                channel: 2,
            },
        ],
    );
    plan.sweep_length = "128k".into();
    plan.window.pre_roll_seconds = 0.25;
    plan.window.noise_seconds = 0.5;
    plan.window.post_onset_seconds = 4.0;
    let got = measure_position(&room, &plan, 1, &CancelToken::new(), &mut |_| {}).unwrap();
    for (r, path) in got.roles.iter().zip(&paths) {
        let error = band_error(r, &path[0], 223);
        println!("dolby {} band_error_db={error:.3}", r.role);
        assert!(error < -60.0, "{error}");
    }
    let original: Vec<_> = paths
        .iter()
        .map(|p| onset(&p[0].iter().map(|x| *x as f32).collect::<Vec<_>>()) as f64)
        .collect();
    let measured: Vec<_> = got
        .roles
        .iter()
        .map(|r| r.t0 * 48_000.0 + onset(&r.ir) as f64)
        .collect();
    assert!(((measured[1] - measured[0]) - (original[1] - original[0])).abs() <= 1.0);
}

#[test]
fn snr_matches_known_white_noise_within_one_db() {
    let fs = 48_000;
    let estimator = SweepEstimator::new(fs, 20.0, 20_000.0, 65_536, -20.0, 0.005).unwrap();
    // Independent reference: Farina direct response to the emitted sweep.
    let response = estimator.estimate_raw(&estimator.test_signal);
    let center = estimator.test_signal.len() - 1;
    let energy = response[center..center + 480]
        .iter()
        .map(|x| x * x)
        .sum::<f64>()
        / 480.0;
    let noise_gain = estimator.inverse_filter.iter().map(|x| x * x).sum::<f64>();
    for noise_db in [-50.0, -65.0] {
        let room = SimRoom::new(fs, vec![vec![impulse(500, 120, 1.0)]], 15, noise_db, 498).unwrap();
        let mut plan = MeasurementPlan::new(
            room.endpoint(),
            room.endpoint(),
            vec![RoleOutput {
                role: "L".into(),
                channel: 1,
            }],
        );
        plan.sweep_length = "64k".into();
        let got = measure_position(&room, &plan, 1, &CancelToken::new(), &mut |_| {}).unwrap();
        let expected = 10.0 * (energy / (10_f64.powf(noise_db / 10.0) * noise_gain)).log10();
        println!(
            "snr noise={noise_db} expected={expected:.3} measured={:.3}",
            got.roles[0].snr_db
        );
        assert!((got.roles[0].snr_db - expected).abs() < 1.0);
    }
}
