//! Any bytes as the template: building a measurement returns Ok or Err
//! without panicking (the app only ever passes the bundled template, but the
//! builder parses it like any other untrusted stream). One measurement carries
//! a source environment, so scrubbing and the boxed-value and calibration-array
//! descriptors run against whatever graph the bytes make.
#![no_main]

use libfuzzer_sys::fuzz_target;
use secs_mdat::{Boxed, CalInfo, MeasurementInput, SourceDate, SourceInfo, build};

fuzz_target!(|data: &[u8]| {
    let plain = MeasurementInput {
        title: "fuzz".into(),
        notes: String::new(),
        sample_rate_hz: 48000,
        samples: vec![0.0, 1.0, 0.5, -0.25],
        start_time_s: 0.0,
        spl_offset_db: None,
        source_info: None,
        computed: false,
    };
    let source = SourceInfo {
        strings: vec![("outputName".into(), "out".into())],
        boxed: vec![
            ("micSensitivity".into(), Boxed::Double(-1.5)),
            ("usesEmbeddedCalData".into(), Boxed::Boolean(true)),
        ],
        cal: vec![(
            "meterCal".into(),
            CalInfo {
                inverse_c_selected: false,
                source_name: "mic.txt".into(),
                source_path: "/cal/mic.txt".into(),
                freq: Some(vec![10.0, 100.0]),
                gain: Some(vec![-1.0, 0.0]),
                phase: None,
                boxed: vec![("serialNum".into(), Boxed::Integer(7))],
            },
        )],
        date: Some(SourceDate::Int(1)),
        notes: Some("note".into()),
        format: None,
    };
    let measured = MeasurementInput {
        source_info: Some(source),
        ..plain.clone()
    };
    let _ = build(&[plain, measured], data, None, 1, &mut || 0x0123_4567_89ab_cdef);
});
