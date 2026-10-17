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

fn boxed(value: &Json) -> secs_mdat::Boxed {
    match value["type"].as_str().unwrap() {
        "java.lang.Double" => secs_mdat::Boxed::Double(number(&value["value"])),
        "java.lang.Integer" => secs_mdat::Boxed::Integer(value["value"].as_i64().unwrap()),
        "java.lang.Boolean" => secs_mdat::Boxed::Boolean(value["value"].as_bool().unwrap()),
        kind => panic!("unknown boxed kind {kind}"),
    }
}

fn pairs<T>(value: &Json, mut convert: impl FnMut(&Json) -> T) -> Vec<(String, T)> {
    value
        .as_object()
        .into_iter()
        .flat_map(|fields| fields.iter())
        .map(|(name, value)| (name.clone(), convert(value)))
        .collect()
}

fn fixture_f32s(value: Option<&Json>) -> Option<Vec<f32>> {
    let values = value?.as_array()?;
    Some(
        values
            .iter()
            .map(|value| match value {
                Json::String(value) if value == "nan" => f32::NAN,
                Json::String(value) if value == "+inf" => f32::INFINITY,
                Json::String(value) if value == "-inf" => f32::NEG_INFINITY,
                value => number(value) as f32,
            })
            .collect(),
    )
}

fn source_info(value: &Json) -> Option<secs_mdat::SourceInfo> {
    let fields = value.as_object()?;
    Some(secs_mdat::SourceInfo {
        strings: pairs(&value["strings"], |value| value.as_str().unwrap().into()),
        boxed: pairs(&value["boxed"], boxed),
        cal: pairs(&value["cal"], |value| secs_mdat::CalInfo {
            inverse_c_selected: value["inverseCSelected"].as_bool().unwrap_or(false),
            source_name: value["sourceName"].as_str().unwrap_or_default().into(),
            source_path: value["sourcePath"].as_str().unwrap_or_default().into(),
            freq: fixture_f32s(value.get("freqArray")),
            gain: fixture_f32s(value.get("gainArray")),
            phase: fixture_f32s(value.get("phaseArray")),
            boxed: value
                .as_object()
                .into_iter()
                .flat_map(|fields| fields.iter())
                .filter(|(name, _)| {
                    ["sensitivitydB", "sensitivitydBFS", "serialNum", "revision"]
                        .contains(&name.as_str())
                })
                .map(|(name, value)| (name.clone(), boxed(value)))
                .collect(),
        }),
        date: fields.get("date").and_then(|value| {
            value
                .as_i64()
                .map(secs_mdat::SourceDate::Int)
                .or_else(|| value.as_f64().map(secs_mdat::SourceDate::Float))
        }),
        notes: fields.get("notes").and_then(Json::as_str).map(Into::into),
        format: fields.get("format").and_then(Json::as_str).map(Into::into),
    })
}

fn assert_f32s(actual: &Option<Vec<f32>>, expected: &Option<Vec<f32>>) {
    match (actual, expected) {
        (Some(actual), Some(expected)) => {
            assert_eq!(actual.len(), expected.len());
            for (actual, expected) in actual.iter().zip(expected) {
                assert_eq!(actual.to_bits(), expected.to_bits());
            }
        }
        (None, None) => {}
        _ => panic!("array presence differs"),
    }
}

fn assert_source_eq(actual: &secs_mdat::SourceInfo, expected: &secs_mdat::SourceInfo) {
    assert_eq!(actual.strings, expected.strings);
    assert_eq!(actual.boxed, expected.boxed);
    assert_eq!(actual.date, expected.date);
    assert_eq!(actual.notes, expected.notes);
    assert_eq!(actual.format, expected.format);
    assert_eq!(actual.cal.len(), expected.cal.len());
    for ((actual_name, actual), (expected_name, expected)) in actual.cal.iter().zip(&expected.cal) {
        assert_eq!(actual_name, expected_name);
        assert_eq!(actual.inverse_c_selected, expected.inverse_c_selected);
        assert_eq!(actual.source_name, expected.source_name);
        assert_eq!(actual.source_path, expected.source_path);
        assert_f32s(&actual.freq, &expected.freq);
        assert_f32s(&actual.gain, &expected.gain);
        assert_f32s(&actual.phase, &expected.phase);
        assert_eq!(actual.boxed, expected.boxed);
    }
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
            source_info: source_info(&m["source_info"]),
            computed: m["computed"].as_bool().unwrap_or(false),
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
    for fx in Fixture::all("mdat", "build_")
        .into_iter()
        .chain([Fixture::load("mdat", "source_info_build")])
    {
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
            fx.inputs()["now_ms"].as_i64().unwrap(),
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
fn source_info_extracts_and_round_trips_oracle_values() {
    let fixture = Fixture::load("mdat", "source_info_extract");
    let bytes = fixture.blob(&fixture.inputs()["bytes"]);
    let project = load_mdat_project(&bytes, &secs_jos::Limits::default()).unwrap();
    let expected = fixture.outputs()["source_info"].as_array().unwrap();
    assert_eq!(project.measurements.len(), expected.len());
    for (actual, expected) in project.measurements.iter().zip(expected) {
        let expected = source_info(expected).unwrap();
        assert_source_eq(&actual.source_info, &expected);
    }

    let round_trip = Fixture::load("mdat", "source_info_round_trip");
    let bytes = round_trip.blob(&round_trip.outputs()["bytes"]);
    let project = load_mdat_project(&bytes, &secs_jos::Limits::default()).unwrap();
    for (actual, expected) in project
        .measurements
        .iter()
        .zip(round_trip.outputs()["source_info"].as_array().unwrap())
    {
        let expected = source_info(expected).unwrap();
        assert_source_eq(&actual.source_info, &expected);
    }
}

#[test]
fn source_info_from_rew_measurement_matches_oracle() {
    for fixture in Fixture::all("mdat", "source_info_from_") {
        let input = fixture.inputs();
        let cal = (!input["path"].as_str().unwrap().is_empty()).then(|| secs_mdat::CalFile {
            path: input["path"].as_str().unwrap().into(),
            text: input["text"]
                .as_object()
                .map(|descriptor| fixture.blob(&Json::Object(descriptor.clone()))),
        });
        let result = secs_mdat::source_info::from_rew_measurement(
            cal.as_ref(),
            input["output_device"].as_str().unwrap(),
            input["input_device"].as_str().unwrap(),
            input["output_channel"].as_str().unwrap(),
            input["now_ms"].as_i64().unwrap(),
        );
        if let Some(expected) = fixture.outputs().get("source_info") {
            let expected = source_info(expected);
            let actual = result.unwrap();
            match (actual, expected) {
                (Some(actual), Some(expected)) => assert_source_eq(&actual, &expected),
                (None, None) => {}
                values => panic!("{} source presence differs: {values:?}", fixture.id),
            }
        } else {
            assert!(result.is_err(), "{} should be an error", fixture.id);
        }
    }
}

#[test]
fn built_schema_matches_every_field() {
    for fx in Fixture::all("mdat", "build_")
        .into_iter()
        .chain([Fixture::load("mdat", "source_info_build")])
    {
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
    for fx in Fixture::all("mdat", "build_")
        .into_iter()
        .chain([Fixture::load("mdat", "source_info_build")])
    {
        let input = inputs(&fx);
        let expected = fx.blob(&fx.outputs()["bytes"]);
        let project = load_mdat_project(&expected, &secs_jos::Limits::default()).unwrap();
        for (index, (info, input)) in project.measurements.iter().zip(input).enumerate() {
            assert_eq!(info.uuid, format!("mdat:{index}"));
            assert_eq!(info.title, input.title);
            let mut expected_notes = input.notes.clone();
            if !input.computed
                && let Some(original) = input
                    .source_info
                    .as_ref()
                    .and_then(|source| source.notes.as_deref())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            {
                expected_notes.push_str("\n\n[원 측정 메모]\n");
                expected_notes.push_str(original);
            }
            assert_eq!(info.notes, expected_notes);
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
