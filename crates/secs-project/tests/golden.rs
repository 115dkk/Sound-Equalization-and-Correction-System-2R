#![forbid(unsafe_code)]

use secs_oracle::{Fixture, assert_same, number};
use secs_project::{
    Band, DspProfile, Project, Speaker, SpeakerConfig, TargetCurve, python_json,
    read_mono_float_wav, read_wav_float32, snap_focus, write_wav_float32,
};
use secs_types::role::{
    ROLES, role_code, role_from_title, role_group, role_kind, role_label, role_order, role_position,
};
use serde_json::Value;

const SERDE_NUMBER_KEY_FOR_TEST: &str = "$serde_json::private::Number";

fn str_array(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect()
}

#[test]
fn role_vocabulary_matches_oracle() {
    let fx = Fixture::load("project", "roles");
    let filter = str_array(&fx.inputs()["filter"]);
    for row in fx.outputs()["roles"].as_array().unwrap() {
        let role = row["role"].as_str().unwrap();
        assert_eq!(role_label(role), row["label"].as_str().unwrap());
        assert_eq!(
            format!("{:?}", role_kind(role)).to_lowercase(),
            row["kind"].as_str().unwrap()
        );
        assert_eq!(role_group(role), row["group"].as_str().unwrap());
        assert_eq!(role_code(role), row["code"].as_str().unwrap());
        assert_eq!(role_order(role) as u64, row["order"].as_u64().unwrap());
        match role_position(role) {
            Some((x, y)) => {
                assert_eq!(x.to_bits(), number(&row["position"][0]).to_bits());
                assert_eq!(y.to_bits(), number(&row["position"][1]).to_bits());
            }
            None => assert!(row["position"].is_null()),
        }
    }
    for row in fx.outputs()["titles"].as_array().unwrap() {
        let title = row[0].as_str().unwrap();
        assert_eq!(role_from_title(title, None), row[1].as_str());
        assert_eq!(role_from_title(title, Some(&filter)), row[2].as_str());
    }
    assert_eq!(ROLES.len(), 27);
}

#[test]
fn project_load_migrations_and_save_match_oracle() {
    for fx in Fixture::all("project", "migrate_") {
        let input = fx.blob(&fx.inputs()["text"]);
        let project = Project::load_text(std::str::from_utf8(&input).unwrap()).unwrap();
        assert_eq!(project.to_value(), fx.outputs()["value"]);
        assert_eq!(
            project.save_text().as_bytes(),
            fx.blob(&fx.outputs()["save"])
        );
    }
}

#[test]
fn direct_project_preset_matches_oracle() {
    let fx = Fixture::load("project", "direct_preset");
    let project = Project::from_preset(fx.inputs()["preset"].as_str().unwrap()).unwrap();
    assert_eq!(project.to_value(), fx.outputs()["value"]);
    assert_eq!(project.dsp, DspProfile::default());
    assert_eq!(
        project.dsp.to_value(),
        fx.outputs()["dsp_dataclass_default"]
    );
}

#[test]
fn project_edge_contracts_match_oracle() {
    let future = Fixture::load("project", "dsp_future");
    let project = Project::from_value(serde_json::json!({"dsp": future.inputs()})).unwrap();
    assert_eq!(
        project.dsp.fir_taps_max,
        future.outputs()["profile"]["fir_taps_max"]
    );
    assert_eq!(project.dsp.profile, future.outputs()["profile"]["profile"]);

    let focus = Fixture::load("project", "focus");
    for (input, expected) in focus.inputs()["values"]
        .as_array()
        .unwrap()
        .iter()
        .zip(focus.outputs()["snapped"].as_array().unwrap())
    {
        let value = match input.as_str() {
            Some("nan") => Value::String("NaN".into()),
            Some("+inf") => Value::String("Infinity".into()),
            Some("-inf") => Value::String("-Infinity".into()),
            _ => input.clone(),
        };
        assert_eq!(snap_focus(&value), expected.as_i64().unwrap());
    }

    let collections = Fixture::load("project", "collections");
    for (input, expected) in collections.inputs()["projects"]
        .as_array()
        .unwrap()
        .iter()
        .zip(collections.outputs()["rows"].as_array().unwrap())
    {
        let project = Project::from_value(input.clone()).unwrap();
        assert_eq!(project.curtain.0, number(&expected[0][0]));
        assert_eq!(project.curtain.1, number(&expected[0][1]));
        assert_eq!(
            project.bass_path_count() as i64,
            expected[1].as_i64().unwrap()
        );
    }

    let validation = Fixture::load("project", "config_validation");
    let config = SpeakerConfig {
        speakers: vec![
            Speaker {
                role: "L".into(),
                size: "small".into(),
                bands: vec![Band {
                    band: "NOPE".into(),
                    out: 1,
                    pol: "x".into(),
                }],
            },
            Speaker {
                role: "R".into(),
                size: "small".into(),
                bands: vec![Band {
                    band: "SUB".into(),
                    out: 1,
                    pol: "+".into(),
                }],
            },
            Speaker {
                role: "L".into(),
                size: "small".into(),
                bands: vec![],
            },
            Speaker {
                role: "C".into(),
                size: "small".into(),
                bands: vec![Band {
                    band: "FULL".into(),
                    out: 1,
                    pol: "+".into(),
                }],
            },
        ],
        ..SpeakerConfig::default()
    };
    assert_eq!(
        config.validate(),
        str_array(&validation.outputs()["messages"])
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        config
            .groups()
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
        str_array(&validation.outputs()["groups"])
    );

    let sort = Fixture::load("project", "target_sort");
    for (input, expected) in sort.inputs()["points"]
        .as_array()
        .unwrap()
        .iter()
        .zip(sort.outputs()["points"].as_array().unwrap())
    {
        let curve = TargetCurve::new(
            input
                .as_array()
                .unwrap()
                .iter()
                .map(|point| (number(&point[0]), number(&point[1])))
                .collect(),
        );
        for (actual, expected) in curve.points.iter().zip(expected.as_array().unwrap()) {
            let frequency = number(&expected[0]);
            assert!(
                actual.0.to_bits() == frequency.to_bits()
                    || actual.0.is_nan() && frequency.is_nan()
            );
            assert_eq!(actual.1.to_bits(), number(&expected[1]).to_bits());
        }
    }
}

#[test]
fn project_presets_save_exactly() {
    for fx in Fixture::all("project", "preset_") {
        let expected = &fx.outputs()["value"];
        let loaded = Project::from_value(expected.clone()).unwrap();
        assert_eq!(loaded.to_value(), *expected, "{}", fx.id);
        assert_eq!(
            loaded.save_text().as_bytes(),
            fx.blob(&fx.outputs()["save"]),
            "{}",
            fx.id
        );
    }
}

#[test]
fn dsp_tap_allocation_matches_oracle() {
    for fx in Fixture::all("project", "taps_") {
        let profile = fx.inputs()["profile"]["profile"].as_str().unwrap();
        let mut dsp = DspProfile::presets().remove(profile).unwrap();
        dsp.support_mode = fx.inputs()["profile"]["support_mode"]
            .as_str()
            .unwrap()
            .to_owned();
        for row in fx.outputs()["rows"].as_array().unwrap() {
            let config = SpeakerConfig::from_preset(row[0].as_str().unwrap(), "", true).unwrap();
            let phase = row[1].as_str().unwrap();
            let paths = row[2].as_i64().unwrap();
            let expected = row[3].as_array().unwrap();
            assert_eq!(
                dsp.allocate_taps(&config, phase, paths, 256),
                (expected[0].as_i64().unwrap(), expected[1].as_i64().unwrap()),
                "{}",
                fx.id
            );
            let used = dsp.fir_used(&config, phase, paths);
            for key in ["chan", "sup", "total"] {
                assert_eq!(used[key], row[4][key].as_i64().unwrap(), "{} {key}", fx.id);
            }
        }
    }
}

#[test]
fn target_curves_match_oracle() {
    for fx in Fixture::all("project", "target_") {
        if fx.id.ends_with("target_sort") {
            continue;
        }
        let points = fx.inputs()["points"]["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (number(&p[0]), number(&p[1])))
            .collect();
        let curve = TargetCurve::new(points);
        let frequencies = fx.f64s(&fx.inputs()["freqs"]);
        let expected = fx.f64s(&fx.outputs()["values"]);
        assert_same(&fx.id, &curve.sample(&frequencies), &expected);
    }
}

#[test]
fn python_json_edge_cases_match_oracle() {
    let cases = [
        serde_json::json!({"floats":[0.0,-0.0,1e-7,1e-6,1e-5,1e15,1e16,100000.0,1.2345678901234567],"ints":[0,-1,9007199254740992_i64,1208925819614629174706176_u128],"strings":["","quote \\\"","line\n\ttab\u{8}","한글","😀","\u{2028}"],"empty":[[],{}]}),
        serde_json::json!([
            0.0,
            -0.0,
            1e-7,
            1e-6,
            1e-5,
            1e15,
            1e16,
            100000.0,
            1.2345678901234567
        ]),
        serde_json::json!({"z":1,"a":2}),
    ];
    for (index, value) in cases.iter().enumerate() {
        let fx = Fixture::load("project", &format!("json_{index}"));
        let expected = fx.blob(&fx.outputs()["finite_save"]);
        assert_eq!(python_json(value).as_bytes(), expected, "{}", fx.id);
        if index < 2 {
            let expected_nonfinite = String::from_utf8(fx.blob(&fx.outputs()["save"])).unwrap();
            let project = Project::default();
            let mut rust_value = project.to_value();
            rust_value["curtain"] = serde_json::json!([0.0, 1.0]);
            let mut project = Project::from_value(rust_value).unwrap();
            project.curtain = (f64::NAN, f64::INFINITY);
            let actual_nonfinite = project.save_text();
            assert!(expected_nonfinite.contains("NaN"));
            assert!(actual_nonfinite.contains("NaN"));
            assert!(actual_nonfinite.contains("Infinity"));
        }
    }

    for fx in Fixture::all("project", "json_round_trip_") {
        let input = fx.blob(&fx.inputs()["text"]);
        let project = Project::load_text(std::str::from_utf8(&input).unwrap()).unwrap();
        let expected = if fx.id.ends_with("json_round_trip_2") {
            let mut value = fx.outputs()["value"].clone();
            value["future"] = serde_json::json!({SERDE_NUMBER_KEY_FOR_TEST: "123"});
            value
        } else {
            fx.outputs()["value"].clone()
        };
        assert_eq!(project.to_value(), expected, "{}", fx.id);
        assert_eq!(
            project.save_text().as_bytes(),
            fx.blob(&fx.outputs()["save"]),
            "{}",
            fx.id
        );
    }

    let fx = Fixture::load("project", "typed_negative_zero");
    let project = Project::from_value(fx.inputs()["value"].clone()).unwrap();
    assert_eq!(project.to_value(), fx.outputs()["value"]);
    assert_eq!(
        project.save_text().as_bytes(),
        fx.blob(&fx.outputs()["save"])
    );
}

#[test]
fn wav_writer_and_readers_match_oracle() {
    for fx in Fixture::all("project", "wav_") {
        let input = fx.inputs();
        let rate = input["rate"].as_u64().unwrap() as u32;
        let samples: Vec<f32> = input["samples"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| number(v) as f32)
            .collect();
        let wav = write_wav_float32(&samples, rate);
        assert_eq!(wav, fx.blob(&fx.outputs()["wav"]), "{}", fx.id);
        let (minimal, minimal_rate) = read_wav_float32(&wav).unwrap();
        let (general, general_rate) = read_mono_float_wav(&wav).unwrap();
        assert_eq!(
            minimal_rate as u64,
            fx.outputs()["minimal"]["rate"].as_u64().unwrap()
        );
        assert_eq!(
            general_rate as u64,
            fx.outputs()["fast"]["rate"].as_u64().unwrap()
        );
        let expected: Vec<f32> = fx.outputs()["minimal"]["samples"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| number(v) as f32)
            .collect();
        assert_eq!(minimal, expected);
        assert_eq!(general, expected);
    }
}
