#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! In-memory schema probes, separate from the upstream golden comparisons.

use secs_jos::{ArrayData, Limits, Node, Stream, Value};
use secs_mdat::{CalInfo, MdatError, SourceInfo, TEMPLATE, load, load_mdat_project};

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
#[cfg_attr(
    miri,
    ignore = "parses the 218 kB template; too slow under Miri, runs natively"
)]
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
#[cfg_attr(
    miri,
    ignore = "parses the 218 kB template; too slow under Miri, runs natively"
)]
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
#[cfg_attr(
    miri,
    ignore = "parses the 218 kB template; too slow under Miri, runs natively"
)]
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
#[cfg_attr(
    miri,
    ignore = "parses the 218 kB template; too slow under Miri, runs natively"
)]
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
        source_info: None,
        computed: false,
    };
    assert!(secs_mdat::build(&[input], &bytes, None, 0, &mut || 0).is_err());
}

#[test]
#[cfg_attr(
    miri,
    ignore = "builds a 512 MB calibration array against the template; too slow under Miri, runs natively"
)]
fn source_info_allocation_is_bounded() {
    let count = secs_jos::Limits::default().max_file_bytes / 4;
    let source_info = SourceInfo {
        strings: vec![("outputName".into(), "x".repeat(1_000_000))],
        cal: vec![(
            "meterCal".into(),
            CalInfo {
                inverse_c_selected: false,
                source_name: "mic.cal".into(),
                source_path: "/cal/mic.cal".into(),
                freq: Some(vec![1.0; count]),
                gain: None,
                phase: None,
                boxed: Vec::new(),
            },
        )],
        ..SourceInfo::default()
    };
    let input = secs_mdat::MeasurementInput {
        title: "IR".into(),
        notes: String::new(),
        sample_rate_hz: 48000,
        samples: vec![1.0],
        start_time_s: 0.0,
        spl_offset_db: None,
        source_info: Some(source_info),
        computed: false,
    };
    assert!(matches!(
        secs_mdat::build(&[input], TEMPLATE, None, 0, &mut || 0),
        Err(MdatError::Input(ref message)) if message == "IR samples exceed allocation limit"
    ));
}

#[test]
#[cfg_attr(
    miri,
    ignore = "parses the template and an 8 MiB calibration array; too slow under Miri, runs natively"
)]
fn shared_calibration_array_copies_obey_the_parse_budget_td18() {
    let (mut s, id) = template_graph();
    let float_desc = match s.get(id, "splValues") {
        Some(Value::Node(array)) => match &s.nodes[*array] {
            Node::Array { classdesc, .. } => *classdesc,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    // 8 MiB of float32 calibration data, stored once in the file.
    let count = 2 * 1024 * 1024;
    let array = s.push(Node::Array {
        classdesc: float_desc,
        data: ArrayData::Float(vec![0.5; count]),
    });
    let cal = match s.get(id, "meterCal") {
        Some(Value::Node(cal)) => *cal,
        _ => unreachable!(),
    };
    assert!(s.set(cal, "freqArray", Value::Node(array)));

    // A second reference to the measurement loads normally, with its own copy.
    s.values.push(Value::Node(id));
    let project = load_mdat_project(&secs_jos::write(&s), &Limits::default()).unwrap();
    assert_eq!(project.measurements.len(), 2);
    for measurement in &project.measurements {
        let (name, cal) = &measurement.source_info.cal[0];
        assert_eq!(name, "meterCal");
        assert_eq!(cal.freq.as_deref(), Some(&vec![0.5; count][..]));
    }
    drop(project);

    // 200 references in a file of about 8 MiB would copy the array to 1.6 GiB.
    s.values.extend(std::iter::repeat_n(Value::Node(id), 198));
    let bytes = secs_jos::write(&s);
    assert!(bytes.len() < 9 * 1024 * 1024, "{} bytes", bytes.len());
    match load_mdat_project(&bytes, &Limits::default()) {
        Err(MdatError::Schema(message)) => {
            assert_eq!(message, "adapted data exceeds allocation limit")
        }
        Err(other) => panic!("unexpected error: {other}"),
        Ok(project) => panic!(
            "{} measurements copied the shared array past the parse budget",
            project.measurements.len()
        ),
    }
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

#[test]
#[cfg_attr(
    miri,
    ignore = "parses the template several times with a 1M-sample IR; too slow under Miri, runs natively"
)]
fn a_valid_file_whose_adaptation_fills_most_of_the_budget_still_extracts_td18() {
    // Each IR is referenced once. Extraction copies what adaptation already
    // holds, so it must not be squeezed into what adaptation left over: a file
    // that adapts within the budget also extracts (audit #92 TD-18).
    let (mut s, id) = template_graph();
    let data = (|| {
        let Some(Value::Node(ir_data)) = s.get(id, "irData") else {
            return None;
        };
        let Some(Value::Node(ir)) = s.get(*ir_data, "ir") else {
            return None;
        };
        let Some(Value::Node(data)) = s.get(*ir, "data") else {
            return None;
        };
        Some(*data)
    })()
    .expect("template IR data array");
    let Node::Array { data: samples, .. } = &mut s.nodes[data] else {
        unreachable!()
    };
    *samples = ArrayData::Float(vec![0.25; 1 << 20]);
    let bytes = secs_jos::write(&s);
    let limits = |max_file_bytes| Limits {
        max_file_bytes,
        ..Limits::default()
    };
    // The smallest budget in which the file parses and adapts.
    let (mut low, mut high) = (bytes.len(), 256 * 1024 * 1024);
    while low < high {
        let mid = low + (high - low) / 2;
        if load(&bytes, &limits(mid)).is_ok() {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    let budget = limits(low + low / 2);
    let project =
        load_mdat_project(&bytes, &budget).expect("a file that adapts within the budget extracts");
    assert_eq!(project.measurements.len(), 1);
}
