//! Any bytes as the template: building a measurement returns Ok or Err
//! without panicking (the app only ever passes the bundled template, but the
//! builder parses it like any other untrusted stream).
#![no_main]

use libfuzzer_sys::fuzz_target;
use secs_mdat::{MeasurementInput, build};

fuzz_target!(|data: &[u8]| {
    let input = MeasurementInput {
        title: "fuzz".into(),
        notes: String::new(),
        sample_rate_hz: 48000,
        samples: vec![0.0, 1.0, 0.5, -0.25],
        start_time_s: 0.0,
        spl_offset_db: None,
    };
    let _ = build(&[input], data, None, &mut || 0x0123_4567_89ab_cdef);
});
