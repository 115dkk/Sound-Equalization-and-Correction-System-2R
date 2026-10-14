#![forbid(unsafe_code)]
use secs_audio::level::{TARGET_WINDOW, analyse_level, level_signal};
use secs_audio::*;

#[test]
fn calibration_files_and_rew_spl_reference() {
    let analog = MicCalibration::parse(include_str!("data/analog.cal")).unwrap();
    let usb = MicCalibration::parse(include_str!("data/usb.cal")).unwrap();
    assert_eq!(analog.spl_reference().unwrap().offset_db, 120.0);
    assert_eq!(usb.usb_sens_factor_db(), Some(-18.25));
    assert!(usb.spl_reference().is_none());
    assert_eq!(usb.points()[2].phase_degrees, Some(-20.0));
    let reference = SplReference::from_usb(&usb, -12.0).unwrap();
    assert_eq!(reference.offset_db, 124.25);
    let samples = level_signal(
        LevelSignal::SubSine { frequency: 1000.0 },
        48_000,
        -26.0,
        1.0,
    )
    .unwrap();
    let report = analyse_level(&samples, 48_000, Some(&analog), None).unwrap();
    assert!((report.spl_db.unwrap() - 94.0).abs() < 1e-9);
    let report = analyse_level(&samples, 48_000, Some(&usb), Some(reference)).unwrap();
    assert!((report.spl_db.unwrap() - 96.25).abs() < 1e-9);
    let no_reference = analyse_level(&samples, 48_000, Some(&usb), None).unwrap();
    assert!(no_reference.spl_db.is_none());
}

#[test]
fn malformed_calibrations_return_errors_without_panics() {
    for text in [
        "",
        "20 0",
        "20 NaN\n1000 0",
        "0 0\n1000 0",
        "1000 0\n20 0",
        "20 0\n20 1",
        "Sens Factor = broken\n20 0\n1000 0",
        "Sensitivity NaN dBFS\n20 0\n1000 0",
        "20 0 nope\n1000 0",
        "20 0 0 0\n1000 0",
        "Sensitivity -20 dBFS\nSensitivity -10 dBFS\n20 0\n1000 0",
    ] {
        assert!(MicCalibration::parse(text).is_err(), "{text}");
    }
    for n in 0..256 {
        let text = format!(
            "20 {}\n1000 {}",
            char::from_u32(n).unwrap(),
            char::from_u32(255 - n).unwrap()
        );
        assert!(std::panic::catch_unwind(|| MicCalibration::parse(&text)).is_ok());
    }
}

#[test]
fn level_known_signals_clip_and_recommendation() {
    let sine = level_signal(LevelSignal::SubSine { frequency: 50.0 }, 48_000, -20.0, 1.0).unwrap();
    let report = analyse_level(&sine, 48_000, None, None).unwrap();
    assert!((report.input_dbfs + 20.0).abs() < 1e-10);
    assert!((report.peak_dbfs - (-20.0 + 10.0 * 2_f64.log10())).abs() < 1e-10);
    assert!(!report.clip);
    assert!(
        analyse_level(&[0.9, -0.9], 48_000, None, None)
            .unwrap()
            .clip
    );
    assert!(
        !analyse_level(&[10_f64.powf(-1.0 / 20.0)], 48_000, None, None)
            .unwrap()
            .clip
    );
    let pink = level_signal(LevelSignal::Pink, 48_000, -20.0, 1.0).unwrap();
    assert_eq!(
        pink,
        level_signal(LevelSignal::Pink, 48_000, -20.0, 1.0).unwrap()
    );
    assert!((analyse_level(&pink, 48_000, None, None).unwrap().input_dbfs + 20.0).abs() < 1e-10);
    for measured in [-40.0, -25.0, -10.0] {
        let next = recommend_sweep_level(measured, -30.0, TARGET_WINDOW).unwrap();
        let predicted = measured + next + 30.0;
        assert!((TARGET_WINDOW.0..=TARGET_WINDOW.1).contains(&predicted));
    }
    assert!(recommend_sweep_level(f64::NEG_INFINITY, -20.0, TARGET_WINDOW).is_err());
    assert!(recommend_sweep_level(-100.0, -20.0, TARGET_WINDOW).is_err());
}

#[test]
fn level_check_uses_simulated_capture_and_calibration() {
    let room = SimRoom::new(48_000, vec![vec![vec![0.5]]], 2317, f64::NEG_INFINITY, 1).unwrap();
    let output = LevelOutput {
        endpoint: room.endpoint(),
        channel: 1,
        sample_rate: 48_000,
        signal: LevelSignal::Pink,
        share: SharePreference::Auto,
    };
    let input = LevelInput {
        endpoint: room.endpoint(),
        channel: 1,
        calibration: Some(MicCalibration::parse(include_str!("data/analog.cal")).unwrap()),
        spl_reference: None,
    };
    let measured = level_check(&room, &output, &input, -20.0, 1.0).unwrap();
    assert!((measured.input_dbfs - (-20.0 + 20.0 * 0.5_f64.log10())).abs() < 1e-6);
    let level = recommend_sweep_level(measured.input_dbfs, -20.0, TARGET_WINDOW).unwrap();
    let second = level_check(&room, &output, &input, level, 1.0).unwrap();
    assert!((second.input_dbfs + 22.5).abs() < 1e-6);
    assert!((second.spl_db.unwrap() - 97.5).abs() < 1e-6);
}
