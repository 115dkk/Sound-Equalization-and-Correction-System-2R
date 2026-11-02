//! Any bytes: loading a REW .mdat returns Ok or Err without panicking.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = secs_mdat::load(data, &secs_jos::Limits::default());

    let mut project_limits = secs_jos::Limits::default();
    project_limits.max_file_bytes = 64 * 1024;
    let _ = secs_mdat::load_mdat_project(data, &project_limits);
});
