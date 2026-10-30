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
        let object = actual.as_object_mut().unwrap();
        object.remove("sample_rate_policy");
        assert_eq!(object.remove("rc_hi_basis"), Some("strict".into()));
        assert_eq!(object.remove("rc_hi_transition"), Some(1.into()));
        assert_eq!(object.remove("low_protect"), Some(false.into()));
        assert_eq!(
            object.remove("peq_analytic_derivatives"),
            Some(false.into())
        );
        assert_eq!(
            object.remove("mimo_iir_analytic_derivatives"),
            Some(false.into())
        );
        assert_eq!(actual, expected["settings"]);
        store_calibration(&mut project, &settings);
        assert_eq!(project.save_text(), expected["saved"].as_str().unwrap());
        let json: Value = serde_json::from_str(&dumps(&project.to_json())).unwrap();
        assert_eq!(json["keep"], expected["extra_keep"]);
    }
}

#[test]
fn low_protect_round_trip_and_legacy_default() {
    let mut project = Project::from_preset("2.1").unwrap();
    let mut settings = calibration_from_project(&project);
    assert!(!settings.low_protect);
    store_calibration(&mut project, &settings);
    let baseline = project.save_text();
    assert!(!baseline.contains("low_protect"));
    settings.low_protect = true;
    store_calibration(&mut project, &settings);
    let mut reopened = Project::from_json(loads(&project.save_text()).unwrap()).unwrap();
    let mut parsed = calibration_from_project(&reopened);
    assert!(parsed.low_protect);
    parsed.low_protect = false;
    store_calibration(&mut reopened, &parsed);
    assert_eq!(reopened.save_text(), baseline);
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
fn analytic_derivative_settings_round_trip_and_defaults_preserve_saved_bytes() {
    let mut project = Project::from_preset("2.1").unwrap();
    let mut settings = calibration_from_project(&project);
    store_calibration(&mut project, &settings);
    let default_text = project.save_text();
    assert!(!settings.peq_analytic_derivatives);
    assert!(!settings.mimo_iir_analytic_derivatives);

    settings.peq_analytic_derivatives = true;
    settings.mimo_iir_analytic_derivatives = true;
    store_calibration(&mut project, &settings);
    let saved = project.save_text();
    assert!(saved.contains("peq_analytic_derivatives"));
    assert!(saved.contains("mimo_iir_analytic_derivatives"));
    let reopened = Project::from_json(loads(&saved).unwrap()).unwrap();
    let reopened = calibration_from_project(&reopened);
    assert!(reopened.peq_analytic_derivatives);
    assert!(reopened.mimo_iir_analytic_derivatives);

    settings.peq_analytic_derivatives = false;
    settings.mimo_iir_analytic_derivatives = false;
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

#[test]
fn rc_upper_round_trip_and_legacy_unknown_defaults() {
    use secs_types::calibration::{CalibrationSettings, RcHiBasis};
    for basis in [RcHiBasis::Strict, RcHiBasis::Lenient, RcHiBasis::Agreement] {
        for factor in [1, 2, 4] {
            let mut project = Project::from_preset("2.0").unwrap();
            let settings = CalibrationSettings {
                rc_hi_basis: basis,
                rc_hi_transition: factor,
                ..Default::default()
            };
            store_calibration(&mut project, &settings);
            let saved = project.save_text();
            let reopened = Project::from_json(loads(&saved).unwrap()).unwrap();
            assert_eq!(calibration_from_project(&reopened), settings);
            // The upstream writer ignores unknown calibration keys. Removing
            // the two additions reproduces its strict ×1 interpretation.
            let mut upstream = reopened;
            let (_, secs_project::PyJson::Object(fields)) = upstream
                .extra
                .iter_mut()
                .find(|(key, _)| key == "calibration")
                .unwrap()
            else {
                panic!()
            };
            fields.retain(|(key, _)| key != "rc_hi_basis" && key != "rc_hi_transition");
            let legacy = calibration_from_project(&upstream);
            assert_eq!(legacy.rc_hi_basis, RcHiBasis::Strict);
            assert_eq!(legacy.rc_hi_transition, 1);
        }
    }
    for (basis, factor) in [
        ("\"future\"", "3"),
        ("null", "null"),
        ("[]", "[]"),
        ("true", "true"),
        ("1", "\"2\""),
        ("\"LENIENT\"", "2.5"),
        ("{}", "-4"),
        ("\"strict\"", "256"),
    ] {
        let mut project = Project::from_preset("2.0").unwrap();
        project.extra.push((
            "calibration".into(),
            loads(&format!(
                "{{\"rc_hi_basis\":{basis},\"rc_hi_transition\":{factor}}}"
            ))
            .unwrap(),
        ));
        let settings = calibration_from_project(&project);
        assert_eq!(settings.rc_hi_basis, RcHiBasis::Strict);
        assert_eq!(settings.rc_hi_transition, 1);
        store_calibration(&mut project, &settings);
        assert!(!project.save_text().contains("rc_hi_basis"));
        assert!(!project.save_text().contains("rc_hi_transition"));
    }
}
