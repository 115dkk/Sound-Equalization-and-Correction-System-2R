#![forbid(unsafe_code)]

mod support;
use support::*;

#[test]
fn a_failed_project_save_after_a_recorded_measurement_leaves_the_project_dirty() {
    let root = temp_root("failed-record-save");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("room.secs");
    let service = service();
    call_ok(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: path.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    call_ok(
        &service,
        Request::MeasureConfigure(secs_types::ipc::MeasureConfigureParams {
            method: "mock".into(),
            ..Default::default()
        }),
        Arc::new(NoEvents),
    );
    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        Arc::new(NoEvents),
    );
    let Response::ProjectInfo(before) = call_ok(&service, Request::ProjectInfo, Arc::new(NoEvents))
    else {
        panic!()
    };
    assert!(!before.dirty);
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();

    let events = Arc::new(Events::default());
    let Response::MeasurePosition(started) = call_ok(
        &service,
        Request::MeasurePosition(secs_types::ipc::MeasurePositionParams { position: 1 }),
        events.clone(),
    ) else {
        panic!("measurement must start")
    };
    let JobEvent::Finished {
        status: JobStatus::Failed,
        error: Some(_),
        ..
    } = events.wait(started.job, Duration::from_secs(30))
    else {
        panic!("record must fail when the project destination is a directory")
    };
    let Response::ProjectInfo(info) = call_ok(&service, Request::ProjectInfo, Arc::new(NoEvents))
    else {
        panic!()
    };
    assert_eq!(info.positions, [1]);
    assert!(info.dirty);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn opening_a_missing_project_names_the_path_td208() {
    let root = temp_root("missing-project-open");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("missing.secs");
    let service = service();

    let Envelope::Err(error) = service.call(
        Request::ProjectOpen(secs_types::ipc::ProjectPathParams {
            path: path.to_string_lossy().into_owned(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("opening a missing project must fail")
    };
    assert_eq!(error.code, ErrorCode::ProjectIo);
    assert_eq!(
        error.params.get("path").map(String::as_str),
        Some(path.to_string_lossy().as_ref())
    );
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("not_found")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_open_surfaces_sidecar_warnings_td183() {
    let root = temp_root("sidecar-warning");
    let path = root.join("warning.secs");
    let irs = path.with_extension("irs");
    fs::create_dir_all(&irs).unwrap();
    let wav = irs.join("L_P1.wav");
    fs::write(&wav, []).unwrap();
    let sidecar = secs_project::source_path(&wav);
    fs::write(&sidecar, b"{not json").unwrap();
    let mut project = Project::from_preset("2.0").unwrap();
    project.measurements = secs_project::PyJson::Array(vec![secs_project::PyJson::Object(vec![
        ("pos".into(), secs_project::PyJson::Int("1".into())),
        (
            "ir".into(),
            secs_project::PyJson::Object(vec![(
                "L".into(),
                secs_project::PyJson::Str("warning.irs/L_P1.wav".into()),
            )]),
        ),
    ])]);
    fs::write(&path, project.save_text()).unwrap();
    let service = service();

    let Response::ProjectOpen(info) = call_ok(
        &service,
        Request::ProjectOpen(secs_types::ipc::ProjectPathParams {
            path: path.to_string_lossy().into_owned(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!();
    };

    assert!(info.migration_warnings.iter().any(|warning| {
        warning.code == "measurement.source.skipped" && warning.entry == sidecar.to_string_lossy()
    }));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_sidecar_curve_is_excluded_and_noticed_td262() {
    let root = temp_root("invalid-sidecar-curve");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let store = MeasurementStore::open(&path).unwrap();
    let first = store.records().into_iter().next().unwrap();
    let position = first.pos;
    let role = first.ir.keys().next().unwrap().clone();
    let wav = path.parent().unwrap().join(&first.ir[&role]);
    secs_project::save_source(
        &wav,
        Some(&secs_mdat::SourceInfo {
            cal: vec![(
                "meterCal".into(),
                secs_mdat::CalInfo {
                    inverse_c_selected: false,
                    source_name: "unsorted.cal".into(),
                    source_path: "unsorted.cal".into(),
                    freq: Some(vec![1_000.0, 100.0]),
                    gain: Some(vec![0.0, 1.0]),
                    phase: None,
                    boxed: Vec::new(),
                },
            )],
            ..Default::default()
        }),
    )
    .unwrap();
    let service = deterministic_service();
    open(&service, &path);

    let Response::MicCalibrationGet(status) =
        call_ok(&service, Request::MicCalibrationGet, Arc::new(NoEvents))
    else {
        panic!();
    };
    assert_eq!(status.attached, 0);

    let events = Arc::new(Events::default());
    let job = start_calibration(&service, events.clone(), settings);
    finished_result(events.wait(job, Duration::from_secs(120)));
    let results = read_results(&service);
    let notices = &results.calibration.unwrap().notices;
    assert!(notices.iter().any(|notice| {
        notice.code == "calibration_curve_invalid"
            && notice.params.get("role") == Some(&role)
            && notice.params.get("position") == Some(&position.to_string())
    }));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn damaged_settings_file_is_reported_and_not_overwritten_until_set_td169() {
    let root = temp_root("damaged-settings");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.json");
    let damaged = b"{not json";
    fs::write(&path, damaged).unwrap();
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink = lines.clone();
    let service = Service::new(Deps {
        app_settings_path: Some(path.clone()),
        log: Arc::new(move |level, scope, line| {
            lock(&sink).push((level, scope.to_owned(), line.to_owned()));
        }),
        ..Deps::default()
    });

    assert!(matches!(
        call_ok(&service, Request::AppSettingsGet, Arc::new(NoEvents)),
        Response::AppSettingsGet(settings) if settings == AppSettings::default()
    ));
    assert_eq!(fs::read(&path).unwrap(), damaged);
    let lines = lock(&lines);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].0, LogLevel::Warn);
    assert_eq!(lines[0].1, "settings");
    assert!(lines[0].2.contains(&path.to_string_lossy().into_owned()));
    drop(lines);

    let replacement = AppSettings {
        language: "en".into(),
        ..AppSettings::default()
    };
    assert!(matches!(
        call_ok(
            &service,
            Request::AppSettingsSet(AppSettingsParams {
                settings: replacement,
            }),
            Arc::new(NoEvents),
        ),
        Response::AppSettingsSet(_)
    ));
    assert_ne!(fs::read(&path).unwrap(), damaged);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn newer_settings_file_is_never_overwritten_td169() {
    let root = temp_root("newer-settings");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.json");
    let newer_settings = AppSettings {
        language: "newer".into(),
        ..AppSettings::default()
    };
    let original = serde_json::to_vec(&serde_json::json!({
        "version": 2,
        "settings": newer_settings,
    }))
    .unwrap();
    fs::write(&path, &original).unwrap();
    let service = Service::new(Deps {
        app_settings_path: Some(path.clone()),
        ..Deps::default()
    });

    let Envelope::Err(error) = service.call(
        Request::AppSettingsSet(AppSettingsParams {
            settings: AppSettings::default(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("a newer settings file must not be overwritten")
    };
    assert_eq!(error.code, ErrorCode::Unsupported);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("app_settings_newer")
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn newer_settings_file_written_while_running_is_never_overwritten_td169() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct CountingEffect(Arc<AtomicUsize>);

    impl SettingsEffect for CountingEffect {
        fn changed(&self, _previous: &AppSettings, _next: &AppSettings) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    let root = temp_root("newer-settings-while-running");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.json");
    let service = Service::new(Deps {
        app_settings_path: Some(path.clone()),
        ..Deps::default()
    });
    let calls = Arc::new(AtomicUsize::new(0));
    service.register_settings_effect(Arc::new(CountingEffect(calls.clone())));
    let original = serde_json::to_vec(&serde_json::json!({
        "version": 2,
        "settings": AppSettings {
            language: "newer".into(),
            ..AppSettings::default()
        },
    }))
    .unwrap();
    fs::write(&path, &original).unwrap();

    let Envelope::Err(error) = service.call(
        Request::AppSettingsSet(AppSettingsParams {
            settings: AppSettings {
                language: "replacement".into(),
                ..AppSettings::default()
            },
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("a settings file that became newer while running must not be overwritten")
    };
    assert_eq!(error.code, ErrorCode::Unsupported);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("app_settings_newer")
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unreadable_settings_file_is_never_overwritten_td169() {
    // A valid file over the 1 MiB read bound: this app cannot tell which
    // version wrote it, so an explicit set must not replace it (audit #186 A-14).
    let root = temp_root("unreadable-settings");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.json");
    let mut original = serde_json::to_vec(&serde_json::json!({
        "version": 2,
        "settings": AppSettings::default(),
        "padding": "x".repeat(1024 * 1024 + 16),
    }))
    .unwrap();
    original.push(b'\n');
    fs::write(&path, &original).unwrap();
    let service = Service::new(Deps {
        app_settings_path: Some(path.clone()),
        ..Deps::default()
    });

    let Envelope::Err(error) = service.call(
        Request::AppSettingsSet(AppSettingsParams {
            settings: AppSettings::default(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("a settings file this app cannot read must not be overwritten")
    };
    assert_eq!(error.code, ErrorCode::Unsupported);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("settings_unreadable")
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn observer_receives_every_caller_event_in_the_same_order() {
    let root = temp_root("observer");
    fs::create_dir_all(&root).unwrap();
    let observer = Arc::new(Events::default());
    let service = Service::new(Deps {
        observer: observer.clone(),
        ..Deps::default()
    });
    call_ok(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: root.join("sample.secs").to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: true,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    let caller = Arc::new(Events::default());
    let Response::SampleLoad(started) = call_ok(
        &service,
        Request::SampleLoad(Default::default()),
        caller.clone(),
    ) else {
        panic!("sample load must start")
    };
    observer.wait(started.job, Duration::from_secs(10));
    caller.wait(started.job, Duration::from_secs(10));
    assert_eq!(*lock(&caller.values), *lock(&observer.values));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn new_projects_match_upstream_bytes() {
    let root = temp_root("new-projects");
    fs::create_dir_all(&root).unwrap();
    for fixture in secs_oracle::Fixture::all("service_new", "") {
        let inputs = fixture.inputs();
        let path = root.join("project.secs");
        let response = call_ok(
            &service(),
            Request::ProjectNew(ProjectNewParams {
                path: path.to_string_lossy().into_owned(),
                preset: inputs["preset"].as_str().unwrap().into(),
                sub_count: inputs["sub_count"].as_u64().unwrap() as u8,
                sample: inputs["sample"].as_bool().unwrap(),
                tutorial: false,
            }),
            Arc::new(NoEvents),
        );
        assert!(
            matches!(response, Response::ProjectNew(_)),
            "{}",
            fixture.id
        );
        let mut created =
            secs_project::Project::load_text(&fs::read_to_string(&path).unwrap()).unwrap();
        created.extra.retain(|(key, _)| key != "devices");
        let saved = created.save_text();
        let legacy = saved
            .lines()
            .filter(|line| !line.trim_start().starts_with("\"output_rate\":"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert_eq!(
            legacy.into_bytes(),
            fixture.blob(&fixture.outputs()["project"]),
            "{}",
            fixture.id
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn new_projects_store_none_measurement_method() {
    let root = temp_root("measurement-method-default");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("new.secs");
    let service = service();
    call_ok(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: path.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    let project = secs_project::Project::load_text(&fs::read_to_string(&path).unwrap()).unwrap();
    let devices = project
        .extra
        .iter()
        .find(|(key, _)| key == "devices")
        .map(|(_, value)| value)
        .unwrap();
    let secs_project::PyJson::Object(devices) = devices else {
        panic!("devices must be an object");
    };
    assert_eq!(
        devices
            .iter()
            .find(|(key, _)| key == "method")
            .map(|(_, value)| value),
        Some(&secs_project::PyJson::Str("none".into()))
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tutorial_folder_lives_under_app_data_and_refuses_an_existing_path_td210() {
    let root = temp_root("tutorial-app-data");
    let app_data = root.join("app-data");
    fs::create_dir_all(&root).unwrap();
    let service = Service::new(Deps {
        app_data_path: app_data.clone(),
        ..Deps::default()
    });
    let create = |name: &str| {
        call_ok(
            &service,
            Request::ProjectNew(ProjectNewParams {
                path: root.join(name).to_string_lossy().into_owned(),
                preset: "2.0".into(),
                sub_count: 0,
                sample: false,
                tutorial: true,
            }),
            Arc::new(NoEvents),
        );
        let mut folders = fs::read_dir(app_data.join("tutorial"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        folders.sort();
        assert_eq!(folders.len(), 1);
        folders.remove(0)
    };

    let first = create("first.secs");
    assert!(first.starts_with(&app_data));
    assert_ne!(first.parent(), Some(std::env::temp_dir().as_path()));
    let second = create("second.secs");
    assert!(second.starts_with(&app_data));
    assert_ne!(second, first);
    assert!(!first.exists());

    drop(service);
    assert!(!second.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tutorial_projects_block_save_without_writing() {
    let root = temp_root("tutorial");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("tutorial.secs");
    let save_as = root.join("saved.secs");
    let service = service();
    let Response::ProjectNew(info) = call_ok(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: path.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: true,
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("tutorial must be created");
    };
    assert!(info.tutorial);
    assert!(!path.exists());
    let Envelope::Err(error) = service.call(
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams {
            path: Some(save_as.to_string_lossy().into_owned()),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("tutorial save must fail");
    };
    assert_eq!(error.code, ErrorCode::TutorialBlocked);
    let export_folder = root.join("export");
    let Envelope::Err(error) = service.call(
        Request::Export(ExportParams {
            kind: ExportKind::CamillaDsp,
            folder: export_folder.to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("tutorial export must fail");
    };
    assert_eq!(error.code, ErrorCode::TutorialBlocked);
    assert!(!path.exists());
    assert!(!save_as.exists());
    assert!(!export_folder.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn bootstrap_carries_the_settings_notice() {
    let root = temp_root("bootstrap-settings-notice");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.json");
    fs::write(&path, b"{not json").unwrap();
    let service = Service::new(Deps {
        app_settings_path: Some(path.clone()),
        ..Deps::default()
    });
    let Response::Bootstrap(info) = call_ok(&service, Request::Bootstrap, Arc::new(NoEvents))
    else {
        panic!("bootstrap response must match the request")
    };
    let notice = info.settings_notice.expect("settings notice is absent");
    assert_eq!(notice.code, "app_settings_unreadable");
    assert_eq!(
        notice.params.get("path").map(String::as_str),
        Some(path.to_string_lossy().as_ref())
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn bootstrap_reports_version_and_upstream_base() {
    let service = Service::new(Deps {
        app_version: "42.0.0".into(),
        ..Deps::default()
    });
    let Envelope::Ok(response) = service.call(Request::Bootstrap, Arc::new(NoEvents)) else {
        panic!("bootstrap must succeed");
    };
    let Response::Bootstrap(info) = *response else {
        panic!("bootstrap response must match the request");
    };
    assert_eq!(info.app_name, "SECS 2R");
    assert_eq!(info.version, "42.0.0");
    assert!(!info.prerelease);
    assert_eq!(info.upstream.commit.len(), 40);
    assert_eq!(info.log_dir, None);

    let root = temp_root("bootstrap-log-dir");
    let settings_path = root.join("settings.json");
    let settings_only = Service::new(Deps {
        app_settings_path: Some(settings_path.clone()),
        ..Deps::default()
    });
    let Response::Bootstrap(info) = call_ok(&settings_only, Request::Bootstrap, Arc::new(NoEvents))
    else {
        panic!("bootstrap response must match the request");
    };
    assert_eq!(info.log_dir, None);

    let log_dir = root.join("actual-logs");
    let service = Service::new(Deps {
        app_settings_path: Some(settings_path),
        log_dir: Some(log_dir.clone()),
        ..Deps::default()
    });
    let Response::Bootstrap(info) = call_ok(&service, Request::Bootstrap, Arc::new(NoEvents))
    else {
        panic!("bootstrap response must match the request");
    };
    assert_eq!(info.log_dir, Some(log_dir.to_string_lossy().into_owned()));
}

#[test]
fn malformed_requests_become_invalid_request_envelopes() {
    let service = service();
    for request in [
        serde_json::json!({"method": "no_such_method"}),
        serde_json::json!({"params": {}}),
        serde_json::json!("bootstrap"),
    ] {
        let Envelope::Err(error) = service.call_value(request, Arc::new(NoEvents)) else {
            panic!("request must fail");
        };
        assert_eq!(error.code, ErrorCode::InvalidRequest);
    }
}

#[test]
fn named_filter_bundles_replace_heal_index_hash_every_file_and_share_design_id() {
    let root = temp_root("named-filter-bundles");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("stereo.secs");
    let base = root.join("exports");
    let service = deterministic_service();
    call_ok(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: path.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    call_ok(
        &service,
        Request::MeasureConfigure(secs_types::ipc::MeasureConfigureParams {
            method: "mock".into(),
            ..Default::default()
        }),
        Arc::new(NoEvents),
    );
    let measure_events = Arc::new(Events::default());
    let Response::MeasurePosition(measure) = call_ok(
        &service,
        Request::MeasurePosition(secs_types::ipc::MeasurePositionParams { position: 1 }),
        measure_events.clone(),
    ) else {
        panic!("measurement must start");
    };
    finished_result(measure_events.wait(measure.job, Duration::from_secs(120)));
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(
        &service,
        calibration_events.clone(),
        CalibrationSettings::default(),
    );
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    // An explicit folder is the named bundle's base: neither the project's
    // export folder nor the host's home directory (not implemented here) is read.
    let explicit = root.join("explicit");
    let events = Arc::new(Events::default());
    let Response::Export(started) = call_ok(
        &service,
        Request::Export(ExportParams {
            kind: ExportKind::CamillaDsp,
            folder: explicit.to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: Some("Cinema".into()),
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        events.clone(),
    ) else {
        panic!("a named export into an explicit folder must start");
    };
    let JobResult::Export(result) =
        finished_result(events.wait(started.job, Duration::from_secs(120)))
    else {
        panic!("a named export into an explicit folder must finish");
    };
    assert_eq!(Path::new(&result.folder).parent(), Some(explicit.as_path()));
    assert!(explicit.join("designs.json").is_file());

    call_ok(
        &service,
        Request::ExportFolderSet(secs_types::ipc::ExportFolderSetParams {
            folder: base.to_string_lossy().into_owned(),
        }),
        Arc::new(NoEvents),
    );

    let Envelope::Err(error) = service.call(
        Request::Export(ExportParams {
            kind: ExportKind::FilterText,
            folder: String::new(),
            mdat_scope: None,
            design_name: Some("bad/name".into()),
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("an invalid design name must fail synchronously");
    };
    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("design_name_character")
    );
    assert!(!base.exists());

    let export = |kind, design_name: Option<&str>| {
        let events = Arc::new(Events::default());
        let Response::Export(started) = call_ok(
            &service,
            Request::Export(ExportParams {
                kind,
                folder: String::new(),
                mdat_scope: None,
                design_name: design_name.map(str::to_owned),
                camilla_devices: None,
                extra_rates: Vec::new(),
            }),
            events.clone(),
        ) else {
            panic!("filter export must start");
        };
        let JobResult::Export(result) =
            finished_result(events.wait(started.job, Duration::from_secs(120)))
        else {
            panic!("filter export must finish");
        };
        result
    };

    let first = export(ExportKind::CamillaDsp, Some("Cinema"));
    let named_folder = PathBuf::from(&first.folder);
    fs::write(named_folder.join("obsolete.txt"), b"old").unwrap();
    fs::write(base.join("designs.json"), b"corrupt").unwrap();
    let second = export(ExportKind::CamillaDsp, Some("Cinema"));
    assert_eq!(first.folder, second.folder);
    assert!(!named_folder.join("obsolete.txt").exists());
    let index: serde_json::Value =
        serde_json::from_slice(&fs::read(base.join("designs.json")).unwrap()).unwrap();
    assert_eq!(index.as_array().unwrap().len(), 1);
    assert_eq!(index[0]["design_name"], "Cinema");

    fn count_files(folder: &Path) -> usize {
        fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .map(|path| if path.is_dir() { count_files(&path) } else { 1 })
            .sum()
    }

    let mut design_ids = Vec::new();
    for result in [
        second,
        export(ExportKind::FilterText, Some("Cinema")),
        export(ExportKind::EqualizerApo, Some("Cinema")),
        export(ExportKind::Convolution, Some("Cinema")),
    ] {
        assert_eq!(result.output_rate, 48_000);
        assert!(result.notes.is_empty());
        let folder = PathBuf::from(&result.folder);
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(folder.join("secs-design.json")).unwrap()).unwrap();
        assert_eq!(manifest["sample_rate"], 48_000);
        assert_eq!(manifest["design_sample_rate"], 48_000);
        assert_eq!(manifest["output_rate_auto"], false);
        design_ids.push(manifest["design_id"].as_str().unwrap().to_owned());
        let files = manifest["files"].as_array().unwrap();
        assert_eq!(files.len() + 1, count_files(&folder));
        for record in files {
            let bytes = fs::read(folder.join(record["path"].as_str().unwrap())).unwrap();
            assert_eq!(record["bytes"], bytes.len() as u64);
            let hash: String = Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            assert_eq!(record["sha256"], hash);
        }
    }
    assert!(design_ids.windows(2).all(|pair| pair[0] == pair[1]));

    let unnamed = export(ExportKind::FilterText, None);
    assert!(
        Path::new(&unnamed.folder)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("20240102-030405")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn save_as_failure_and_nested_destination_leave_projects_untouched() {
    let root = temp_root("save-as-stage");
    fs::create_dir_all(&root).unwrap();
    let source = root.join("source.secs");
    let reverse_service = service();
    let service = service();
    call_ok(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: source.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    let source_irs = source.with_extension("irs");
    fs::create_dir_all(&source_irs).unwrap();
    fs::write(source_irs.join("marker"), b"source").unwrap();

    let nested = source_irs.join("inner.secs");
    let Envelope::Err(error) = service.call(
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams {
            path: Some(nested.to_string_lossy().into_owned()),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("nested save as must fail");
    };
    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert_eq!(
        error.detail,
        "cannot save a project inside its own measurement folder"
    );
    assert!(!nested.exists());
    assert!(!nested.with_extension("irs").exists());

    let reverse_root = root.join("case.irs");
    fs::create_dir(&reverse_root).unwrap();
    let reverse_source = reverse_root.join("room.secs");
    call_ok(
        &reverse_service,
        Request::ProjectNew(ProjectNewParams {
            path: reverse_source.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    let reverse_source_irs = reverse_source.with_extension("irs");
    fs::create_dir_all(&reverse_source_irs).unwrap();
    fs::write(reverse_root.join("keep.txt"), b"keep").unwrap();
    fs::write(reverse_source_irs.join("marker"), b"reverse").unwrap();
    let reverse_destination = root.join("case.secs");
    let Envelope::Err(error) = reverse_service.call(
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams {
            path: Some(reverse_destination.to_string_lossy().into_owned()),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("save as whose measurement folder contains the source must fail");
    };
    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert!(reverse_source.is_file());
    assert_eq!(fs::read(reverse_root.join("keep.txt")).unwrap(), b"keep");
    assert_eq!(
        fs::read(reverse_source_irs.join("marker")).unwrap(),
        b"reverse"
    );
    assert!(!reverse_destination.exists());

    let target = root.join("target.secs");
    fs::create_dir(&target).unwrap();
    let target_irs = target.with_extension("irs");
    fs::create_dir(&target_irs).unwrap();
    fs::write(target_irs.join("marker"), b"destination").unwrap();
    let Envelope::Err(error) = service.call(
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams {
            path: Some(target.to_string_lossy().into_owned()),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("unwritable project path must fail");
    };
    assert_eq!(error.code, ErrorCode::ProjectIo);
    let Response::ProjectInfo(info) = call_ok(&service, Request::ProjectInfo, Arc::new(NoEvents))
    else {
        panic!();
    };
    assert_eq!(info.path, source.to_string_lossy());
    assert_eq!(fs::read(target_irs.join("marker")).unwrap(), b"destination");
    assert_eq!(fs::read(source_irs.join("marker")).unwrap(), b"source");
    assert_eq!(
        fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .count(),
        0
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn extension_only_save_as_without_a_source_measurement_folder_succeeds_td04() {
    let root = temp_root("save-as-extension-only");
    fs::create_dir_all(&root).unwrap();
    let source = root.join("room.secs");
    let destination = root.join("room.backup");
    let service = service();
    call_ok(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: source.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    assert!(!source.with_extension("irs").exists());

    let Response::ProjectSave(info) = call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams {
            path: Some(destination.to_string_lossy().into_owned()),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!();
    };
    assert_eq!(info.path, destination.to_string_lossy());
    assert!(source.is_file());
    assert!(destination.is_file());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn measurement_handlers_are_connected_and_fail_with_typed_errors() {
    use secs_types::ipc::{
        MdatImportParams, MdatScanParams, MeasureConfigureParams, MeasurePositionParams,
        PositionClearParams, UndoParams,
    };

    let service = service();
    let not_open = Err((ErrorCode::ProjectNotOpen, None));
    let cases = [
        (Request::MeasureMethods, Ok("measure_methods")),
        (
            Request::MeasureConfigure(MeasureConfigureParams {
                method: "mock".into(),
                ..Default::default()
            }),
            not_open,
        ),
        (
            Request::MeasureConfigure(MeasureConfigureParams::default()),
            Err((ErrorCode::MeasurementInput, Some(("field", "method")))),
        ),
        (
            Request::MeasurePosition(MeasurePositionParams { position: 1 }),
            not_open,
        ),
        (
            Request::PositionClear(PositionClearParams { position: 1 }),
            not_open,
        ),
        (
            Request::Undo(UndoParams {
                token: "undo".into(),
            }),
            not_open,
        ),
        // The headless host reports its structured launch failure.
        (
            Request::RewLaunch,
            Err((ErrorCode::RewFailed, Some(("reason", "cannot_launch")))),
        ),
        (
            Request::MdatScan(MdatScanParams {
                path: "input.mdat".into(),
            }),
            Err((ErrorCode::MeasurementInput, Some(("field", "path")))),
        ),
        (
            Request::MdatImport(MdatImportParams {
                path: "input.mdat".into(),
                assignments: vec![],
                position: 1,
            }),
            not_open,
        ),
    ];
    // Without an open project (and with a file that does not exist) each
    // measurement handler answers exactly as below; none is a stub any more.
    for (request, expected) in cases {
        let method = request.method();
        match (service.call(request, Arc::new(NoEvents)), expected) {
            (Envelope::Ok(response), Ok(name)) => {
                assert_eq!(serde_json::to_value(&response).unwrap()["method"], name)
            }
            (Envelope::Err(error), Err((code, param))) => {
                assert_eq!(error.code, code, "{method}: {error:?}");
                if let Some((key, value)) = param {
                    assert_eq!(
                        error.params.get(key).map(String::as_str),
                        Some(value),
                        "{method}"
                    );
                }
            }
            (envelope, expected) => panic!("{method}: {envelope:?}, expected {expected:?}"),
        }
    }
}

#[test]
fn project_replacement_rejects_old_compute_commit_and_preserves_completion() {
    for mimo in [false, true] {
        let root = temp_root(if mimo {
            "replace-mimo"
        } else {
            "replace-calibration"
        });
        fs::create_dir_all(&root).unwrap();
        let (source, mut settings) = write_case(&root, "room_dba");
        settings.mimo_on = true;
        settings.mimo_fir = true;
        settings.mimo_max_supports = 1;
        let service = service();
        open(&service, &source);
        if mimo {
            let events = Arc::new(Events::default());
            let id = start_calibration(&service, events.clone(), settings.clone());
            finished_result(events.wait(id, Duration::from_secs(120)));
        }
        let previous_cache =
            fs::read(source.with_extension("irs").join("secs_run_cache.json")).ok();
        let events = Arc::new(PausedProgress::default());
        let request = if mimo {
            Request::Mimo(MimoParams {
                settings: Some(settings),
            })
        } else {
            Request::Calibrate(CalibrateParams {
                settings: Some(settings),
                ..Default::default()
            })
        };
        let response = call_ok(&service, request, events.clone());
        let job = match response {
            Response::Mimo(started) | Response::Calibrate(started) => started.job,
            _ => panic!(),
        };
        events.wait_until_paused();
        let replacement = root.join("replacement.secs");
        let create = Request::ProjectNew(ProjectNewParams {
            path: replacement.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        });
        if mimo {
            // Exercise open as well as new while the old compute is active.
            call_ok(&Service::new(Deps::default()), create, Arc::new(NoEvents));
            open(&service, &replacement);
        } else {
            call_ok(&service, create, Arc::new(NoEvents));
        }
        events.release();
        let finished = events.events.wait(job, Duration::from_secs(120));
        assert!(
            matches!(finished, JobEvent::Finished { status: JobStatus::Failed, result: None, error: Some(ref error), .. } if error.code == ErrorCode::StaleInputs)
        );
        let current = read_results(&service);
        assert!(current.calibration.is_none() && current.mimo.is_none());
        let Response::ProjectInfo(info) =
            call_ok(&service, Request::ProjectInfo, Arc::new(NoEvents))
        else {
            panic!()
        };
        assert!(!info.dirty);
        assert!(
            !replacement
                .with_extension("irs")
                .join("secs_run_cache.json")
                .exists()
        );
        assert_eq!(
            fs::read(source.with_extension("irs").join("secs_run_cache.json")).ok(),
            previous_cache
        );
        let Response::Jobs(jobs) = call_ok(&service, Request::Jobs, Arc::new(NoEvents)) else {
            panic!()
        };
        assert!(
            jobs.iter()
                .any(|snapshot| snapshot.job == job && snapshot.status == JobStatus::Failed)
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn save_as_into_a_folder_that_does_not_exist_yet_carries_the_measurements() {
    let root = temp_root("save-as-new-folder");
    fs::create_dir_all(&root).unwrap();
    let source = root.join("source.secs");
    let service = service();
    call_ok(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: source.to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    let source_irs = source.with_extension("irs");
    fs::create_dir_all(&source_irs).unwrap();
    fs::write(source_irs.join("marker"), b"source").unwrap();

    let target = root.join("new").join("deeper").join("copy.secs");
    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams {
            path: Some(target.to_string_lossy().into_owned()),
        }),
        Arc::new(NoEvents),
    );
    assert!(target.is_file());
    assert_eq!(
        fs::read(target.with_extension("irs").join("marker")).unwrap(),
        b"source"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn settings_effect_runs_once_after_a_successful_set_and_never_after_a_refusal() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct CountingEffect(Arc<AtomicUsize>);

    impl SettingsEffect for CountingEffect {
        fn changed(&self, _previous: &AppSettings, _next: &AppSettings) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    let root = temp_root("app-settings-observer");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.json");
    let calls = Arc::new(AtomicUsize::new(0));
    let service = Service::new(Deps {
        app_settings_path: Some(path.clone()),
        ..Deps::default()
    });
    service.register_settings_effect(Arc::new(CountingEffect(calls.clone())));

    let accepted = service.call(
        Request::AppSettingsSet(AppSettingsParams {
            settings: AppSettings {
                language: "en".into(),
                ..AppSettings::default()
            },
        }),
        Arc::new(NoEvents),
    );
    assert!(matches!(accepted, Envelope::Ok(_)));
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let refused = service.call(
        Request::AppSettingsSet(AppSettingsParams {
            settings: AppSettings {
                last_window_size: Some((0, 720)),
                ..AppSettings::default()
            },
        }),
        Arc::new(NoEvents),
    );
    assert!(matches!(refused, Envelope::Err(_)));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[cfg(windows)]
fn opening_an_unreadable_project_is_a_permission_error_td208() {
    check_opening_an_unreadable_project_is_a_permission_error_td208();
}
