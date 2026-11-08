#![forbid(unsafe_code)]

use secs_jos::{Limits, parse};
use secs_oracle::Fixture;

#[test]
#[cfg_attr(miri, ignore)]
fn deterministic_3000_mutations() {
    let fixture = Fixture::load("mdat", "template");
    let original = fixture.blob(&fixture.inputs()["bytes"]);
    let limits = Limits {
        max_file_bytes: 4 * 1024 * 1024,
        max_array_bytes: 1024 * 1024,
        max_array_elements: 262144,
        max_string_bytes: 65536,
        max_depth: 64,
        max_handles: 10000,
        max_fields_per_class: 512,
        max_top_level_values: 256,
    };
    assert!(parse(&original, &limits).is_ok());
    let mut state = 0x42f01377_u32;
    for iteration in 0..3000 {
        let mut bytes = original.clone();
        for _ in 0..3 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let index = state as usize % bytes.len();
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            match iteration % 3 {
                0 => bytes[index] = (state >> 24) as u8,
                1 => {
                    bytes.truncate(index);
                    break;
                }
                _ => bytes.insert(index, (state >> 24) as u8),
            }
        }
        let _ = parse(&bytes, &limits);
    }
}
