#![forbid(unsafe_code)]

use secs_oracle::{Fixture, number};
use secs_project::{
    MeasurementPlan, Project, TimingReference, default_ref_role, estimate_seconds,
    estimate_subs_seconds, estimate_xo_seconds, measured_positions, measurement_roles,
    measurement_signature, plan_from_project, position_seconds, split_role, start_delay_seconds,
    sweep_length_samples, sweep_seconds, target_key, timing_settings,
};
use serde_json::Value;

fn project(value: &Value) -> Project {
    Project::load_text(&value.to_string()).unwrap()
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= expected.abs().max(1.0) * 1e-12,
        "{actual} != {expected}"
    );
}

#[test]
fn service_plan_matches_upstream() {
    for fx in Fixture::all("service_plan", "engine_")
        .into_iter()
        .chain(Fixture::all("service_plan", "project_"))
        .chain(Fixture::all("service_plan", "multi_amp"))
        .chain(Fixture::all("service_plan", "single_position"))
    {
        let project = project(&fx.inputs()["project"]);
        let positions = fx.inputs()["positions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_i64().unwrap())
            .collect::<Vec<_>>();
        let plan = plan_from_project(&project, positions.clone());
        let expected = &fx.outputs()["plan"];
        assert_plan(&plan, expected, &fx.id);
        assert_eq!(plan.positions, positions, "{}", fx.id);
        for row in expected["names"].as_array().unwrap() {
            assert_eq!(
                plan.name_for(row[0].as_str().unwrap(), row[1].as_i64().unwrap()),
                row[2],
                "{}",
                fx.id
            );
        }
        for row in expected["outputs_for"].as_array().unwrap() {
            assert_eq!(
                plan.output_for(row[0].as_str().unwrap()),
                row[1].as_i64().unwrap(),
                "{}",
                fx.id
            );
        }

        let roles = measurement_roles(&project);
        let expected_roles = fx.outputs()["measurement_roles"].as_array().unwrap();
        assert_eq!(roles.len(), expected_roles.len(), "{}", fx.id);
        for ((role, output), expected) in roles.iter().zip(expected_roles) {
            assert_eq!(role, expected[0].as_str().unwrap(), "{}", fx.id);
            assert_eq!(*output, expected[1].as_i64().unwrap(), "{}", fx.id);
        }
        assert_eq!(
            default_ref_role(&project),
            fx.outputs()["default_ref_role"],
            "{}",
            fx.id
        );
        let (kind, output) = timing_settings(&project);
        let expected_timing = &fx.outputs()["timing_settings"];
        assert_eq!(
            kind.as_disk_str(),
            expected_timing[0].as_str().unwrap(),
            "{}",
            fx.id
        );
        assert_eq!(output, expected_timing[1].as_i64(), "{}", fx.id);
        close(
            sweep_seconds(&project),
            number(&fx.outputs()["sweep_seconds"]),
        );
        close(
            position_seconds(&project),
            number(&fx.outputs()["position_seconds"]),
        );
        assert_eq!(
            measured_positions(&project),
            ints(&fx.outputs()["measured_positions"]),
            "{}",
            fx.id
        );
        assert_signature(&project, &fx.outputs()["measurement_signature"], &fx.id);
        let actual_target = target_key(&project);
        let expected_target = fx.outputs()["target_key"].as_array().unwrap();
        assert_eq!(actual_target.len(), expected_target.len(), "{}", fx.id);
        for ((frequency, level), expected) in actual_target.iter().zip(expected_target) {
            close(frequency.0, number(&expected[0]));
            close(level.0, number(&expected[1]));
        }
        for mode in ["zero", "mixed", "linear"] {
            close(
                estimate_seconds(&project, mode),
                number(&fx.outputs()["estimate_seconds"][mode]),
            );
        }
        close(
            estimate_subs_seconds(&project),
            number(&fx.outputs()["estimate_subs_seconds"]),
        );
        for mode in ["zero", "mixed"] {
            close(
                estimate_xo_seconds(&project, mode, 4),
                number(&fx.outputs()["estimate_xo_seconds"][mode]),
            );
        }
    }
}

#[test]
fn pure_measurement_helpers_match_upstream() {
    let fx = Fixture::load("service_plan", "helpers");
    for row in fx.outputs()["split_role"].as_array().unwrap() {
        let actual = split_role(row[0].as_str().unwrap());
        assert_eq!(actual.0, row[1][0].as_str().unwrap());
        assert_eq!(actual.1.as_deref(), row[1][1].as_str());
    }
    for row in fx.outputs()["sweep_length_samples"].as_array().unwrap() {
        let actual = sweep_length_samples(row[0].as_str().unwrap());
        if row[1].is_null() {
            assert!(actual.is_err());
        } else {
            assert_eq!(actual.unwrap(), row[1].as_i64().unwrap());
        }
    }
    for row in fx.outputs()["start_delay_seconds"].as_array().unwrap() {
        assert_eq!(
            start_delay_seconds(row[0].as_str().unwrap_or("")),
            row[1].as_i64().unwrap()
        );
    }
}

#[test]
fn invalid_plans_match_upstream_behavior() {
    let fx = Fixture::load("service_plan", "validation");
    let plans = [
        MeasurementPlan::default(),
        MeasurementPlan {
            roles: vec!["L".into(), "L".into()],
            outputs: [("L".into(), 1)].into(),
            positions: vec![1, 1],
            ..MeasurementPlan::default()
        },
        MeasurementPlan {
            roles: vec!["L".into(), "R".into()],
            outputs: [("L".into(), 1)].into(),
            positions: vec![1],
            repetitions: 2,
            timing_reference: TimingReference::Acoustic,
            sweep_length: "bad".into(),
            ..MeasurementPlan::default()
        },
    ];
    for (plan, expected) in plans.iter().zip(fx.outputs()["rows"].as_array().unwrap()) {
        assert_eq!(
            plan.validate()
                .iter()
                .map(|error| error.code)
                .collect::<Vec<_>>(),
            validation_codes(&expected["validate"]),
        );
        assert_eq!(
            plan.total_steps() as u64,
            expected["total_steps"].as_u64().unwrap()
        );
        assert_eq!(plan.name_for("L:LF", 3), expected["names"][0]);
        assert_eq!(
            plan.output_for("missing"),
            expected["names"][1].as_i64().unwrap()
        );
    }
}

#[test]
fn timing_reference_has_upstream_disk_forms() {
    assert_eq!(TimingReference::Acoustic.as_disk_str(), "acoustic");
    assert_eq!(TimingReference::Loopback.as_disk_str(), "loopback");
    assert_eq!(TimingReference::None.as_disk_str(), "none");
}

fn assert_plan(plan: &MeasurementPlan, expected: &Value, id: &str) {
    assert_eq!(plan.roles, strings(&expected["roles"]), "{id}");
    let expected_outputs = expected["outputs"].as_object().unwrap();
    assert_eq!(plan.outputs.len(), expected_outputs.len(), "{id}");
    for (role, output) in expected_outputs {
        assert_eq!(plan.outputs.get(role), output.as_i64().as_ref(), "{id}");
    }
    assert_eq!(plan.positions, ints(&expected["positions"]), "{id}");
    assert_eq!(plan.output_device, expected["output_device"], "{id}");
    assert_eq!(plan.input_device, expected["input_device"], "{id}");
    assert_eq!(
        plan.input_channel,
        expected["input_channel"].as_i64().unwrap(),
        "{id}"
    );
    close(plan.sweep_level_db, number(&expected["sweep_level_db"]));
    assert_eq!(plan.sweep_length, expected["sweep_length"], "{id}");
    assert_eq!(
        plan.repetitions,
        expected["repetitions"].as_i64().unwrap(),
        "{id}"
    );
    assert_eq!(
        plan.timing_reference.as_disk_str(),
        expected["timing_reference"],
        "{id}"
    );
    assert_eq!(
        plan.timing_ref_output,
        expected["timing_ref_output"].as_i64(),
        "{id}"
    );
    assert_eq!(plan.naming, expected["naming"], "{id}");
    close(plan.start_frequency, number(&expected["start_frequency"]));
    close(plan.end_frequency, number(&expected["end_frequency"]));
    assert_eq!(
        plan.sample_rate,
        expected["sample_rate"].as_i64().unwrap(),
        "{id}"
    );
    assert_eq!(
        plan.validate()
            .iter()
            .map(|error| error.code)
            .collect::<Vec<_>>(),
        validation_codes(&expected["validate"]),
        "{id}"
    );
    assert_eq!(
        plan.total_steps() as u64,
        expected["total_steps"].as_u64().unwrap(),
        "{id}"
    );
}

fn assert_signature(project: &Project, expected: &Value, id: &str) {
    let actual = measurement_signature(project);
    let expected = expected.as_array().unwrap();
    assert_eq!(actual.len(), expected.len(), "{id}");
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(actual.pos, expected[0], "{id}");
        assert_eq!(actual.roles, strings(&expected[1]), "{id}");
        let expected_t0 = expected[2].as_array().unwrap();
        assert_eq!(actual.t0.len(), expected_t0.len(), "{id}");
        for ((role, value), expected) in actual.t0.iter().zip(expected_t0) {
            assert_eq!(role, expected[0].as_str().unwrap(), "{id}");
            assert_eq!(value, expected[1].as_str().unwrap(), "{id}");
        }
    }
}

fn validation_codes(messages: &Value) -> Vec<&'static str> {
    messages
        .as_array()
        .unwrap()
        .iter()
        .map(|message| match message.as_str().unwrap() {
            "측정할 채널이 없습니다" => "measurement.roles.empty",
            "역할 중복이 있습니다" => "measurement.roles.duplicate",
            "측정 위치가 없습니다" => "measurement.positions.empty",
            "측정 위치 번호 중복이 있습니다" => "measurement.positions.duplicate",
            message if message.ends_with("측정 출력 채널이 지정되지 않았습니다") => {
                "measurement.output.missing"
            }
            "음향 타이밍 레퍼런스에서는 스윕 반복이 1 이어야 합니다" => {
                "measurement.acoustic_repetitions.invalid"
            }
            message if message.starts_with("스윕 길이는") => {
                "measurement.sweep_length.invalid"
            }
            message => panic!("unknown validation message: {message}"),
        })
        .collect()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().into())
        .collect()
}

fn ints(value: &Value) -> Vec<i64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_i64().unwrap())
        .collect()
}
