#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Small, filesystem-free regressions for adversarial Java stream graphs.

use secs_jos::{JosError, Limits, parse};

fn header() -> Vec<u8> {
    vec![0xac, 0xed, 0, 5]
}

fn descriptor(bytes: &mut Vec<u8>, name: &[u8], flags: u8) {
    bytes.push(0x72);
    bytes.extend_from_slice(&(name.len() as u16).to_be_bytes());
    bytes.extend_from_slice(name);
    bytes.extend_from_slice(&0_i64.to_be_bytes());
    bytes.push(flags);
    bytes.extend_from_slice(&0_u16.to_be_bytes());
    bytes.push(0x78);
}

#[test]
fn cyclic_superclass_cannot_hang_object_loading() {
    let mut bytes = header();
    bytes.push(0x73);
    descriptor(&mut bytes, b"Cycle", 2);
    bytes.extend_from_slice(&[0x71, 0, 0x7e, 0, 0]);
    assert!(parse(&bytes, &Limits::default()).is_err());
}

#[test]
fn class_hierarchy_obeys_depth_limit() {
    let mut bytes = header();
    bytes.push(0x73);
    for _ in 0..32 {
        descriptor(&mut bytes, b"Nested", 2);
    }
    bytes.push(0x70);
    let limits = Limits {
        max_depth: 8,
        ..Limits::default()
    };
    assert!(matches!(
        parse(&bytes, &limits),
        Err(JosError::LimitExceeded { .. })
    ));
}

#[test]
fn declared_array_sizes_do_not_allocate_before_truncation_checks() {
    for name in [b"[D".as_slice(), b"[Ljava/lang/Object;"] {
        let mut bytes = header();
        bytes.push(0x75);
        descriptor(&mut bytes, name, 2);
        bytes.push(0x70);
        bytes.extend_from_slice(&49_999_999_i32.to_be_bytes());
        assert!(parse(&bytes, &Limits::default()).is_err());
    }
}

#[test]
fn negative_and_oversized_lengths_are_errors() {
    for count in [i64::MIN, -1, i64::MAX] {
        let mut bytes = header();
        bytes.push(0x7c);
        bytes.extend_from_slice(&count.to_be_bytes());
        assert!(parse(&bytes, &Limits::default()).is_err());
    }
    for count in [i32::MIN, -1, i32::MAX] {
        let mut bytes = header();
        bytes.push(0x7a);
        bytes.extend_from_slice(&count.to_be_bytes());
        assert!(parse(&bytes, &Limits::default()).is_err());
    }
}

#[test]
fn truncated_array_descriptor_name_does_not_panic() {
    let mut bytes = header();
    bytes.push(0x75);
    descriptor(&mut bytes, b"[", 2);
    bytes.push(0x70);
    bytes.extend_from_slice(&1_i32.to_be_bytes());
    assert!(parse(&bytes, &Limits::default()).is_err());
}

#[test]
fn short_block_data_preserves_upstream_limit_exemption() {
    let mut bytes = header();
    bytes.extend_from_slice(&[0x77, 4, 0, 0, 0, 0]);
    let limits = Limits {
        max_array_bytes: 3,
        ..Limits::default()
    };
    // The upstream only applies max_array_bytes to long blocks; short blocks
    // have their own 255-byte wire bound. Keep its error offsets unchanged.
    assert!(parse(&bytes, &limits).is_ok());
}

#[test]
fn shared_signature_copies_obey_aggregate_allocation_budget() {
    let mut bytes = header();
    bytes.extend_from_slice(&[0x72, 0, 1, b'C']);
    bytes.extend_from_slice(&0_i64.to_be_bytes());
    bytes.push(2);
    bytes.extend_from_slice(&100_u16.to_be_bytes());
    for i in 0..100_u8 {
        bytes.extend_from_slice(&[b'L', 0, 1, b'A' + i % 26]);
        if i == 0 {
            bytes.push(0x74);
            bytes.extend_from_slice(&1024_u16.to_be_bytes());
            bytes.extend(std::iter::repeat_n(b'a', 1024));
        } else {
            // Descriptor is handle 0; the field signature is handle 1.
            bytes.extend_from_slice(&[0x71, 0, 0x7e, 0, 1]);
        }
    }
    bytes.extend_from_slice(&[0x78, 0x70]);
    let limits = Limits {
        max_file_bytes: 65536,
        ..Limits::default()
    };
    assert!(bytes.len() < limits.max_file_bytes);
    assert!(matches!(
        parse(&bytes, &limits),
        Err(JosError::LimitExceeded { .. })
    ));
}

#[test]
fn each_modified_utf8_failure_is_a_stream_error() {
    for raw in [
        &[0][..],
        &[0xc0],
        &[0xc0, 0x7f],
        &[0xc1, 0x81],
        &[0xe0, 0x80],
        &[0xe0, 0x80, 0x80],
        &[0xf0, 0x90, 0x80, 0x80],
    ] {
        let mut bytes = header();
        bytes.push(0x74);
        bytes.extend_from_slice(&(raw.len() as u16).to_be_bytes());
        bytes.extend_from_slice(raw);
        assert!(matches!(
            parse(&bytes, &Limits::default()),
            Err(JosError::Format { .. })
        ));
    }
}
