#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! In-memory schema probes, separate from the upstream golden comparisons.

use secs_jos::{ArrayData, Limits, Node, Stream, Value};
use secs_mdat::{MdatError, TEMPLATE, load};

fn template_graph() -> (Stream, usize) {
    let s = secs_jos::parse(TEMPLATE, &Limits::default()).unwrap();
    let id = s
        .values
        .iter()
        .find_map(|v| match v {
            Value::Node(id) if s.class_name(*id) == Some("roomeqwizard.MeasData") => Some(*id),
            _ => None,
        })
        .unwrap();
    (s, id)
}

#[test]
fn missing_required_fields_are_schema_errors() {
    let (mut s, id) = template_graph();
    assert!(s.set(id, "splValues", Value::Null));
    assert!(s.set(id, "irData", Value::Null));
    let error = load(&secs_jos::write(&s), &Limits::default()).unwrap_err();
    assert!(
        matches!(error, MdatError::Schema(ref message) if message == "MeasData is missing required fields: splValues, irData")
    );
}

#[test]
fn cyclic_boxed_enum_is_rejected_without_recursing() {
    let (mut s, id) = template_graph();
    let desc = match &s.nodes[id] {
        Node::Object(o) => o.classdesc,
        _ => unreachable!(),
    };
    let cyclic = s.push(Node::Enum {
        classdesc: desc,
        constant: Value::Null,
    });
    s.nodes[cyclic] = Node::Enum {
        classdesc: desc,
        constant: Value::Node(cyclic),
    };
    assert!(s.set(id, "rewVersion", Value::Node(cyclic)));
    assert!(matches!(
        load(&secs_jos::write(&s), &Limits::default()),
        Err(MdatError::Schema(_))
    ));
}

#[test]
fn signed_byte_arrays_keep_their_signed_values() {
    let (mut s, id) = template_graph();
    let array = match s.get(id, "splValues").unwrap() {
        Value::Node(id) => *id,
        _ => unreachable!(),
    };
    let old_desc = match &s.nodes[array] {
        Node::Array { classdesc, .. } => *classdesc,
        _ => unreachable!(),
    };
    let mut desc = match &s.nodes[old_desc] {
        Node::ClassDesc(desc) => desc.clone(),
        _ => unreachable!(),
    };
    desc.name = "[B".into();
    let new_desc = s.push(Node::ClassDesc(desc));
    s.nodes[array] = Node::Array {
        classdesc: new_desc,
        data: ArrayData::Byte(vec![-128, -1, 0, 127]),
    };
    let file = load(&secs_jos::write(&s), &Limits::default()).unwrap();
    assert_eq!(
        file.measurements[0].frequency_response.spl_db,
        [-128.0, -1.0, 0.0, 127.0]
    );
}

#[test]
fn unsupported_template_sentinel_is_a_build_error_not_a_panic() {
    let (mut s, _) = template_graph();
    s.values[0] = Value::Null;
    let mut bytes = secs_jos::write(&s);
    // TC_ENDBLOCKDATA parses as a top-level sentinel but cannot be written by
    // the upstream writer. Keeping its width avoids disturbing handle IDs.
    bytes[4] = 0x78;
    let input = secs_mdat::MeasurementInput {
        title: "IR".into(),
        notes: String::new(),
        sample_rate_hz: 48000,
        samples: vec![1.0],
        start_time_s: 0.0,
        spl_offset_db: None,
    };
    assert!(secs_mdat::build(&[input], &bytes, None, &mut || 0).is_err());
}

#[test]
fn valid_java_stream_without_mdat_header_is_schema_error() {
    for bytes in [
        vec![0xac, 0xed, 0, 5],
        vec![0xac, 0xed, 0, 5, 0x70, 0x70, 0x70, 0x70],
    ] {
        assert!(matches!(
            load(&bytes, &Limits::default()),
            Err(MdatError::Schema(_))
        ));
    }
}
