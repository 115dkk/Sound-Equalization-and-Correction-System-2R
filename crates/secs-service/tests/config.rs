#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use secs_service::{Deps, NoEvents, Service};
use secs_types::calibration::{CalibrationSettings, Mode};
use secs_types::ipc::*;

static TEMP: AtomicU64 = AtomicU64::new(0);

fn root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "secs-service-config-{name}-{}-{}",
        std::process::id(),
        TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn call(service: &Service, request: Request) -> Response {
    match service.call(request, Arc::new(NoEvents)) {
        Envelope::Ok(response) => *response,
        Envelope::Err(error) => panic!("request failed: {error:?}"),
    }
}

fn error(service: &Service, request: Request) -> IpcError {
    match service.call(request, Arc::new(NoEvents)) {
        Envelope::Err(error) => error,
        Envelope::Ok(response) => panic!("request unexpectedly succeeded: {response:?}"),
    }
}

fn new_project(service: &Service, path: &Path, preset: &str) {
    assert!(matches!(
        call(
            service,
            Request::ProjectNew(ProjectNewParams {
                path: path.to_string_lossy().into_owned(),
                preset: preset.into(),
                sub_count: 0,
                sample: false,
                tutorial: false,
            })
        ),
        Response::ProjectNew(_)
    ));
}

#[test]
fn config_edit_methods_mark_dirty_and_emit_upstream_patch_sheet() {
    let root = root("edit");
    let project = root.join("edit.secs");
    let patch = root.join("patch.csv");
    let service = Service::new(Deps::default());
    new_project(&service, &project, "2.0");

    let Response::ConfigGet(initial) = call(&service, Request::ConfigGet) else {
        panic!()
    };
    assert_eq!(initial.preset, "2.0");
    assert_eq!(
        initial
            .groups
            .iter()
            .map(|group| group.key.as_str())
            .collect::<Vec<_>>(),
        ["프론트"]
    );
    assert_eq!(initial.groups[0].id, "front");
    assert_eq!(initial.summary.outputs, [1, 2]);

    let Response::LayoutGet(layout) = call(&service, Request::LayoutGet) else {
        panic!()
    };
    assert_eq!(layout.timing.role, "R");

    let Response::ConfigSetSpeaker(edited) = call(
        &service,
        Request::ConfigSetSpeaker(ConfigSetSpeakerParams {
            index: 0,
            role: None,
            size: Some("large".into()),
            band_index: Some(0),
            out: Some(3),
            pol: Some("-".into()),
        }),
    ) else {
        panic!()
    };
    assert_eq!(edited.preset, "custom");
    assert_eq!(edited.speakers[0].size, "large");
    assert_eq!(edited.speakers[0].bands[0].out, 3);
    assert_eq!(edited.speakers[0].bands[0].pol, "-");

    let Response::ConfigAddSpeaker(added) = call(&service, Request::ConfigAddSpeaker) else {
        panic!()
    };
    assert_eq!(added.speakers.last().unwrap().role, "C");
    let Response::ConfigRemoveSpeaker(removed) = call(
        &service,
        Request::ConfigRemoveSpeaker(ConfigIndexParams { index: 2 }),
    ) else {
        panic!()
    };
    assert_eq!(removed.speakers.len(), 2);
    let Response::ConfigRenumber(renumbered) = call(&service, Request::ConfigRenumber) else {
        panic!()
    };
    assert_eq!(renumbered.summary.outputs, [1, 2]);

    let Response::PatchSheetCsv(result) = call(
        &service,
        Request::PatchSheetCsv(PathParams {
            path: patch.to_string_lossy().into_owned(),
        }),
    ) else {
        panic!()
    };
    assert_eq!(result.path, patch.to_string_lossy());
    let bytes = fs::read(&patch).unwrap();
    assert!(bytes.starts_with(&[0xef, 0xbb, 0xbf]));
    assert_eq!(
        std::str::from_utf8(&bytes[3..]).unwrap(),
        "역할,이름,크기,대역,측정 출력,극성\nL,Left,large,FULL,1,-\nR,Right,small,FULL,2,+\n"
    );

    let Response::ProjectInfo(info) = call(&service, Request::ProjectInfo) else {
        panic!()
    };
    assert!(info.dirty);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn destructive_config_and_layout_edits_require_confirmation_and_undo_once() {
    let root = root("undo");
    let project = root.join("undo.secs");
    let service = Service::new(Deps::default());
    new_project(&service, &project, "2.0");

    let mut stored = secs_project::MeasurementStore::open(&project).unwrap();
    let roles = stored
        .project()
        .config
        .speakers
        .iter()
        .map(|speaker| speaker.role.clone())
        .collect::<Vec<_>>();
    stored.start_position(1);
    for role in &roles {
        stored
            .record(
                secs_project::MeasurementResult {
                    role: role.clone(),
                    pos: 1,
                    ir: vec![0.0, 1.0, 0.0],
                    sample_rate: 48_000,
                    t0_s: 0.0,
                    spl_db: None,
                    spl_offset_db: None,
                    snr_db: Some(30.0),
                    uuid: Some(format!("{role}-1")),
                    source: Some("test".into()),
                    source_info: None,
                },
                Some(&roles),
            )
            .unwrap();
    }
    stored.save().unwrap();
    assert!(matches!(
        call(
            &service,
            Request::ProjectOpen(ProjectPathParams {
                path: project.to_string_lossy().into_owned(),
            })
        ),
        Response::ProjectOpen(_)
    ));

    let request = Request::ConfigApplyPreset(ConfigPresetParams {
        preset: "2.1".into(),
        confirm: false,
    });
    let denied = error(&service, request);
    assert_eq!(denied.code, ErrorCode::ConfirmRequired);
    assert_eq!(
        denied.params.get("measurements").map(String::as_str),
        Some("1")
    );
    let Response::ConfigGet(unchanged) = call(&service, Request::ConfigGet) else {
        panic!()
    };
    assert_eq!(unchanged.preset, "2.0");

    let Response::ConfigApplyPreset(changed) = call(
        &service,
        Request::ConfigApplyPreset(ConfigPresetParams {
            preset: "2.1".into(),
            confirm: true,
        }),
    ) else {
        panic!()
    };
    let token = changed.undo_token.unwrap();
    assert_eq!(changed.config.preset, "2.1");
    let Response::Undo(restored) = call(
        &service,
        Request::Undo(UndoParams {
            token: token.clone(),
        }),
    ) else {
        panic!()
    };
    assert_eq!(restored.positions, [1]);
    assert!(restored.dirty);
    assert_eq!(
        error(&service, Request::Undo(UndoParams { token })).code,
        ErrorCode::InvalidRequest
    );

    let Response::LayoutSet(layout) = call(
        &service,
        Request::LayoutSet(LayoutSetParams {
            count: Some(17),
            confirm: true,
            ..Default::default()
        }),
    ) else {
        panic!()
    };
    assert_eq!(layout.layout.count, 17);
    assert!(layout.undo_token.is_some());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_edit_surface_round_trips_dsp_layout_target_and_calibration() {
    let root = root("surface");
    let project = root.join("surface.secs");
    let target = root.join("target.json");
    let service = Service::new(Deps::default());
    new_project(&service, &project, "2.0");

    let Response::DspSet(dsp) = call(
        &service,
        Request::DspSet(DspSetParams {
            preset: None,
            fields: Some(DspFields {
                output_rate: OutputRateChoice::Fixed(48_000),
                output_rate_editable: false,
                fir_taps: 96_000,
                fir_budget: 48_000,
                iir_bands: 8,
                allpass_bands: 2,
                bm_path_delays: 3,
            }),
        }),
    ) else {
        panic!()
    };
    assert_eq!(dsp.profile, "custom");
    assert_eq!(dsp.window_ms, 2_000.0);
    assert_eq!(dsp.resolution_hz, 0.5);

    let Response::LayoutSet(layout) = call(
        &service,
        Request::LayoutSet(LayoutSetParams {
            count: Some(13),
            start_delay: Some("5 초".into()),
            timing: Some(LayoutTiming {
                reference: "none".into(),
                role: String::new(),
                output: 1,
            }),
            confirm: false,
        }),
    ) else {
        panic!()
    };
    assert_eq!(layout.layout.count, 13);
    assert_eq!(
        layout
            .layout
            .arrangements
            .iter()
            .map(|item| item.count)
            .collect::<Vec<_>>(),
        [1, 9, 13, 17]
    );
    assert_eq!(layout.layout.arrangements[3].points.len(), 17);
    let Response::PositionEstimate(estimate) = call(&service, Request::PositionEstimate) else {
        panic!()
    };
    assert_eq!(estimate.positions, 13);
    assert!(estimate.total_seconds > estimate.per_position_seconds);

    let Response::TargetSetPoints(target_result) = call(
        &service,
        Request::TargetSetPoints(TargetPointsParams {
            points: vec![(30_000.0, 40.0), (5.0, -40.0), (1_000.0, 2.0)],
        }),
    ) else {
        panic!()
    };
    assert_eq!(
        target_result.points,
        [(10.0, -30.0), (1_000.0, 2.0), (24_000.0, 30.0)]
    );
    assert_eq!(target_result.preset, None);
    assert!(matches!(
        call(
            &service,
            Request::TargetSave(PathParams {
                path: target.to_string_lossy().into_owned(),
            })
        ),
        Response::TargetSave(_)
    ));
    assert_ne!(fs::read(&target).unwrap().last(), Some(&b'\n'));
    let Response::TargetApplyPreset(preset) = call(
        &service,
        Request::TargetApplyPreset(TargetPresetParams {
            preset: "flat".into(),
        }),
    ) else {
        panic!()
    };
    assert_eq!(preset.preset.as_deref(), Some("flat"));
    let Response::TargetLoad(loaded) = call(
        &service,
        Request::TargetLoad(PathParams {
            path: target.to_string_lossy().into_owned(),
        }),
    ) else {
        panic!()
    };
    assert_eq!(loaded.points, target_result.points);
    assert_eq!(loaded.preset, None);

    let settings = CalibrationSettings {
        mode: Mode::Ultra,
        ultra_ms: 180.0,
        rc_lo: 25.0,
        ..CalibrationSettings::default()
    };
    let Response::CalibrationSettingsSet(stored) = call(
        &service,
        Request::CalibrationSettingsSet(CalibrationSettingsParams {
            settings: settings.clone(),
        }),
    ) else {
        panic!()
    };
    assert_eq!(stored, settings);
    let Response::CalibrationSettingsGet(read) = call(&service, Request::CalibrationSettingsGet)
    else {
        panic!()
    };
    assert_eq!(read, settings);
    let Response::ModeSpec(spec) = call(
        &service,
        Request::ModeSpec(ModeSpecParams {
            mode: Mode::Ultra,
            ultra_ms: Some(240.0),
            mimo_fir: None,
        }),
    ) else {
        panic!()
    };
    assert!(spec.fir_taps > 0);
    assert!(spec.max_latency_ms >= spec.lift_ms);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dsp_output_rate_follows_presets_and_only_camilla_or_custom_is_editable() {
    let root = root("dsp-output-rate");
    let project = root.join("dsp-output-rate.secs");
    let service = Service::new(Deps::default());
    new_project(&service, &project, "2.0");

    let Response::DspSet(minidsp) = call(
        &service,
        Request::DspSet(DspSetParams {
            preset: Some("minidsp".into()),
            fields: None,
        }),
    ) else {
        panic!()
    };
    assert_eq!(minidsp.fields.output_rate, OutputRateChoice::Fixed(96_000));
    assert!(!minidsp.fields.output_rate_editable);

    let mut refused = minidsp.fields.clone();
    refused.output_rate = OutputRateChoice::Fixed(44_100);
    let invalid = error(
        &service,
        Request::DspSet(DspSetParams {
            preset: None,
            fields: Some(refused),
        }),
    );
    assert_eq!(invalid.code, ErrorCode::InvalidRequest);
    assert_eq!(
        invalid.params.get("field").map(String::as_str),
        Some("output_rate")
    );

    let Response::DspSet(camilla) = call(
        &service,
        Request::DspSet(DspSetParams {
            preset: Some("camilla".into()),
            fields: None,
        }),
    ) else {
        panic!()
    };
    assert!(camilla.fields.output_rate_editable);
    // Changing only the rate keeps the preset.
    let mut editable = camilla.fields;
    editable.output_rate = OutputRateChoice::Auto;
    let Response::DspSet(auto) = call(
        &service,
        Request::DspSet(DspSetParams {
            preset: None,
            fields: Some(editable.clone()),
        }),
    ) else {
        panic!()
    };
    assert_eq!(auto.profile, "camilla");
    assert_eq!(auto.fields.output_rate, OutputRateChoice::Auto);
    assert!(auto.fields.output_rate_editable);
    // Any other field makes it custom, keeping the chosen rate.
    editable.fir_budget += 1024;
    let Response::DspSet(custom) = call(
        &service,
        Request::DspSet(DspSetParams {
            preset: None,
            fields: Some(editable),
        }),
    ) else {
        panic!()
    };
    assert_eq!(custom.profile, "custom");
    assert_eq!(custom.fields.output_rate, OutputRateChoice::Auto);
    assert!(custom.fields.output_rate_editable);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn mode_spec_follows_mimo_fir_1018() {
    let service = Service::new(Deps::default());
    for mode in [Mode::Zero, Mode::Mixed, Mode::Linear] {
        for (mimo_fir, expected) in [(Some(true), "fir"), (Some(false), "iir"), (None, "iir")] {
            let Response::ModeSpec(spec) = call(
                &service,
                Request::ModeSpec(ModeSpecParams {
                    mode,
                    ultra_ms: None,
                    mimo_fir,
                }),
            ) else {
                panic!()
            };
            assert_eq!(spec.mimo_filter, expected);
        }
    }
}

#[test]
fn custom_preset_is_empty_and_target_load_is_bounded() {
    let root = root("bounds");
    let project = root.join("bounds.secs");
    let target = root.join("large.json");
    let service = Service::new(Deps::default());
    new_project(&service, &project, "2.0");

    let Response::ConfigApplyPreset(custom) = call(
        &service,
        Request::ConfigApplyPreset(ConfigPresetParams {
            preset: "custom".into(),
            confirm: false,
        }),
    ) else {
        panic!()
    };
    assert!(custom.config.speakers.is_empty());
    assert_eq!(custom.config.preset, "custom");

    fs::write(&target, vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert_eq!(
        error(
            &service,
            Request::TargetLoad(PathParams {
                path: target.to_string_lossy().into_owned(),
            })
        )
        .code,
        ErrorCode::ProjectIo
    );
    fs::remove_dir_all(root).unwrap();
}

fn assert_config_oracle(result: &ConfigResult, fixture: &secs_oracle::Fixture) {
    assert_eq!(
        serde_json::to_value(&result.preset).unwrap(),
        fixture.outputs()["config"]["preset"],
        "{}",
        fixture.id
    );
    assert_eq!(
        serde_json::to_value(result.summary.sub_count).unwrap(),
        fixture.outputs()["config"]["sub_count"],
        "{}",
        fixture.id
    );
    assert_eq!(
        serde_json::to_value(&result.summary.outputs).unwrap(),
        fixture.outputs()["config"]["outputs"],
        "{}",
        fixture.id
    );
    assert_eq!(
        serde_json::to_value(
            result
                .speakers
                .iter()
                .map(|speaker| speaker.role.as_str())
                .collect::<Vec<_>>()
        )
        .unwrap(),
        fixture.outputs()["config"]["roles"],
        "{}",
        fixture.id
    );
}

#[test]
fn service_config_oracle_replays_values_and_saved_project_bytes() {
    let root = root("oracle");
    for fixture in secs_oracle::Fixture::all("service_config", "") {
        let inputs = fixture.inputs();
        let op = inputs["op"].as_str().unwrap();
        let path = root.join("project.secs");
        fs::write(&path, fixture.blob(&fixture.outputs()["before"])).unwrap();
        let service = Service::new(Deps::default());
        assert!(matches!(
            call(
                &service,
                Request::ProjectOpen(ProjectPathParams {
                    path: path.to_string_lossy().into_owned(),
                })
            ),
            Response::ProjectOpen(_)
        ));
        let response = match op {
            "set_sub_count" => call(
                &service,
                Request::ConfigSetSubCount(ConfigSubCountParams {
                    count: inputs["count"].as_i64().unwrap(),
                    confirm: false,
                }),
            ),
            "config_edits" => {
                assert!(matches!(
                    call(
                        &service,
                        Request::ConfigSetSubCount(ConfigSubCountParams {
                            count: 3,
                            confirm: false,
                        })
                    ),
                    Response::ConfigSetSubCount(_)
                ));
                assert!(matches!(
                    call(
                        &service,
                        Request::ConfigSetSubCount(ConfigSubCountParams {
                            count: 1,
                            confirm: false,
                        })
                    ),
                    Response::ConfigSetSubCount(_)
                ));
                assert!(matches!(
                    call(
                        &service,
                        Request::ConfigSetSpeaker(ConfigSetSpeakerParams {
                            index: 0,
                            role: Some("Lw".into()),
                            size: None,
                            band_index: Some(0),
                            out: Some(9),
                            pol: None,
                        })
                    ),
                    Response::ConfigSetSpeaker(_)
                ));
                assert!(matches!(
                    call(&service, Request::ConfigAddSpeaker),
                    Response::ConfigAddSpeaker(_)
                ));
                assert!(matches!(
                    call(
                        &service,
                        Request::ConfigRemoveSpeaker(ConfigIndexParams { index: 1 })
                    ),
                    Response::ConfigRemoveSpeaker(_)
                ));
                call(&service, Request::ConfigRenumber)
            }
            "validate" => call(&service, Request::ConfigValidate),
            "dsp_preset" => call(
                &service,
                Request::DspSet(DspSetParams {
                    preset: Some(inputs["preset"].as_str().unwrap().into()),
                    fields: None,
                }),
            ),
            "dsp_manual" => call(
                &service,
                Request::DspSet(DspSetParams {
                    preset: None,
                    fields: Some(DspFields {
                        output_rate: OutputRateChoice::Fixed(48_000),
                        output_rate_editable: false,
                        fir_taps: 96_000,
                        fir_budget: 48_000,
                        iir_bands: 8,
                        allpass_bands: 2,
                        bm_path_delays: 3,
                    }),
                }),
            ),
            "target_preset" => call(
                &service,
                Request::TargetApplyPreset(TargetPresetParams {
                    preset: inputs["preset"].as_str().unwrap().into(),
                }),
            ),
            "target_manual" => call(
                &service,
                Request::TargetSetPoints(TargetPointsParams {
                    points: vec![(10.0, -30.0), (1_000.0, 2.0), (24_000.0, 30.0)],
                }),
            ),
            "layout" => call(
                &service,
                Request::LayoutSet(LayoutSetParams {
                    count: Some(inputs["count"].as_u64().unwrap() as usize),
                    start_delay: Some("5 초".into()),
                    timing: Some(LayoutTiming {
                        reference: inputs["timing"].as_str().unwrap().into(),
                        role: "L".into(),
                        output: 1,
                    }),
                    confirm: false,
                }),
            ),
            "calibration" => call(
                &service,
                Request::CalibrationSettingsSet(CalibrationSettingsParams {
                    settings: CalibrationSettings {
                        mode: Mode::Ultra,
                        ultra_ms: 180.0,
                        rc_lo: 25.0,
                        mimo_on: true,
                        ..CalibrationSettings::default()
                    },
                }),
            ),
            "patch" => {
                let patch = root.join("patch.csv");
                let response = call(
                    &service,
                    Request::PatchSheetCsv(PathParams {
                        path: patch.to_string_lossy().into_owned(),
                    }),
                );
                assert_eq!(
                    fs::read(patch).unwrap(),
                    fixture.blob(&fixture.outputs()["patch"]),
                    "{}",
                    fixture.id
                );
                response
            }
            other => panic!("unknown oracle operation {other}"),
        };
        match response {
            Response::ConfigSetSubCount(result) => {
                assert_config_oracle(&result.config, &fixture);
            }
            Response::ConfigRenumber(result) => {
                assert_config_oracle(&result, &fixture);
            }
            Response::ConfigValidate(notices) => {
                assert_eq!(
                    notices
                        .iter()
                        .map(|notice| notice.code.as_str())
                        .collect::<Vec<_>>(),
                    [
                        "role_duplicate",
                        "out_duplicate",
                        "band_unknown",
                        "pol_invalid",
                        "band_combination",
                        "bands_empty",
                    ],
                    "{}",
                    fixture.id
                );
                assert_eq!(notices[0].params.get("role").map(String::as_str), Some("L"));
                assert_eq!(notices[1].params.get("out").map(String::as_str), Some("1"));
                assert_eq!(
                    notices[2].params.get("band").map(String::as_str),
                    Some("BAD")
                );
                assert_eq!(notices[3].params.get("pol").map(String::as_str), Some("x"));
            }
            Response::DspSet(result) => {
                assert_eq!(
                    serde_json::to_value(&result.profile).unwrap(),
                    fixture.outputs()["dsp"]["profile"],
                    "{}",
                    fixture.id
                );
                assert_eq!(
                    serde_json::to_value(&result.name).unwrap(),
                    fixture.outputs()["dsp"]["name"],
                    "{}",
                    fixture.id
                );
                assert_eq!(
                    result.fields.output_rate,
                    OutputRateChoice::Fixed(if result.profile == "minidsp" {
                        96_000
                    } else {
                        48_000
                    }),
                    "{}",
                    fixture.id
                );
                assert_eq!(
                    result.fields.output_rate_editable,
                    matches!(result.profile.as_str(), "camilla" | "custom"),
                    "{}",
                    fixture.id
                );
                let mut fields = serde_json::to_value(&result.fields).unwrap();
                let object = fields.as_object_mut().unwrap();
                object.remove("output_rate");
                object.remove("output_rate_editable");
                assert_eq!(
                    fields,
                    serde_json::json!({
                        "fir_taps": fixture.outputs()["dsp"]["fir_taps"],
                        "fir_budget": fixture.outputs()["dsp"]["fir_budget"],
                        "iir_bands": fixture.outputs()["dsp"]["iir_bands"],
                        "allpass_bands": fixture.outputs()["dsp"]["allpass_bands"],
                        "bm_path_delays": fixture.outputs()["dsp"]["bm_path_delays"],
                    }),
                    "{}",
                    fixture.id
                );
            }
            Response::TargetApplyPreset(result) | Response::TargetSetPoints(result) => {
                assert_eq!(
                    serde_json::to_value(&result.points).unwrap(),
                    fixture.outputs()["target"]["points"],
                    "{}",
                    fixture.id
                );
                assert_eq!(
                    serde_json::to_value(&result.preset).unwrap(),
                    fixture.outputs()["target"]["preset"],
                    "{}",
                    fixture.id
                );
            }
            Response::LayoutSet(_) => {
                let Response::PositionEstimate(result) = call(&service, Request::PositionEstimate)
                else {
                    panic!()
                };
                assert_eq!(
                    result.per_position_seconds,
                    fixture.outputs()["per_position_seconds"].as_f64().unwrap(),
                    "{}",
                    fixture.id
                );
                assert_eq!(
                    result.total_seconds,
                    fixture.outputs()["total_seconds"].as_f64().unwrap(),
                    "{}",
                    fixture.id
                );
            }
            Response::CalibrationSettingsSet(result) => {
                let mut actual = serde_json::to_value(result).unwrap();
                assert_eq!(actual["sample_rate_policy"], "reject", "{}", fixture.id);
                let object = actual.as_object_mut().unwrap();
                object.remove("sample_rate_policy");
                assert_eq!(
                    object.remove("peq_analytic_derivatives"),
                    Some(false.into())
                );
                assert_eq!(
                    object.remove("mimo_iir_analytic_derivatives"),
                    Some(false.into())
                );
                assert_eq!(actual, fixture.outputs()["calibration"], "{}", fixture.id);
            }
            Response::PatchSheetCsv(_) => {}
            other => panic!("unexpected oracle response: {other:?}"),
        }
        assert!(matches!(
            call(
                &service,
                Request::ProjectSave(ProjectSaveParams { path: None })
            ),
            Response::ProjectSave(_)
        ));
        let saved = fs::read_to_string(&path).unwrap();
        let legacy = saved
            .lines()
            .filter(|line| !line.trim_start().starts_with("\"output_rate\":"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert_eq!(
            legacy.as_bytes(),
            fixture.blob(&fixture.outputs()["project"]),
            "{}",
            fixture.id
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn app_settings_are_versioned_bounded_and_capped() {
    let root = root("settings");
    let path = root.join("settings.json");
    let service = Service::new(Deps {
        app_settings_path: Some(path.clone()),
        ..Deps::default()
    });
    let settings = AppSettings {
        compute: secs_types::compute::ComputePreference::Cpu,
        language: "en".into(),
        theme: AppTheme::Dark,
        icon: AppIcon::Civilian,
        skin: AppSkin::Rack,
        local_api: LocalApiSettings::default(),
        apply: ApplySettings::default(),
        updates: UpdatePolicy::Notify,
        recent_projects: (0..12).map(|index| format!("{index}.secs")).collect(),
        last_window_size: Some((1280, 720)),
    };
    let Response::AppSettingsSet(saved) = call(
        &service,
        Request::AppSettingsSet(AppSettingsParams { settings }),
    ) else {
        panic!()
    };
    assert_eq!(saved.recent_projects.len(), 10);
    let reopened = Service::new(Deps {
        app_settings_path: Some(path.clone()),
        ..Deps::default()
    });
    let Response::AppSettingsGet(loaded) = call(&reopened, Request::AppSettingsGet) else {
        panic!()
    };
    assert_eq!(loaded, saved);
    assert_eq!(loaded.skin, AppSkin::Rack);

    fs::write(
        &path,
        br#"{"version":1,"settings":{"language":"en","expert_display":true,"recent_projects":[],"last_window_size":null}}"#,
    )
    .unwrap();
    let old_file = Service::new(Deps {
        app_settings_path: Some(path.clone()),
        ..Deps::default()
    });
    let Response::AppSettingsGet(old_settings) = call(&old_file, Request::AppSettingsGet) else {
        panic!()
    };
    assert_eq!(
        old_settings.compute,
        secs_types::compute::ComputePreference::Auto
    );
    assert_eq!(old_settings.theme, AppTheme::System);
    assert_eq!(old_settings.icon, AppIcon::Expert);
    assert_eq!(old_settings.skin, AppSkin::Default);

    fs::write(&path, b"not json").unwrap();
    let broken = Service::new(Deps {
        app_settings_path: Some(path.clone()),
        ..Deps::default()
    });
    assert!(
        matches!(call(&broken, Request::AppSettingsGet), Response::AppSettingsGet(AppSettings { language, .. }) if language == "ko")
    );

    fs::write(&path, br#"{"version":99,"settings":{"language":"en","expert_display":true,"recent_projects":[],"last_window_size":null}}"#).unwrap();
    let unknown = Service::new(Deps {
        app_settings_path: Some(path.clone()),
        ..Deps::default()
    });
    assert!(
        matches!(call(&unknown, Request::AppSettingsGet), Response::AppSettingsGet(AppSettings { language, .. }) if language == "ko")
    );

    fs::write(&path, vec![b' '; 1024 * 1024 + 1]).unwrap();
    let oversized = Service::new(Deps {
        app_settings_path: Some(path.clone()),
        ..Deps::default()
    });
    assert!(
        matches!(call(&oversized, Request::AppSettingsGet), Response::AppSettingsGet(AppSettings { language, .. }) if language == "ko")
    );

    assert_eq!(
        error(
            &service,
            Request::AppSettingsSet(AppSettingsParams {
                settings: AppSettings {
                    last_window_size: Some((0, 720)),
                    ..AppSettings::default()
                },
            })
        )
        .code,
        ErrorCode::InvalidRequest
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_expert_preference_is_ignored_without_resetting_other_settings() {
    let root = root("legacy-expert");
    let path = root.join("settings.json");
    let deps = Deps {
        app_settings_path: Some(path.clone()),
        ..Deps::default()
    };
    for legacy_expert in [false, true] {
        for icon in [AppIcon::Expert, AppIcon::Civilian] {
            let expected = AppSettings {
                compute: secs_types::compute::ComputePreference::Cpu,
                language: "en".into(),
                theme: AppTheme::Dark,
                icon,
                skin: AppSkin::Default,
                local_api: LocalApiSettings::default(),
                apply: ApplySettings::default(),
                updates: UpdatePolicy::Auto,
                recent_projects: vec!["living-room.secs".into(), "studio.secs".into()],
                last_window_size: Some((1440, 900)),
            };
            let mut old_settings = serde_json::to_value(&expected).unwrap();
            old_settings["expert_display"] = legacy_expert.into();
            fs::write(
                &path,
                serde_json::to_vec(&serde_json::json!({
                    "version": 1,
                    "settings": old_settings,
                }))
                .unwrap(),
            )
            .unwrap();

            let service = Service::new(deps.clone());
            let Response::AppSettingsGet(loaded) = call(&service, Request::AppSettingsGet) else {
                panic!("settings must load")
            };
            assert_eq!(loaded, expected);

            // An older caller's extra field is ignored at the same IPC boundary.
            // The saved file and reply use the current field set.
            let Envelope::Ok(reply) = service.call_value(
                serde_json::json!({
                    "method": "app_settings_set",
                    "params": { "settings": old_settings },
                }),
                Arc::new(NoEvents),
            ) else {
                panic!("legacy settings payload must remain accepted")
            };
            let Response::AppSettingsSet(saved) = *reply else {
                panic!("settings must save")
            };
            assert_eq!(saved, expected);
            assert!(
                serde_json::to_value(&saved)
                    .unwrap()
                    .get("expert_display")
                    .is_none()
            );

            let persisted: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            assert_eq!(persisted["version"], 1);
            assert_eq!(
                persisted["settings"],
                serde_json::to_value(&expected).unwrap()
            );
            assert!(persisted["settings"].get("expert_display").is_none());

            let reopened = Service::new(deps.clone());
            let Response::AppSettingsGet(reloaded) = call(&reopened, Request::AppSettingsGet)
            else {
                panic!("rewritten settings must reload")
            };
            assert_eq!(reloaded, expected);
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn app_settings_writes_are_serialized_with_memory_and_bounded_by_the_read_cap() {
    let root = root("settings-race");
    let path = root.join("settings.json");
    let deps = Deps {
        app_settings_path: Some(path.clone()),
        ..Deps::default()
    };
    let service = Arc::new(Service::new(deps.clone()));
    // Each round two writers start together, as Tauri runs requests in parallel.
    for round in 0..200 {
        let start = Arc::new(std::sync::Barrier::new(2));
        let writers: Vec<_> = ["ko", "en"]
            .into_iter()
            .map(|language| {
                let service = service.clone();
                let start = start.clone();
                std::thread::spawn(move || {
                    start.wait();
                    call(
                        &service,
                        Request::AppSettingsSet(AppSettingsParams {
                            settings: AppSettings {
                                language: language.into(),
                                recent_projects: vec![format!("{language}-{round}.secs")],
                                ..AppSettings::default()
                            },
                        }),
                    );
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        let Response::AppSettingsGet(current) = call(&service, Request::AppSettingsGet) else {
            panic!()
        };
        let Response::AppSettingsGet(restored) =
            call(&Service::new(deps.clone()), Request::AppSettingsGet)
        else {
            panic!()
        };
        assert_eq!(current, restored, "round {round}");
    }

    let Response::AppSettingsGet(before) = call(&service, Request::AppSettingsGet) else {
        panic!()
    };
    let saved = fs::read(&path).unwrap();
    let oversized = service.call(
        Request::AppSettingsSet(AppSettingsParams {
            settings: AppSettings {
                recent_projects: vec!["x".repeat(1024 * 1024)],
                ..AppSettings::default()
            },
        }),
        Arc::new(NoEvents),
    );
    let Envelope::Err(oversized) = oversized else {
        panic!("settings over the 1 MiB read cap were saved");
    };
    assert_eq!(oversized.code, ErrorCode::InvalidRequest);
    assert_eq!(fs::read(&path).unwrap(), saved);
    assert_eq!(
        call(&service, Request::AppSettingsGet),
        Response::AppSettingsGet(before)
    );
    fs::remove_dir_all(root).unwrap();
}
