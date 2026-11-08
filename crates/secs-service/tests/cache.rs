#![allow(deprecated)]
#![forbid(unsafe_code)]

mod support;
use support::*;

#[test]
fn reopening_after_calibrating_with_an_invalid_curve_keeps_results_fresh_td262() {
    let root = temp_root("invalid-sidecar-reopen");
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
                    inverse_c_selected: true,
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

    let events = Arc::new(Events::default());
    let job = start_calibration(&service, events.clone(), settings);
    finished_result(events.wait(job, Duration::from_secs(120)));
    let live = read_results(&service).calibration.unwrap();
    assert!(!live.stale);
    assert_eq!(
        live.notices
            .iter()
            .filter(|notice| {
                notice.code == "calibration_curve_invalid"
                    && notice.params.get("role") == Some(&role)
                    && notice.params.get("position") == Some(&position.to_string())
            })
            .count(),
        1
    );

    let save_events = Arc::new(Events::default());
    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        save_events.clone(),
    );
    let save_job = save_events.wait_started(JobKind::Export, Duration::from_secs(2));
    assert!(matches!(
        save_events.wait(save_job, Duration::from_secs(30)),
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            ..
        }
    ));
    drop(service);

    let reopened = deterministic_service();
    open(&reopened, &path);
    let cached = read_results(&reopened).calibration.unwrap();
    assert!(
        !cached.stale,
        "invalid calibration changed the reopened signature"
    );
    assert_eq!(
        cached
            .notices
            .iter()
            .filter(|notice| notice.code == "calibration_curve_invalid")
            .count(),
        1
    );
    let Response::MicCalibrationGet(status) =
        call_ok(&reopened, Request::MicCalibrationGet, Arc::new(NoEvents))
    else {
        panic!();
    };
    assert_eq!(status.attached, 0);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn resampled_calibration_and_cached_mdat_export_preserve_original_ir() {
    use secs_types::calibration::SampleRatePolicy;
    let root = temp_root("resample-flow");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    let ir_path = secs_project::ir_file(&path, "L", 1);
    let (samples, _) = secs_project::read_wav_float32(&fs::read(&ir_path).unwrap()).unwrap();
    fs::write(&ir_path, secs_project::write_wav_float32(&samples, 96_000)).unwrap();
    let original = fs::read(&ir_path).unwrap();
    let original_project: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let service = deterministic_service();
    open(&service, &path);

    let rejected = Arc::new(Events::default());
    let job = start_calibration(&service, rejected.clone(), settings.clone());
    let JobEvent::Finished {
        status,
        error: Some(error),
        ..
    } = rejected.wait(job, Duration::from_secs(120))
    else {
        panic!("default policy must reject the mismatched measurement")
    };
    assert_eq!(status, JobStatus::Failed);
    assert_eq!(
        error.params.get("actual_hz").map(String::as_str),
        Some("96000")
    );

    settings.sample_rate_policy = SampleRatePolicy::Resample;
    let events = Arc::new(Events::default());
    let job = start_calibration(&service, events.clone(), settings);
    finished_result(events.wait(job, Duration::from_secs(120)));
    assert_eq!(fs::read(&ir_path).unwrap(), original);
    let summary = read_results(&service).calibration.unwrap();
    let notice = summary
        .notices
        .iter()
        .find(|notice| notice.code == "measurement_resampled")
        .expect("converted measurements must be visible in the calculation notes");
    assert_eq!(notice.params["count"], "1");
    assert_eq!(notice.params["target_hz"], "48000");
    let labels: serde_json::Value = cached_field(&path, "labels");
    assert_eq!(labels["sample_rate_policy"], "resample");
    let xo_events = Arc::new(Events::default());
    let Response::RecommendXo(xo) =
        call_ok(&service, Request::RecommendXo(None), xo_events.clone())
    else {
        panic!("XO recommendation must start with the resampling policy")
    };
    assert!(matches!(
        finished_result(xo_events.wait(xo.job, Duration::from_secs(120))),
        JobResult::Xo(_)
    ));

    // Saving another preparation policy makes the result stale. A stale cached
    // design cannot be rebuilt for mdat until calibration runs again.
    call_ok(
        &service,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: secs_types::settings_edit::SettingEdit::SampleRatePolicy {
                policy: SampleRatePolicy::Reject,
            },
        }),
        Arc::new(NoEvents),
    );
    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        Arc::new(NoEvents),
    );
    let reopened = deterministic_service();
    open(&reopened, &path);
    assert!(read_results(&reopened).calibration.unwrap().stale);
    let Envelope::Err(error) = reopened.call(
        Request::Export(ExportParams {
            kind: ExportKind::StagedMdat,
            folder: root.join("cached-mdat").to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("stale cached mdat export must fail")
    };
    assert_eq!(error.code, ErrorCode::ExportScope);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("stale_cache")
    );
    let originals = export_scope_and_wait(
        &reopened,
        ExportKind::BeforeAfterMdat,
        &root.join("cached-originals"),
        Some(secs_types::ipc::MdatScope::Measurements),
        Arc::new(Events::default()),
    );
    assert!(!originals.stale);
    let loaded = secs_mdat::load_mdat_project(
        &fs::read(&originals.files[0]).unwrap(),
        &secs_jos::Limits::default(),
    )
    .unwrap();
    assert_eq!(
        loaded
            .measurements
            .iter()
            .find(|measurement| measurement.title == "L_P1")
            .unwrap()
            .sample_rate,
        96_000
    );
    assert_eq!(fs::read(&ir_path).unwrap(), original);
    assert_eq!(secs_project::read_wav_float32(&original).unwrap().1, 96_000);
    let saved_project: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        saved_project["measurements"],
        original_project["measurements"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn filter_text_export_matches_direct_and_reopened_bytes_without_redesign() {
    let root = temp_root("filter-text");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("stereo.secs");
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
    let settings = CalibrationSettings::default();
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&service, calibration_events.clone(), settings.clone());
    let JobResult::Calibration =
        finished_result(calibration_events.wait(calibration, Duration::from_secs(120)))
    else {
        panic!("calibration completion must be returned");
    };
    let results = read_results(&service);
    let summary = results.calibration.unwrap();
    assert!(summary.sub_roles.is_empty());
    let chain = cached_chain(&path);

    let folder = root.join("filter-text-export");
    let live = export_and_wait(
        &service,
        ExportKind::FilterText,
        &folder,
        Arc::new(Events::default()),
    );
    let live_snapshot = snapshot_relative_files(&folder, &live.files);
    let secs_types::notes::Note::RcLimit {
        hz: rc_hi_hz,
        auto: rc_auto,
        schroeder_hz,
        ..
    } = summary
        .notes
        .iter()
        .find(|note| matches!(note, secs_types::notes::Note::RcLimit { .. }))
        .unwrap()
    else {
        unreachable!("matched the RC limit variant")
    };
    let rc_pieces = secs_export::rc_pieces_from_values(
        *rc_hi_hz,
        *rc_auto,
        *schroeder_hz,
        settings.room_volume_m3,
        false,
    );
    let label = settings.label();
    let direct_run = secs_filters::FilterRun {
        chain: &chain,
        mimo: None,
        mode: "mixed",
        note: &secs_camilla::note::eqapo_note("mixed", summary.latency_ms, *rc_hi_hz, None),
        label: &label,
        latency_ms: summary.latency_ms,
        rc_pieces: &rc_pieces,
        stamp: "2024-01-02 03:04",
    };
    let direct_files = secs_filters::export_filter_text(&direct_run, &folder).unwrap();
    compare_snapshot(&folder, &live_snapshot, &direct_files);

    let reopened = deterministic_service();
    open(&reopened, &path);
    let reopened_events = Arc::new(Events::default());
    let cached = export_and_wait(
        &reopened,
        ExportKind::FilterText,
        &folder,
        reopened_events.clone(),
    );
    assert_eq!(
        live_snapshot,
        snapshot_relative_files(&folder, &cached.files)
    );
    assert!(
        !lock(&reopened_events.values).iter().any(|event| {
            matches!(event, JobEvent::Progress { stage, .. } if stage == "design")
        })
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn exports_after_reopening_match_pre_close_bytes() {
    let root = temp_root("reopened-exports");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "dolby");
    let measured = secs_project::MeasurementStore::open(&path).unwrap();
    let first = measured.records().into_iter().next().unwrap();
    let pos = u32::try_from(first.pos).unwrap();
    let role = first.ir.keys().next().unwrap().clone();
    let wav = path
        .with_extension("irs")
        .join(format!("{role}_P{pos}.wav"));
    secs_project::save_source(
        &wav,
        Some(&secs_mdat::SourceInfo {
            strings: vec![("outputName".into(), "원 측정 장치".into())],
            date: Some(secs_mdat::SourceDate::Int(FIXED_MS)),
            ..Default::default()
        }),
    )
    .unwrap();
    let running = deterministic_service();
    open(&running, &path);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&running, calibration_events.clone(), settings);
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    for kind in [
        ExportKind::CamillaDsp,
        ExportKind::StagedMdat,
        ExportKind::BeforeAfterMdat,
        ExportKind::EqualizerApo,
    ] {
        let folder = root.join(format!("reopen-{kind:?}"));
        let before_events = Arc::new(Events::default());
        let before = export_and_wait(&running, kind, &folder, before_events);
        let before_snapshot = snapshot_relative_files(&folder, &before.files);

        let reopened = deterministic_service();
        open(&reopened, &path);
        let after_events = Arc::new(Events::default());
        let after = export_and_wait(&reopened, kind, &folder, after_events.clone());
        compare_snapshot(
            &folder,
            &before_snapshot,
            &after.files.iter().map(PathBuf::from).collect::<Vec<_>>(),
        );

        let values = lock(&after_events.values);
        assert!(
            !values.iter().any(|event| matches!(
                event,
                JobEvent::Started {
                    kind: secs_types::ipc::JobKind::Calibrate,
                    ..
                }
            )),
            "reopened export must not launch calibration"
        );
        let has_design = values.iter().any(|event| {
            matches!(
                event,
                JobEvent::Progress {
                    stage,
                    ..
                } if stage == "design"
            )
        });
        assert_eq!(
            has_design,
            matches!(kind, ExportKind::StagedMdat | ExportKind::BeforeAfterMdat)
        );
        if kind == ExportKind::BeforeAfterMdat {
            let loaded = secs_mdat::load_mdat_project(
                &fs::read(&after.files[0]).unwrap(),
                &secs_jos::Limits::default(),
            )
            .unwrap();
            let before = loaded
                .measurements
                .iter()
                .find(|measurement| {
                    measurement.title.contains(&format!("_{role}_1 "))
                        && measurement.title.contains("보정 전")
                })
                .unwrap();
            assert_eq!(
                before
                    .source_info
                    .strings
                    .iter()
                    .find(|(key, _)| key == "outputName")
                    .map(|(_, value)| value.as_str()),
                Some("원 측정 장치")
            );
            let after = loaded
                .measurements
                .iter()
                .find(|measurement| {
                    measurement.title.contains(&format!("_{role}_2 "))
                        && measurement.title.contains("보정 후")
                })
                .unwrap();
            assert_eq!(
                after
                    .source_info
                    .strings
                    .iter()
                    .find(|(key, _)| key == "outputName")
                    .map(|(_, value)| value.as_str()),
                Some("원 측정 장치")
            );
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn save_as_preserves_reopened_cached_results_and_export() {
    let root = temp_root("save-as-cache");
    fs::create_dir_all(&root).unwrap();
    let (source, settings) = write_case(&root, "dolby");
    let target = root.join("saved-as.secs");
    let service = deterministic_service();
    open(&service, &source);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&service, calibration_events.clone(), settings);
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    open(&service, &source);
    assert!(read_results(&service).calibration.is_some());
    let Response::ProjectSave(info) = call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams {
            path: Some(target.to_string_lossy().into_owned()),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("save as must succeed");
    };
    assert_eq!(info.path, target.to_string_lossy());
    assert!(read_results(&service).calibration.is_some());
    export_and_wait(
        &service,
        ExportKind::CamillaDsp,
        &root.join("export"),
        Arc::new(Events::default()),
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn stale_calibration_exports_with_flag_and_cache_survives_reopen() {
    let root = temp_root("stale-cache");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "dolby");
    let running_service = service();
    open(&running_service, &path);
    let events = Arc::new(Events::default());
    let job = start_calibration(&running_service, events.clone(), settings);
    finished_result(events.wait(job, Duration::from_secs(120)));

    let before = match call_ok(&running_service, Request::Results, Arc::new(NoEvents)) {
        Response::Results(results) => results.calibration.unwrap(),
        other => panic!("unexpected results response: {other:?}"),
    };
    assert!(!before.stale);
    let reopened = Service::new(Deps::default());
    open(&reopened, &path);
    let cached = match call_ok(&reopened, Request::Results, Arc::new(NoEvents)) {
        Response::Results(results) => results.calibration.unwrap(),
        other => panic!("unexpected results response: {other:?}"),
    };
    assert!(!cached.stale);
    assert!(cached.cached);
    assert_eq!(
        cached,
        secs_types::ipc::CalibrationSummary {
            cached: true,
            ..before
        }
    );

    {
        let mut store = secs_project::MeasurementStore::open(&path).unwrap();
        let record = store.records().into_iter().next().unwrap();
        let role = record.ir.keys().next().unwrap().clone();
        let (mut samples, sample_rate) = store.ir_of(&role, record.pos).unwrap();
        samples[0] += 0.25;
        store
            .record(
                MeasurementResult {
                    role,
                    pos: record.pos,
                    ir: samples,
                    sample_rate,
                    t0_s: 0.0,
                    spl_db: None,
                    spl_offset_db: None,
                    snr_db: None,
                    uuid: Some("changed".into()),
                    source: Some("test".into()),
                    source_info: None,
                },
                None,
            )
            .unwrap();
        store.save().unwrap();
    }
    open(&running_service, &path);
    let stale = match call_ok(&running_service, Request::Results, Arc::new(NoEvents)) {
        Response::Results(results) => results.calibration.unwrap(),
        other => panic!("unexpected results response: {other:?}"),
    };
    assert!(stale.stale);
    assert_eq!(stale.stale_reason.as_deref(), Some("measurements"));
    assert!(stale.cached);
    let Envelope::Err(error) = running_service.call(
        Request::Export(ExportParams {
            kind: ExportKind::StagedMdat,
            folder: root.join("stale-export").to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("stale cached mdat export must fail")
    };
    assert_eq!(error.code, ErrorCode::ExportScope);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("stale_cache")
    );

    let changed_reopen = service();
    open(&changed_reopen, &path);
    let changed = match call_ok(&changed_reopen, Request::Results, Arc::new(NoEvents)) {
        Response::Results(results) => results.calibration.unwrap(),
        other => panic!("unexpected results response: {other:?}"),
    };
    assert!(changed.stale);
    assert_eq!(changed.stale_reason.as_deref(), Some("measurements"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn removing_a_speaker_marks_live_and_cached_calibration_stale() {
    let root = temp_root("speaker-config-stale");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room_dba");
    let service = service();
    open(&service, &path);
    let events = Arc::new(Events::default());
    let calibration = start_calibration(&service, events.clone(), settings);
    finished_result(events.wait(calibration, Duration::from_secs(120)));

    call_ok(
        &service,
        Request::ConfigRemoveSpeaker(secs_types::ipc::ConfigIndexParams { index: 0 }),
        Arc::new(NoEvents),
    );
    let live = read_results(&service).calibration.unwrap();
    assert!(live.stale);
    assert_eq!(live.stale_reason.as_deref(), Some("config"));
    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        Arc::new(NoEvents),
    );

    let reopened = Service::new(Deps::default());
    open(&reopened, &path);
    let cached = read_results(&reopened).calibration.unwrap();
    assert!(cached.stale);
    assert_eq!(cached.stale_reason.as_deref(), Some("config"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cached_mimo_becomes_stale_when_recalibration_uses_replaced_measurements() {
    let root = temp_root("cached-mimo-measurement-stale");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let service = service();
    open(&service, &path);
    calibrate_and_run_mimo(&service, &settings);

    replace_measurement(&service, "C");
    let events = Arc::new(Events::default());
    let calibration = start_calibration(&service, events.clone(), settings);
    finished_result(events.wait(calibration, Duration::from_secs(120)));

    let reopened = Service::new(Deps::default());
    open(&reopened, &path);
    assert!(read_results(&reopened).mimo.unwrap().stale);
    assert!(!automatic_camilla_has_mimo(
        &reopened,
        &root.join("automatic-cached")
    ));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn old_cache_without_run_key_keeps_calibration_but_marks_mimo_stale() {
    let root = temp_root("old-cache-run-key");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let service = service();
    open(&service, &path);
    calibrate_and_run_mimo(&service, &settings);

    let cache_path = path.with_extension("irs").join("secs_run_cache.json");
    let mut cache: serde_json::Value =
        serde_json::from_slice(&fs::read(&cache_path).unwrap()).unwrap();
    let results = cache["results"].as_object_mut().unwrap();
    results.remove("run_key");
    results.insert("mimo_calibration_key".into(), results["engine_key"].clone());
    fs::write(&cache_path, serde_json::to_vec(&cache).unwrap()).unwrap();

    let reopened = Service::new(Deps::default());
    open(&reopened, &path);
    let results = read_results(&reopened);
    assert!(!results.calibration.unwrap().stale);
    assert!(results.mimo.unwrap().stale);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn convolution_export_with_subs_and_mimo_reopens_byte_identically() {
    let root = temp_root("convolution-reopen");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let service = deterministic_service();
    open(&service, &path);
    calibrate_and_run_mimo(&service, &settings);
    call_ok(
        &service,
        Request::ExportFolderSet(secs_types::ipc::ExportFolderSetParams {
            folder: root.join("automatic").to_string_lossy().into_owned(),
        }),
        Arc::new(NoEvents),
    );
    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        Arc::new(NoEvents),
    );

    let export = export_and_wait(
        &service,
        ExportKind::Convolution,
        Path::new(""),
        Arc::new(Events::default()),
    );
    let folder = PathBuf::from(&export.folder);
    assert!(
        folder
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("SECS_convolution_RC_BA_MIMO_")
    );
    let routing: serde_json::Value =
        serde_json::from_slice(&fs::read(folder.join("routing.json")).unwrap()).unwrap();
    assert!(
        routing["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|input| input == "LFE")
    );
    let paths = routing["paths"].as_array().unwrap();
    assert!(paths.iter().any(|path| path["input"] == "LFE"));
    assert!(
        paths
            .iter()
            .any(|path| path["input"] != path["output"] && path["input"] != "LFE")
    );
    let before = snapshot_relative_files(&folder, &export.files);

    let reopened = deterministic_service();
    open(&reopened, &path);
    let events = Arc::new(Events::default());
    let after = export_and_wait(
        &reopened,
        ExportKind::Convolution,
        Path::new(""),
        events.clone(),
    );
    assert_eq!(Path::new(&after.folder), folder);
    assert_eq!(before, snapshot_relative_files(&folder, &after.files));
    assert!(
        !lock(&events.values).iter().any(|event| {
            matches!(event, JobEvent::Progress { stage, .. } if stage == "design")
        })
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn gpu_reports_survive_cache_and_fallback_exports() {
    use secs_types::compute::*;
    use std::sync::atomic::AtomicBool;
    let root = temp_root("gpu-report");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_fir = true;
    settings.mimo_max_supports = 1;
    let fail = Arc::new(AtomicBool::new(false));
    let provider_fail = fail.clone();
    let deps = Deps {
        acceleration: Arc::new(move |preference| {
            if preference == ComputePreference::Cpu {
                return secs_engine::AccelerationSelection {
                    status: ComputeStatus {
                        preference,
                        reason: ComputeReason::UserCpu,
                        ..Default::default()
                    },
                    backend: None,
                };
            }
            let device = GpuDeviceInfo {
                id: 0,
                name: "Test GPU".into(),
                compute_major: 7,
                compute_minor: 5,
                multiprocessors: 48,
                memory_bytes: 8 << 30,
                integrated: false,
                supported: true,
                recommended: true,
                reason: ComputeReason::Recommended,
            };
            secs_engine::AccelerationSelection {
                status: ComputeStatus {
                    preference,
                    selected: ComputeBackend::Cuda,
                    reason: ComputeReason::Recommended,
                    gpu_available: true,
                    device: Some(device.clone()),
                    devices: vec![device],
                },
                backend: Some(Arc::new(FakeRidge {
                    fail: provider_fail.load(Ordering::SeqCst),
                    cpu: secs_engine::CpuRidgeBackend,
                })),
            }
        }),
        ..Deps::default()
    };
    let service = Service::new(deps.clone());
    open(&service, &path);
    let events = Arc::new(Events::default());
    let job = start_calibration(&service, events.clone(), settings.clone());
    finished_result(events.wait(job, Duration::from_secs(120)));
    let mut previous_paths = None;
    for fallback in [false, true] {
        fail.store(fallback, Ordering::SeqCst);
        let events = Arc::new(Events::default());
        let Response::Mimo(started) = call_ok(
            &service,
            Request::Mimo(MimoParams {
                settings: Some(settings.clone()),
            }),
            events.clone(),
        ) else {
            panic!()
        };
        let JobResult::Mimo = finished_result(events.wait(started.job, Duration::from_secs(120)))
        else {
            panic!()
        };
        let summary = read_results(&service).mimo.unwrap();
        let paths = cached_paths(&path);
        assert_eq!(summary.compute.requested, ComputePreference::Auto);
        assert_eq!(
            summary.compute.backend,
            if fallback {
                ComputeBackend::Cpu
            } else {
                ComputeBackend::Cuda
            }
        );
        if fallback {
            assert!(summary.compute.fallbacks > 0);
            assert!(summary.compute.cpu_systems > 0);
            assert_eq!(summary.compute.accelerated_systems, 0);
            // TD-54: the backend's own failure text survives beside the reason.
            assert_eq!(
                summary.compute.detail.as_deref(),
                Some("simulated GPU failure")
            );
            assert!(
                summary
                    .notices
                    .iter()
                    .any(|notice| notice.code == "compute_fallback")
            );
            assert_eq!(previous_paths.as_ref(), Some(&paths));
        } else {
            assert!(summary.compute.accelerated_systems > 0);
            assert_eq!(summary.compute.stage, Some(ComputeStage::IdealFirSweep));
            assert_eq!(summary.compute.device.as_deref(), Some("Test GPU"));
            assert_eq!(summary.compute.fallbacks, 0);
            assert_eq!(summary.compute.detail, None);
            previous_paths = Some(paths);
        }
        call_ok(
            &service,
            Request::ProjectSave(secs_types::ipc::ProjectSaveParams::default()),
            Arc::new(NoEvents),
        );
        let reopened = Service::new(deps.clone());
        open(&reopened, &path);
        let Response::Results(cached) = call_ok(&reopened, Request::Results, Arc::new(NoEvents))
        else {
            panic!()
        };
        assert_eq!(cached.mimo.unwrap(), summary);
        for current in [&service, &reopened] {
            let folder = root.join("export");
            let export = export_and_wait(
                current,
                ExportKind::CamillaDsp,
                &folder,
                Arc::new(Events::default()),
            );
            let provenance = folder.join("compute-provenance.json");
            assert!(
                export
                    .files
                    .iter()
                    .any(|path| Path::new(path) == provenance)
            );
            let report: ComputeReport =
                serde_json::from_slice(&fs::read(provenance).unwrap()).unwrap();
            assert_eq!(report, summary.compute);
            for disable_mimo in [true, false] {
                if disable_mimo {
                    let mut disabled = settings.clone();
                    disabled.mimo_on = false;
                    call_ok(
                        current,
                        Request::CalibrationSettingsSet(
                            secs_types::ipc::CalibrationSettingsParams { settings: disabled },
                        ),
                        Arc::new(NoEvents),
                    );
                } else {
                    call_ok(
                        current,
                        Request::AppSettingsSet(secs_types::ipc::AppSettingsParams {
                            settings: secs_types::ipc::AppSettings {
                                compute: ComputePreference::Cpu,
                                ..Default::default()
                            },
                        }),
                        Arc::new(NoEvents),
                    );
                }
                let cpu_export = export_and_wait(
                    current,
                    ExportKind::CamillaDsp,
                    &folder,
                    Arc::new(Events::default()),
                );
                assert!(!folder.join("compute-provenance.json").exists());
                assert!(
                    cpu_export
                        .files
                        .iter()
                        .all(|path| !path.ends_with("compute-provenance.json"))
                );
                // Restore the choice; the previous GPU/fallback run can be exported again.
                call_ok(
                    current,
                    Request::CalibrationSettingsSet(secs_types::ipc::CalibrationSettingsParams {
                        settings: settings.clone(),
                    }),
                    Arc::new(NoEvents),
                );
                call_ok(
                    current,
                    Request::AppSettingsSet(secs_types::ipc::AppSettingsParams {
                        settings: Default::default(),
                    }),
                    Arc::new(NoEvents),
                );
                export_and_wait(
                    current,
                    ExportKind::CamillaDsp,
                    &folder,
                    Arc::new(Events::default()),
                );
                assert!(folder.join("compute-provenance.json").exists());
            }
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn staged_mdat_states_the_mimo_support_cap_like_upstream_live_and_cached() {
    let root = temp_root("mimo-cap-text");
    fs::create_dir_all(&root).unwrap();
    let (source, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 4;
    let mut store = secs_project::MeasurementStore::open(&source).unwrap();
    let mut project_settings = secs_project::calibration_from_project(store.project());
    project_settings.mimo_on = true;
    project_settings.mimo_max_supports = 4;
    secs_project::store_calibration(store.project_mut(), &project_settings);
    store.save().unwrap();
    let running = service();
    open(&running, &source);
    calibrate_and_run_mimo(&running, &settings);

    // `calibration.py:1000-1002`: "(Fsiso 150 Hz · 입력당 서포트 ≤ 4)".
    let live = export_and_wait(
        &running,
        ExportKind::StagedMdat,
        &root.join("live"),
        Arc::new(Events::default()),
    );
    let reopened = service();
    open(&reopened, &source);
    assert!(!read_results(&reopened).mimo.unwrap().stale);
    let cached = export_and_wait(
        &reopened,
        ExportKind::StagedMdat,
        &root.join("cached"),
        Arc::new(Events::default()),
    );
    for (path, files) in [("live", &live.files), ("cached", &cached.files)] {
        let notes = mimo_notes(files);
        assert!(!notes.is_empty(), "{path}: no MIMO measurement");
        for note in notes {
            assert!(
                note.contains(" Hz · 입력당 서포트 ≤ 4)"),
                "{path}: cap text missing: {note}"
            );
            assert!(!note.contains("max"), "{path}: raw cap text: {note}");
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn reopened_fresh_cache_keeps_mimo_in_mdat() {
    let root = temp_root("fresh-cached-mimo-mdat");
    fs::create_dir_all(&root).unwrap();
    let (source, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let mut store = secs_project::MeasurementStore::open(&source).unwrap();
    let mut project_settings = secs_project::calibration_from_project(store.project());
    project_settings.mimo_on = true;
    project_settings.mimo_max_supports = 1;
    secs_project::store_calibration(store.project_mut(), &project_settings);
    store.save().unwrap();
    let running = deterministic_service();
    open(&running, &source);
    calibrate_and_run_mimo(&running, &settings);

    let live_folder = root.join("live-mdat");
    let live = export_and_wait(
        &running,
        ExportKind::StagedMdat,
        &live_folder,
        Arc::new(Events::default()),
    );
    let live_snapshot = snapshot_relative_files(&live_folder, &live.files);

    let reopened = deterministic_service();
    open(&reopened, &source);
    let cached = read_results(&reopened);
    assert!(cached.calibration.unwrap().cached);
    assert!(!cached.mimo.unwrap().stale);
    let reopened_folder = root.join("reopened-mdat");
    let reopened_export = export_and_wait(
        &reopened,
        ExportKind::StagedMdat,
        &reopened_folder,
        Arc::new(Events::default()),
    );
    let reopened_snapshot = snapshot_relative_files(&reopened_folder, &reopened_export.files);
    assert_eq!(
        live_snapshot
            .iter()
            .map(|(_, bytes)| bytes)
            .collect::<Vec<_>>(),
        reopened_snapshot
            .iter()
            .map(|(_, bytes)| bytes)
            .collect::<Vec<_>>()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn reopened_stale_cache_blocks_mdat_but_allows_camilla() {
    let root = temp_root("stale-cached-mdat");
    fs::create_dir_all(&root).unwrap();
    let (source, settings) = write_case(&root, "dolby");
    let running = service();
    open(&running, &source);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&running, calibration_events.clone(), settings.clone());
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    let reopened = service();
    open(&reopened, &source);
    call_ok(
        &reopened,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: secs_types::settings_edit::SettingEdit::RcLo {
                hz: settings.rc_lo + 5.0,
            },
        }),
        Arc::new(NoEvents),
    );
    let summary = read_results(&reopened).calibration.unwrap();
    assert!(summary.cached);
    assert!(summary.stale);

    let Envelope::Err(error) = reopened.call(
        Request::Export(ExportParams {
            kind: ExportKind::StagedMdat,
            folder: root.join("blocked-mdat").to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("stale cached mdat must be blocked")
    };
    assert_eq!(error.code, ErrorCode::ExportScope);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("stale_cache")
    );

    let camilla = export_and_wait(
        &reopened,
        ExportKind::CamillaDsp,
        &root.join("camilla"),
        Arc::new(Events::default()),
    );
    assert!(camilla.stale);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cached_resample_replay_releases_read_memory_before_calibrating_td130() {
    let root = temp_root("cached-resample-memory");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("resample.secs");
    let mut project = Project::from_preset("2.0").unwrap();
    let mut settings = secs_project::calibration_from_project(&project);
    settings.sample_rate_policy = secs_types::calibration::SampleRatePolicy::Resample;
    secs_project::store_calibration(&mut project, &settings);
    let mut store = MeasurementStore::new(&path, project);
    for role in ["L", "R"] {
        let mut ir = vec![0.0; 8192];
        ir[100] = 1.0;
        store
            .record(
                MeasurementResult {
                    role: role.into(),
                    pos: 1,
                    ir,
                    sample_rate: 44_100,
                    t0_s: 0.0,
                    spl_db: None,
                    spl_offset_db: None,
                    snr_db: None,
                    uuid: None,
                    source: Some("test".into()),
                    source_info: None,
                },
                None,
            )
            .unwrap();
    }
    store.save().unwrap();

    let running = service();
    open(&running, &path);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&running, calibration_events.clone(), settings);
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    let large_samples = vec![0.0_f32; 5 * 1024 * 1024 / std::mem::size_of::<f32>()];
    fs::write(
        secs_project::ir_file(&path, "L", 1),
        secs_project::write_wav_float32(&large_samples, 44_100),
    )
    .unwrap();

    let replay = Service::new(Deps {
        memory: Arc::new(secs_compute::FixedMemory(64 * 1024 * 1024)),
        ..Deps::default()
    });
    open(&replay, &path);
    let events = Arc::new(Events::default());
    let Response::Export(started) = call_ok(
        &replay,
        Request::Export(ExportParams {
            kind: ExportKind::StagedMdat,
            folder: root.join("export").to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        events.clone(),
    ) else {
        panic!()
    };
    let Some(finished) = events.try_wait(started.job, Duration::from_secs(5)) else {
        call_ok(
            &replay,
            Request::JobCancel(JobCancelParams { job: started.job }),
            Arc::new(NoEvents),
        );
        let _ = events.wait(started.job, Duration::from_secs(10));
        panic!("cached resample replay kept the read permit while calibrating");
    };
    assert!(matches!(
        finished,
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            ..
        }
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cached_mdat_cancel_after_a_real_read_finishes_cancelled_td130() {
    let root = temp_root("cancel-cached-read");
    fs::create_dir_all(&root).unwrap();
    let (source, settings) = write_case(&root, "dolby");
    let running = service();
    open(&running, &source);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&running, calibration_events.clone(), settings);
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    let reopened = service();
    open(&reopened, &source);
    let events = Arc::new(PausedReadProgress::default());
    let Response::Export(started) = call_ok(
        &reopened,
        Request::Export(ExportParams {
            kind: ExportKind::StagedMdat,
            folder: root.join("cancelled-export").to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        events.clone(),
    ) else {
        panic!()
    };
    events.wait_until_paused();
    // The progress point is after the snapshot read. Removing a source now
    // proves cancellation does not begin before the real WAV read.
    fs::remove_file(secs_project::ir_file(&source, "L", 1)).unwrap();
    call_ok(
        &reopened,
        Request::JobCancel(JobCancelParams { job: started.job }),
        Arc::new(NoEvents),
    );
    events.release();
    let finished = events.0.events.wait(started.job, Duration::from_secs(120));
    assert!(matches!(
        finished,
        JobEvent::Finished {
            status: JobStatus::Cancelled,
            ..
        }
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cached_mdat_export_rejects_project_replacement_before_writing() {
    let root = temp_root("replace-cached-export");
    fs::create_dir_all(&root).unwrap();
    let (source, settings) = write_case(&root, "dolby");
    let running = service();
    open(&running, &source);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&running, calibration_events.clone(), settings);
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    let reopened = service();
    open(&reopened, &source);
    let folder = root.join("export");
    let events = Arc::new(PausedProgress::default());
    let Response::Export(started) = call_ok(
        &reopened,
        Request::Export(ExportParams {
            kind: ExportKind::StagedMdat,
            folder: folder.to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        events.clone(),
    ) else {
        panic!()
    };
    events.wait_until_paused();
    call_ok(
        &reopened,
        Request::ProjectNew(ProjectNewParams {
            path: root.join("replacement.secs").to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    events.release();
    let finished = events.events.wait(started.job, Duration::from_secs(120));
    assert!(
        matches!(finished, JobEvent::Finished { status: JobStatus::Failed, error: Some(ref error), .. }
            if error.code == ErrorCode::StaleInputs && error.detail == "project changed while computing")
    );
    assert!(!folder.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cached_mdat_export_rejects_changed_calibration_inputs_before_writing() {
    let root = temp_root("edit-cached-export");
    fs::create_dir_all(&root).unwrap();
    let (source, settings) = write_case(&root, "dolby");
    let running = service();
    open(&running, &source);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&running, calibration_events.clone(), settings);
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    let reopened = service();
    open(&reopened, &source);
    let folder = root.join("export-edited");
    let events = Arc::new(PausedProgress::default());
    let Response::Export(started) = call_ok(
        &reopened,
        Request::Export(ExportParams {
            kind: ExportKind::StagedMdat,
            folder: folder.to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        events.clone(),
    ) else {
        panic!()
    };
    events.wait_until_paused();
    call_ok(
        &reopened,
        Request::TargetApplyPreset(TargetPresetParams {
            preset: "harman".into(),
        }),
        Arc::new(NoEvents),
    );
    events.release();
    let finished = events.events.wait(started.job, Duration::from_secs(120));
    assert!(
        matches!(finished, JobEvent::Finished { status: JobStatus::Failed, error: Some(ref error), .. }
            if error.code == ErrorCode::StaleInputs && error.detail == "project changed while computing")
    );
    assert!(!folder.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_cached_mdat_export_rejects_inputs_changed_after_planning_td155() {
    let root = temp_root("planned-cached-export");
    fs::create_dir_all(&root).unwrap();
    let (source, settings) = write_case(&root, "dolby");
    let running = service();
    open(&running, &source);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&running, calibration_events.clone(), settings.clone());
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    let reopened = service();
    open(&reopened, &source);
    let folder = root.join("export-replanned");
    let events = Arc::new(PausedProgress::default());
    let Response::Export(started) = call_ok(
        &reopened,
        Request::Export(ExportParams {
            kind: ExportKind::StagedMdat,
            folder: folder.to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        events.clone(),
    ) else {
        panic!()
    };
    events.wait_until_paused();

    call_ok(
        &reopened,
        Request::TargetApplyPreset(TargetPresetParams {
            preset: "harman".into(),
        }),
        Arc::new(NoEvents),
    );
    let recalibration_events = Arc::new(Events::default());
    let recalibration = start_calibration(&reopened, recalibration_events.clone(), settings);
    finished_result(recalibration_events.wait(recalibration, Duration::from_secs(120)));

    events.release();
    let finished = events.events.wait(started.job, Duration::from_secs(120));
    assert!(
        matches!(finished, JobEvent::Finished { status: JobStatus::Failed, error: Some(ref error), .. }
            if error.code == ErrorCode::StaleInputs)
    );
    assert!(!folder.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cached_mdat_export_rejects_changed_sample_rate_policy_td116() {
    let root = temp_root("policy-cached-export");
    fs::create_dir_all(&root).unwrap();
    let (source, settings) = write_case(&root, "dolby");
    let running = service();
    open(&running, &source);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&running, calibration_events.clone(), settings);
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    let reopened = service();
    open(&reopened, &source);
    let folder = root.join("export-policy-edited");
    let events = Arc::new(PausedProgress::default());
    let Response::Export(started) = call_ok(
        &reopened,
        Request::Export(ExportParams {
            kind: ExportKind::StagedMdat,
            folder: folder.to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        events.clone(),
    ) else {
        panic!()
    };
    events.wait_until_paused();
    call_ok(
        &reopened,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: secs_types::settings_edit::SettingEdit::SampleRatePolicy {
                policy: secs_types::calibration::SampleRatePolicy::Resample,
            },
        }),
        Arc::new(NoEvents),
    );
    events.release();
    let finished = events.events.wait(started.job, Duration::from_secs(120));
    assert!(matches!(
        finished,
        JobEvent::Finished {
            status: JobStatus::Failed,
            error: Some(ref error),
            ..
        } if error.code == ErrorCode::StaleInputs
    ));
    assert!(!folder.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn commit_results_saves_the_cache_outside_the_lock_and_reports_a_failed_save_once() {
    let root = temp_root("cache-save-warning");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    // A non-empty folder where the cache file goes: the final rename fails
    // on every platform and for every user (a read-only folder does not stop root).
    let obstacle = path.with_extension("irs").join("secs_run_cache.json");
    fs::create_dir_all(obstacle.join("in-the-way")).unwrap();
    let service = service();
    open(&service, &path);

    calibrate_and_run_mimo(&service, &settings);
    let results = read_results(&service);
    let calibration = results.calibration.unwrap();
    let mimo = results.mimo.unwrap();
    assert!(!calibration.stale && !mimo.stale);
    assert!(
        has_cache_warning(&calibration.notices),
        "{:?}",
        calibration.notices
    );
    assert!(has_cache_warning(&mimo.notices), "{:?}", mimo.notices);
    assert_eq!(
        calibration
            .notices
            .iter()
            .filter(|notice| notice.code == "cache_save_failed")
            .count(),
        1
    );
    assert_eq!(
        mimo.notices
            .iter()
            .filter(|notice| notice.code == "cache_save_failed")
            .count(),
        1
    );

    // Once a save succeeds the warning goes, and it is never written to the cache.
    fs::remove_dir_all(&obstacle).unwrap();
    let events = Arc::new(Events::default());
    let Response::Mimo(started) = call_ok(
        &service,
        Request::Mimo(MimoParams {
            settings: Some(settings),
        }),
        events.clone(),
    ) else {
        panic!()
    };
    finished_result(events.wait(started.job, Duration::from_secs(120)));
    let results = read_results(&service);
    assert!(!has_cache_warning(&results.calibration.unwrap().notices));
    assert!(!has_cache_warning(&results.mimo.unwrap().notices));
    let cached: secs_types::ipc::CalibrationSummary = cached_field(&path, "calibration");
    assert!(
        cached
            .notices
            .iter()
            .all(|notice| notice.code != "cache_save_failed")
    );
    fs::remove_dir_all(root).unwrap();
}
