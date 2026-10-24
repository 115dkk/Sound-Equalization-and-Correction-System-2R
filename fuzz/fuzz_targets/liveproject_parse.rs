//! Any bytes: reading a Dirac Live project returns Ok or Err without panicking.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = secs_dirac::parse(data, &secs_dirac::Limits::default());
});
