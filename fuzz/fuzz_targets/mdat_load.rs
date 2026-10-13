//! Any bytes: loading a REW .mdat returns Ok or Err without panicking.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = secs_mdat::load(data, &secs_jos::Limits::default());
});
