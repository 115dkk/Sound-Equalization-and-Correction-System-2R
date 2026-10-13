#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![cfg(not(miri))]

use secs_jos::{ArrayData, Node, Stream, Value};
use secs_mdat::{MeasurementInput, TEMPLATE, build, load, load_mdat_project};
use secs_oracle::{Fixture, number};
use serde_json::Value as Json;

const CACHES: [&str; 9] = [
    "splValues",
    "rawValues",
    "phaseValues",
    "unwPhaseValues",
    "rawUnwPhaseValues",
    "gdValues",
    "rawGDValues",
    "fdwReal",
    "fdwImag",
];

fn compare(fx: &Fixture, path: &str, got: &Json, expected: &Json) {
    if let Some(cases) = expected.get("cases") {
        let actual = got.as_array().unwrap();
        let cases = cases.as_array().unwrap();
        assert_eq!(actual.len(), cases.len(), "{path}");
        for (i, (actual, case)) in actual.iter().zip(cases).enumerate() {
            let child = Fixture::load("mdat", case.as_str().unwrap());
            compare(
                &child,
                &format!("{path}[{i}]"),
                actual,
                &child.outputs()["adapted"],
            );
        }
        return;
    }
    if Fixture::is_digest(expected) {
        let actual: Vec<f64> = got.as_array().unwrap().iter().map(number).collect();
        fx.assert_digest(path, expected, &actual);
        return;
    }
    if expected.get("file").is_some()
        || expected
            .as_array()
            .is_some_and(|v| v.first().is_some_and(|v| v.get("file").is_some()))
    {
        let expected = fx.f64s(expected);
        let actual = got.as_array().unwrap();
        assert_eq!(actual.len(), expected.len(), "{path}");
        for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert_eq!(
                number(actual).to_bits(),
                expected.to_bits(),
                "{path}[{index}]"
            );
        }
        return;
    }
    match (got, expected) {
        (Json::Object(a), Json::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}: field count");
            for (name, expected) in b {
                compare(fx, &format!("{path}.{name}"), &a[name], expected);
            }
        }
        (Json::Array(a), Json::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(fx, &format!("{path}[{i}]"), a, b);
            }
        }
        (Json::Number(a), Json::Number(b)) if a.is_f64() || b.is_f64() => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "{path}"
        ),
        _ => assert_eq!(got, expected, "{path}"),
    }
}

#[test]
fn template_schema_matches_every_field() {
    let fx = Fixture::load("mdat", "template");
    assert_eq!(fx.blob(&fx.inputs()["bytes"]), TEMPLATE);
    let model = load(TEMPLATE, &secs_jos::Limits::default()).unwrap();
    assert_eq!(
        model.unknown_top_level.len() as u64,
        fx.outputs()["unknown_top_level_count"].as_u64().unwrap()
    );
    compare(
        &fx,
        "template",
        &serde_json::to_value(&model).unwrap(),
        &fx.outputs()["adapted"],
    );
    assert_eq!(secs_jos::write(&model.stream), TEMPLATE);
}

fn inputs(fx: &Fixture) -> Vec<MeasurementInput> {
    fx.inputs()["measurements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| MeasurementInput {
            title: m["title"].as_str().unwrap().into(),
            notes: m["notes"].as_str().unwrap().into(),
            sample_rate_hz: m["sample_rate_hz"].as_u64().unwrap() as u32,
            samples: fx.f64s(&m["samples"]),
            start_time_s: number(&m["start_time_s"]),
            spl_offset_db: m["spl_offset_db"].as_f64(),
        })
        .collect()
}

fn measurement_ids(s: &Stream) -> Vec<usize> {
    s.values
        .iter()
        .filter_map(|v| match v {
            Value::Node(id) if s.class_name(*id) == Some("roomeqwizard.MeasData") => Some(*id),
            _ => None,
        })
        .collect()
}

fn array_id(s: &Stream, object: usize, name: &str) -> usize {
    match s.get(object, name).unwrap() {
        Value::Node(id) => *id,
        v => panic!("expected array reference: {v:?}"),
    }
}

fn floats(s: &Stream, id: usize) -> &[f32] {
    match &s.nodes[id] {
        Node::Array {
            data: ArrayData::Float(v),
            ..
        } => v,
        v => panic!("expected f32 array: {v:?}"),
    }
}

fn ordered(value: f32) -> u32 {
    let bits = value.to_bits();
    if bits & 0x8000_0000 != 0 {
        !bits
    } else {
        bits | 0x8000_0000
    }
}

#[test]
fn builds_match_bytes_except_nine_one_ulp_caches() {
    let mut failures = Vec::new();
    for fx in Fixture::all("mdat", "build_") {
        let input = inputs(&fx);
        let uuids: Vec<u128> = fx.inputs()["uuid"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().parse().unwrap())
            .collect();
        let mut consumed = 0;
        let actual = build(
            &input,
            TEMPLATE,
            fx.inputs()["spl_offset_db"].as_f64(),
            &mut || {
                let id = uuids[consumed];
                consumed += 1;
                id
            },
        )
        .unwrap();
        assert_eq!(consumed, input.len());
        let expected = fx.blob(&fx.outputs()["bytes"]);
        let mut a = secs_jos::parse(&actual, &secs_jos::Limits::default()).unwrap();
        let b = secs_jos::parse(&expected, &secs_jos::Limits::default()).unwrap();
        let aids = measurement_ids(&a);
        let bids = measurement_ids(&b);
        assert_eq!(aids.len(), bids.len());
        for (i, (&ai, &bi)) in aids.iter().zip(&bids).enumerate() {
            for field in CACHES {
                let aa = array_id(&a, ai, field);
                let ba = array_id(&b, bi, field);
                let av = floats(&a, aa);
                let bv = floats(&b, ba);
                assert_eq!(av.len(), bv.len());
                let mut differences = 0;
                let mut max_ulp = 0;
                let mut max_abs = 0.0_f64;
                for (&a, &b) in av.iter().zip(bv) {
                    assert!(a.is_finite() && b.is_finite());
                    differences += usize::from(a.to_bits() != b.to_bits());
                    max_ulp = max_ulp.max(ordered(a).abs_diff(ordered(b)));
                    max_abs = max_abs.max((f64::from(a) - f64::from(b)).abs());
                }
                println!(
                    "MEASURE {} measurement={i} {field} n={} differing={differences} max_ulp={max_ulp} max_abs={max_abs:e}",
                    fx.id,
                    av.len()
                );
                if max_ulp > 1 {
                    failures.push(format!(
                        "{} measurement={i} {field} max_ulp={max_ulp}",
                        fx.id
                    ));
                }
                let normalized = bv.to_vec();
                if let Node::Array { data, .. } = &mut a.nodes[aa] {
                    *data = ArrayData::Float(normalized);
                }
            }
        }
        let normalized = secs_jos::write(&a);
        let first_difference = normalized.iter().zip(&expected).position(|(a, b)| a != b);
        assert!(
            first_difference.is_none() && normalized.len() == expected.len(),
            "{}: non-cache bytes differ at {first_difference:?}, lengths {} vs {}",
            fx.id,
            normalized.len(),
            expected.len()
        );
    }
    assert!(
        failures.is_empty(),
        "cache budget exceeded: {}",
        failures.join("; ")
    );
}

#[test]
fn built_schema_matches_every_field() {
    for fx in Fixture::all("mdat", "build_") {
        let expected = fx.blob(&fx.outputs()["bytes"]);
        let model = load(&expected, &secs_jos::Limits::default()).unwrap();
        assert_eq!(
            model.unknown_top_level.len() as u64,
            fx.outputs()["unknown_top_level_count"].as_u64().unwrap()
        );
        compare(
            &fx,
            &fx.id,
            &serde_json::to_value(&model).unwrap(),
            &fx.outputs()["adapted"],
        );
    }
}

#[test]
fn built_ir_converts_back_in_float32_precision() {
    for fx in Fixture::all("mdat", "build_") {
        let input = inputs(&fx);
        let expected = fx.blob(&fx.outputs()["bytes"]);
        let project = load_mdat_project(&expected, &secs_jos::Limits::default()).unwrap();
        for (index, (info, input)) in project.measurements.iter().zip(input).enumerate() {
            assert_eq!(info.uuid, format!("mdat:{index}"));
            assert_eq!(info.title, input.title);
            assert_eq!(info.notes, input.notes);
            assert_eq!(info.sample_rate, i64::from(input.sample_rate_hz));
            assert_eq!(info.start_time_s.to_bits(), input.start_time_s.to_bits());
            let (samples, rate) = project.ir_provider.get_ir(&info.uuid).unwrap();
            assert_eq!(rate, i64::from(input.sample_rate_hz));
            assert_eq!(samples.len(), input.samples.len());
            for (&got, expected) in samples.iter().zip(input.samples) {
                assert_eq!(
                    got.to_bits(),
                    f64::from((expected * 0.01) as f32 / 0.01_f32).to_bits()
                );
            }
        }
    }
}
