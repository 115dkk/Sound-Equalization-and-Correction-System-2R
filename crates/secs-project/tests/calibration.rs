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
        assert_eq!(object.remove("rc_bands"), Some(Value::Array(vec![])));
        assert_eq!(object.remove("low_protect"), Some(false.into()));
        assert_eq!(object.remove("lf_align"), Some(false.into()));
        assert_eq!(object.remove("mode_decay"), Some(false.into()));
        assert_eq!(object.remove("hf_phase"), Some(false.into()));
        assert_eq!(object.remove("input_hp_hz"), Some(Value::Null));
        assert_eq!(
            object.remove("peq_analytic_derivatives"),
            Some(false.into())
        );
        assert_eq!(
            object.remove("mimo_iir_analytic_derivatives"),
            Some(false.into())
        );
        if settings.xo_rec_include {
            assert_eq!(object.remove("xo_rec_include"), None);
        } else {
            assert_eq!(object.remove("xo_rec_include"), Some(false.into()));
        }
        assert_eq!(actual, expected["settings"]);
        store_calibration(&mut project, &settings);
        if !settings.xo_rec_include {
            assert!(project.save_text().contains("xo_rec_include"));
            let mut upstream = settings.clone();
            upstream.xo_rec_include = true;
            store_calibration(&mut project, &upstream);
        }
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
fn lf_align_and_mode_decay_round_trip_and_legacy_default() {
    let mut project = Project::from_preset("2.1").unwrap();
    let mut settings = calibration_from_project(&project);
    assert!(!settings.lf_align);
    assert!(!settings.mode_decay);
    store_calibration(&mut project, &settings);
    let baseline = project.save_text();
    assert!(!baseline.contains("lf_align"));
    assert!(!baseline.contains("mode_decay"));

    settings.lf_align = true;
    settings.mode_decay = true;
    store_calibration(&mut project, &settings);
    let saved = project.save_text();
    let saved_json: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(saved_json["calibration"]["lf_align"], true);
    assert_eq!(saved_json["calibration"]["mode_decay"], true);
    let mut reopened = Project::from_json(loads(&saved).unwrap()).unwrap();
    let mut parsed = calibration_from_project(&reopened);
    assert!(parsed.lf_align);
    assert!(parsed.mode_decay);

    parsed.lf_align = false;
    parsed.mode_decay = false;
    store_calibration(&mut reopened, &parsed);
    assert_eq!(reopened.save_text(), baseline);
}

#[test]
fn hf_phase_round_trip_and_legacy_default() {
    let mut project = Project::from_preset("2.1").unwrap();
    let mut settings = calibration_from_project(&project);
    assert!(!settings.hf_phase);
    store_calibration(&mut project, &settings);
    let baseline = project.save_text();
    assert!(!baseline.contains("hf_phase"));

    settings.hf_phase = true;
    store_calibration(&mut project, &settings);
    let saved = project.save_text();
    let saved_json: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(saved_json["calibration"]["hf_phase"], true);
    let mut reopened = Project::from_json(loads(&saved).unwrap()).unwrap();
    let mut parsed = calibration_from_project(&reopened);
    assert!(parsed.hf_phase);

    parsed.hf_phase = false;
    store_calibration(&mut reopened, &parsed);
    assert_eq!(reopened.save_text(), baseline);
}

#[test]
fn input_hp_round_trip_clamps_and_legacy_default() {
    let mut project = Project::from_preset("2.1").unwrap();
    let mut settings = calibration_from_project(&project);
    assert_eq!(settings.input_hp_hz, None);
    store_calibration(&mut project, &settings);
    let baseline = project.save_text();
    assert!(!baseline.contains("input_hp_hz"));

    settings.input_hp_hz = Some(27.0);
    store_calibration(&mut project, &settings);
    let saved = project.save_text();
    let saved_json: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(saved_json["calibration"]["input_hp_hz"], 27.0);
    let mut reopened = Project::from_json(loads(&saved).unwrap()).unwrap();
    let mut parsed = calibration_from_project(&reopened);
    assert_eq!(parsed.input_hp_hz, Some(27.0));

    parsed.input_hp_hz = None;
    store_calibration(&mut reopened, &parsed);
    assert_eq!(reopened.save_text(), baseline);

    for (raw, expected) in [
        (r#"9.9"#, Some(10.0)),
        (r#"50.1"#, Some(50.0)),
        (r#""NaN""#, None),
        ("null", None),
    ] {
        let json = format!(r#"{{"extra":{{"calibration":{{"input_hp_hz":{raw}}}}}}}"#);
        let project = Project::from_json(loads(&json).unwrap()).unwrap();
        assert_eq!(calibration_from_project(&project).input_hp_hz, expected);
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

#[test]
fn rc_bands_round_trip_filter_sort_trim_and_clear() {
    use secs_types::calibration::{CalibrationSettings, RcBand};

    let bands = vec![
        RcBand {
            lo_hz: 1_000.0,
            hi_hz: 2_000.0,
        },
        RcBand {
            lo_hz: 15_000.0,
            hi_hz: 20_000.0,
        },
    ];
    let mut project = Project::from_preset("2.0").unwrap();
    let settings = CalibrationSettings {
        rc_bands: bands.clone(),
        ..Default::default()
    };
    store_calibration(&mut project, &settings);
    let saved = project.save_text();
    let saved_json: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        saved_json["calibration"]["rc_bands"],
        serde_json::json!([[1_000.0, 2_000.0], [15_000.0, 20_000.0]])
    );
    let reopened = Project::from_json(loads(&saved).unwrap()).unwrap();
    assert_eq!(calibration_from_project(&reopened).rc_bands, bands);

    let mut cleared = calibration_from_project(&reopened);
    cleared.rc_bands.clear();
    let mut reopened = reopened;
    store_calibration(&mut reopened, &cleared);
    assert!(!reopened.save_text().contains("rc_bands"));

    let raw = r#"{"extra":{"calibration":{"rc_lo":15,"rc_bands":[[5000,6000],[10,100],[4000,4500],[3000,3500],[2000,2500],[1000,1500],[9000,8000],[15000,"NaN"],[20,30,40]]}}}"#;
    let project = Project::from_json(loads(raw).unwrap()).unwrap();
    let parsed = calibration_from_project(&project);
    assert_eq!(parsed.rc_lo, 15.0);
    assert_eq!(
        parsed.rc_bands,
        [
            RcBand {
                lo_hz: 1_000.0,
                hi_hz: 1_500.0,
            },
            RcBand {
                lo_hz: 2_000.0,
                hi_hz: 2_500.0,
            },
            RcBand {
                lo_hz: 3_000.0,
                hi_hz: 3_500.0,
            },
            RcBand {
                lo_hz: 4_000.0,
                hi_hz: 4_500.0,
            },
        ]
    );
}

#[test]
fn xo_recommendation_metadata_round_trips_and_defaults_are_compatible() {
    let mut project = Project::from_preset("2.1").unwrap();
    let mut settings = calibration_from_project(&project);
    assert!(settings.xo_rec_include);
    assert!(settings.xo_rec_key.is_empty());
    store_calibration(&mut project, &settings);
    let baseline = project.save_text();
    assert!(!baseline.contains("xo_rec_key"));
    assert!(!baseline.contains("xo_rec_include"));

    settings.xo_rec = vec![("프론트".into(), 80.0)];
    settings.xo_rec_key = "measurement|target|config|settings".into();
    settings.xo_rec_include = false;
    store_calibration(&mut project, &settings);
    let saved = project.save_text();
    let reopened = Project::from_json(loads(&saved).unwrap()).unwrap();
    let parsed = calibration_from_project(&reopened);
    assert_eq!(parsed.xo_rec_key, settings.xo_rec_key);
    assert_eq!(parsed.xo_rec, settings.xo_rec);
    assert!(!parsed.xo_rec_include);

    let legacy =
        Project::from_json(loads(r#"{"extra":{"calibration":{"xo_rec":{"프론트":80}}}}"#).unwrap())
            .unwrap();
    let legacy = calibration_from_project(&legacy);
    assert!(legacy.xo_rec_include);
    assert!(legacy.xo_rec_key.is_empty());

    let matching = Project::from_json(
        loads(
            r#"{"extra":{"calibration":{"xo":{"센터":100,"프론트":80},"xo_rec":{"프론트":80,"센터":100}}}}"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert!(calibration_from_project(&matching).xo_rec_include);

    let manual = Project::from_json(
        loads(r#"{"extra":{"calibration":{"xo":{"프론트":90},"xo_rec":{"프론트":80}}}}"#).unwrap(),
    )
    .unwrap();
    assert!(!calibration_from_project(&manual).xo_rec_include);
}
