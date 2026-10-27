#![forbid(unsafe_code)]
use secs_audio::*;
use secs_dsp::fft;

const UMIK_90: &str = include_str!("data/umik1_90deg_synthetic.txt");
const UMIK_AGAIN: &str = include_str!("data/umik1_again_synthetic.txt");

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected} (tolerance {tolerance})"
    );
}

fn point(frequency: f64, gain_db: f64, phase_degrees: Option<f64>) -> CalibrationPoint {
    CalibrationPoint {
        frequency,
        gain_db,
        phase_degrees,
    }
}

fn flat_curve(gain_db: f64, phase: Option<f64>) -> MicCalibration {
    MicCalibration::from_points(vec![
        point(10.0, gain_db, phase),
        point(20_000.0, gain_db, phase),
    ])
    .unwrap()
}

fn spectrum(ir: &[f32]) -> Vec<fft::Complex64> {
    fft::rfft(&ir.iter().map(|&x| f64::from(x)).collect::<Vec<_>>())
}

#[test]
fn umik1_files_parse_with_serial_gain_and_notes() {
    assert!(UMIK_90.contains("\r\n"));
    assert!(!UMIK_90.replace("\r\n", "").contains('\n'));
    assert!(!UMIK_AGAIN.contains('\r'));
    let mic = MicCalibration::parse_checked(UMIK_90).unwrap();
    assert_eq!(mic, MicCalibration::parse(UMIK_90).unwrap());
    assert_eq!(mic.usb_sens_factor_db(), Some(-0.7312));
    assert_eq!(mic.analog_gain_db(), None);
    assert_eq!(mic.serial(), Some("7000001"));
    assert_eq!(
        mic.notes(),
        &["Auto-generated 90-degree calibration file for use with nanoAVR DL"]
    );
    assert_eq!(mic.points().len(), 21);
    assert_eq!(mic.points()[0].frequency, 10.0);
    assert_eq!(mic.points()[20].frequency, 20_000.0);
    assert!(!mic.has_phase());
    assert_eq!(mic.sensitivity_dbfs_at_94(), None);
    assert!(mic.spl_reference().is_none());
    close(mic.gain_db(mic.points()[10].frequency), 0.4, 1e-12);
    close(
        mic.gain_db((mic.points()[10].frequency * mic.points()[11].frequency).sqrt()),
        0.325,
        1e-12,
    );
    assert_eq!(mic.gain_db(0.0), -3.6);
    assert_eq!(mic.gain_db(48_000.0), -5.5);

    let mic = MicCalibration::parse(UMIK_AGAIN).unwrap();
    assert_eq!(mic.usb_sens_factor_db(), Some(4.125));
    assert_eq!(mic.analog_gain_db(), Some(18.0));
    assert_eq!(mic.serial(), Some("7000001"));
    assert!(mic.notes().is_empty());
    assert_eq!(mic.points().len(), 3);
    assert_eq!(mic.points()[0].frequency, 10.0);
    assert_eq!(mic.points()[2].frequency, 20_000.0);
    assert!(!mic.has_phase());
    assert!(mic.spl_reference().is_none());
    close(mic.gain_db(1000.0), 0.0, 1e-12);
    close(mic.gain_db(100.0), -0.125, 1e-12);
}

#[test]
fn rew_headings_are_skipped_and_broken_rows_still_fail() {
    let mic = MicCalibration::parse_checked(
        "\u{feff}  Freq(Hz)\tSPL(dB)\tPhase(deg)\n\
         This unquoted note is not retained\n\
         # comment\n; comment\n// comment\n* comment\n\n\
         +20, -.5, 10 # row comment\n1000\t1\t-20 ; row comment",
    )
    .unwrap();
    assert!(mic.notes().is_empty());
    assert!(mic.has_phase());
    assert_eq!(
        mic.points(),
        &[
            point(20.0, -0.5, Some(10.0)),
            point(1000.0, 1.0, Some(-20.0))
        ]
    );
    let fractional = MicCalibration::parse(".5 0\n1 0").unwrap();
    assert_eq!(fractional.points()[0].frequency, 0.5);

    for (text, issue, line) in [
        (
            "Sens Factor = broken\n20 0\n1000 0",
            CalibrationIssue::Header,
            Some(1),
        ),
        (
            "20 0\nSensitivity NaN dBFS\n1000 0",
            CalibrationIssue::Header,
            Some(2),
        ),
        (
            "Sensitivity 1 dBFS\n20 0\n1000 0",
            CalibrationIssue::Header,
            Some(1),
        ),
        (
            "Sens Factor =1dB\nSens Factor =2dB\n20 0\n1000 0",
            CalibrationIssue::Header,
            Some(2),
        ),
        (
            "Sensitivity -20 dBFS\nSensitivity -10 dBFS\n20 0\n1000 0",
            CalibrationIssue::Header,
            Some(2),
        ),
        (
            "\"Sens Factor = 1dB, AGain = NaNdB\"\n20 0\n1000 0",
            CalibrationIssue::Header,
            Some(1),
        ),
        (
            "\"Sens Factor = 1dB, AGain broken\"\n20 0\n1000 0",
            CalibrationIssue::Header,
            Some(1),
        ),
        (
            "Sens Factor = 1dB, SERNO broken\n20 0\n1000 0",
            CalibrationIssue::Header,
            Some(1),
        ),
        (
            "Sens Factor = 1dB garbage\n20 0\n1000 0",
            CalibrationIssue::Header,
            Some(1),
        ),
        (
            "Sens Factor = infdB\n20 0\n1000 0",
            CalibrationIssue::Header,
            Some(1),
        ),
        ("20 0 nope\n1000 0", CalibrationIssue::Row, Some(1)),
        ("heading\n20 0 NaN\n1000 0", CalibrationIssue::Row, Some(2)),
        ("20 0\n1000 inf", CalibrationIssue::Row, Some(2)),
        ("20 0\n1e999 0", CalibrationIssue::Row, Some(2)),
        ("20 0\n1000", CalibrationIssue::Row, Some(2)),
        ("20 0\n1000 0 0 0", CalibrationIssue::Row, Some(2)),
        ("20 0\n1000x 0", CalibrationIssue::Row, Some(2)),
        ("-heading\n20 0\n1000 0", CalibrationIssue::Row, Some(1)),
        ("0 0\n1000 0", CalibrationIssue::Order, Some(1)),
        ("-20 0\n1000 0", CalibrationIssue::Order, Some(1)),
        ("20 0\n20 1", CalibrationIssue::Order, Some(2)),
        ("1000 0\n20 1", CalibrationIssue::Order, Some(2)),
        ("20 0\n1000 201", CalibrationIssue::Range, Some(2)),
        ("20 -201\n1000 0", CalibrationIssue::Range, Some(1)),
        ("heading\n20 0", CalibrationIssue::TooFewPoints, None),
        ("", CalibrationIssue::TooFewPoints, None),
        (
            "Sens Factor = 1dB\nSensitivity -20 dBFS\n20 0\n1000 0",
            CalibrationIssue::BothConventions,
            None,
        ),
        (
            "Sensitivity -20 dBFS\nSens Factor = 1dB\n20 0\n1000 0",
            CalibrationIssue::BothConventions,
            None,
        ),
    ] {
        assert_eq!(
            MicCalibration::parse_checked(text).unwrap_err(),
            CalibrationError { issue, line },
            "{text}"
        );
        assert!(MicCalibration::parse(text).is_err(), "{text}");
    }
    let too_large = " ".repeat(4 * 1024 * 1024 + 1);
    assert_eq!(
        MicCalibration::parse_checked(&too_large).unwrap_err(),
        CalibrationError {
            issue: CalibrationIssue::TooLarge,
            line: None
        }
    );
    let prefix = "20 0\n1000 0\n#";
    let at_limit = format!("{prefix}{}", " ".repeat(4 * 1024 * 1024 - prefix.len()));
    assert!(MicCalibration::parse_checked(&at_limit).is_ok());
    assert!(matches!(
        MicCalibration::parse("heading\n20 broken"),
        Err(AudioError::UnsupportedFormat(message)) if message == "invalid calibration line 2"
    ));
}

#[test]
fn quoted_metadata_is_bounded_and_unknown_fields_are_ignored() {
    let note = "한".repeat(201);
    let serial = "가".repeat(65);
    let text = format!(
        "\"Sens Factor =-.125dB, ignored = anything, SERNO:  {serial}  , AGain = +18dB\"\n{}20 -200\n1000 200",
        format!("\"{note}\"\n").repeat(10)
    );
    let mic = MicCalibration::parse_checked(&text).unwrap();
    assert_eq!(mic.serial().unwrap(), "가".repeat(64));
    assert_eq!(mic.notes().len(), 8);
    assert!(mic.notes().iter().all(|s| s == &"한".repeat(200)));
    assert_eq!(mic.analog_gain_db(), Some(18.0));
    assert_eq!(mic.usb_sens_factor_db(), Some(-0.125));
    let mic = MicCalibration::parse("\"20 not a data row\"\n\"Sensitivity broken\"\n20 0\n1000 0")
        .unwrap();
    assert_eq!(mic.notes(), &["20 not a data row", "Sensitivity broken"]);
    assert_eq!(mic.sensitivity_dbfs_at_94(), None);
}

#[test]
fn from_points_validates_like_the_parser() {
    let parsed = MicCalibration::parse("20 -200 10\n200 0\n2000 200 -30").unwrap();
    let curve = MicCalibration::from_points(parsed.points().to_vec()).unwrap();
    assert_eq!(curve, parsed);
    assert!(curve.has_phase());
    assert_eq!(curve.serial(), None);
    assert_eq!(curve.analog_gain_db(), None);
    assert!(curve.notes().is_empty());
    assert_eq!(curve.usb_sens_factor_db(), None);
    assert_eq!(curve.sensitivity_dbfs_at_94(), None);
    for points in [vec![], vec![point(20.0, 0.0, None)]] {
        assert_eq!(
            MicCalibration::from_points(points).unwrap_err(),
            CalibrationError {
                issue: CalibrationIssue::TooFewPoints,
                line: None
            }
        );
    }
    for (bad, issue) in [
        (point(f64::NAN, 0.0, None), CalibrationIssue::Row),
        (point(f64::INFINITY, 0.0, None), CalibrationIssue::Row),
        (point(1000.0, f64::NAN, None), CalibrationIssue::Row),
        (
            point(1000.0, f64::NEG_INFINITY, None),
            CalibrationIssue::Row,
        ),
        (point(1000.0, 0.0, Some(f64::NAN)), CalibrationIssue::Row),
        (
            point(1000.0, 0.0, Some(f64::INFINITY)),
            CalibrationIssue::Row,
        ),
        (point(0.0, 0.0, None), CalibrationIssue::Order),
        (point(-1.0, 0.0, None), CalibrationIssue::Order),
        (point(20.0, 0.0, None), CalibrationIssue::Order),
        (point(10.0, 0.0, None), CalibrationIssue::Order),
        (point(1000.0, 200.01, None), CalibrationIssue::Range),
        (point(1000.0, -200.01, None), CalibrationIssue::Range),
    ] {
        assert_eq!(
            MicCalibration::from_points(vec![point(20.0, 0.0, None), bad]).unwrap_err(),
            CalibrationError {
                issue,
                line: Some(2)
            }
        );
    }
    assert_eq!(
        MicCalibration::from_points(vec![point(0.0, 0.0, None), point(1000.0, 0.0, None)])
            .unwrap_err(),
        CalibrationError {
            issue: CalibrationIssue::Order,
            line: Some(1)
        }
    );
}

#[test]
fn empty_correction_is_bit_exact() {
    let ir = [
        0.0,
        -0.0,
        0.25,
        -0.75,
        f32::MIN_POSITIVE,
        f32::from_bits(1),
        f32::MAX,
    ];
    for correction in [
        ResponseCorrection::default(),
        ResponseCorrection::new(None, vec![], false),
    ] {
        assert!(correction.is_empty());
        assert_eq!(correction.response(1000.0), (0.0, 0.0));
        let result = correction.apply(&ir, 48_000).unwrap();
        assert_eq!(
            result.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
            ir.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
        );
    }
}

#[test]
fn flat_curve_scales_the_ir() {
    let mut seed = 7031_u32;
    let ir: Vec<f32> = (0..1024)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (f64::from(seed) / f64::from(u32::MAX) - 0.5) as f32
        })
        .collect();
    for (gain_db, correction_db) in [
        (3.0, -3.0),
        (-40.0, MAX_CORRECTION_BOOST_DB),
        (200.0, -200.0),
    ] {
        let correction = ResponseCorrection::new(Some(flat_curve(gain_db, None)), vec![], false);
        assert!(!correction.is_empty());
        for length in [1, 2, 31, 1024] {
            let result = correction.apply(&ir[..length], 48_000).unwrap();
            assert_eq!(result.len(), length);
            for (&input, output) in ir.iter().zip(result) {
                let expected = f64::from(input) * 10_f64.powf(correction_db / 20.0);
                close(f64::from(output), expected, expected.abs() * 1e-6);
            }
        }
    }
}

#[test]
fn correction_matches_the_curve_on_bins() {
    let curve = MicCalibration::parse(UMIK_90).unwrap();
    let correction = ResponseCorrection::new(Some(curve.clone()), vec![], false);
    let mut ir = vec![0.0; 65_536];
    ir[1000] = 1.0;
    let result = correction.apply(&ir, 48_000).unwrap();
    assert_eq!(result.len(), ir.len());
    assert_eq!(
        result
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .unwrap()
            .0,
        1000
    );
    let output = spectrum(&result);
    for target in [31.5, 1000.0, 10_000.0, 16_000.0] {
        let bin = (target * ir.len() as f64 / 48_000.0).round() as usize;
        let frequency = bin as f64 * 48_000.0 / ir.len() as f64;
        close(
            20.0 * output[bin].norm().log10(),
            -curve.gain_db(frequency),
            0.05,
        );
    }
}

#[test]
fn phase_column_is_removed() {
    let correction = ResponseCorrection::new(Some(flat_curve(0.0, Some(90.0))), vec![], false);
    let mut ir = vec![0.0; 65_536];
    ir[1000] = 1.0;
    let result = correction.apply(&ir, 48_000).unwrap();
    let output = spectrum(&result);
    let input = spectrum(&ir);
    let ratio = output[4096] / input[4096];
    close(ratio.arg().to_degrees(), -90.0, 0.05);
    close(ratio.norm(), 1.0, 0.001);
    // A twice-padded single sample has only DC and Nyquist: both ignore phase.
    assert_eq!(correction.apply(&[1.0], 48_000).unwrap(), [1.0]);
}

#[test]
fn response_sums_curves_and_interpolates_phase() {
    let curve = MicCalibration::from_points(vec![
        point(20.0, 2.0, Some(10.0)),
        point(2000.0, 4.0, Some(90.0)),
    ])
    .unwrap();
    let correction = ResponseCorrection::new(Some(curve), vec![flat_curve(-1.0, None)], false);
    assert_eq!(correction.response(0.0), (1.0, 10.0));
    let (gain, phase) = correction.response(200.0);
    close(gain, 2.0, 1e-12);
    close(phase, 50.0, 1e-12);
    assert_eq!(correction.response(24_000.0), (3.0, 90.0));
    let mixed =
        MicCalibration::from_points(vec![point(20.0, 0.0, None), point(2000.0, 0.0, Some(90.0))])
            .unwrap();
    assert!(mixed.has_phase());
    let correction =
        ResponseCorrection::new(Some(mixed), vec![flat_curve(0.0, Some(-10.0))], false);
    close(correction.response(200.0).1, 35.0, 1e-12);
    let correction = ResponseCorrection::new(
        Some(MicCalibration::parse(UMIK_AGAIN).unwrap()),
        vec![],
        false,
    );
    assert_eq!(correction.response(1000.0), (0.0, 0.0));
    // Boost is limited after summing curves, not separately for each curve.
    let correction = ResponseCorrection::new(
        Some(flat_curve(-40.0, None)),
        vec![flat_curve(35.0, None)],
        false,
    );
    close(
        f64::from(correction.apply(&[1.0], 48_000).unwrap()[0]),
        10_f64.powf(5.0 / 20.0),
        1e-6,
    );
}

#[test]
fn c_weighting_matches_iec_table() {
    for (frequency, expected) in [
        (31.5, -3.0),
        (63.0, -0.8),
        (1000.0, 0.0),
        (8000.0, -3.0),
        (10_000.0, -4.4),
    ] {
        close(c_weighting_db(frequency), expected, 0.1);
    }
    close(c_weighting_db(1000.0), 0.0, 0.01);
    assert_eq!(c_weighting_db(0.0), f64::NEG_INFINITY);
    let correction = ResponseCorrection::new(None, vec![], true);
    assert!(!correction.is_empty());
    close(-correction.response(31.5).0, 3.0, 0.1);
    assert_eq!(correction.response(31.5).1, 0.0);
    let mut ir = vec![0.0; 65_536];
    ir[32_768] = 1.0;
    let result = correction.apply(&ir, 48_000).unwrap();
    let output = spectrum(&result);
    let bin = (31.5 * ir.len() as f64 / 48_000.0).round() as usize;
    close(20.0 * output[bin].norm().log10(), 3.0, 0.1);
    // With the meter's own file, C weighting is compensated only outside its
    // range (10 Hz-20 kHz here); inside, the file already holds the meter.
    let combined = ResponseCorrection::new(Some(flat_curve(2.0, Some(10.0))), vec![], true);
    close(combined.response(31.5).0, 2.0, 1e-12);
    assert_eq!(combined.response(31.5).1, 10.0);
    close(combined.response(5.0).0, c_weighting_db(5.0) + 2.0, 1e-12);
    close(
        combined.response(22_000.0).0,
        c_weighting_db(22_000.0) + 2.0,
        1e-12,
    );
    // A soundcard curve does not limit it.
    let soundcard = ResponseCorrection::new(None, vec![flat_curve(2.0, None)], true);
    close(
        soundcard.response(31.5).0,
        c_weighting_db(31.5) + 2.0,
        1e-12,
    );
    // At n=2 the inverse averages DC (capped +20 dB) and Nyquist.
    let nyquist_gain = 10_f64.powf((-c_weighting_db(24_000.0)).min(MAX_CORRECTION_BOOST_DB) / 20.0);
    close(
        f64::from(correction.apply(&[1.0], 48_000).unwrap()[0]),
        (10.0 + nyquist_gain) / 2.0,
        1e-6,
    );
}

#[test]
fn bad_inputs_error_without_panics() {
    for correction in [
        ResponseCorrection::default(),
        ResponseCorrection::new(Some(flat_curve(0.0, None)), vec![], false),
    ] {
        for (ir, rate) in [
            (&[][..], 48_000),
            (&[1.0][..], 0),
            (&[0.0, f32::NAN][..], 48_000),
            (&[f32::INFINITY][..], 48_000),
            (&[f32::NEG_INFINITY][..], 48_000),
        ] {
            assert!(
                std::panic::catch_unwind(|| correction.apply(ir, rate))
                    .unwrap()
                    .is_err()
            );
        }
    }
}

#[test]
#[ignore = "reads a vendor file named by SECS_REAL_MIC_CAL without copying it into the repository"]
fn real_umik1_file() {
    let Ok(path) = std::env::var("SECS_REAL_MIC_CAL") else {
        println!("real_umik1_file: skipped; SECS_REAL_MIC_CAL is not set");
        return;
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            println!("real_umik1_file: skipped; file missing: {path}");
            return;
        }
        Err(error) => panic!("could not read {path}: {error}"),
    };
    let mic = MicCalibration::parse_checked(&text).unwrap();
    println!(
        "real_umik1_file: serial={:?}, sens_factor={:?}, notes={:?}, point_count={}, gain_db(10000.0)={}",
        mic.serial(),
        mic.usb_sens_factor_db(),
        mic.notes(),
        mic.points().len(),
        mic.gain_db(10_000.0)
    );
    // A UMIK file carries its serial and USB sensitivity in the first line.
    assert!(mic.serial().is_some());
    assert!(mic.usb_sens_factor_db().is_some());
    assert!(mic.points().len() > 100);
}
