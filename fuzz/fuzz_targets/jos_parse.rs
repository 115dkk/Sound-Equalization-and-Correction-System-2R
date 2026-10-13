//! Any bytes: `parse` returns Ok or Err without panicking, and a stream it
//! accepts and the writer can serialize parses back to the same bytes.
#![no_main]

use libfuzzer_sys::fuzz_target;
use secs_jos::{Limits, parse, try_write};

fuzz_target!(|data: &[u8]| {
    let limits = Limits::default();
    let Ok(stream) = parse(data, &limits) else {
        return;
    };
    let Ok(bytes) = try_write(&stream) else {
        return;
    };
    let again = parse(&bytes, &limits).expect("a written stream parses");
    let rewritten = try_write(&again).expect("a reparsed stream writes");
    assert_eq!(rewritten, bytes, "write(parse(write(s))) == write(s)");
});
