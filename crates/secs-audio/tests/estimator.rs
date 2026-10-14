// Adapted from Impulcifer-pip313 crates/impulcifer-dsp/tests/{golden_brir_objects,properties_brir_objects}.rs, cac2217.
// MIT; original copyright and license in ../THIRD_PARTY_LICENSES.txt.
// The fixed-length/band/level API deliberately replaces the legacy duration/layout API.
#![forbid(unsafe_code)]
use secs_audio::estimator::{SWEEP_LENGTHS, SweepEstimator, sweep_length_samples};
use secs_audio::measurement::sweep_sequence;
use secs_audio::*;

#[test]
fn sweep_parameters_inverse_and_delayed_reflection() {
    let e = SweepEstimator::new(48_000, 20.0, 20_000.0, 65_536, -12.0, 0.005).unwrap();
    assert_eq!(e.test_signal.len(), 65_536);
    assert_eq!(e.test_signal[0], 0.0);
    assert_eq!(e.test_signal[65_535], 0.0);
    assert!(
        e.test_signal
            .iter()
            .all(|x| x.abs() <= 10_f64.powf(-12.0 / 20.0) + 1e-8)
    );
    let mut recording = vec![0.0; e.test_signal.len() + 3000];
    for (i, x) in e.test_signal.iter().enumerate() {
        recording[i + 1000] += x;
        recording[i + 3000] += 0.1 * x;
    }
    let raw = e.estimate_raw(&recording);
    let peak = raw
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))
        .unwrap()
        .0;
    assert_eq!(peak, 65_535 + 1000);
    assert!((raw[peak] - 1.0).abs() < 0.001);
    assert!((raw[65_535 + 3000] - 0.1).abs() < 0.001);
    let got = e.estimate(&recording).unwrap();
    let peak = got
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))
        .unwrap()
        .0;
    assert_eq!(peak, 1000);
    assert!((got[3000] / got[1000] - 0.1).abs() < 1e-5);
}

#[test]
fn sweep_fades_and_invalid_arguments_are_errors() {
    for fade in [-1.0, f64::NAN, f64::INFINITY, 1.0] {
        assert!(SweepEstimator::new(48_000, 20.0, 20_000.0, 65_536, -20.0, fade).is_err());
    }
    for (fs, low, high, n, level) in [
        (0, 20.0, 20_000.0, 65_536, -20.0),
        (48_000, 0.0, 20_000.0, 65_536, -20.0),
        (48_000, 100.0, 20.0, 65_536, -20.0),
        (48_000, 20.0, 24_000.0, 65_536, -20.0),
        (48_000, 20.0, 20_000.0, 0, -20.0),
        (48_000, 20.0, 20_000.0, 65_536, 1.0),
    ] {
        assert!(SweepEstimator::new(fs, low, high, n, level, 0.005).is_err());
    }
    for (i, s) in SWEEP_LENGTHS.iter().enumerate() {
        assert_eq!(sweep_length_samples(s).unwrap(), 65_536 << i);
    }
    assert!(sweep_length_samples("32k").is_err());
}

#[test]
fn sweep_sequence_routes_in_plan_order_and_preserves_reused_channel() {
    let room = SimRoom::new(48_000, vec![vec![vec![1.0]]; 2], 0, f64::NEG_INFINITY, 1).unwrap();
    let plan = MeasurementPlan::new(
        room.endpoint(),
        room.endpoint(),
        vec![
            RoleOutput {
                role: "R".into(),
                channel: 2,
            },
            RoleOutput {
                role: "L".into(),
                channel: 1,
            },
            RoleOutput {
                role: "L:HF".into(),
                channel: 1,
            },
        ],
    );
    let e = SweepEstimator::new(48_000, 20.0, 20_000.0, 65_536, -20.0, 0.005).unwrap();
    let sequence = sweep_sequence(&plan, &e).unwrap();
    assert_eq!(sequence.playback.channels, 2);
    for (i, role) in plan.roles.iter().enumerate() {
        let start = sequence.sweep_starts[i];
        for (j, &sample) in e.test_signal.iter().enumerate() {
            let frame = &sequence.playback.interleaved[(start + j) * 2..(start + j) * 2 + 2];
            assert_eq!(frame[usize::from(role.channel - 1)], sample as f32);
            assert_eq!(frame[usize::from(2 - role.channel)], 0.0);
        }
    }
}
