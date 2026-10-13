use super::*;
use constants::*;
use std::collections::BTreeMap;

fn small_graph() -> Stream {
    let mut s = Stream::default();
    let signature = s.push(Node::String {
        value: "Ljava/lang/Object;".into(),
        long: false,
    });
    let d = s.push(Node::ClassDesc(ClassDescriptor {
        name: "Example".into(),
        flags: SC_SERIALIZABLE | SC_WRITE_METHOD,
        fields: vec![
            FieldDescriptor {
                typecode: 'I',
                name: "number".into(),
                type_string: None,
                type_string_node: None,
            },
            FieldDescriptor {
                typecode: 'L',
                name: "self".into(),
                type_string: Some("Ljava/lang/Object;".into()),
                type_string_node: Some(signature),
            },
        ],
        ..Default::default()
    }));
    let obj = s.push(Node::Object(JavaObject {
        classdesc: d,
        field_data: BTreeMap::from([(
            "Example".into(),
            BTreeMap::from([
                ("number".into(), Value::Int(42)),
                ("self".into(), Value::Null),
            ]),
        )]),
        annotations: BTreeMap::new(),
    }));
    s.set(obj, "self", Value::Node(obj));
    s.values = vec![Value::Node(obj), Value::Node(obj)];
    s
}
#[test]
fn roundtrip_identity_fields_and_clone() {
    let bytes = write(&small_graph());
    let mut s = parse(&bytes, &Limits::default()).unwrap();
    assert_eq!(bytes, write(&s));
    assert_eq!(s.values[0], s.values[1]);
    let Value::Node(obj) = s.values[0] else {
        panic!()
    };
    assert_eq!(s.get(obj, "self"), Some(&Value::Node(obj)));
    assert_eq!(s.field_names(obj), ["number", "self"]);
    assert!(!s.set(obj, "absent", Value::Null));
    let roots = s.values.clone();
    let cloned = s.clone_values(&roots);
    assert_eq!(cloned[0], cloned[1]);
    assert_ne!(cloned[0], roots[0]);
    let Value::Node(new) = cloned[0] else {
        panic!()
    };
    assert_eq!(s.get(new, "self"), Some(&Value::Node(new)));
    assert!(s.set(new, "number", Value::Int(-5)));
    assert_eq!(s.get(obj, "number"), Some(&Value::Int(42)));
    let (Node::Object(a), Node::Object(b)) = (&s.nodes[obj], &s.nodes[new]) else {
        panic!()
    };
    assert_ne!(a.classdesc, b.classdesc);
    let (Node::ClassDesc(a), Node::ClassDesc(b)) = (&s.nodes[a.classdesc], &s.nodes[b.classdesc])
    else {
        panic!()
    };
    assert_ne!(a.fields[1].type_string_node, b.fields[1].type_string_node);
    s.values = cloned;
    assert!(parse(&write(&s), &Limits::default()).is_ok());
}
#[test]
fn utf8_and_lone_surrogates() {
    for text in ["", "abc\0", "한글", "😀", "\u{7f}\u{80}\u{7ff}\u{800}"] {
        assert_eq!(
            decode_modified_utf8(&encode_modified_utf8(text)).unwrap(),
            text
        );
    }
    let raw = modified_utf8::encode_units([0xd800, 0x61, 0xdc00]);
    assert_eq!(
        modified_utf8::decode_units(&raw).unwrap(),
        [0xd800, 0x61, 0xdc00]
    );
    assert_eq!(decode_modified_utf8(&raw).unwrap(), "�a�");
    let mut bytes = vec![0xac, 0xed, 0, 5, TC_STRING, 0, raw.len() as u8];
    bytes.extend(raw);
    let mut s = parse(&bytes, &Limits::default()).unwrap();
    assert_eq!(write(&s), bytes);
    let roots = s.values.clone();
    s.values = s.clone_values(&roots);
    assert_eq!(write(&s), bytes);
}
#[test]
fn malformed_offsets_and_limits() {
    let err = parse(
        &[0xac, 0xed, 0, 5, TC_REFERENCE, 0, 0, 0, 1],
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "invalid or reset handle 0x00000001 at offset 0x4"
    );
    let bytes = write(&small_graph());
    for limits in [
        Limits {
            max_file_bytes: 0,
            ..Default::default()
        },
        Limits {
            max_depth: 0,
            ..Default::default()
        },
        Limits {
            max_handles: 0,
            ..Default::default()
        },
        Limits {
            max_string_bytes: 0,
            ..Default::default()
        },
        Limits {
            max_fields_per_class: 0,
            ..Default::default()
        },
        Limits {
            max_top_level_values: 0,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            parse(&bytes, &limits),
            Err(JosError::LimitExceeded { .. })
        ));
    }
    for end in 0..bytes.len() {
        let _ = parse(&bytes[..end], &Limits::default());
    }
}
#[test]
fn reset_and_exception_handle_epochs() {
    let bytes = [
        0xac,
        0xed,
        0,
        5,
        TC_STRING,
        0,
        1,
        b'a',
        TC_RESET,
        TC_REFERENCE,
        0,
        0x7e,
        0,
        0,
    ];
    assert!(matches!(
        parse(&bytes, &Limits::default()),
        Err(JosError::InvalidHandle { .. })
    ));
    let s = parse(&bytes[..9], &Limits::default()).unwrap();
    assert_eq!(s.handle_count, 1);
    assert!(try_write(&s).is_err());
    let bytes = [
        0xac,
        0xed,
        0,
        5,
        TC_EXCEPTION,
        TC_STRING,
        0,
        1,
        b'a',
        TC_STRING,
        0,
        1,
        b'b',
        TC_REFERENCE,
        0,
        0x7e,
        0,
        0,
    ];
    let s = parse(&bytes, &Limits::default()).unwrap();
    assert_eq!(write(&s), bytes);
    assert_eq!(s.handle_count, 2);
}
#[test]
fn deep_stream_uses_heap_stack() {
    let mut bytes = vec![0xac, 0xed, 0, 5];
    bytes.extend(std::iter::repeat_n(TC_EXCEPTION, 510));
    bytes.push(TC_NULL);
    let limits = Limits {
        max_depth: 512,
        ..Default::default()
    };
    let s = parse(&bytes, &limits).unwrap();
    assert_eq!(write(&s), bytes);
}
#[test]
fn all_primitive_arrays_and_fields() {
    let arrays = vec![
        ('B', ArrayData::Byte(vec![-128, 0, 127]), Value::Byte(-7)),
        (
            'C',
            ArrayData::Char(vec![0, 0xd800, 0xffff]),
            Value::Char(0xd800),
        ),
        (
            'D',
            ArrayData::Double(vec![-0.0, f64::INFINITY, 1.25]),
            Value::Double(-2.5),
        ),
        (
            'F',
            ArrayData::Float(vec![-0.0, f32::NEG_INFINITY, 1.25]),
            Value::Float(2.5),
        ),
        (
            'I',
            ArrayData::Int(vec![i32::MIN, 0, i32::MAX]),
            Value::Int(-100),
        ),
        (
            'J',
            ArrayData::Long(vec![i64::MIN, 0, i64::MAX]),
            Value::Long(-200),
        ),
        (
            'S',
            ArrayData::Short(vec![i16::MIN, 0, i16::MAX]),
            Value::Short(-300),
        ),
        ('Z', ArrayData::Bool(vec![false, true]), Value::Bool(true)),
    ];
    let mut s = Stream::default();
    for (code, data, value) in arrays {
        let classdesc = s.push(Node::ClassDesc(ClassDescriptor {
            name: format!("[{code}"),
            flags: SC_SERIALIZABLE,
            ..Default::default()
        }));
        let id = s.push(Node::Array { classdesc, data });
        s.values.push(Value::Node(id));
        let classdesc = s.push(Node::ClassDesc(ClassDescriptor {
            name: format!("Field{code}"),
            flags: SC_SERIALIZABLE,
            fields: vec![FieldDescriptor {
                typecode: code,
                name: "value".into(),
                type_string: None,
                type_string_node: None,
            }],
            ..Default::default()
        }));
        let id = s.push(Node::Object(JavaObject {
            classdesc,
            field_data: BTreeMap::from([(
                format!("Field{code}"),
                BTreeMap::from([("value".into(), value)]),
            )]),
            annotations: BTreeMap::new(),
        }));
        s.values.push(Value::Node(id));
    }
    let bytes = write(&s);
    let parsed = parse(&bytes, &Limits::default()).unwrap();
    assert_eq!(write(&parsed), bytes);
}
#[test]
fn proxy_enum_class_and_externalizable_annotations() {
    let mut s = Stream::default();
    let block = s.push(Node::BlockData {
        data: vec![1, 2, 3],
        long: true,
    });
    let d = s.push(Node::ClassDesc(ClassDescriptor {
        name: "<proxy>".into(),
        flags: SC_SERIALIZABLE,
        is_proxy: true,
        interfaces: vec!["SomeInterface".into()],
        annotations: vec![Value::Node(block)],
        ..Default::default()
    }));
    let c = s.push(Node::Class { classdesc: d });
    s.values.push(Value::Node(c));
    let text = s.push(Node::String {
        value: "CONSTANT".into(),
        long: true,
    });
    let e = s.push(Node::Enum {
        classdesc: d,
        constant: Value::Node(text),
    });
    s.values.push(Value::Node(e));
    let d = s.push(Node::ClassDesc(ClassDescriptor {
        name: "External".into(),
        flags: SC_EXTERNALIZABLE | SC_BLOCK_DATA,
        ..Default::default()
    }));
    let o = s.push(Node::Object(JavaObject {
        classdesc: d,
        field_data: BTreeMap::new(),
        annotations: BTreeMap::from([(
            "External".into(),
            vec![Value::Node(block), Value::Node(text)],
        )]),
    }));
    s.values.push(Value::Node(o));
    let bytes = write(&s);
    assert_eq!(write(&parse(&bytes, &Limits::default()).unwrap()), bytes);
    let Node::ClassDesc(d) = &mut s.nodes[d] else {
        panic!()
    };
    d.flags = SC_EXTERNALIZABLE;
    assert!(matches!(try_write(&s), Err(JosError::Unsupported { .. })));
}
#[test]
fn signature_reference_amplification_is_bounded() {
    let mut s = Stream::default();
    let text = "X".repeat(1024);
    let signature = s.push(Node::String {
        value: text.clone(),
        long: false,
    });
    let d = s.push(Node::ClassDesc(ClassDescriptor {
        name: "C".into(),
        fields: (0..100)
            .map(|i| FieldDescriptor {
                typecode: 'L',
                name: format!("f{i}"),
                type_string: Some(text.clone()),
                type_string_node: Some(signature),
            })
            .collect(),
        ..Default::default()
    }));
    s.values.push(Value::Node(d));
    let bytes = write(&s);
    assert!(bytes.len() < 4096);
    let limits = Limits {
        max_file_bytes: 32768,
        ..Default::default()
    };
    let error = parse(&bytes, &limits).unwrap_err();
    assert!(matches!(error, JosError::LimitExceeded { .. }));
    assert_eq!(error.message(), "decoded allocation limit exceeded");
}
#[test]
#[cfg_attr(miri, ignore)]
fn deterministic_3000_mutations() {
    let original = include_bytes!("../../../src/secs/rew_mdat/template_1.mdat");
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
    assert!(parse(original, &limits).is_ok());
    let mut state = 0x42f01377_u32;
    for iteration in 0..3000 {
        let mut bytes = original.to_vec();
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
