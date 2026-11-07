#![forbid(unsafe_code)]
//! Records deterministic service answers for the screen mock (ADR 0014).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::time::Duration;

use secs_service::{Deps, EventSink, LocalTime, NoEvents, Service};
use secs_types::calibration::{Mode, RcBand, RcHiBasis};
use secs_types::chain::BandType;
use secs_types::ipc::*;
use secs_types::settings_edit::{
    BandTypeChoice, SettingEdit, SubBand, SubLayout, SubModeChoice, SubPatch,
};
use serde_json::{Value, json};

static TEMP: AtomicU64 = AtomicU64::new(0);

/// A clock no recorded response may show (`2001-02-03 04:05:06`).
const NOW: LocalTime = LocalTime {
    year: 2001,
    month: 2,
    day: 3,
    hour: 4,
    minute: 5,
    second: 6,
};

/// Every configuration preset `config_apply_preset` accepts.
const CONFIG_PRESETS: [&str; 11] = [
    "2.0", "2.1", "2.2", "4.0", "4.1", "4.2", "5.1", "7.1", "7.1.4", "9.1.6", "custom",
];

fn modes() -> [Mode; 4] {
    let all = [Mode::Zero, Mode::Mixed, Mode::Linear, Mode::Ultra];
    for mode in all {
        match mode {
            Mode::Zero | Mode::Mixed | Mode::Linear | Mode::Ultra => {}
        }
    }
    all
}

#[derive(Debug)]
struct JobEvents(mpsc::Sender<JobEvent>);

impl EventSink for JobEvents {
    fn send(&self, event: JobEvent) {
        let _ = self.0.send(event);
    }
}

/// A service with a freshly created project in its own temporary folder.
struct Fresh {
    service: Service,
    root: PathBuf,
}

impl Fresh {
    fn new() -> Self {
        Self::create("2.0", 0, false)
    }

    fn create(preset: &str, sub_count: u8, sample: bool) -> Self {
        let root = std::env::temp_dir().join(format!(
            "secs-service-recorded-{}-{}",
            std::process::id(),
            TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let service = Service::new(Deps {
            clock: Arc::new(|| NOW),
            unix_ms: Arc::new(|| 981_173_106_000),
            ..Deps::default()
        });
        let fresh = Self { service, root };
        fresh.call(Request::ProjectNew(ProjectNewParams {
            path: fresh
                .root
                .join("recorded.secs")
                .to_string_lossy()
                .into_owned(),
            preset: preset.into(),
            sub_count,
            sample,
            tutorial: false,
        }));
        fresh
    }

    fn call(&self, request: Request) -> Response {
        match self.service.call(request, Arc::new(NoEvents)) {
            Envelope::Ok(response) => *response,
            Envelope::Err(error) => panic!("request failed: {error:?}"),
        }
    }

    fn run_job(&self, request: Request) -> JobResult {
        let (sender, receiver) = mpsc::channel();
        let response = match self.service.call(request, Arc::new(JobEvents(sender))) {
            Envelope::Ok(response) => *response,
            Envelope::Err(error) => panic!("job request failed: {error:?}"),
        };
        let job = match response {
            Response::SampleLoad(started)
            | Response::Calibrate(started)
            | Response::Mimo(started)
            | Response::Export(started)
            | Response::RecommendXo(started)
            | Response::MeasurePosition(started)
            | Response::MdatImport(started)
            | Response::ApplyCamilla(started)
            | Response::ApplyEqapo(started) => started.job,
            response => panic!("request did not start a job: {response:?}"),
        };
        loop {
            match receiver.recv_timeout(Duration::from_secs(120)).unwrap() {
                JobEvent::Finished {
                    job: finished,
                    status: JobStatus::Succeeded,
                    result: Some(result),
                    ..
                } if finished == job => return *result,
                JobEvent::Finished {
                    job: finished,
                    status,
                    error,
                    ..
                } if finished == job => panic!("job failed: {status:?} {error:?}"),
                _ => {}
            }
        }
    }

    fn calibrated(edits: &[SettingEdit]) -> Self {
        let fresh = Self::create("2.0", 0, true);
        fresh.run_job(Request::SampleLoad(SampleLoadParams { positions: Some(9) }));
        for edit in edits {
            fresh.call(Request::CalibrationSettingsEdit(
                CalibrationSettingsEditParams { edit: edit.clone() },
            ));
        }
        fresh.run_job(Request::Calibrate(CalibrateParams::default()));
        fresh
    }

    /// The response's `data`, checked for the temporary folder and the clock.
    fn data(&self, request: Request) -> Value {
        let data = serde_json::to_value(self.call(request)).unwrap()["data"].clone();
        self.check_portable(&data);
        data
    }

    fn check_portable(&self, data: &Value) {
        let text = data.to_string();
        let root = serde_json::to_string(&self.root.to_string_lossy()).unwrap();
        assert!(
            !text.contains(root.trim_matches('"')),
            "a recorded response carries a machine path: {text}"
        );
        for stamp in [NOW.date(), NOW.stamp(), "981173106".to_owned()] {
            assert!(
                !text.contains(&stamp),
                "a recorded response carries the time: {text}"
            );
        }
    }

    /// One recorded entry: the request's params and the response's data.
    fn entry(&self, request: Request) -> Value {
        let params = serde_json::to_value(&request).unwrap()["params"].clone();
        json!({ "params": params, "data": self.data(request) })
    }

    fn envelope_entry(&self, request: Request) -> Value {
        let params = serde_json::to_value(&request).unwrap()["params"].clone();
        let data = serde_json::to_value(self.service.call(request, Arc::new(NoEvents))).unwrap();
        self.check_portable(&data);
        json!({ "params": params, "data": data })
    }
}

impl Drop for Fresh {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn recorded_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/secs2r/ui/src/ipc/recorded")
}

/// Compares (or with `SECS_RECORD=1` writes) one recorded file.
fn check(name: &str, entries: Vec<Value>) -> Option<String> {
    let path = recorded_dir().join(format!("{name}.json"));
    let actual = Value::Array(entries);
    if std::env::var("SECS_RECORD").as_deref() == Ok("1") {
        fs::create_dir_all(recorded_dir()).unwrap();
        let text = serde_json::to_string_pretty(&actual).unwrap() + "\n";
        fs::write(&path, text).unwrap();
        return None;
    }
    let expected = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    (expected.as_ref() != Some(&actual)).then(|| {
        format!(
            "{} differs from what the service answers; run `SECS_RECORD=1 cargo test -p secs-service --test recorded` and commit the result",
            path.display()
        )
    })
}

fn mode_spec() -> Vec<Value> {
    let fresh = Fresh::new();
    let Value::Number(ultra_ms) = fresh.data(Request::CalibrationSettingsGet)["ultra_ms"].clone()
    else {
        panic!("calibration settings carry ultra_ms")
    };
    let ultra_ms = ultra_ms.as_f64();
    let mut entries = Vec::new();
    for mode in modes() {
        let request = |mimo_fir| {
            Request::ModeSpec(ModeSpecParams {
                mode,
                ultra_ms,
                mimo_fir: Some(mimo_fir),
            })
        };
        let iir = fresh.entry(request(false));
        let fir = fresh.entry(request(true));
        let differs = iir["data"] != fir["data"];
        entries.push(iir);
        if differs {
            entries.push(fir);
        }
    }
    entries
}

fn config_apply_preset() -> Vec<Value> {
    CONFIG_PRESETS
        .into_iter()
        .map(|preset| {
            Fresh::new().entry(Request::ConfigApplyPreset(ConfigPresetParams {
                preset: preset.into(),
                confirm: false,
            }))
        })
        .collect()
}

fn target_apply_preset() -> Vec<Value> {
    let Response::TargetGet(target) = Fresh::new().call(Request::TargetGet) else {
        panic!("target_get answers a target")
    };
    target
        .presets
        .into_iter()
        .map(|(preset, _)| {
            Fresh::new().entry(Request::TargetApplyPreset(TargetPresetParams { preset }))
        })
        .collect()
}

/// The recorded entries of one method.
type Entries = fn() -> Vec<Value>;

fn calibration_settings_get() -> Vec<Value> {
    vec![Fresh::new().entry(Request::CalibrationSettingsGet)]
}

fn valid_edits() -> Vec<SettingEdit> {
    vec![
        SettingEdit::Mode { mode: Mode::Linear },
        SettingEdit::SampleRatePolicy {
            policy: secs_types::calibration::SampleRatePolicy::Resample,
        },
        SettingEdit::UltraMs { ms: 120.0 },
        SettingEdit::LowProtect { on: true },
        SettingEdit::LfAlign { on: true },
        SettingEdit::ModeDecay { on: true },
        SettingEdit::HfPhase { on: true },
        SettingEdit::DipLimit { on: true },
        SettingEdit::InputHp { hz: Some(25.0) },
        SettingEdit::RcLo { hz: 30.0 },
        SettingEdit::RcHi { hz: Some(500.0) },
        SettingEdit::RcHiBasis {
            basis: RcHiBasis::Lenient,
        },
        SettingEdit::RcHiTransition { factor: 2 },
        SettingEdit::RcBands {
            bands: vec![RcBand {
                lo_hz: 1_000.0,
                hi_hz: 2_000.0,
            }],
        },
        SettingEdit::RoomVolume { m3: 60.0 },
        SettingEdit::RoomDims {
            dims: [5.0, 4.0, 3.0],
        },
        SettingEdit::BoostMax { db: 6.0 },
        SettingEdit::Xo {
            group: "프론트".into(),
            hz: 80.0,
        },
        SettingEdit::XoRecInclude { on: false },
        SettingEdit::XoPeq {
            on: Some(true),
            lo: Some(40.0),
            hi: Some(160.0),
        },
        SettingEdit::Infra {
            role: Some("SW1".into()),
            hz: 25.0,
            typ: BandType::Lr24,
            subs: vec!["SW1".into(), "SW2".into(), "SW3".into()],
        },
        SettingEdit::InfraBoost { db: 12.0 },
        SettingEdit::SubMode {
            role: "SW1".into(),
            mode: SubModeChoice::Fixed,
        },
        SettingEdit::SubLayout {
            bundle: "A".into(),
            layout: SubLayout::Values,
        },
        SettingEdit::RearTrim {
            bundle: "A".into(),
            ms: 2.0,
        },
        SettingEdit::Sub {
            role: "SW1".into(),
            patch: SubPatch {
                gain_db: Some(-3.0),
                ..Default::default()
            },
        },
        SettingEdit::SubBandType {
            role: "SW1".into(),
            band: SubBand::Hp,
            typ: BandTypeChoice::Band(BandType::Lr24),
        },
        SettingEdit::MainDelay { ms: 4.0 },
        SettingEdit::MimoOn { on: true },
        SettingEdit::Fsiso { hz: 120.0 },
        SettingEdit::MimoLow { hz: Some(30.0) },
        SettingEdit::MimoMax { count: 2.0 },
        SettingEdit::MimoFir { on: false },
        SettingEdit::MimoEarly { ms: 30.0 },
        SettingEdit::MimoSeatGuard { on: true },
        SettingEdit::AnalyticDerivatives {
            peq: Some(true),
            mimo_iir: Some(true),
        },
    ]
}

fn invalid_edits() -> Vec<SettingEdit> {
    vec![
        SettingEdit::UltraMs { ms: 1_000.0 },
        SettingEdit::InputHp { hz: Some(1.0) },
        SettingEdit::RcLo { hz: 1.0 },
        SettingEdit::RcHi { hz: Some(50_000.0) },
        SettingEdit::RcHiTransition { factor: 3 },
        SettingEdit::RcBands {
            bands: vec![RcBand {
                lo_hz: 2_000.0,
                hi_hz: 1_000.0,
            }],
        },
        SettingEdit::RoomVolume { m3: 1_000_000.0 },
        SettingEdit::RoomDims {
            dims: [200.0, 200.0, 200.0],
        },
        SettingEdit::BoostMax { db: 100.0 },
        SettingEdit::Xo {
            group: "프론트".into(),
            hz: 1_000.0,
        },
        SettingEdit::XoPeq {
            on: None,
            lo: Some(-100.0),
            hi: Some(2_000.0),
        },
        SettingEdit::Infra {
            role: Some("SW1".into()),
            hz: 1_000.0,
            typ: BandType::Lr24,
            subs: vec!["SW1".into(), "SW2".into(), "SW3".into()],
        },
        SettingEdit::InfraBoost { db: 100.0 },
        SettingEdit::RearTrim {
            bundle: "A".into(),
            ms: 100.0,
        },
        SettingEdit::Sub {
            role: "SW1".into(),
            patch: SubPatch {
                gain_db: Some(1_000.0),
                delay_ms: Some(1_000.0),
                hp_hz: Some(1_000.0),
                lp_hz: Some(1_000.0),
                ..Default::default()
            },
        },
        SettingEdit::MainDelay { ms: 1_000.0 },
        SettingEdit::Fsiso { hz: 1_000.0 },
        SettingEdit::MimoLow { hz: Some(1_000.0) },
        SettingEdit::MimoMax { count: 100.0 },
        SettingEdit::MimoEarly { ms: 1_000.0 },
    ]
}

fn calibration_settings_edit() -> Vec<Value> {
    valid_edits()
        .into_iter()
        .chain(invalid_edits())
        .map(|edit| {
            Fresh::create("7.1.4", 3, true).envelope_entry(Request::CalibrationSettingsEdit(
                CalibrationSettingsEditParams { edit },
            ))
        })
        .collect()
}

fn result_cases() -> Vec<(&'static str, Vec<SettingEdit>)> {
    vec![
        ("default", vec![]),
        (
            "strict",
            vec![SettingEdit::RcHiBasis {
                basis: RcHiBasis::Strict,
            }],
        ),
        (
            "lenient",
            vec![SettingEdit::RcHiBasis {
                basis: RcHiBasis::Lenient,
            }],
        ),
        (
            "agreement",
            vec![SettingEdit::RcHiBasis {
                basis: RcHiBasis::Agreement,
            }],
        ),
        (
            "transition_1",
            vec![SettingEdit::RcHiTransition { factor: 1 }],
        ),
        (
            "transition_2",
            vec![SettingEdit::RcHiTransition { factor: 2 }],
        ),
        (
            "transition_4",
            vec![SettingEdit::RcHiTransition { factor: 4 }],
        ),
        ("manual", vec![SettingEdit::RcHi { hz: Some(500.0) }]),
        ("no_volume", vec![SettingEdit::RoomVolume { m3: 0.0 }]),
        ("low_protect", vec![SettingEdit::LowProtect { on: true }]),
        ("input_hp", vec![SettingEdit::InputHp { hz: Some(25.0) }]),
        (
            "extra_bands",
            vec![SettingEdit::RcBands {
                bands: vec![
                    RcBand {
                        lo_hz: 1_000.0,
                        hi_hz: 2_000.0,
                    },
                    RcBand {
                        lo_hz: 15_000.0,
                        hi_hz: 20_000.0,
                    },
                ],
            }],
        ),
    ]
}

static RECORDED_RESULTS: OnceLock<(Vec<Value>, Vec<Value>)> = OnceLock::new();

fn recorded_results() -> &'static (Vec<Value>, Vec<Value>) {
    RECORDED_RESULTS.get_or_init(|| {
        let mut summaries = Vec::new();
        let mut lines = Vec::new();
        for (_case, edits) in result_cases() {
            let fresh = Fresh::calibrated(&edits);
            let Response::CalibrationSettingsGet(settings) =
                fresh.call(Request::CalibrationSettingsGet)
            else {
                unreachable!()
            };
            let params = json!({ "engine_key": settings.engine_key() });
            summaries.push(json!({ "params": params, "data": fresh.data(Request::Results) }));
            lines.push(json!({ "params": params, "data": fresh.data(Request::ResultLines) }));
        }
        (summaries, lines)
    })
}

fn results() -> Vec<Value> {
    recorded_results().0.clone()
}

fn result_lines() -> Vec<Value> {
    recorded_results().1.clone()
}

fn relative(path: &str, root: &Path) -> String {
    Path::new(path)
        .strip_prefix(root)
        .unwrap_or_else(|_| Path::new(path))
        .to_string_lossy()
        .replace('\\', "/")
}

fn export() -> Vec<Value> {
    let fresh = Fresh::create("2.0", 0, false);
    fresh.run_job(Request::SampleLoad(SampleLoadParams { positions: Some(9) }));
    fresh.run_job(Request::Calibrate(CalibrateParams::default()));
    [
        ExportKind::CamillaDsp,
        ExportKind::BeforeAfterMdat,
        ExportKind::StagedMdat,
        ExportKind::FilterText,
        ExportKind::EqualizerApo,
        ExportKind::Convolution,
    ]
    .into_iter()
    .map(|kind| {
        let folder = fresh.root.join(format!("export-{kind:?}"));
        let JobResult::Export(mut result) = fresh.run_job(Request::Export(ExportParams {
            kind,
            folder: folder.to_string_lossy().into_owned(),
            mdat_scope: (kind == ExportKind::BeforeAfterMdat).then_some(MdatScope::All),
            design_name: None,
            camilla_devices: None,
            extra_rates: Vec::new(),
        })) else {
            panic!("export job returns export data")
        };
        result.files = result
            .files
            .iter()
            .map(|file| relative(file, &folder))
            .collect();
        result.folder = folder.file_name().unwrap().to_string_lossy().into_owned();
        let data = serde_json::to_value(result).unwrap();
        fresh.check_portable(&data);
        json!({
            "params": { "kind": serde_json::to_value(kind).unwrap(), "design_name": null, "extra_rates": [] },
            "data": data,
        })
    })
    .collect()
}

#[test]
fn responses_match_the_recorded_copies() {
    let cases: [(&str, Entries); 8] = [
        ("mode_spec", mode_spec),
        ("config_apply_preset", config_apply_preset),
        ("target_apply_preset", target_apply_preset),
        ("calibration_settings_get", calibration_settings_get),
        ("calibration_settings_edit", calibration_settings_edit),
        ("results", results),
        ("result_lines", result_lines),
        ("export", export),
    ];
    let failures: Vec<String> = cases
        .into_iter()
        .filter_map(|(name, entries)| check(name, entries()))
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
