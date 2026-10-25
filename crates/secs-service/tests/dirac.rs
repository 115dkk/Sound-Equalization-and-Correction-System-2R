#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use secs_dirac::test_support::{SyntheticChannel, SyntheticProject, write};
use secs_service::dirac_import::{self, base_role};
use secs_service::mdat_import::Assignment;
use secs_service::{Deps, EventSink, NoEvents, Service};
use secs_types::ipc::*;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/dirac-service-{}-{label}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Debug, Default)]
struct Events {
    values: Mutex<Vec<JobEvent>>,
    changed: Condvar,
}

impl EventSink for Events {
    fn send(&self, event: JobEvent) {
        self.values.lock().unwrap().push(event);
        self.changed.notify_all();
    }
}

impl Events {
    fn wait(&self, job: JobId) -> JobEvent {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut values = self.values.lock().unwrap();
        loop {
            if let Some(event) = values
                .iter()
                .find(|event| matches!(event, JobEvent::Finished { job: id, .. } if *id == job))
            {
                return event.clone();
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "job timeout: {values:?}");
            values = self.changed.wait_timeout(values, remaining).unwrap().0;
        }
    }
}

fn call(service: &Service, request: Request) -> Response {
    match service.call(request, Arc::new(NoEvents)) {
        Envelope::Ok(response) => *response,
        other => panic!("{other:?}"),
    }
}

fn new_project(service: &Service, path: &Path, preset: &str, sub_count: u8) {
    call(
        service,
        Request::ProjectNew(ProjectNewParams {
            path: path.to_string_lossy().into_owned(),
            preset: preset.into(),
            sub_count,
            sample: false,
            tutorial: false,
        }),
    );
}

fn scan(service: &Service, path: &Path) -> MdatScanResult {
    let Response::MdatScan(scan) = call(
        service,
        Request::MdatScan(MdatScanParams {
            path: path.to_string_lossy().into_owned(),
        }),
    ) else {
        panic!()
    };
    scan
}

fn main_response() -> Vec<f64> {
    let mut response = vec![0.0; 32769];
    for index in 0..128 {
        response[12000 + index] = (-(index as f64) / 18.0).exp() * ((index as f64) * 1.731).sin();
    }
    response[12000] = 1.0;
    response
}

fn sub_response() -> Vec<f64> {
    let mut response = vec![0.0; 32769];
    for (index, sample) in response.iter_mut().enumerate().skip(12000) {
        let elapsed = (index - 12000) as f64;
        *sample =
            (std::f64::consts::TAU * 40.0 * elapsed / 48000.0).sin() * (-elapsed / 2400.0).exp();
    }
    response
}

fn four_channel_project(unmeasured: Option<usize>) -> SyntheticProject {
    let channels = vec![
        SyntheticChannel {
            name: "Left".into(),
            kind: 1,
        },
        SyntheticChannel {
            name: "Right".into(),
            kind: 1,
        },
        SyntheticChannel {
            name: "Subwoofer".into(),
            kind: 4,
        },
        SyntheticChannel {
            name: "Subwoofer".into(),
            kind: 4,
        },
    ];
    let responses = (0..8)
        .map(|index| {
            if Some(index) == unmeasured {
                None
            } else if index % 4 < 2 {
                Some(main_response())
            } else {
                Some(sub_response())
            }
        })
        .collect();
    SyntheticProject {
        format_version: 10,
        name: "Synthetic import".into(),
        microphone: "Synthetic microphone".into(),
        sample_rate: 48000,
        channels,
        positions: 2,
        responses,
    }
}

#[test]
fn dirac_channel_names_suggest_roles() {
    for (name, expected) in [
        ("Left", Some("L")),
        ("Right", Some("R")),
        ("Center", Some("C")),
        ("Centre", Some("C")),
        ("LeftFront", Some("L")),
        ("LeftSurround", Some("Ls")),
        ("RightSide", Some("Rs")),
        ("LeftSurroundBack", Some("Lb")),
        ("RightBack", Some("Rb")),
        ("LeftRear", Some("Lb")),
        ("LeftWide", Some("Lw")),
        ("LeftTopFront", Some("Tfl")),
        ("TopFrontRight", Some("Tfr")),
        ("LeftTopMiddle", Some("Tml")),
        ("LeftTopRear", Some("Tbl")),
        ("LeftHeight", Some("Hfl")),
        ("LeftFrontHeight", Some("Hfl")),
        ("RightRearHeight", Some("Hbr")),
        ("Left Surround", Some("Ls")),
        ("Top Left", None),
        ("LeftRight", None),
        ("Unknown", None),
        ("", None),
    ] {
        assert_eq!(base_role(name), expected, "{name:?}");
    }
}

#[test]
fn dirac_scan_lists_positions_channels_and_sub_flags() {
    let directory = TempDir::new("scan");
    let source = directory.0.join("scan.LIVEPROJECT");
    fs::write(&source, write(&four_channel_project(Some(5)))).unwrap();

    let result = scan(&Service::new(Deps::default()), &source);
    assert_eq!(result.measurements.len(), 7);
    assert_eq!(
        result
            .measurements
            .iter()
            .map(|row| row.title.as_str())
            .collect::<Vec<_>>(),
        [
            "Left P1",
            "Right P1",
            "Subwoofer 1 P1",
            "Subwoofer 2 P1",
            "Left P2",
            "Subwoofer 1 P2",
            "Subwoofer 2 P2",
        ]
    );
    assert_eq!(
        result
            .measurements
            .iter()
            .map(|row| row.position)
            .collect::<Vec<_>>(),
        [1, 1, 1, 1, 2, 2, 2]
    );
    assert!(result.measurements.iter().all(|row| row.position_tagged));
    assert_eq!(
        result
            .measurements
            .iter()
            .map(|row| row.role.as_deref())
            .collect::<Vec<_>>(),
        [
            Some("L"),
            Some("R"),
            Some("SW1"),
            Some("SW2"),
            Some("L"),
            Some("SW1"),
            Some("SW2"),
        ]
    );
    assert_eq!(
        result
            .measurements
            .iter()
            .map(|row| row.looks_like_sub)
            .collect::<Vec<_>>(),
        [
            Some(false),
            Some(false),
            Some(true),
            Some(true),
            Some(false),
            Some(true),
            Some(true),
        ]
    );
    assert_eq!(result.suggestions.len(), 7);
    assert!(result.problems.is_empty());
}

#[test]
fn dirac_import_records_irs_t0_and_source() {
    let directory = TempDir::new("import");
    let source = directory.0.join("room.liveproject");
    let project_source = four_channel_project(None);
    let encoded = write(&project_source);
    fs::write(&source, &encoded).unwrap();
    let parsed = secs_dirac::parse(&encoded, &secs_dirac::Limits::default()).unwrap();

    let project_path = directory.0.join("room.secs");
    let service = Service::new(Deps::default());
    new_project(&service, &project_path, "2.2", 2);
    let scanned = scan(&service, &source);
    assert_eq!(scanned.suggestions.len(), 8);

    let events = Arc::new(Events::default());
    let response = service.call(
        Request::MdatImport(MdatImportParams {
            path: source.to_string_lossy().into_owned(),
            assignments: scanned.suggestions,
            position: 1,
        }),
        events.clone(),
    );
    let Envelope::Ok(response) = response else {
        panic!("{response:?}")
    };
    let Response::MdatImport(started) = *response else {
        panic!()
    };
    assert!(matches!(
        events.wait(started.job),
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            result: Some(result),
            ..
        } if matches!(*result, JobResult::MdatImport(_))
    ));

    let mut store = secs_project::MeasurementStore::open(&project_path).unwrap();
    let roles = ["L", "R", "SW1", "SW2"];
    for measured in parsed.measured() {
        let position = measured.position as i64 + 1;
        let role = roles[measured.channel];
        let (actual, sample_rate) = store.ir_of(role, position).unwrap();
        assert_eq!(
            actual,
            measured
                .samples
                .iter()
                .map(|sample| *sample as f32)
                .collect::<Vec<_>>()
        );
        assert_eq!(sample_rate, parsed.sample_rate);
        assert_eq!(store.t0_of(role, position), -0.25);
        assert_eq!(
            store.entry_for(position).unwrap().source[role],
            "room.liveproject"
        );
        let wav = project_path
            .with_extension("irs")
            .join(format!("{role}_P{position}.wav"));
        assert!(!secs_project::source_path(&wav).exists());
    }
}

fn import_dirac(service: &Service, source: &Path) {
    let scanned = scan(service, source);
    let events = Arc::new(Events::default());
    let Envelope::Ok(response) = service.call(
        Request::MdatImport(MdatImportParams {
            path: source.to_string_lossy().into_owned(),
            assignments: scanned.suggestions,
            position: 1,
        }),
        events.clone(),
    ) else {
        panic!("Dirac import must start")
    };
    let Response::MdatImport(started) = *response else {
        panic!()
    };
    assert!(matches!(
        events.wait(started.job),
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            ..
        }
    ));
}

fn calibrate(service: &Service) {
    let events = Arc::new(Events::default());
    let Envelope::Ok(response) =
        service.call(Request::Calibrate(Default::default()), events.clone())
    else {
        panic!("calibration must start")
    };
    let Response::Calibrate(started) = *response else {
        panic!()
    };
    assert!(matches!(
        events.wait(started.job),
        JobEvent::Finished {
            status: JobStatus::Succeeded,
            ..
        }
    ));
}

fn calibration_summary(service: &Service) -> CalibrationSummary {
    let Response::Results(results) = call(service, Request::Results) else {
        panic!()
    };
    results.calibration.unwrap()
}

#[test]
fn dirac_content_identity_marks_changed_ir_stale_but_not_identical_reimport() {
    let directory = TempDir::new("content-identity");
    let source = directory.0.join("room.liveproject");
    let project_path = directory.0.join("room.secs");
    let service = Service::new(Deps::default());
    new_project(&service, &project_path, "2.2", 2);

    let original = four_channel_project(None);
    fs::write(&source, write(&original)).unwrap();
    import_dirac(&service, &source);
    calibrate(&service);
    assert!(!calibration_summary(&service).stale);

    import_dirac(&service, &source);
    assert!(!calibration_summary(&service).stale);

    let mut changed = original;
    changed.responses[0].as_mut().unwrap()[12001] += 0.125;
    fs::write(&source, write(&changed)).unwrap();
    import_dirac(&service, &source);
    let summary = calibration_summary(&service);
    assert!(summary.stale);
    assert_eq!(summary.stale_reason.as_deref(), Some("measurements"));
}

#[test]
fn dirac_import_skips_unplanned_and_outside_rows_and_rejects_bad_files() {
    let directory = TempDir::new("invalid");
    let source = directory.0.join("valid.liveproject");
    fs::write(&source, write(&SyntheticProject::minimal())).unwrap();

    let direct_path = directory.0.join("direct.secs");
    let mut direct = secs_project::MeasurementStore::new(
        &direct_path,
        secs_project::Project::from_preset("2.0").unwrap(),
    );
    let before_direct = direct.project().save_text();
    let assignments = [
        Assignment {
            role: Some("L".into()),
            position: Some(10),
        },
        Assignment {
            role: Some("SW1".into()),
            position: Some(1),
        },
        Assignment {
            role: None,
            position: Some(2),
        },
    ];
    assert_eq!(
        dirac_import::import(&mut direct, &source, &assignments).unwrap(),
        0
    );
    assert_eq!(direct.project().save_text(), before_direct);
    assert!(!direct_path.with_extension("irs").exists());
    assert!(matches!(
        dirac_import::import(&mut direct, &source, &assignments[..2]),
        Err(secs_service::measure::MeasureError::Input(_))
    ));
    assert_eq!(direct.project().save_text(), before_direct);

    let truncated = directory.0.join("truncated.liveproject");
    let valid_bytes = write(&SyntheticProject::minimal());
    fs::write(&truncated, &valid_bytes[..10]).unwrap();
    let empty = directory.0.join("empty.liveproject");
    let mut no_measurements = SyntheticProject::minimal();
    no_measurements.responses.fill(None);
    fs::write(&empty, write(&no_measurements)).unwrap();
    let missing = directory.0.join("missing.liveproject");

    let project_path = directory.0.join("unchanged.secs");
    let service = Service::new(Deps::default());
    new_project(&service, &project_path, "2.0", 0);
    let before = fs::read(&project_path).unwrap();
    for bad in [&truncated, &empty, &missing] {
        let Envelope::Err(error) = service.call(
            Request::MdatScan(MdatScanParams {
                path: bad.to_string_lossy().into_owned(),
            }),
            Arc::new(NoEvents),
        ) else {
            panic!("{bad:?}")
        };
        assert_eq!(error.code, ErrorCode::MeasurementInput, "{bad:?}");
        assert_eq!(error.params.get("field").map(String::as_str), Some("path"));
        assert_eq!(fs::read(&project_path).unwrap(), before);
        assert!(!project_path.with_extension("irs").exists());
    }
}
