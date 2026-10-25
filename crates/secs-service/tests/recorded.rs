#![forbid(unsafe_code)]
//! Records the service's no-input responses for the screen and the mock
//! service (ADR 0014). Each `apps/secs2r/ui/src/ipc/recorded/<method>.json`
//! is a list of `{ "params": …, "data": … }` entries, one per request, as
//! `Service::call` answers on a freshly created project. A mismatch fails;
//! `SECS_RECORD=1 cargo test -p secs-service --test recorded` rewrites them.
//! None of these responses carries a machine path or a time; the test fails
//! if one ever does instead of recording it.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use secs_service::{Deps, LocalTime, NoEvents, Service};
use secs_types::calibration::Mode;
use secs_types::ipc::*;
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
    // A new mode stops this match compiling until it is recorded too.
    let all = [Mode::Zero, Mode::Mixed, Mode::Linear, Mode::Ultra];
    for mode in all {
        match mode {
            Mode::Zero | Mode::Mixed | Mode::Linear | Mode::Ultra => {}
        }
    }
    all
}

/// A service with a freshly created project in its own temporary folder.
struct Fresh {
    service: Service,
    root: PathBuf,
}

impl Fresh {
    fn new() -> Self {
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
            preset: "2.0".into(),
            sub_count: 0,
            sample: false,
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

    /// The response's `data`, checked for the temporary folder and the clock.
    fn data(&self, request: Request) -> Value {
        let data = serde_json::to_value(self.call(request)).unwrap()["data"].clone();
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
        data
    }

    /// One recorded entry: the request's params and the response's data.
    fn entry(&self, request: Request) -> Value {
        let params = serde_json::to_value(&request).unwrap()["params"].clone();
        json!({ "params": params, "data": self.data(request) })
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

#[test]
fn no_input_responses_match_the_recorded_copies() {
    let cases: [(&str, Entries); 4] = [
        ("mode_spec", mode_spec),
        ("config_apply_preset", config_apply_preset),
        ("target_apply_preset", target_apply_preset),
        ("calibration_settings_get", calibration_settings_get),
    ];
    let failures: Vec<String> = cases
        .into_iter()
        .filter_map(|(name, entries)| check(name, entries()))
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
