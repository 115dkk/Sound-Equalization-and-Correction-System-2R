#![forbid(unsafe_code)]

use secs_oracle::Fixture;
use secs_project::{Project, calibration_from_project, dumps, loads, store_calibration};
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
        assert_eq!(
            serde_json::to_value(&settings).unwrap(),
            expected["settings"]
        );
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
