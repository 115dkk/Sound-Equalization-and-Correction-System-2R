#![forbid(unsafe_code)]

use secs_oracle::Fixture;
use secs_project::{Project, calibration_from_project, dumps, loads, store_calibration};
use secs_types::calibration::SampleRatePolicy;
use serde_json::Value;

#[test]
fn calibration_parse_store_and_saved_bytes_match_oracle() {
    for fixture in Fixture::all("calibration", "project_") {
        let expected = fixture.outputs();
        let input = &fixture.inputs()["calibration"];
        let mut project = Project::from_preset("2.0").unwrap();
        project
            .extra
            .push(("keep".into(), loads(r#"{"unknown":1}"#).unwrap()));
        if !fixture.id.ends_with("project_missing") {
            project
                .extra
                .push(("calibration".into(), loads(&input.to_string()).unwrap()));
        }
        let settings = calibration_from_project(&project);
        let mut actual = serde_json::to_value(&settings).unwrap();
        assert_eq!(actual["sample_rate_policy"], "reject");
        actual.as_object_mut().unwrap().remove("sample_rate_policy");
        assert_eq!(actual, expected["settings"]);
        store_calibration(&mut project, &settings);
        assert_eq!(project.save_text(), expected["saved"].as_str().unwrap());
        let json: Value = serde_json::from_str(&dumps(&project.to_json())).unwrap();
        assert_eq!(json["keep"], expected["extra_keep"]);
    }
}

#[test]
fn calibration_wrong_top_level_uses_defaults() {
    let project = Project::from_json(loads(r#"{"extra":{"calibration":[]}}"#).unwrap()).unwrap();
    assert_eq!(
        calibration_from_project(&project),
        secs_types::calibration::CalibrationSettings::default()
    );
}

#[test]
fn sample_rate_policy_round_trips_and_default_preserves_saved_bytes() {
    let mut project = Project::from_preset("2.1").unwrap();
    let mut settings = calibration_from_project(&project);
    assert_eq!(settings.sample_rate_policy, SampleRatePolicy::Reject);
    store_calibration(&mut project, &settings);
    let default_text = project.save_text();
    assert!(!default_text.contains("sample_rate_policy"));
    settings.sample_rate_policy = SampleRatePolicy::Resample;
    store_calibration(&mut project, &settings);
    let reopened = Project::from_json(loads(&project.save_text()).unwrap()).unwrap();
    assert_eq!(
        calibration_from_project(&reopened).sample_rate_policy,
        SampleRatePolicy::Resample
    );
    settings.sample_rate_policy = SampleRatePolicy::Reject;
    store_calibration(&mut project, &settings);
    assert_eq!(project.save_text(), default_text);
}

#[test]
fn invalid_or_unknown_sample_rate_policy_uses_reject() {
    for value in [
        r#""reject""#,
        r#""future_policy""#,
        r#""Resample""#,
        "true",
        "1",
        "null",
        "[]",
        "{}",
    ] {
        let json = format!(r#"{{"extra":{{"calibration":{{"sample_rate_policy":{value}}}}}}}"#);
        let mut project = Project::from_json(loads(&json).unwrap()).unwrap();
        let settings = calibration_from_project(&project);
        assert_eq!(
            settings.sample_rate_policy,
            SampleRatePolicy::Reject,
            "{value}"
        );
        store_calibration(&mut project, &settings);
        assert!(
            !project.save_text().contains("sample_rate_policy"),
            "{value}"
        );
    }
}
