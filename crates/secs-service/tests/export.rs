#![allow(deprecated)]
#![forbid(unsafe_code)]

mod support;
use support::*;

#[test]
fn project_save_returns_before_the_side_mdat_exists_and_a_job_writes_it_td149() {
    let root = temp_root("side-mdat-job");
    fs::create_dir_all(&root).unwrap();
    let (path, _) = write_case(&root, "room");
    let side_mdat = path.with_extension("mdat");
    let service = deterministic_service();
    open(&service, &path);
    let events = Arc::new(Events::holding_progress());

    let response = service.call(
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        events.clone(),
    );
    assert!(
        matches!(response, Envelope::Ok(response) if matches!(*response, Response::ProjectSave(_)))
    );
    assert!(!side_mdat.exists());
    let job = events.wait_started(JobKind::Export, Duration::from_secs(2));
    events.release_progress();
    assert!(matches!(
        events.wait(job, Duration::from_secs(30)),
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            error: None,
            ..
        }
    ));
    assert!(side_mdat.is_file());
    secs_mdat::load_mdat_project(&fs::read(&side_mdat).unwrap(), &secs_jos::Limits::default())
        .unwrap();

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_side_mdat_job_reads_sources_from_the_snapshot_not_the_disk() {
    let root = temp_root("side-mdat-source-snapshot");
    fs::create_dir_all(&root).unwrap();
    let (path, _) = write_case(&root, "room");
    let store = secs_project::MeasurementStore::open(&path).unwrap();
    let raw = store
        .records()
        .into_iter()
        .flat_map(|record| record.ir.into_values())
        .find(|raw| !raw.is_empty())
        .unwrap();
    let wav = if Path::new(&raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        path.parent().unwrap().join(raw)
    };
    secs_project::save_source(
        &wav,
        Some(&secs_mdat::SourceInfo {
            strings: vec![("outputName".into(), "snapshot source".into())],
            ..Default::default()
        }),
    )
    .unwrap();

    let service = deterministic_service();
    open(&service, &path);
    secs_project::save_source(
        &wav,
        Some(&secs_mdat::SourceInfo {
            strings: vec![("outputName".into(), "disk source".into())],
            ..Default::default()
        }),
    )
    .unwrap();
    let events = Arc::new(Events::default());
    let response = service.call(
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        events.clone(),
    );
    assert!(matches!(response, Envelope::Ok(_)));
    let job = events.wait_started(JobKind::Export, Duration::from_secs(2));
    assert!(matches!(
        events.wait(job, Duration::from_secs(30)),
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            ..
        }
    ));
    let loaded = secs_mdat::load_mdat_project(
        &fs::read(path.with_extension("mdat")).unwrap(),
        &secs_jos::Limits::default(),
    )
    .unwrap();
    assert!(loaded.measurements.iter().any(|measurement| {
        measurement
            .source_info
            .strings
            .iter()
            .any(|(name, value)| name == "outputName" && value == "snapshot source")
    }));
    assert!(loaded.measurements.iter().all(|measurement| {
        measurement
            .source_info
            .strings
            .iter()
            .all(|(name, value)| name != "outputName" || value != "disk source")
    }));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn side_mdat_failure_is_a_job_error_not_a_silent_save_td149() {
    let root = temp_root("side-mdat-job-error");
    fs::create_dir_all(&root).unwrap();
    let (path, _) = write_case(&root, "room");
    let side_mdat = path.with_extension("mdat");
    fs::create_dir(&side_mdat).unwrap();
    let service = service();
    open(&service, &path);
    let events = Arc::new(Events::default());

    let response = service.call(
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        events.clone(),
    );
    assert!(
        matches!(response, Envelope::Ok(response) if matches!(*response, Response::ProjectSave(_)))
    );
    let job = events.wait_started(JobKind::Export, Duration::from_secs(2));
    let JobEvent::Finished {
        status: JobStatus::Failed,
        error: Some(error),
        ..
    } = events.wait(job, Duration::from_secs(30))
    else {
        panic!("side mdat failure must finish the export job with an error")
    };
    assert!(
        error
            .detail
            .contains(&side_mdat.to_string_lossy().into_owned())
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn side_mdat_job_reports_sidecar_warnings_td183() {
    let root = temp_root("side-mdat-sidecar-warning");
    fs::create_dir_all(&root).unwrap();
    let (path, _) = write_case(&root, "room");
    let store = MeasurementStore::open(&path).unwrap();
    let record = store.records().into_iter().next().unwrap();
    let role = record.ir.keys().next().unwrap();
    let wav = path.parent().unwrap().join(&record.ir[role]);
    let sidecar = secs_project::source_path(&wav);
    secs_project::save_source(&wav, Some(&secs_mdat::SourceInfo::default())).unwrap();
    fs::remove_file(&sidecar).unwrap();
    fs::create_dir(&sidecar).unwrap();
    let service = deterministic_service();
    open(&service, &path);
    let events = Arc::new(Events::default());

    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        events.clone(),
    );
    let job = events.wait_started(JobKind::Export, Duration::from_secs(2));
    assert!(matches!(
        events.wait(job, Duration::from_secs(30)),
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            ..
        }
    ));
    let expected_entry = sidecar
        .components()
        .collect::<PathBuf>()
        .to_string_lossy()
        .into_owned();
    let values = lock(&events.values);
    assert!(
        values.iter().any(|event| {
            matches!(event, JobEvent::Log { code, params, .. }
                if code == "measurement.source.skipped"
                    && params.get("entry") == Some(&expected_entry))
        }),
        "events: {values:?}; sidecar: {}",
        sidecar.display()
    );
    drop(values);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn successive_saves_to_two_paths_write_both_side_mdats_td149() {
    let root = temp_root("side-mdat-two-paths");
    fs::create_dir_all(&root).unwrap();
    let (source, _) = write_case(&root, "room");
    let first = root.join("first.secs");
    let second = root.join("second.secs");
    let first_mdat = first.with_extension("mdat");
    let second_mdat = second.with_extension("mdat");
    let (blocked_tx, blocked_rx) = std::sync::mpsc::sync_channel(1);
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let uuid_calls = Arc::new(AtomicU64::new(0));
    let worker_gate = gate.clone();
    let worker_calls = uuid_calls.clone();
    let service = Service::new(Deps {
        uuid: Arc::new(Mutex::new(Box::new(move || {
            let call = worker_calls.fetch_add(1, Ordering::Relaxed);
            if call == 0 {
                blocked_tx.send(()).unwrap();
                let mut released = lock(&worker_gate.0);
                while !*released {
                    released = worker_gate.1.wait(released).unwrap();
                }
            }
            Ok(0x1234_5678_9abc_def0 + u128::from(call))
        }))),
        unix_ms: Arc::new(|| FIXED_MS),
        ..Deps::default()
    });
    open(&service, &source);
    let events = Arc::new(Events::default());

    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams {
            path: Some(first.to_string_lossy().into_owned()),
        }),
        events.clone(),
    );
    blocked_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let job = events.wait_started(JobKind::Export, Duration::from_secs(2));
    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams {
            path: Some(second.to_string_lossy().into_owned()),
        }),
        events.clone(),
    );
    *lock(&gate.0) = true;
    gate.1.notify_all();
    for job in events.started_jobs(JobKind::Export) {
        assert!(matches!(
            events.wait(job, Duration::from_secs(30)),
            JobEvent::Finished {
                status: JobStatus::Succeeded,
                error: None,
                ..
            }
        ));
    }
    assert!(events.try_wait(job, Duration::from_secs(1)).is_some());
    assert!(first_mdat.is_file(), "first side mdat was not written");
    assert!(second_mdat.is_file(), "second side mdat was not written");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn saves_while_a_side_mdat_job_runs_coalesce_into_one_job_td149() {
    let root = temp_root("side-mdat-coalesce");
    fs::create_dir_all(&root).unwrap();
    let (path, _) = write_case(&root, "room");
    let side_mdat = path.with_extension("mdat");
    let mut store = MeasurementStore::open(&path).unwrap();
    let record = store.records().into_iter().next().unwrap();
    let role = record.ir.keys().next().unwrap().clone();
    let raw = PathBuf::from(record.ir[&role].clone());
    let wav = if raw.is_absolute() {
        raw
    } else {
        path.parent().unwrap().join(raw)
    };
    let (samples, sample_rate) = store.ir_of(&role, record.pos).unwrap();
    let mut samples = samples.to_vec();
    let changed_sample = if samples[0] == 0.25 { 0.5 } else { 0.25 };

    let (blocked_tx, blocked_rx) = std::sync::mpsc::sync_channel(1);
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let uuid_calls = Arc::new(AtomicU64::new(0));
    let worker_gate = gate.clone();
    let worker_calls = uuid_calls.clone();
    let service = Service::new(Deps {
        uuid: Arc::new(Mutex::new(Box::new(move || {
            let call = worker_calls.fetch_add(1, Ordering::Relaxed);
            if call == 0 {
                blocked_tx.send(()).unwrap();
                let mut released = lock(&worker_gate.0);
                while !*released {
                    released = worker_gate.1.wait(released).unwrap();
                }
            }
            Ok(0x1234_5678_9abc_def0 + u128::from(call))
        }))),
        unix_ms: Arc::new(|| FIXED_MS),
        ..Deps::default()
    });
    open(&service, &path);
    let events = Arc::new(Events::default());

    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        events.clone(),
    );
    blocked_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    samples[0] = changed_sample;
    fs::write(&wav, secs_project::write_wav_float32(&samples, sample_rate)).unwrap();
    for _ in 0..2 {
        call_ok(
            &service,
            Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
            events.clone(),
        );
    }
    let Response::Jobs(running) = call_ok(&service, Request::Jobs, Arc::new(NoEvents)) else {
        panic!("jobs request must return snapshots")
    };
    let running_side_mdats = running
        .iter()
        .filter(|snapshot| snapshot.kind == JobKind::Export && !snapshot.status.is_terminal())
        .count();
    let started = events.started_jobs(JobKind::Export);
    *lock(&gate.0) = true;
    gate.1.notify_all();
    for job in &started {
        events.wait(*job, Duration::from_secs(30));
    }

    assert_eq!(
        started.len(),
        1,
        "side mdat saves started more than one job"
    );
    assert_eq!(
        running_side_mdats, 1,
        "more than one side mdat job was running"
    );
    let loaded =
        secs_mdat::load_mdat_project(&fs::read(&side_mdat).unwrap(), &secs_jos::Limits::default())
            .unwrap();
    let measurement = loaded
        .measurements
        .iter()
        .find(|measurement| measurement.title == format!("{role}_P{}", record.pos))
        .unwrap();
    let (saved, _) = loaded.ir_provider.get_ir(&measurement.uuid).unwrap();
    assert!((saved[0] - f64::from(changed_sample)).abs() < 1e-6);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn shutdown_waits_for_a_running_side_mdat_job_td149() {
    let root = temp_root("side-mdat-shutdown");
    fs::create_dir_all(&root).unwrap();
    let (path, _) = write_case(&root, "room");
    let side_mdat = path.with_extension("mdat");
    let (blocked_tx, blocked_rx) = std::sync::mpsc::sync_channel(1);
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let worker_gate = gate.clone();
    let first = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let worker_first = first.clone();
    let service = Arc::new(Service::new(Deps {
        uuid: Arc::new(Mutex::new(Box::new(move || {
            if worker_first.swap(false, Ordering::Relaxed) {
                blocked_tx.send(()).unwrap();
                let mut released = lock(&worker_gate.0);
                while !*released {
                    released = worker_gate.1.wait(released).unwrap();
                }
            }
            Ok(0x1234_5678_9abc_def0)
        }))),
        unix_ms: Arc::new(|| FIXED_MS),
        ..Deps::default()
    }));
    open(&service, &path);
    let events = Arc::new(Events::default());
    call_ok(
        &service,
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        events.clone(),
    );
    blocked_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let job = events.wait_started(JobKind::Export, Duration::from_secs(2));

    let shutdown_service = service.clone();
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::sync_channel(1);
    let shutdown = std::thread::spawn(move || {
        shutdown_tx
            .send(shutdown_service.shutdown(Duration::from_secs(5)))
            .unwrap();
    });
    let returned_early = shutdown_rx.recv_timeout(Duration::from_millis(200)).ok();
    *lock(&gate.0) = true;
    gate.1.notify_all();
    let completed = returned_early.unwrap_or_else(|| {
        shutdown_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("shutdown did not finish after the side mdat job")
    });
    shutdown.join().unwrap();

    assert!(
        returned_early.is_none(),
        "shutdown returned before the side mdat job"
    );
    assert!(completed);
    assert!(matches!(
        events.wait(job, Duration::from_secs(30)),
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            error: None,
            ..
        }
    ));
    assert!(side_mdat.is_file());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn side_mdat_failure_reaches_the_log_sink_td151() {
    let root = temp_root("side-mdat-log");
    fs::create_dir_all(&root).unwrap();
    let (path, _) = write_case(&root, "room");
    let side_mdat = path.with_extension("mdat");
    fs::create_dir(&side_mdat).unwrap();
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink_lines = lines.clone();
    let service = Service::new(Deps {
        log: Arc::new(move |level, scope, line| {
            lock(&sink_lines).push((level, scope.to_owned(), line.to_owned()));
        }),
        ..Deps::default()
    });
    open(&service, &path);

    let events = Arc::new(Events::default());
    let response = service.call(
        Request::ProjectSave(secs_types::ipc::ProjectSaveParams { path: None }),
        events.clone(),
    );
    assert!(
        matches!(response, Envelope::Ok(response) if matches!(*response, Response::ProjectSave(_)))
    );
    let job = events.wait_started(JobKind::Export, Duration::from_secs(2));
    assert!(matches!(
        events.wait(job, Duration::from_secs(30)),
        JobEvent::Finished {
            status: JobStatus::Failed,
            error: Some(_),
            ..
        }
    ));
    assert!(lock(&lines).is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn camilla_export_cancelled_before_commit_writes_nothing_td196() {
    let root = temp_root("cancel-camilla-prepare");
    fs::create_dir_all(&root).unwrap();
    let destination = root.join("export");
    fs::create_dir(&destination).unwrap();
    let (path, settings) = write_case(&root, "room");
    let service = Arc::new(deterministic_service());
    open(&service, &path);
    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&service, calibration_events.clone(), settings);
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));
    let Response::DspSet(camilla) = call_ok(
        &service,
        Request::DspSet(DspSetParams {
            preset: Some("camilla".into()),
            fields: None,
        }),
        Arc::new(NoEvents),
    ) else {
        panic!();
    };
    let mut fields = camilla.fields;
    fields.output_rate = OutputRateChoice::Fixed(96_000);
    call_ok(
        &service,
        Request::DspSet(DspSetParams {
            preset: None,
            fields: Some(fields),
        }),
        Arc::new(NoEvents),
    );
    let events = Arc::new(Events::default());
    let sink = Arc::new(CancelFirstExportProgress {
        service: service.clone(),
        events: events.clone(),
        requested: std::sync::atomic::AtomicBool::new(false),
    });
    let Response::Export(started) = call_ok(
        &service,
        Request::Export(ExportParams {
            kind: ExportKind::CamillaDsp,
            folder: destination.to_string_lossy().into_owned(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        sink.clone(),
    ) else {
        panic!();
    };

    assert!(matches!(
        events.wait(started.job, Duration::from_secs(180)),
        JobEvent::Finished {
            status: JobStatus::Cancelled,
            ..
        }
    ));
    assert!(sink.requested.load(Ordering::SeqCst));
    assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn changing_only_output_rate_keeps_calibration_fresh_for_export() {
    let root = temp_root("output-rate-fresh");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "dolby");
    let service = deterministic_service();
    open(&service, &path);

    let calibration_events = Arc::new(Events::default());
    let calibration = start_calibration(&service, calibration_events.clone(), settings);
    finished_result(calibration_events.wait(calibration, Duration::from_secs(120)));

    let Response::DspSet(camilla) = call_ok(
        &service,
        Request::DspSet(DspSetParams {
            preset: Some("camilla".into()),
            fields: None,
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("CamillaDSP preset must be selected")
    };
    let mut fields = camilla.fields;
    fields.output_rate = OutputRateChoice::Fixed(96_000);
    let Response::DspSet(changed) = call_ok(
        &service,
        Request::DspSet(DspSetParams {
            preset: None,
            fields: Some(fields),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("output rate must change")
    };
    assert_eq!(changed.fields.output_rate, OutputRateChoice::Fixed(96_000));

    let result = export_and_wait(
        &service,
        ExportKind::CamillaDsp,
        &root.join("export"),
        Arc::new(Events::default()),
    );
    assert!(!result.stale);
    assert_eq!(result.output_rate, 96_000);
    assert!(result.notes.is_empty());
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(Path::new(&result.folder).join("secs-design.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["sample_rate"], 96_000);
    assert_eq!(manifest["design_sample_rate"], 48_000);
    assert_eq!(manifest["output_rate_auto"], false);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn measurement_mdat_exports_every_position_without_a_result() {
    let root = temp_root("measurement-mdat");
    fs::create_dir_all(&root).unwrap();
    let (path, _) = write_case(&root, "room_dba");
    let stored = stored_titles_and_rates(&path);
    assert!(
        stored
            .iter()
            .map(|(position, _, _)| *position)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            >= 2
    );
    let service = deterministic_service();
    open(&service, &path);
    assert!(read_results(&service).calibration.is_none());
    let base = root.join("automatic");
    call_ok(
        &service,
        Request::ExportFolderSet(secs_types::ipc::ExportFolderSetParams {
            folder: base.to_string_lossy().into_owned(),
        }),
        Arc::new(NoEvents),
    );
    let result = export_scope_and_wait(
        &service,
        ExportKind::BeforeAfterMdat,
        Path::new(""),
        Some(MdatScope::Measurements),
        Arc::new(Events::default()),
    );
    assert!(!result.stale);
    assert_eq!(result.output_rate, 48_000);
    assert!(result.notes.is_empty());
    let folder = PathBuf::from(&result.folder);
    assert_eq!(folder.parent(), Some(base.as_path()));
    assert_eq!(
        folder.file_name().and_then(|name| name.to_str()),
        Some("SECS_mdat_measurements_20240102-030405")
    );
    assert_eq!(result.files.len(), 1);
    let file = Path::new(&result.files[0]);
    assert_eq!(
        file.file_name().and_then(|name| name.to_str()),
        Some("SECS 원 측정.mdat")
    );
    assert!(!folder.join("compute-provenance.json").exists());
    let loaded =
        secs_mdat::load_mdat_project(&fs::read(file).unwrap(), &secs_jos::Limits::default())
            .unwrap();
    assert_eq!(loaded.measurements.len(), stored.len());
    for (measurement, (_, title, sample_rate)) in loaded.measurements.iter().zip(&stored) {
        assert_eq!(&measurement.title, title);
        assert_eq!(measurement.sample_rate, i64::from(*sample_rate));
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rew_mdat_scope_names_say_what_the_file_holds() {
    let root = temp_root("rew-mdat-scopes");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room_dba");
    let stored = stored_titles_and_rates(&path);
    let service = deterministic_service();
    open(&service, &path);
    let events = Arc::new(Events::default());
    let calibration = start_calibration(&service, events.clone(), settings.clone());
    finished_result(events.wait(calibration, Duration::from_secs(120)));

    let all = export_scope_and_wait(
        &service,
        ExportKind::BeforeAfterMdat,
        &root.join("all"),
        Some(MdatScope::All),
        Arc::new(Events::default()),
    );
    let label = settings.label();
    let all_filename = format!("SECS 원 측정+보정 결과 {label}.mdat");
    assert_eq!(
        Path::new(&all.files[0])
            .file_name()
            .and_then(|name| name.to_str()),
        Some(all_filename.as_str())
    );
    let all_loaded = secs_mdat::load_mdat_project(
        &fs::read(&all.files[0]).unwrap(),
        &secs_jos::Limits::default(),
    )
    .unwrap();
    let direct = secs_engine_testkit::production_case("room_dba", 1);
    let run = secs_export::ExportRun {
        design: &direct.design,
        settings: &settings,
    };
    let main_position = direct.design.main_position();
    let before_after =
        secs_export::rew_mdat_items(Some(&run), MdatScope::All, &[], None, None).unwrap();
    assert_eq!(
        all_loaded.measurements[..before_after.len()]
            .iter()
            .map(|measurement| measurement.title.as_str())
            .collect::<Vec<_>>(),
        before_after
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>()
    );
    let other_titles = stored
        .iter()
        .filter(|(position, _, _)| *position != main_position)
        .map(|(_, title, _)| title.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        all_loaded.measurements[before_after.len()..]
            .iter()
            .map(|measurement| measurement.title.as_str())
            .collect::<Vec<_>>(),
        other_titles
    );

    let results = export_scope_and_wait(
        &service,
        ExportKind::BeforeAfterMdat,
        &root.join("results"),
        Some(MdatScope::Results),
        Arc::new(Events::default()),
    );
    let results_filename = format!("SECS 보정 결과 {label}.mdat");
    assert_eq!(
        Path::new(&results.files[0])
            .file_name()
            .and_then(|name| name.to_str()),
        Some(results_filename.as_str())
    );
    let results_loaded = secs_mdat::load_mdat_project(
        &fs::read(&results.files[0]).unwrap(),
        &secs_jos::Limits::default(),
    )
    .unwrap();
    let expected_results =
        secs_export::rew_mdat_items(Some(&run), MdatScope::Results, &[], None, None).unwrap();
    assert_eq!(
        results_loaded
            .measurements
            .iter()
            .map(|measurement| measurement.title.as_str())
            .collect::<Vec<_>>(),
        expected_results
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>()
    );
    assert!(results_loaded.measurements.iter().all(|measurement| {
        !measurement.title.contains("_1 ")
            && (measurement.title.contains("_2 ") || measurement.title.contains("_3 "))
    }));

    let default = export_scope_and_wait(
        &service,
        ExportKind::BeforeAfterMdat,
        &root.join("default"),
        None,
        Arc::new(Events::default()),
    );
    assert_eq!(
        fs::read(&default.files[0]).unwrap(),
        fs::read(&all.files[0]).unwrap()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn exported_measurements_keep_their_curves_and_results_drop_them() {
    use secs_mdat::{CalInfo, SourceInfo};

    let root = temp_root("rew-mdat-curves");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "room_dba");
    let direct = secs_engine_testkit::production_case("room_dba", 1);
    let position = direct.design.main_position();
    let role = direct.design.main_roles().next().unwrap().to_owned();
    let ir_path = secs_project::ir_file(&path, &role, i64::from(position));
    let (raw, _) = secs_project::read_wav_float32(&fs::read(&ir_path).unwrap()).unwrap();
    secs_project::save_source(
        &ir_path,
        Some(&SourceInfo {
            strings: vec![("outputName".into(), "calibrated microphone".into())],
            cal: vec![(
                "meterCal".into(),
                CalInfo {
                    inverse_c_selected: false,
                    source_name: "mic.cal".into(),
                    source_path: "mic.cal".into(),
                    freq: Some(vec![20.0, 20_000.0]),
                    gain: Some(vec![6.0, 6.0]),
                    phase: None,
                    boxed: Vec::new(),
                },
            )],
            ..SourceInfo::default()
        }),
    )
    .unwrap();
    let service = deterministic_service();
    open(&service, &path);
    let events = Arc::new(Events::default());
    let calibration = start_calibration(&service, events.clone(), settings);
    finished_result(events.wait(calibration, Duration::from_secs(120)));
    let export = export_scope_and_wait(
        &service,
        ExportKind::BeforeAfterMdat,
        &root.join("all"),
        Some(MdatScope::All),
        Arc::new(Events::default()),
    );
    let loaded = secs_mdat::load_mdat_project(
        &fs::read(&export.files[0]).unwrap(),
        &secs_jos::Limits::default(),
    )
    .unwrap();
    let measured = loaded
        .measurements
        .iter()
        .find(|measurement| measurement.title.contains(&format!("_{role}_1 ")))
        .unwrap();
    assert!(
        measured
            .source_info
            .cal
            .iter()
            .any(|(name, _)| name == "meterCal")
    );
    let (exported, sample_rate) = loaded.ir_provider.get_ir(&measured.uuid).unwrap();
    assert_eq!(sample_rate, 48_000);
    let expected = raw
        .iter()
        .map(|&sample| f64::from((f64::from(sample) * 0.01) as f32 / 0.01_f32))
        .collect::<Vec<_>>();
    assert_eq!(
        exported
            .iter()
            .map(|sample| sample.to_bits())
            .collect::<Vec<_>>(),
        expected
            .iter()
            .map(|sample| sample.to_bits())
            .collect::<Vec<_>>()
    );
    let computed = loaded
        .measurements
        .iter()
        .find(|measurement| measurement.title.contains(&format!("_{role}_2 ")))
        .unwrap();
    let meter = computed
        .source_info
        .cal
        .iter()
        .find(|(name, _)| name == "meterCal")
        .map(|(_, meter)| meter)
        .unwrap();
    assert!(meter.source_name.is_empty());
    assert!(meter.source_path.is_empty());
    assert!(meter.freq.is_none() && meter.gain.is_none() && meter.phase.is_none());
    assert_eq!(
        computed
            .source_info
            .strings
            .iter()
            .find(|(name, _)| name == "outputName")
            .map(|(_, value)| value.as_str()),
        Some("calibrated microphone")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn relative_export_folder_is_rejected_td211() {
    let root = temp_root("relative-export");
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
    let relative = format!(
        "relative-export-{}-{}",
        std::process::id(),
        TEMP.fetch_add(1, Ordering::Relaxed)
    );
    let relative_path = PathBuf::from(&relative);
    let _ = fs::remove_dir_all(&relative_path);
    let events = Arc::new(Events::default());

    let Envelope::Err(error) = service.call(
        Request::Export(ExportParams {
            kind: ExportKind::CamillaDsp,
            folder: relative.clone(),
            mdat_scope: None,
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        events.clone(),
    ) else {
        panic!("a relative export folder must fail before a job starts")
    };
    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("relative_folder")
    );
    assert_eq!(
        error.params.get("field").map(String::as_str),
        Some("folder")
    );
    assert!(lock(&events.values).is_empty());
    assert!(!relative_path.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn mdat_scope_is_refused_for_other_kinds() {
    let root = temp_root("mdat-scope-invalid");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("room.secs");
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
    for kind in [
        ExportKind::CamillaDsp,
        ExportKind::StagedMdat,
        ExportKind::FilterText,
        ExportKind::EqualizerApo,
        ExportKind::Convolution,
    ] {
        let Envelope::Err(error) = service.call(
            Request::Export(ExportParams {
                kind,
                folder: root
                    .join(format!("{kind:?}"))
                    .to_string_lossy()
                    .into_owned(),
                mdat_scope: Some(MdatScope::All),
                design_name: None,
                camilla_devices: None,
                extra_rates: Vec::new(),
            }),
            Arc::new(NoEvents),
        ) else {
            panic!("{kind:?} must refuse mdat_scope");
        };
        assert_eq!(error.code, ErrorCode::InvalidRequest);
        assert_eq!(
            error.params.get("reason").map(String::as_str),
            Some("mdat_scope")
        );
    }
    let Envelope::Err(error) = service.call(
        Request::Export(ExportParams {
            kind: ExportKind::BeforeAfterMdat,
            folder: root.join("empty").to_string_lossy().into_owned(),
            mdat_scope: Some(MdatScope::Measurements),
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!("an empty project must refuse a measurement-only export");
    };
    assert_eq!(error.code, ErrorCode::EngineInput);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("no_measurements")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn oracle_project_calibrates_recommends_xo_and_exports() {
    let root = temp_root("oracle-flow");
    fs::create_dir_all(&root).unwrap();
    let (path, settings) = write_case(&root, "dolby");
    let service = deterministic_service();
    open(&service, &path);

    let calibration_events = Arc::new(Events::default());
    let calibration_job = start_calibration(&service, calibration_events.clone(), settings.clone());
    let JobResult::Calibration =
        finished_result(calibration_events.wait(calibration_job, Duration::from_secs(120)))
    else {
        panic!("calibration completion must be returned");
    };
    let results = read_results(&service);
    assert!(!results.calibration.as_ref().unwrap().stale);
    assert!(
        results
            .calibration
            .as_ref()
            .unwrap()
            .notices
            .iter()
            .all(|notice| notice.code != "measurement_resampled")
    );
    assert_eq!(results.calibration.as_ref().unwrap().main_roles.len(), 11);
    let direct = secs_engine_testkit::production_case("dolby", 1);
    assert_eq!(cached_chain(&path), *direct.design.signal_chain());
    let blocked_folder = root.join("blocked-filter-text-subs");
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
        panic!("filter text with subs must fail synchronously");
    };
    assert_eq!(error.code, ErrorCode::ExportScope);
    assert_eq!(error.params.get("reason").map(String::as_str), Some("subs"));
    assert!(!blocked_folder.exists());

    let xo_events = Arc::new(Events::default());
    let Response::RecommendXo(xo_started) =
        call_ok(&service, Request::RecommendXo(None), xo_events.clone())
    else {
        panic!("XO recommendation must start");
    };
    let JobResult::Xo(recommendation) =
        finished_result(xo_events.wait(xo_started.job, Duration::from_secs(120)))
    else {
        panic!("XO result must be returned");
    };
    let view = secs_engine::ProjectView {
        groups: &direct.config.groups,
        project_xo: &settings.xo,
        target: &direct.config.target,
        inverted: &[],
    };
    let ctx = secs_compute::Context::new(
        1,
        secs_compute::MemoryBudget::new(u64::MAX),
        Arc::new(secs_compute::NoProgress),
        secs_compute::CancelToken::new(),
    );
    let direct_xo = secs_engine::xo::recommend_xo(
        &view,
        &direct.measurements,
        &settings,
        &secs_engine::Tuning::default(),
        &secs_engine::xo::XO_CANDIDATES,
        &ctx,
    )
    .unwrap();
    assert_eq!(recommendation.best, direct_xo.best);
    assert_eq!(recommendation.candidates, direct_xo.candidates);
    let Response::CalibrationSettingsGet(applied) = call_ok(
        &service,
        Request::CalibrationSettingsGet,
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    assert_eq!(applied.xo_rec, recommendation.best);
    for (group, hz) in &recommendation.best {
        assert_eq!(
            applied.xo.iter().find(|(name, _)| name == group),
            Some(&(group.clone(), *hz))
        );
    }
    assert_eq!(
        read_results(&service).calibration.unwrap().stale_reason,
        Some("settings".into())
    );

    for kind in [
        ExportKind::CamillaDsp,
        ExportKind::StagedMdat,
        ExportKind::BeforeAfterMdat,
        ExportKind::EqualizerApo,
    ] {
        let folder = root.join(format!("export-{kind:?}"));
        let events = Arc::new(Events::default());
        let Response::Export(started) = call_ok(
            &service,
            Request::Export(ExportParams {
                kind,
                folder: folder.to_string_lossy().into_owned(),
                mdat_scope: None,
                design_name: None,
                camilla_devices: None,
                extra_rates: Vec::new(),
            }),
            events.clone(),
        ) else {
            panic!("export must start");
        };
        let JobResult::Export(export) =
            finished_result(events.wait(started.job, Duration::from_secs(120)))
        else {
            panic!("export result must be returned");
        };
        assert!(!export.files.is_empty());
        assert!(export.files.iter().all(|file| Path::new(file).exists()));
        match kind {
            ExportKind::CamillaDsp => {
                let snapshot = snapshot_relative_files(&folder, &export.files);
                let labels = secs_camilla::RunLabels {
                    mode: export_mode(&settings),
                    note: secs_camilla::note::camilla_note(
                        settings.mode,
                        direct.design.latency_ms(),
                        direct.design.rc_hi,
                        None,
                    ),
                    latency_ms: direct.design.latency_ms(),
                };
                let project_name = {
                    let store = secs_project::MeasurementStore::open(&path).unwrap();
                    (!store.project().name.trim().is_empty())
                        .then(|| store.project().name.trim().to_owned())
                };
                let design_id = secs_graph::design::design_id(
                    direct.design.signal_chain(),
                    None,
                    secs_graph::LfeChoice::DOLBY,
                )
                .unwrap();
                let direct_files = secs_camilla::export_with_options(
                    direct.design.signal_chain(),
                    &folder,
                    &labels,
                    None,
                    secs_camilla::Options {
                        design: Some(secs_camilla::DesignLabels {
                            project_name,
                            design_name: secs_types::calibration::mode_label(
                                settings.mode,
                                Some(settings.ultra_ms),
                            ),
                            design_id,
                            bypass: false,
                        }),
                        headroom: true,
                        labels: true,
                        bypass: true,
                        ..secs_camilla::Options::default()
                    },
                )
                .unwrap();
                assert_eq!(export.files.len(), direct_files.len() + 1);
                compare_snapshot(&folder, &snapshot, &direct_files);
            }
            ExportKind::StagedMdat | ExportKind::BeforeAfterMdat => {
                let direct_folder = root.join(format!("direct-{kind:?}"));
                let run = secs_export::ExportRun {
                    design: &direct.design,
                    settings: &settings,
                };
                let mut uuid = || Ok(0x1234_5678_9abc_def0);
                let direct_file = match kind {
                    ExportKind::StagedMdat => secs_export::export_mdat_with_mimo_using(
                        &run,
                        &direct_folder,
                        None,
                        None,
                        FIXED_MS,
                        &mut uuid,
                    ),
                    ExportKind::BeforeAfterMdat => {
                        secs_export::export_before_after_mdat_with_mimo_using(
                            &run,
                            &direct_folder,
                            None,
                            None,
                            None,
                            FIXED_MS,
                            &mut uuid,
                        )
                    }
                    _ => unreachable!(),
                }
                .unwrap();
                assert_eq!(
                    fs::read(&export.files[0]).unwrap(),
                    fs::read(direct_file).unwrap()
                );
            }
            ExportKind::EqualizerApo => {
                let snapshot = snapshot_relative_files(&folder, &export.files);
                let direct_folder = folder.clone();
                let export_run = secs_export::ExportRun {
                    design: &direct.design,
                    settings: &settings,
                };
                let rc_pieces = secs_export::rc_pieces(&export_run);
                let label = settings.label();
                let filter_run = secs_filters::FilterRun {
                    chain: direct.design.signal_chain(),
                    mimo: None,
                    mode: &export_mode(&settings),
                    note: &secs_camilla::note::eqapo_note(
                        &export_mode(&settings),
                        direct.design.latency_ms(),
                        direct.design.rc_hi,
                        None,
                    ),
                    label: &label,
                    latency_ms: direct.design.latency_ms(),
                    rc_pieces: &rc_pieces,
                    stamp: "2024-01-02 03:04",
                };
                let direct_files = secs_filters::export_eqapo(&filter_run, &direct_folder).unwrap();
                let mut direct_snapshot: Vec<_> = direct_files
                    .iter()
                    .map(|file| {
                        (
                            file.strip_prefix(&direct_folder).unwrap().to_owned(),
                            fs::read(file).unwrap(),
                        )
                    })
                    .collect();
                direct_snapshot.sort_by(|left, right| left.0.cmp(&right.0));
                let snapshot: Vec<_> = snapshot
                    .into_iter()
                    .filter(|(path, _)| path != Path::new("secs-design.json"))
                    .collect();
                assert_eq!(snapshot, direct_snapshot);
            }
            ExportKind::FilterText | ExportKind::Convolution => unreachable!(),
        }
    }
    let status = service.status();
    let last = status.last_export.expect("successful export is remembered");
    assert_eq!(last.kind, ExportKind::EqualizerApo);
    assert_eq!(
        last.folder,
        root.join("export-EqualizerApo").to_string_lossy()
    );
    assert!(!last.mimo);
    assert_eq!(last.mode, "mixed");
    assert_eq!(last.time, "2024-01-02T03:04:05Z");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn measurement_mdat_reads_outside_session_lock_and_cancels_during_read() {
    let root = temp_root("measurement-export-read");
    fs::create_dir_all(&root).unwrap();
    let (source, _) = write_case(&root, "dolby");
    let service = service();
    open(&service, &source);
    let folder = root.join("measurement-export");
    let events = Arc::new(PausedProgress::default());
    let Response::Export(started) = call_ok(
        &service,
        Request::Export(ExportParams {
            kind: ExportKind::BeforeAfterMdat,
            folder: folder.to_string_lossy().into_owned(),
            mdat_scope: Some(MdatScope::Measurements),
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        events.clone(),
    ) else {
        panic!()
    };
    events.wait_until_paused();
    let Response::ProjectInfo(info) = call_ok(&service, Request::ProjectInfo, Arc::new(NoEvents))
    else {
        panic!()
    };
    assert_eq!(info.path, source.to_string_lossy());
    call_ok(
        &service,
        Request::JobCancel(JobCancelParams { job: started.job }),
        Arc::new(NoEvents),
    );
    events.release();
    let finished = events.events.wait(started.job, Duration::from_secs(120));
    assert!(matches!(
        finished,
        JobEvent::Finished {
            status: JobStatus::Cancelled,
            ..
        }
    ));
    assert!(!folder.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn measurement_mdat_rejects_all_originals_that_exceed_one_budget_td130() {
    let root = temp_root("measurement-export-budget");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("large.secs");
    let mut store = MeasurementStore::new(&path, Project::from_preset("2.0").unwrap());
    for (role, position) in [("L", 1), ("R", 2)] {
        store
            .record(
                MeasurementResult {
                    role: role.into(),
                    pos: position,
                    ir: vec![0.0; 786_432],
                    sample_rate: 48_000,
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

    let service = Service::new(Deps {
        memory: Arc::new(secs_compute::FixedMemory(64 * 1024 * 1024)),
        ..Deps::default()
    });
    open(&service, &path);
    let events = Arc::new(Events::default());
    let Response::Export(started) = call_ok(
        &service,
        Request::Export(ExportParams {
            kind: ExportKind::BeforeAfterMdat,
            folder: root.join("export").to_string_lossy().into_owned(),
            mdat_scope: Some(MdatScope::Measurements),
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        }),
        events.clone(),
    ) else {
        panic!()
    };
    let first = events.try_wait(started.job, Duration::from_secs(1));
    if first.is_none() {
        call_ok(
            &service,
            Request::JobCancel(JobCancelParams { job: started.job }),
            Arc::new(NoEvents),
        );
        let _ = events.wait(started.job, Duration::from_secs(10));
    }
    assert!(matches!(
        first,
        Some(JobEvent::Finished {
            status: JobStatus::Failed,
            error: Some(ref error),
            ..
        }) if error.params.get("reason").map(String::as_str) == Some("memory_budget")
    ));
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn mdat_import_reads_the_file_once_before_taking_the_session_lock() {
    let root = temp_root("import-once");
    fs::create_dir_all(&root).unwrap();
    let service = Arc::new(service());
    call_ok(
        &service,
        Request::ProjectNew(ProjectNewParams {
            path: root.join("room.secs").to_string_lossy().into_owned(),
            preset: "7.1.4".into(),
            sub_count: 1,
            sample: false,
            tutorial: false,
        }),
        Arc::new(NoEvents),
    );
    let golden = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../oracle/goldens/mdat/build_3_real_48000_bytes.bin");
    let Response::MdatScan(scan) = call_ok(
        &service,
        Request::MdatScan(secs_types::ipc::MdatScanParams {
            path: golden.to_string_lossy().into_owned(),
        }),
        Arc::new(NoEvents),
    ) else {
        panic!()
    };
    let fifo = Fifo::create(&root.join("room.mdat"), fs::read(&golden).unwrap());
    let events = Arc::new(Events::default());
    let Response::MdatImport(started) = call_ok(
        &service,
        Request::MdatImport(secs_types::ipc::MdatImportParams {
            path: fifo.path.to_string_lossy().into_owned(),
            assignments: scan.suggestions,
            position: 1,
        }),
        events.clone(),
    ) else {
        panic!()
    };
    assert!(
        matches!(
            call_within(&service, Request::ProjectInfo, Duration::from_secs(5)),
            Some(Envelope::Ok(_))
        ),
        "project_info waited for the mdat parse"
    );
    let writer = fifo.release();
    let Some(finished) = events.try_wait(started.job, Duration::from_secs(30)) else {
        fifo.release();
        panic!("mdat import read the file a second time");
    };
    finished_result(finished);
    writer.join().unwrap();
    let Response::ProjectInfo(info) = call_ok(&service, Request::ProjectInfo, Arc::new(NoEvents))
    else {
        panic!()
    };
    assert_eq!(info.positions, [1]);
    fs::remove_dir_all(root).unwrap();
}
