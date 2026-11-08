use std::collections::BTreeSet;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use secs_engine::Samples;
use secs_project::{Band, MeasurementResult, MeasurementStore, Project, Speaker, SpeakerConfig};
use secs_types::calibration::CalibrationSettings;
use secs_types::ipc::{
    CalibrateParams, CamillaDevices, Envelope, ErrorCode, ExportKind, ExportParams, JobEvent,
    JobResult, JobStatus, MdatScope, ProjectNewParams, ProjectPathParams, Request, Response,
};

use crate::export_plan::ExportPurpose;
use crate::{Deps, EventSink, NoEvents, Service, lock};

fn root(name: &str) -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "secs-export-plan-{}-{name}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn write_case(path: &std::path::Path, name: &str) -> (std::path::PathBuf, CalibrationSettings) {
    let case = secs_engine_testkit::replay_case(name);
    let settings: CalibrationSettings = serde_json::from_value(case.settings).unwrap();
    let roles = case
        .config
        .mains
        .iter()
        .chain(&case.config.subs)
        .cloned()
        .collect::<Vec<_>>();
    let role_set = roles.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let speakers = roles
        .iter()
        .enumerate()
        .map(|(index, role)| Speaker {
            role: role.clone(),
            size: if case.config.subs.contains(role) {
                "sub"
            } else {
                "small"
            }
            .into(),
            bands: vec![Band {
                band: String::new(),
                out: index as i64 + 1,
                pol: "+".into(),
            }],
        })
        .collect();
    let mut project = Project {
        name: name.into(),
        config: SpeakerConfig {
            speakers,
            device: String::new(),
            preset: "custom".into(),
            biamp_default: false,
        },
        target: case.config.target.clone(),
        xo: case.config.xo.clone(),
        ..Project::default()
    };
    secs_project::store_calibration(&mut project, &settings);
    let mut store = MeasurementStore::new(path, project);
    for (position, measurements) in case.measurements.positions {
        store.start_position(i64::from(position));
        for (role, measurement) in measurements {
            if !role_set.contains(role.as_str()) {
                continue;
            }
            let samples = match measurement.samples {
                Samples::F32(samples) => samples.to_vec(),
                Samples::F64(samples) => samples.iter().map(|&value| value as f32).collect(),
            };
            store
                .record(
                    MeasurementResult {
                        role: role.clone(),
                        pos: i64::from(position),
                        ir: samples,
                        sample_rate: measurement.sample_rate,
                        t0_s: measurement.t0,
                        spl_db: None,
                        spl_offset_db: None,
                        snr_db: None,
                        uuid: Some(format!("{name}-{position}-{role}")),
                        source: Some("oracle".into()),
                        source_info: None,
                    },
                    Some(&roles),
                )
                .unwrap();
        }
    }
    store.save().unwrap();
    (path.to_owned(), settings)
}

fn project(service: &Service, path: &std::path::Path, tutorial: bool) {
    let response = service.call(
        Request::ProjectNew(ProjectNewParams {
            path: if tutorial {
                String::new()
            } else {
                path.to_string_lossy().into_owned()
            },
            preset: "2.0".into(),
            sub_count: 0,
            sample: tutorial,
            tutorial,
        }),
        Arc::new(NoEvents),
    );
    assert!(matches!(response, Envelope::Ok(_)));
}

fn params(kind: ExportKind, folder: &std::path::Path) -> ExportParams {
    ExportParams {
        kind,
        folder: folder.to_string_lossy().into_owned(),
        mdat_scope: None,
        design_name: None,
        camilla_devices: None,
        extra_rates: Vec::new(),
    }
}

fn reason(error: secs_types::ipc::IpcError) -> String {
    assert!(matches!(
        error.code,
        ErrorCode::InvalidRequest | ErrorCode::TutorialBlocked
    ));
    error.params.get("reason").cloned().unwrap_or_default()
}

#[derive(Debug, Default)]
struct WriterEvents {
    values: Mutex<Vec<JobEvent>>,
    changed: Condvar,
}

impl EventSink for WriterEvents {
    fn send(&self, event: JobEvent) {
        lock(&self.values).push(event);
        self.changed.notify_all();
    }
}

impl WriterEvents {
    fn wait_finished(&self) {
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut values = lock(&self.values);
        loop {
            if let Some(JobEvent::Finished { status, error, .. }) = values
                .iter()
                .find(|event| matches!(event, JobEvent::Finished { .. }))
            {
                assert_eq!(*status, JobStatus::Succeeded, "{error:?}");
                return;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "writer job did not finish");
            values = self.changed.wait_timeout(values, remaining).unwrap().0;
        }
    }
}

fn relative(folder: &std::path::Path, path: &std::path::Path) -> String {
    path.strip_prefix(folder)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn assert_writer_contract(
    kind: ExportKind,
    expected_files: &[&str],
    expected_main_config: Option<&str>,
    has_design_id: bool,
) {
    let root = root(&format!("writer-{kind:?}"));
    let (project_path, settings) = write_case(&root.join("room.secs"), "room");
    if kind == ExportKind::FilterText {
        let mut store = secs_project::MeasurementStore::open(&project_path).unwrap();
        store
            .project_mut()
            .config
            .speakers
            .retain(|speaker| speaker.size != "sub");
        store.project_mut().config.renumber();
        store.save().unwrap();
    }
    let service = Service::new(Deps {
        uuid: Arc::new(Mutex::new(Box::new(|| Ok(0x1234_5678_9abc_def0)))),
        ..Deps::default()
    });
    assert!(matches!(
        service.call(
            Request::ProjectOpen(ProjectPathParams {
                path: project_path.to_string_lossy().into_owned(),
            }),
            Arc::new(NoEvents),
        ),
        Envelope::Ok(response) if matches!(*response, Response::ProjectOpen(_))
    ));

    let calibration_events = Arc::new(WriterEvents::default());
    let calibration = service.call(
        Request::Calibrate(CalibrateParams {
            settings: Some(settings),
            ..Default::default()
        }),
        calibration_events.clone(),
    );
    assert!(matches!(
        calibration,
        Envelope::Ok(response) if matches!(*response, Response::Calibrate(_))
    ));
    calibration_events.wait_finished();

    let folder = root.join("output");
    let plan = service
        .plan_export(params(kind, &folder), ExportPurpose::Export)
        .unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    let writer_events = Arc::new(WriterEvents::default());
    service
        .start_export(plan, writer_events.clone(), move |result| {
            send.send(result).unwrap();
            Ok(JobResult::Stopped)
        })
        .unwrap();
    let result = receive
        .recv_timeout(Duration::from_secs(120))
        .expect("writer did not return its result");
    writer_events.wait_finished();

    assert_eq!(std::path::Path::new(&result.folder), folder);
    assert!(
        result
            .files
            .iter()
            .all(|file| std::path::Path::new(file).is_file())
    );
    let mut files = result
        .files
        .iter()
        .map(|file| relative(&folder, std::path::Path::new(file)))
        .collect::<Vec<_>>();
    files.sort();
    assert_eq!(files, expected_files);
    assert_eq!(
        result
            .main_config
            .as_deref()
            .map(|path| relative(&folder, path)),
        expected_main_config.map(str::to_owned)
    );
    assert_eq!(result.design_id.is_some(), has_design_id);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn plan_export_rejects_the_four_invalid_requests_with_reasons() {
    let root = root("invalid");
    let service = Service::new(Deps::default());
    project(&service, &root.join("room.secs"), false);

    let mut request = params(ExportKind::FilterText, std::path::Path::new("relative"));
    assert_eq!(
        reason(
            service
                .plan_export(request.clone(), ExportPurpose::Export)
                .unwrap_err()
        ),
        "relative_folder"
    );

    request = params(ExportKind::StagedMdat, &root.join("mdat"));
    request.design_name = Some("named".into());
    assert_eq!(
        reason(
            service
                .plan_export(request, ExportPurpose::Export)
                .unwrap_err()
        ),
        "design_name"
    );

    request = params(ExportKind::FilterText, &root.join("text"));
    request.camilla_devices = Some(CamillaDevices::default());
    assert_eq!(
        reason(
            service
                .plan_export(request, ExportPurpose::Export)
                .unwrap_err()
        ),
        "camilla_devices"
    );

    let tutorial = Service::new(Deps {
        app_data_path: root.join("app-data"),
        ..Deps::default()
    });
    project(&tutorial, std::path::Path::new(""), true);
    let error = tutorial
        .plan_export(
            params(ExportKind::FilterText, &root.join("tutorial")),
            ExportPurpose::Export,
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::TutorialBlocked);
    assert_eq!(
        error.params.get("reason").map(String::as_str),
        Some("tutorial")
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn plan_export_marks_apply_plans_without_index_or_record() {
    let root = root("apply");
    let service = Service::new(Deps::default());
    project(&service, &root.join("room.secs"), false);
    {
        let mut session = lock(&service.session);
        let open = session.require_mut().unwrap();
        open.store_mut()
            .record(
                MeasurementResult {
                    role: "L".into(),
                    pos: 1,
                    ir: vec![1.0, 0.0],
                    sample_rate: 48_000,
                    t0_s: 0.0,
                    spl_db: None,
                    spl_offset_db: None,
                    snr_db: None,
                    uuid: Some("plan-test".into()),
                    source: None,
                    source_info: None,
                },
                Some(&["L".into(), "R".into()]),
            )
            .unwrap();
        open.mark_saved();
    }
    let mut request = params(ExportKind::BeforeAfterMdat, &root.join("mdat"));
    request.mdat_scope = Some(MdatScope::Measurements);
    let plan = service.plan_export(request, ExportPurpose::Apply).unwrap();
    assert!(plan.record.is_none());
    assert!(plan.destination.index_root.is_none());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn camilla_writer_stores_the_same_files_main_config_and_design_id() {
    assert_writer_contract(
        ExportKind::CamillaDsp,
        &[
            "coeffs/secs_mixed_C_fir.wav",
            "coeffs/secs_mixed_L_fir.wav",
            "coeffs/secs_mixed_R_fir.wav",
            "configs/secs_mixed.yml",
            "configs/secs_mixed_bypass.yml",
            "secs-design.json",
        ],
        Some("configs/secs_mixed.yml"),
        true,
    );
}

#[test]
fn convolution_writer_stores_the_same_files_main_config_and_design_id() {
    assert_writer_contract(
        ExportKind::Convolution,
        &[
            "C_to_C.csv",
            "C_to_C.wav",
            "C_to_SW1.csv",
            "C_to_SW1.wav",
            "C_to_SW2.csv",
            "C_to_SW2.wav",
            "LFE_to_SW1.csv",
            "LFE_to_SW1.wav",
            "LFE_to_SW2.csv",
            "LFE_to_SW2.wav",
            "L_to_L.csv",
            "L_to_L.wav",
            "L_to_SW1.csv",
            "L_to_SW1.wav",
            "L_to_SW2.csv",
            "L_to_SW2.wav",
            "R_to_R.csv",
            "R_to_R.wav",
            "R_to_SW1.csv",
            "R_to_SW1.wav",
            "R_to_SW2.csv",
            "R_to_SW2.wav",
            "routing.json",
            "secs-design.json",
            "secs2r_reaper.lua",
            "컨볼루션 적용 방법.txt",
        ],
        None,
        true,
    );
}

#[test]
fn eqapo_writer_stores_the_same_files_main_config_and_design_id() {
    assert_writer_contract(
        ExportKind::EqualizerApo,
        &[
            "EqAPO 적용 방법.txt",
            "config.txt",
            "secs-design.json",
            "secs_mixed/secs_mixed.txt",
            "secs_mixed/secs_mixed_C_fir.wav",
            "secs_mixed/secs_mixed_L_fir.wav",
            "secs_mixed/secs_mixed_R_fir.wav",
        ],
        None,
        true,
    );
}

#[test]
fn filter_text_writer_stores_the_same_files_main_config_and_design_id() {
    assert_writer_contract(
        ExportKind::FilterText,
        &[
            "SECS 필터 저지연 모드.txt",
            "secs-design.json",
            "secs_mixed_C_fir.wav",
            "secs_mixed_L_fir.wav",
            "secs_mixed_R_fir.wav",
        ],
        None,
        true,
    );
}

#[test]
fn before_after_mdat_writer_stores_the_same_files_main_config_and_design_id() {
    assert_writer_contract(
        ExportKind::BeforeAfterMdat,
        &["SECS 원 측정+보정 결과 저지연 모드.mdat"],
        None,
        false,
    );
}

#[test]
fn staged_mdat_writer_stores_the_same_files_main_config_and_design_id() {
    assert_writer_contract(
        ExportKind::StagedMdat,
        &["SECS 보정 설계 저지연 모드.mdat"],
        None,
        false,
    );
}
