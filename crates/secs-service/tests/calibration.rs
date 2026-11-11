#![forbid(unsafe_code)]

mod support;
use support::*;

#[test]
fn status_project_and_results_come_from_the_same_open_project() {
    let root = temp_root("status-session-snapshot");
    fs::create_dir_all(&root).unwrap();
    let (without_subs, settings) = case_project::write(&root.join("without-subs.secs"), "room");
    let mut store = MeasurementStore::open(&without_subs).unwrap();
    store
        .project_mut()
        .config
        .speakers
        .retain(|speaker| speaker.size != "sub");
    store.project_mut().config.renumber();
    store.save().unwrap();
    let (with_subs, with_subs_settings) = case_project::write(&root.join("with-subs.secs"), "room");

    for (path, settings) in [(&without_subs, settings), (&with_subs, with_subs_settings)] {
        let service = service();
        open(&service, path);
        let events = Arc::new(Events::default());
        let job = start_calibration(&service, events.clone(), settings);
        finished_result(events.wait(job, Duration::from_secs(120)));
    }

    let service = Arc::new(service());
    open(&service, &without_subs);
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let opener_service = service.clone();
    let opener_barrier = barrier.clone();
    let first = without_subs.clone();
    let second = with_subs.clone();
    let opener = std::thread::spawn(move || {
        opener_barrier.wait();
        for index in 0..50 {
            open(
                &opener_service,
                if index % 2 == 0 { &second } else { &first },
            );
        }
    });
    barrier.wait();
    for _ in 0..50 {
        let status = service.status();
        assert_eq!(status.project.is_some(), status.results.is_some());
        if let (Some(project), Some(results)) = (status.project, status.results)
            && let Some(calibration) = results.calibration
        {
            let expected_subs = if project.path == without_subs.to_string_lossy() {
                0
            } else {
                2
            };
            assert_eq!(
                calibration.sub_roles.len(),
                expected_subs,
                "{}",
                project.path
            );
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    opener.join().unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn low_protect_edits_persist_and_mark_results_stale() {
    let root = temp_root("low-protect");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let initial = service();
    open(&initial, &path);
    let events = Arc::new(Events::default());
    let job = start_calibration(&initial, events.clone(), settings);
    finished_result(events.wait(job, Duration::from_secs(120)));
    assert!(!read_results(&initial).calibration.unwrap().stale);
    call_ok(
        &initial,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: secs_types::settings_edit::SettingEdit::LowProtect { on: true },
        }),
        Arc::new(NoEvents),
    );
    assert!(read_results(&initial).calibration.unwrap().stale);
    call_ok(
        &initial,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        Arc::new(NoEvents),
    );
    let reopened = service();
    open(&reopened, &path);
    let Response::CalibrationSettingsGet(settings) = call_ok(
        &reopened,
        Request::CalibrationSettingsGet,
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert!(settings.low_protect);
    assert!(read_results(&reopened).calibration.unwrap().stale);
    call_ok(
        &reopened,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: secs_types::settings_edit::SettingEdit::LowProtect { on: false },
        }),
        Arc::new(NoEvents),
    );
    assert!(!read_results(&reopened).calibration.unwrap().stale);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dip_limit_edits_persist_and_mark_results_stale() {
    let root = temp_root("dip-limit");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let initial = service();
    open(&initial, &path);
    let events = Arc::new(Events::default());
    let job = start_calibration(&initial, events.clone(), settings);
    finished_result(events.wait(job, Duration::from_secs(120)));
    assert!(!read_results(&initial).calibration.unwrap().stale);
    call_ok(
        &initial,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: secs_types::settings_edit::SettingEdit::DipLimit { on: true },
        }),
        Arc::new(NoEvents),
    );
    assert!(read_results(&initial).calibration.unwrap().stale);
    call_ok(
        &initial,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        Arc::new(NoEvents),
    );
    let reopened = service();
    open(&reopened, &path);
    let Response::CalibrationSettingsGet(settings) = call_ok(
        &reopened,
        Request::CalibrationSettingsGet,
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert!(settings.dip_limit);
    assert!(read_results(&reopened).calibration.unwrap().stale);
    call_ok(
        &reopened,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: secs_types::settings_edit::SettingEdit::DipLimit { on: false },
        }),
        Arc::new(NoEvents),
    );
    assert!(!read_results(&reopened).calibration.unwrap().stale);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lf_align_and_mode_decay_edits_persist_and_mark_results_stale() {
    use secs_types::settings_edit::SettingEdit;

    let root = temp_root("lf-align-mode-decay");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let initial = service();
    open(&initial, &path);
    let events = Arc::new(Events::default());
    let job = start_calibration(&initial, events.clone(), settings);
    finished_result(events.wait(job, Duration::from_secs(120)));
    assert!(!read_results(&initial).calibration.unwrap().stale);

    for edit in [
        SettingEdit::LfAlign { on: true },
        SettingEdit::ModeDecay { on: true },
        SettingEdit::HfPhase { on: true },
    ] {
        call_ok(
            &initial,
            Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
                edit,
            }),
            Arc::new(NoEvents),
        );
    }
    let summary = read_results(&initial).calibration.unwrap();
    assert!(summary.stale);
    assert_eq!(summary.stale_reason.as_deref(), Some("settings"));
    call_ok(
        &initial,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        Arc::new(NoEvents),
    );

    let reopened = service();
    open(&reopened, &path);
    let Response::CalibrationSettingsGet(settings) = call_ok(
        &reopened,
        Request::CalibrationSettingsGet,
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert!(settings.lf_align);
    assert!(settings.mode_decay);
    assert!(settings.hf_phase);
    assert!(read_results(&reopened).calibration.unwrap().stale);

    call_ok(
        &reopened,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: SettingEdit::LfAlign { on: false },
        }),
        Arc::new(NoEvents),
    );
    assert!(read_results(&reopened).calibration.unwrap().stale);
    call_ok(
        &reopened,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: SettingEdit::ModeDecay { on: false },
        }),
        Arc::new(NoEvents),
    );
    assert!(read_results(&reopened).calibration.unwrap().stale);
    call_ok(
        &reopened,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: SettingEdit::HfPhase { on: false },
        }),
        Arc::new(NoEvents),
    );
    assert!(!read_results(&reopened).calibration.unwrap().stale);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn input_hp_edits_validate_persist_and_mark_results_stale() {
    use secs_types::settings_edit::SettingEdit;

    let root = temp_root("input-hp");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let initial = service();
    open(&initial, &path);
    let events = Arc::new(Events::default());
    let job = start_calibration(&initial, events.clone(), settings.clone());
    finished_result(events.wait(job, Duration::from_secs(120)));
    assert!(!read_results(&initial).calibration.unwrap().stale);

    let Response::CalibrationSettingsEdit(enabled) = call_ok(
        &initial,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: SettingEdit::InputHp { hz: Some(27.0) },
        }),
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert_eq!(enabled.input_hp_hz, Some(27.0));
    assert_ne!(enabled.engine_key(), settings.engine_key());
    let summary = read_results(&initial).calibration.unwrap();
    assert!(summary.stale);
    assert_eq!(summary.stale_reason.as_deref(), Some("settings"));
    call_ok(
        &initial,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        Arc::new(NoEvents),
    );

    let reopened = service();
    open(&reopened, &path);
    let Response::CalibrationSettingsGet(stored) = call_ok(
        &reopened,
        Request::CalibrationSettingsGet,
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert_eq!(stored.input_hp_hz, Some(27.0));
    assert!(read_results(&reopened).calibration.unwrap().stale);

    let Response::CalibrationSettingsEdit(disabled) = call_ok(
        &reopened,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: SettingEdit::InputHp { hz: None },
        }),
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert_eq!(disabled.input_hp_hz, None);
    assert_eq!(disabled.engine_key(), settings.engine_key());
    assert!(!read_results(&reopened).calibration.unwrap().stale);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rc_band_edits_store_sorted_clear_change_keys_and_mark_results_stale() {
    use secs_types::{calibration::RcBand, settings_edit::SettingEdit};

    let root = temp_root("rc-bands-edit");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let service = service();
    open(&service, &path);
    let events = Arc::new(Events::default());
    let job = start_calibration(&service, events.clone(), settings.clone());
    finished_result(events.wait(job, Duration::from_secs(120)));
    let baseline_key = settings.engine_key();

    let Response::CalibrationSettingsEdit(edited) = call_ok(
        &service,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: SettingEdit::RcBands {
                bands: vec![
                    RcBand {
                        lo_hz: 15_000.0,
                        hi_hz: 20_000.0,
                    },
                    RcBand {
                        lo_hz: 1_000.0,
                        hi_hz: 2_000.0,
                    },
                ],
            },
        }),
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert_eq!(
        edited
            .rc_bands
            .iter()
            .map(|band| band.lo_hz)
            .collect::<Vec<_>>(),
        [1_000.0, 15_000.0]
    );
    assert_ne!(edited.engine_key(), baseline_key);
    let summary = read_results(&service).calibration.unwrap();
    assert!(summary.stale);
    assert_eq!(summary.stale_reason.as_deref(), Some("settings"));

    let Response::CalibrationSettingsEdit(cleared) = call_ok(
        &service,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: SettingEdit::RcBands { bands: Vec::new() },
        }),
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert!(cleared.rc_bands.is_empty());
    assert_eq!(cleared.engine_key(), baseline_key);
    assert!(!read_results(&service).calibration.unwrap().stale);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn analytic_derivative_edits_persist_and_mark_results_stale() {
    use secs_types::settings_edit::SettingEdit;

    let root = temp_root("analytic-derivatives");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let initial = service();
    open(&initial, &path);
    let events = Arc::new(Events::default());
    let job = start_calibration(&initial, events.clone(), settings);
    finished_result(events.wait(job, Duration::from_secs(120)));
    assert!(!read_results(&initial).calibration.unwrap().stale);

    // Each part alone, as the CLI sends it; the screen sends both at once.
    for edit in [
        SettingEdit::AnalyticDerivatives {
            peq: Some(true),
            mimo_iir: None,
        },
        SettingEdit::AnalyticDerivatives {
            peq: None,
            mimo_iir: Some(true),
        },
    ] {
        call_ok(
            &initial,
            Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
                edit,
            }),
            Arc::new(NoEvents),
        );
    }
    let results = read_results(&initial);
    let calibration = results.calibration.unwrap();
    assert!(calibration.stale);
    assert_eq!(calibration.stale_reason.as_deref(), Some("settings"));
    call_ok(
        &initial,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        Arc::new(NoEvents),
    );

    let reopened = service();
    open(&reopened, &path);
    let Response::CalibrationSettingsGet(settings) = call_ok(
        &reopened,
        Request::CalibrationSettingsGet,
        Arc::new(NoEvents),
    ) else {
        panic!("expected reopened settings")
    };
    assert!(settings.peq_analytic_derivatives);
    assert!(settings.mimo_iir_analytic_derivatives);
    assert!(read_results(&reopened).calibration.unwrap().stale);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn background_and_foreground_calibration_summaries_are_bit_identical() {
    let root = temp_root("background-determinism");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let service = deterministic_service();
    open(&service, &path);

    let foreground_events = Arc::new(Events::default());
    let foreground_job =
        start_calibration_in_mode(&service, foreground_events.clone(), settings.clone(), false);
    finished_result(foreground_events.wait(foreground_job, Duration::from_secs(120)));
    let foreground = read_results(&service);

    let background_events = Arc::new(Events::default());
    let background_job =
        start_calibration_in_mode(&service, background_events.clone(), settings, true);
    finished_result(background_events.wait(background_job, Duration::from_secs(120)));
    let background = read_results(&service);

    assert_eq!(
        serde_json::to_vec(&background).unwrap(),
        serde_json::to_vec(&foreground).unwrap()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn recommend_xo_accepts_absent_null_and_background_params() {
    let service = service();
    for request in [
        serde_json::json!({"method": "recommend_xo"}),
        serde_json::json!({"method": "recommend_xo", "params": null}),
        serde_json::json!({"method": "recommend_xo", "params": {"background": true}}),
    ] {
        let Envelope::Err(error) = service.call_value(request, Arc::new(NoEvents)) else {
            panic!("recommend_xo without a project must fail");
        };
        assert_eq!(error.code, ErrorCode::ProjectNotOpen);
    }
}

#[test]
fn polarity_setting_marks_results_stale_and_flips_the_engine_input() {
    let root = temp_root("speaker-polarity-stale");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let service = deterministic_service();
    open(&service, &path);
    let events = Arc::new(Events::default());
    let calibration = start_calibration(&service, events.clone(), settings.clone());
    finished_result(events.wait(calibration, Duration::from_secs(120)));

    let Response::ConfigSetSpeaker(config) = call_ok(
        &service,
        Request::ConfigSetSpeaker(secs_types::ipc::ConfigSetSpeakerParams {
            index: 0,
            role: None,
            size: None,
            band_index: Some(0),
            out: None,
            pol: Some("-".into()),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("speaker polarity must change")
    };
    assert_eq!(config.speakers[0].bands[0].pol, "-");
    let stale = read_results(&service).calibration.unwrap();
    assert!(stale.stale);
    assert_eq!(stale.stale_reason.as_deref(), Some("config"));

    let events = Arc::new(Events::default());
    let calibration = start_calibration(&service, events.clone(), settings);
    finished_result(events.wait(calibration, Duration::from_secs(120)));
    assert!(!read_results(&service).calibration.unwrap().stale);
    let export = export_and_wait(
        &service,
        ExportKind::EqualizerApo,
        &root.join("export"),
        Arc::new(Events::default()),
    );
    let body = export
        .files
        .iter()
        .map(fs::read_to_string)
        .filter_map(Result::ok)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(body.contains("Copy: VML=-1.0*VML"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancelled_calibration_preserves_previous_results() {
    let root = temp_root("cancel-preserve");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "dolby");
    let service = service();
    open(&service, &path);
    let initial_events = Arc::new(Events::default());
    let initial = start_calibration(&service, initial_events.clone(), settings.clone());
    finished_result(initial_events.wait(initial, Duration::from_secs(120)));
    let previous = match call_ok(&service, Request::Results, Arc::new(NoEvents)) {
        Response::Results(results) => results,
        other => panic!("unexpected results response: {other:?}"),
    };

    let events = Arc::new(Events::default());
    let job = start_calibration(&service, events.clone(), settings);
    let start = Instant::now();
    let Response::JobCancel(snapshot) = call_ok(
        &service,
        Request::JobCancel(JobCancelParams { job }),
        Arc::new(NoEvents),
    ) else {
        panic!("cancel must return a job snapshot");
    };
    assert_eq!(snapshot.status, JobStatus::CancelRequested);
    let JobEvent::Finished { status, .. } = events.wait(job, Duration::from_secs(1)) else {
        unreachable!();
    };
    assert_eq!(status, JobStatus::Cancelled);
    assert!(start.elapsed() < Duration::from_secs(1));
    let current = match call_ok(&service, Request::Results, Arc::new(NoEvents)) {
        Response::Results(results) => results,
        other => panic!("unexpected results response: {other:?}"),
    };
    assert_eq!(current, previous);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn calibrate_with_mimo_matches_separate_runs_and_orders_progress() {
    let root = temp_root("calibrate-with-mimo");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let service = deterministic_service();
    open(&service, &path);

    let combined_events = Arc::new(Events::default());
    let Response::Calibrate(started) = call_ok(
        &service,
        Request::Calibrate(CalibrateParams {
            settings: Some(settings.clone()),
            mimo: true,
            ..Default::default()
        }),
        combined_events.clone(),
    ) else {
        panic!("combined calibration must start")
    };
    assert_eq!(
        finished_result(combined_events.wait(started.job, Duration::from_secs(120))),
        JobResult::Calibration
    );
    let stages: Vec<_> = lock(&combined_events.values)
        .iter()
        .filter_map(|event| match event {
            JobEvent::Progress { stage, .. } => Some(stage.clone()),
            _ => None,
        })
        .collect();
    let calibration_stage = stages
        .iter()
        .position(|stage| *stage == "engine.design")
        .expect("calibration progress");
    let mimo_stage = stages
        .iter()
        .position(|stage| *stage == "engine.mimo")
        .expect("MIMO progress");
    assert!(calibration_stage < mimo_stage);
    let values = lock(&combined_events.values);
    let progress: Vec<_> = values
        .iter()
        .filter_map(|event| match event {
            JobEvent::Progress {
                stage, done, total, ..
            } => Some((stage.as_str(), *done, *total)),
            _ => None,
        })
        .collect();
    let mut completed = Vec::new();
    let mut start = 0;
    while start < progress.len() {
        let (stage, done, total) = progress[start];
        assert!(!completed.contains(&stage), "stage restarted: {stage}");
        assert_eq!(done, 0, "{stage}");
        if stage == "engine.ba" {
            // The bass alignment cannot be counted before it runs: it is
            // announced once with no total, so the screen names it, barless.
            assert_eq!(total, 0, "{stage}");
            assert!(progress.get(start + 1).is_none_or(|next| next.0 != stage));
            completed.push(stage);
            start += 1;
            continue;
        }
        assert!(total > 0, "{stage}");
        let end = progress[start..]
            .iter()
            .position(|p| p.0 != stage)
            .map_or(progress.len(), |offset| start + offset);
        let phase = &progress[start..end];
        assert!(phase.iter().all(|p| p.2 == total && p.1 <= total));
        assert!(phase.windows(2).all(|pair| pair[0].1 <= pair[1].1));
        assert_eq!(phase.last().unwrap().1, total, "{stage}");
        completed.push(stage);
        start = end;
    }
    assert_eq!(completed.first(), Some(&"read_measurements"));
    assert_eq!(completed.last(), Some(&"engine.mimo_responses"));
    assert!(
        completed
            .iter()
            .position(|s| *s == "engine.late_energy")
            .unwrap()
            < completed.iter().position(|s| *s == "engine.mimo").unwrap()
    );
    drop(values);
    let combined = read_results(&service);
    assert!(!combined.calibration.as_ref().unwrap().stale);
    assert!(!combined.mimo.as_ref().unwrap().stale);

    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&service, calibration_events.clone(), settings.clone());
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));
    let mimo_events = Arc::new(Events::default());
    let Response::Mimo(mimo) = call_ok(
        &service,
        Request::Mimo(MimoParams {
            settings: Some(settings),
        }),
        mimo_events.clone(),
    ) else {
        panic!("separate MIMO must start")
    };
    finished_result(mimo_events.wait(mimo.job, Duration::from_secs(120)));
    let separate = read_results(&service);
    assert_eq!(combined.calibration, separate.calibration);
    assert_eq!(combined.mimo, separate.mimo);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn calibrate_with_mimo_refuses_disabled_and_too_few_positions_before_starting() {
    let root = temp_root("calibrate-mimo-preconditions");
    fs::create_dir_all(&root).unwrap();

    let (disabled_path, disabled_settings) = write_case(&root, "room_dba");
    let disabled = service();
    open(&disabled, &disabled_path);
    let Envelope::Err(error) = disabled.call(
        Request::Calibrate(CalibrateParams {
            settings: Some(disabled_settings),
            mimo: true,
            ..Default::default()
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("disabled MIMO must be refused")
    };
    assert_eq!(error.code, ErrorCode::EngineInput);
    assert!(error.params.is_empty());
    assert_eq!(error.detail, "invalid input: MIMO is disabled");
    let Response::Jobs(jobs) = call_ok(&disabled, Request::Jobs, Arc::new(NoEvents)) else {
        panic!()
    };
    assert!(jobs.is_empty());

    let few_root = root.join("few");
    fs::create_dir_all(&few_root).unwrap();
    let (few_path, mut settings) = write_case(&few_root, "room_dba");
    let mut store = secs_project::MeasurementStore::open(&few_path).unwrap();
    let secs_project::PyJson::Array(measurements) = &mut store.project_mut().measurements else {
        panic!("measurements must be an array")
    };
    measurements.truncate(2);
    store.save().unwrap();
    settings.mimo_on = true;
    let few = service();
    open(&few, &few_path);
    let Envelope::Err(error) = few.call(
        Request::Calibrate(CalibrateParams {
            settings: Some(settings),
            mimo: true,
            ..Default::default()
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("two positions must be refused")
    };
    assert_eq!(error.code, ErrorCode::EngineInput);
    assert!(error.params.is_empty());
    assert!(error.detail.contains("MIMO 는 3곳 이상 필요"));
    let Response::Jobs(jobs) = call_ok(&few, Request::Jobs, Arc::new(NoEvents)) else {
        panic!()
    };
    assert!(jobs.is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancelling_calibrate_during_mimo_keeps_only_the_new_calibration() {
    let root = temp_root("cancel-calibrate-mimo");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let service = Arc::new(service());
    open(&service, &path);
    let events = Arc::new(CancelOnMimo {
        service: service.clone(),
        events: Events::default(),
        requested: std::sync::atomic::AtomicBool::new(false),
    });
    let Response::Calibrate(started) = call_ok(
        &service,
        Request::Calibrate(CalibrateParams {
            settings: Some(settings),
            mimo: true,
            ..Default::default()
        }),
        events.clone(),
    ) else {
        panic!("combined calibration must start")
    };
    let finished = events.events.wait(started.job, Duration::from_secs(120));
    assert!(matches!(
        finished,
        JobEvent::Finished {
            status: JobStatus::Cancelled,
            ..
        }
    ));
    assert!(events.requested.load(Ordering::SeqCst));
    let results = read_results(&service);
    assert!(!results.calibration.unwrap().stale);
    assert!(results.mimo.is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn mimo_failure_after_combined_calibration_keeps_calibration_and_marks_stage() {
    let root = temp_root("failed-calibrate-mimo");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    let mut store = secs_project::MeasurementStore::open(&path).unwrap();
    store
        .project_mut()
        .config
        .speakers
        .retain(|speaker| speaker.role == "L");
    store.project_mut().config.preset = "custom".into();
    store.save().unwrap();
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let service = service();
    open(&service, &path);
    let events = Arc::new(Events::default());
    let Response::Calibrate(started) = call_ok(
        &service,
        Request::Calibrate(CalibrateParams {
            settings: Some(settings),
            mimo: true,
            ..Default::default()
        }),
        events.clone(),
    ) else {
        panic!("combined calibration must start")
    };
    let JobEvent::Finished {
        status,
        result,
        error: Some(error),
        ..
    } = events.wait(started.job, Duration::from_secs(120))
    else {
        panic!("combined job must fail")
    };
    assert_eq!(status, JobStatus::Failed);
    assert!(result.is_none());
    assert_eq!(error.code, ErrorCode::EngineInput);
    assert_eq!(error.params.get("stage").map(String::as_str), Some("mimo"));
    let results = read_results(&service);
    assert!(!results.calibration.unwrap().stale);
    assert!(results.mimo.is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn mimo_becomes_stale_when_recalibration_uses_replaced_measurements() {
    let root = temp_root("mimo-measurement-stale");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let service = service();
    open(&service, &path);
    calibrate_and_run_mimo(&service, &settings);

    replace_measurement(&service, "C");
    let events = Arc::new(Events::default());
    let calibration = start_calibration(&service, events.clone(), settings.clone());
    finished_result(events.wait(calibration, Duration::from_secs(120)));

    assert!(read_results(&service).mimo.unwrap().stale);
    assert!(!automatic_camilla_has_mimo(
        &service,
        &root.join("automatic-live")
    ));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn room_dba_mimo_runs_and_becomes_stale_after_recalibration() {
    let root = temp_root("mimo-stale");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let service = deterministic_service();
    open(&service, &path);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&service, calibration_events.clone(), settings.clone());
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    let mimo_events = Arc::new(Events::default());
    let Response::Mimo(started) = call_ok(
        &service,
        Request::Mimo(MimoParams {
            settings: Some(settings.clone()),
        }),
        mimo_events.clone(),
    ) else {
        panic!("MIMO must start");
    };
    let JobResult::Mimo = finished_result(mimo_events.wait(started.job, Duration::from_secs(120)))
    else {
        panic!("MIMO completion must be returned");
    };
    let results = read_results(&service);
    let calibration_summary = results.calibration.unwrap();
    let summary = results.mimo.unwrap();
    assert!(!summary.stale);
    let direct = secs_engine_testkit::production_case("room_dba", 1);
    let ctx = secs_compute::Context::new(
        1,
        secs_compute::MemoryBudget::new(u64::MAX),
        Arc::new(secs_compute::NoProgress),
        secs_compute::CancelToken::new(),
    );
    let direct_mimo = secs_engine::compute(
        &direct.design,
        &direct.measurements,
        &direct.config,
        &settings,
        &secs_engine::Tuning::default(),
        &ctx,
    )
    .unwrap();
    assert_eq!(cached_paths(&path), direct_mimo.paths);
    assert!(summary.support_count > 0);
    let expected_mains = direct.design.main_roles().count();
    let expected_rows = direct.design.positions().count() * expected_mains;
    assert_eq!(calibration_summary.late_energy.len(), expected_rows);
    assert_eq!(summary.scorecard.len(), expected_mains);
    assert_eq!(summary.late_energy.len(), expected_rows);
    for score in &direct_mimo.scorecard {
        let restored = summary
            .scorecard
            .iter()
            .find(|row| row.role == score.role)
            .unwrap();
        assert_eq!(
            restored.after_mimo.in1_20.to_bits(),
            score.after_mimo.in1_20.to_bits()
        );
        assert_eq!(
            restored.after_mimo.gd_20.to_bits(),
            score.after_mimo.gd_20.to_bits()
        );
    }
    let main_position = direct.design.main_position();
    for role in direct_mimo
        .paths
        .paths
        .iter()
        .map(|path| path.input.as_str())
        .collect::<std::collections::BTreeSet<_>>()
    {
        let after = calibration_summary
            .late_energy
            .iter()
            .find(|row| row.position == main_position && row.role == role)
            .unwrap();
        let after_mimo = summary
            .late_energy
            .iter()
            .find(|row| row.position == main_position && row.role == role)
            .unwrap();
        assert!(after_mimo.after_mimo_db < after.after_db);
    }

    let blocked_folder = root.join("blocked-mimo-FilterText");
    let Envelope::Err(error) = service.call(
        Request::Export(ExportParams {
            kind: ExportKind::FilterText,
            folder: blocked_folder.to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("MIMO filter text export must fail synchronously");
    };
    assert_eq!(error.code, ErrorCode::ExportScope);
    assert_eq!(error.params.get("reason").map(String::as_str), Some("subs"));
    assert!(!blocked_folder.exists());

    let eqapo_folder = root.join("mimo-eqapo");
    let eqapo = export_and_wait(
        &service,
        ExportKind::EqualizerApo,
        &eqapo_folder,
        Arc::new(Events::default()),
    );
    let body = eqapo
        .files
        .iter()
        .find(|path| {
            let path = Path::new(path);
            path.extension().is_some_and(|extension| extension == "txt")
                && path.parent().is_some_and(|parent| parent != eqapo_folder)
                && path.file_name().is_some_and(|name| name != "config.txt")
        })
        .expect("Equalizer APO filter body");
    let body = fs::read_to_string(body).unwrap();
    assert!(body.contains("# Raw input copies for MIMO support"));
    assert!(body.contains("# support "));

    // The same live/cached result must produce identical explicit-folder bytes.
    let folder = root.join("explicit-mimo");
    let live = export_and_wait(
        &service,
        ExportKind::CamillaDsp,
        &folder,
        Arc::new(Events::default()),
    );
    assert_eq!(Path::new(&live.folder), folder);
    let live_bytes = snapshot_relative_files(&folder, &live.files);
    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        Arc::new(NoEvents),
    );
    let reopened = deterministic_service();
    open(&reopened, &path);
    let reopened_results = read_results(&reopened);
    assert_eq!(
        reopened_results.calibration.unwrap().late_energy,
        calibration_summary.late_energy
    );
    let reopened_mimo = reopened_results.mimo.unwrap();
    assert_eq!(reopened_mimo.scorecard, summary.scorecard);
    assert_eq!(reopened_mimo.late_energy, summary.late_energy);
    let cached = export_and_wait(
        &reopened,
        ExportKind::CamillaDsp,
        &folder,
        Arc::new(Events::default()),
    );
    assert_snapshots_equal(
        &live_bytes,
        &snapshot_relative_files(&folder, &cached.files),
    );

    // Preparation policy participates in the calibration identity for both
    // live and reopened results, and consequently invalidates their MIMO.
    for current in [&service, &reopened] {
        for (policy, expected_stale) in [
            (secs_types::calibration::SampleRatePolicy::Resample, true),
            (secs_types::calibration::SampleRatePolicy::Reject, false),
        ] {
            call_ok(
                current,
                Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
                    edit: secs_types::settings_edit::SettingEdit::SampleRatePolicy { policy },
                }),
                Arc::new(NoEvents),
            );
            let result = read_results(current);
            let calibration = result.calibration.unwrap();
            assert_eq!(calibration.stale, expected_stale);
            assert_eq!(
                calibration.stale_reason.as_deref(),
                expected_stale.then_some("settings")
            );
            assert_eq!(result.mimo.unwrap().stale, expected_stale);
            call_ok(
                current,
                Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
                Arc::new(NoEvents),
            );
            let reloaded = Service::new(Deps::default());
            open(&reloaded, &path);
            let result = read_results(&reloaded);
            assert_eq!(result.calibration.unwrap().stale, expected_stale);
            assert_eq!(result.mimo.unwrap().stale, expected_stale);
        }
    }

    // A compute preference change invalidates both live and reopened MIMO,
    // even when both preferences currently resolve to the CPU.
    for current in [&service, &reopened] {
        for (compute, expected_stale) in [
            (secs_types::compute::ComputePreference::Cpu, true),
            (secs_types::compute::ComputePreference::Auto, false),
        ] {
            call_ok(
                current,
                Request::AppSettingsSet(secs_types::ipc::AppSettingsParams {
                    settings: secs_types::ipc::AppSettings {
                        compute,
                        ..Default::default()
                    },
                }),
                Arc::new(NoEvents),
            );
            let Response::Results(result) = call_ok(current, Request::Results, Arc::new(NoEvents))
            else {
                panic!()
            };
            assert_eq!(result.mimo.unwrap().stale, expected_stale);
            let output = export_and_wait(
                current,
                ExportKind::CamillaDsp,
                &folder,
                Arc::new(Events::default()),
            );
            let yaml = output
                .files
                .iter()
                .find(|path| path.ends_with(".yml") || path.ends_with(".yaml"))
                .unwrap();
            assert_eq!(
                fs::read_to_string(yaml).unwrap().contains("mimo_"),
                !expected_stale
            );
            assert!(!folder.join("compute-provenance.json").exists());
        }
    }

    for current in [&service, &reopened] {
        call_ok(
            current,
            Request::ExportFolderSet(secs_types::ipc::ExportFolderSetParams {
                folder: root.join("automatic").to_string_lossy().into_owned(),
            }),
            Arc::new(NoEvents),
        );
        let export_auto = |current: &Service| {
            let events = Arc::new(Events::default());
            let Response::Export(started) = call_ok(
                current,
                Request::Export(ExportParams {
                    kind: ExportKind::CamillaDsp,
                    folder: String::new(),
                    mdat_scope: None,
                    design_name: None,
                    camilla_devices: None,
                    extra_rates: Vec::new(),
                }),
                events.clone(),
            ) else {
                panic!("export must start");
            };
            let JobResult::Export(result) =
                finished_result(events.wait(started.job, Duration::from_secs(120)))
            else {
                panic!("export result");
            };
            result
        };
        assert!(export_auto(current).folder.contains("_MIMO_"));
        let mut disabled = settings.clone();
        disabled.mimo_on = false;
        call_ok(
            current,
            Request::CalibrationSettingsSet(secs_types::ipc::CalibrationSettingsParams {
                settings: disabled,
            }),
            Arc::new(NoEvents),
        );
        let no_mimo = export_auto(current);
        assert!(!no_mimo.folder.contains("_MIMO_"));
        let yaml = no_mimo
            .files
            .iter()
            .find(|file| file.ends_with(".yml") || file.ends_with(".yaml"))
            .unwrap();
        assert!(!fs::read_to_string(yaml).unwrap().contains("mimo_"));
        let mut changed = settings.clone();
        changed.fsiso -= 1.0;
        call_ok(
            current,
            Request::CalibrationSettingsSet(secs_types::ipc::CalibrationSettingsParams {
                settings: changed,
            }),
            Arc::new(NoEvents),
        );
        assert!(!export_auto(current).folder.contains("_MIMO_"));
        let mut stale = settings.clone();
        stale.rc_lo += 1.0;
        call_ok(
            current,
            Request::CalibrationSettingsSet(secs_types::ipc::CalibrationSettingsParams {
                settings: stale,
            }),
            Arc::new(NoEvents),
        );
        let stale = export_auto(current);
        assert!(stale.stale);
        assert!(!stale.folder.contains("_MIMO_"));
    }

    settings.rc_lo += 1.0;
    let recalibration_events = Arc::new(Events::default());
    let recalibration = start_calibration(&service, recalibration_events.clone(), settings);
    finished_result(recalibration_events.wait(recalibration, Duration::from_secs(120)));
    let stale = match call_ok(&service, Request::Results, Arc::new(NoEvents)) {
        Response::Results(results) => results.mimo.unwrap().stale,
        other => panic!("unexpected results response: {other:?}"),
    };
    assert!(stale);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn calibration_preserves_settings_edited_while_the_job_runs() {
    let root = temp_root("settings-race");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room_dba");
    let service = service();
    open(&service, &path);
    let events = Arc::new(PausedProgress::default());
    let job = start_calibration(&service, events.clone(), settings.clone());
    events.wait_until_paused();

    call_ok(
        &service,
        Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
            edit: secs_types::settings_edit::SettingEdit::RcLo {
                hz: settings.rc_lo + 5.0,
            },
        }),
        Arc::new(NoEvents),
    );
    events.release();
    finished_result(events.events.wait(job, Duration::from_secs(120)));

    let Response::CalibrationSettingsGet(current) = call_ok(
        &service,
        Request::CalibrationSettingsGet,
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert_eq!(current.rc_lo, settings.rc_lo + 5.0);
    let summary = read_results(&service).calibration.unwrap();
    assert!(summary.stale);
    assert_eq!(summary.stale_reason.as_deref(), Some("settings"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn recommend_xo_preserves_xo_edits_made_while_the_job_runs() {
    for (case, edit) in [
        (
            "xo",
            secs_types::settings_edit::SettingEdit::Xo {
                group: "프론트".into(),
                hz: 177.0,
            },
        ),
        (
            "include",
            secs_types::settings_edit::SettingEdit::XoRecInclude { on: false },
        ),
    ] {
        let root = temp_root(&format!("xo-settings-race-{case}"));
        fs::create_dir_all(&root).unwrap();
        let (path, _) = write_case(&root, "room_dba");
        let service = service();
        open(&service, &path);
        let events = Arc::new(PausedProgress::default());
        let Response::RecommendXo(started) =
            call_ok(&service, Request::RecommendXo(None), events.clone())
        else {
            panic!("XO recommendation must start");
        };
        events.wait_until_paused();
        call_ok(
            &service,
            Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
                edit,
            }),
            Arc::new(NoEvents),
        );
        let Response::CalibrationSettingsGet(edited) = call_ok(
            &service,
            Request::CalibrationSettingsGet,
            Arc::new(NoEvents),
        ) else {
            panic!()
        };
        events.release();
        assert!(matches!(
            finished_result(events.events.wait(started.job, Duration::from_secs(120))),
            JobResult::Xo(_)
        ));
        let Response::CalibrationSettingsGet(current) = call_ok(
            &service,
            Request::CalibrationSettingsGet,
            Arc::new(NoEvents),
        ) else {
            panic!()
        };
        assert_eq!(current.xo, edited.xo, "{case}");
        assert_eq!(current.xo_rec_include, edited.xo_rec_include, "{case}");
        match case {
            "xo" => assert_eq!(
                current
                    .xo
                    .iter()
                    .find(|(group, _)| group == "프론트")
                    .map(|(_, hz)| *hz),
                Some(177.0)
            ),
            "include" => assert!(!current.xo_rec_include),
            _ => unreachable!(),
        }
        assert!(!current.xo_rec.is_empty(), "{case}");
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn result_curves_and_lines_drop_mimo_after_the_compute_preference_changes() {
    let root = temp_root("mimo-compute-curves");
    fs::create_dir_all(&root).unwrap();
    let (path, mut settings) = write_case(&root, "room_dba");
    settings.mimo_on = true;
    settings.mimo_max_supports = 1;
    let service = service();
    open(&service, &path);
    calibrate_and_run_mimo(&service, &settings);
    assert!(!read_results(&service).mimo.unwrap().stale);
    assert!(spl_has_mimo_curve(&service));
    assert!(support_counts(&service).iter().all(Option::is_some));

    let Response::AppSettingsGet(mut app) =
        call_ok(&service, Request::AppSettingsGet, Arc::new(NoEvents))
    else {
        panic!()
    };
    assert_ne!(app.compute, secs_types::compute::ComputePreference::Cpu);
    app.compute = secs_types::compute::ComputePreference::Cpu;
    call_ok(
        &service,
        Request::AppSettingsSet(secs_types::ipc::AppSettingsParams { settings: app }),
        Arc::new(NoEvents),
    );

    assert!(read_results(&service).mimo.unwrap().stale);
    assert!(!spl_has_mimo_curve(&service), "stale MIMO is still drawn");
    assert!(
        support_counts(&service).iter().all(Option::is_none),
        "stale MIMO support counts are still listed"
    );
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn calibration_reads_measurements_inside_its_job_outside_the_session_lock() {
    let root = temp_root("load-outside-lock");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "dolby");
    let mut wavs: Vec<_> = fs::read_dir(path.with_extension("irs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "wav"))
        .collect();
    wavs.sort();
    let fifo = Fifo::replace(&wavs[wavs.len() / 2]);
    let service = Arc::new(service());
    open(&service, &path);

    let events = Arc::new(Events::default());
    let calibrate = Request::Calibrate(CalibrateParams {
        settings: Some(settings),
        ..Default::default()
    });
    let started =
        match call_with_events(&service, calibrate, events.clone(), Duration::from_secs(10)) {
            Some(Envelope::Ok(response)) => {
                let Response::Calibrate(started) = *response else {
                    panic!()
                };
                started
            }
            Some(Envelope::Err(error)) => panic!("{error:?}"),
            None => {
                fifo.release();
                panic!("calibrate read the measurements before it started a job");
            }
        };
    // The job now waits on the pipe. The session stays free and the compute
    // group is busy, so a second request is refused without reading a WAV.
    assert!(
        matches!(
            call_within(&service, Request::ProjectInfo, Duration::from_secs(5)),
            Some(Envelope::Ok(_))
        ),
        "project_info waited for the measurement load"
    );
    for request in [
        Request::Calibrate(CalibrateParams::default()),
        Request::RecommendXo(None),
    ] {
        let method = request.method();
        match call_within(&service, request, Duration::from_secs(5)) {
            Some(Envelope::Err(error)) => assert_eq!(error.code, ErrorCode::JobBusy, "{method}"),
            other => {
                fifo.release();
                panic!("{method} must be refused at once: {other:?}");
            }
        }
    }
    let Response::Jobs(jobs) = call_ok(&service, Request::Jobs, Arc::new(NoEvents)) else {
        panic!()
    };
    let job = jobs.iter().find(|job| job.job == started.job).unwrap();
    assert_eq!(job.status, JobStatus::Running);
    assert!(job.cancellable);
    call_ok(
        &service,
        Request::JobCancel(JobCancelParams { job: started.job }),
        Arc::new(NoEvents),
    );
    // Hand the bytes over in case the job already waits on the pipe. A job
    // cancelled before it reaches the pipe never opens it, so the writer is
    // left waiting rather than joined.
    fifo.release();
    let finished = events.wait(started.job, Duration::from_secs(60));
    assert!(
        matches!(
            finished,
            JobEvent::Finished {
                status: JobStatus::Cancelled,
                ..
            }
        ),
        "{finished:?}"
    );
    assert!(read_results(&service).calibration.is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rc_agreement_calibration_fails_with_typed_position_reason() {
    let root = temp_root("rc-agreement");
    fs::create_dir_all(&root).unwrap();
    let service = service();
    call_ok(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: root.join("empty.secs").to_string_lossy().into_owned(),
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    let events = Arc::new(Events::default());
    let job = start_calibration(
        &service,
        events.clone(),
        CalibrationSettings {
            rc_hi_basis: secs_types::calibration::RcHiBasis::Agreement,
            ..Default::default()
        },
    );
    let JobEvent::Finished {
        error: Some(error), ..
    } = events.wait(job, Duration::from_secs(30))
    else {
        panic!("agreement must fail without measured positions")
    };
    assert_eq!(error.code, ErrorCode::EngineInput);
    assert_eq!(error.params["reason"], "rc_agreement_positions");
    assert_eq!(error.params["measured"], "0");
    assert_eq!(error.params["required"], "3");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rc_upper_edits_mark_calibration_stale() {
    use secs_types::{calibration::RcHiBasis, settings_edit::SettingEdit};
    let root = temp_root("rc-upper-stale");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room");
    let service = service();
    open(&service, &path);
    let events = Arc::new(Events::default());
    let job = start_calibration(&service, events.clone(), settings.clone());
    finished_result(events.wait(job, Duration::from_secs(120)));
    for edit in [
        SettingEdit::RcHiBasis {
            basis: RcHiBasis::Lenient,
        },
        SettingEdit::RcHiTransition { factor: 2 },
    ] {
        call_ok(
            &service,
            Request::CalibrationSettingsEdit(secs_types::ipc::CalibrationSettingsEditParams {
                edit,
            }),
            Arc::new(NoEvents),
        );
        assert!(read_results(&service).calibration.unwrap().stale);
        call_ok(
            &service,
            Request::CalibrationSettingsSet(secs_types::ipc::CalibrationSettingsParams {
                settings: settings.clone(),
            }),
            Arc::new(NoEvents),
        );
        assert!(!read_results(&service).calibration.unwrap().stale);
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn typed_recommend_xo_can_ask_for_a_background_job_td157() {
    let request = Request::RecommendXo(Some(secs_types::ipc::RecommendXoParams {
        background: true,
    }));
    let root = temp_root("typed-background-xo");
    fs::create_dir_all(&root).unwrap();
    let (path, _) = write_case(&root, "room");
    let service = service();
    open(&service, &path);
    let events = Arc::new(Events::default());
    let Response::RecommendXo(started) = call_ok(&service, request, events.clone()) else {
        panic!("expected recommend_xo response");
    };
    events.wait(started.job, Duration::from_secs(120));
    let values = lock(&events.values);
    let started = values
        .iter()
        .find(|event| matches!(event, JobEvent::Started { .. }))
        .unwrap();
    assert_eq!(
        serde_json::to_value(started).unwrap()["kind"],
        "recommend_xo"
    );
    drop(values);
    fs::remove_dir_all(root).unwrap();
}
