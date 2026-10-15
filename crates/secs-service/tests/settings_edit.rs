#![forbid(unsafe_code)]

use secs_service::{Deps, NoEvents, Service};
use secs_types::{calibration::CalibrationSettings, ipc::*};
use serde_json::Value;
use std::{fs, path::PathBuf, sync::Arc};

fn call(service: &Service, request: Request) -> Response {
    match service.call(request, Arc::new(NoEvents)) {
        Envelope::Ok(response) => *response,
        Envelope::Err(error) => panic!("{error:?}"),
    }
}

#[test]
fn settings_edits_and_sub_warnings_replay_upstream_oracle() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../oracle/goldens/service_settings_edit");
    for name in ["limits", "room", "groups", "sub_limits", "infra"] {
        let fixture: Value =
            serde_json::from_slice(&fs::read(fixtures.join(format!("{name}.json"))).unwrap())
                .unwrap();
        let root = std::env::temp_dir().join(format!("secs-edit-{}-{name}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let service = Service::new(Deps::default());
        call(
            &service,
            Request::ProjectNew(ProjectNewParams {
                path: root.join("edit.secs").to_string_lossy().into_owned(),
                preset: "2.1".into(),
                sub_count: 4,
                sample: false,
                tutorial: false,
            }),
        );
        for (index, (edit, expected)) in fixture["inputs"]["edits"]
            .as_array()
            .unwrap()
            .iter()
            .zip(fixture["outputs"]["steps"].as_array().unwrap())
            .enumerate()
        {
            let request: Request = serde_json::from_value(serde_json::json!({
                "method": "calibration_settings_edit", "params": {"edit": edit}
            }))
            .unwrap();
            let Response::CalibrationSettingsEdit(settings) = call(&service, request) else {
                panic!()
            };
            let expected_settings: CalibrationSettings =
                serde_json::from_value(expected["settings"].clone()).unwrap();
            assert_eq!(
                serde_json::to_value(&settings).unwrap(),
                serde_json::to_value(&expected_settings).unwrap(),
                "{name}/{index}: {edit}"
            );
            let Response::CalibrationSettingsGet(stored) =
                call(&service, Request::CalibrationSettingsGet)
            else {
                panic!()
            };
            assert_eq!(stored, settings);
            let Response::ProjectInfo(info) = call(&service, Request::ProjectInfo) else {
                panic!()
            };
            assert!(info.dirty);
            let Response::SubWarnings(warnings) = call(&service, Request::SubWarnings) else {
                panic!()
            };
            let rendered: Vec<_> = warnings
                .iter()
                .map(|n| {
                    assert_eq!(n.code, "sub_narrow_band");
                    format!(
                        "{}: HP {} Hz, LP {} Hz — 통과 대역이 거의 없습니다",
                        n.params["role"], n.params["hp_hz"], n.params["lp_hz"]
                    )
                })
                .collect();
            let expected_warnings: Vec<String> =
                serde_json::from_value(expected["filter_warnings"].clone()).unwrap();
            assert_eq!(rendered, expected_warnings, "{name}/{index}");
        }
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn settings_edit_and_warnings_require_an_open_project() {
    let service = Service::new(Deps::default());
    for request in [
        Request::SubWarnings,
        Request::CalibrationSettingsEdit(CalibrationSettingsEditParams {
            edit: secs_types::settings_edit::SettingEdit::MimoOn { on: true },
        }),
    ] {
        assert!(matches!(
            service.call(request, Arc::new(NoEvents)),
            Envelope::Err(_)
        ));
    }
}
