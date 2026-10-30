//! The bundle self-test, run when `SECS2R_SELF_TEST` names a report file.
//!
//! CI and release.yml launch the built macOS app this way to prove that the
//! bundle a user downloads opens and calibrates
//! (`.github/scripts/macos_app_check.py`). Once the screen has loaded, the
//! driver in `self_test.js` sends a mock-room measurement, a calibration and
//! an export through `secs_call`, the command the screen uses, and
//! `secs_self_test_report` writes what each call answered to the report file
//! and quits. Without the variable nothing here runs and the report command
//! does nothing. With `SECS2R_SELF_TEST_AUDIO` also set to a device name, the
//! driver then checks native stereo playback/recording in a separate project.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};
use tauri::webview::{PageLoadEvent, PageLoadPayload};
use tauri::{AppHandle, Manager, Webview};

const VARIABLE: &str = "SECS2R_SELF_TEST";
const DRIVER: &str = include_str!("self_test.js");
const WORK_PLACEHOLDER: &str = "__SECS2R_SELF_TEST_WORK__";
const AUDIO_VARIABLE: &str = "SECS2R_SELF_TEST_AUDIO";
const AUDIO_PLACEHOLDER: &str = "__SECS2R_SELF_TEST_AUDIO__";

/// The report file of an armed self-test, and whether its driver has been
/// started (a page reload must not start a second run).
pub(crate) struct SelfTest {
    report: PathBuf,
    audio_device: Option<String>,
    started: AtomicBool,
}

/// Arms the self-test when the environment asks for one.
pub(crate) fn arm(app: &tauri::App) {
    if let Some(report) = std::env::var_os(VARIABLE).filter(|value| !value.is_empty()) {
        app.manage(SelfTest {
            report: PathBuf::from(report),
            audio_device: std::env::var(AUDIO_VARIABLE)
                .ok()
                .filter(|value| !value.is_empty()),
            started: AtomicBool::new(false),
        });
    }
}

/// Starts the driver once the screen's page has loaded. The project and the
/// export go to a folder beside the report.
pub(crate) fn on_page_load(webview: &Webview, payload: &PageLoadPayload<'_>) {
    if payload.event() != PageLoadEvent::Finished {
        return;
    }
    let Some(test) = webview.try_state::<SelfTest>() else {
        return;
    };
    if test.started.swap(true, Ordering::SeqCst) {
        return;
    }
    let work = work_folder(&test.report);
    let started = std::fs::create_dir_all(&work)
        .map_err(|error| format!("cannot create {}: {error}", work.display()))
        .and_then(|()| {
            webview
                .eval(driver(&work, test.audio_device.as_deref()))
                .map_err(|error| format!("cannot start the driver: {error}"))
        });
    if let Err(error) = started {
        finish(
            webview.app_handle(),
            &test.report,
            &json!({"ok": false, "error": error}),
        );
    }
}

/// Writes the driver's report and quits: exit code 0 when the report says
/// `ok`, 1 otherwise.
#[tauri::command]
pub(crate) fn secs_self_test_report(app: AppHandle, report: Value) {
    if let Some(test) = app.try_state::<SelfTest>() {
        finish(&app, &test.report, &report);
    }
}

fn finish(app: &AppHandle, path: &Path, report: &Value) {
    let written = serde_json::to_vec_pretty(report)
        .map_err(|error| error.to_string())
        .and_then(|bytes| std::fs::write(path, bytes).map_err(|error| error.to_string()));
    if let Err(error) = &written {
        eprintln!(
            "SECS 2R self-test: cannot write {}: {error}",
            path.display()
        );
    }
    let ok = written.is_ok() && report.get("ok") == Some(&Value::Bool(true));
    app.exit(if ok { 0 } else { 1 });
}

fn work_folder(report: &Path) -> PathBuf {
    report.with_extension("work")
}

fn driver(work: &Path, audio_device: Option<&str>) -> String {
    let work = Value::String(work.to_string_lossy().into_owned()).to_string();
    let audio = json!(audio_device).to_string();
    // Replace only template text, never placeholder-like text in either value.
    DRIVER
        .split(WORK_PLACEHOLDER)
        .map(|part| part.replace(AUDIO_PLACEHOLDER, &audio))
        .collect::<Vec<_>>()
        .join(&work)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_driver_gets_the_work_folder_as_a_javascript_string() {
        let work = work_folder(Path::new("/tmp/a \"b\"/report.json"));
        assert_eq!(work, Path::new("/tmp/a \"b\"/report.work"));
        let script = driver(&work, None);
        assert!(script.contains("const audioDevice = null;"));
        assert!(!script.contains(AUDIO_PLACEHOLDER));
        assert!(!script.contains(WORK_PLACEHOLDER));
        assert!(script.contains(r#"const work = "/tmp/a \"b\"/report.work";"#));
    }

    #[test]
    fn audio_is_opt_in_and_its_device_is_a_javascript_string() {
        let script = driver(Path::new(AUDIO_PLACEHOLDER), Some("BlackHole \"2ch\""));
        assert!(script.contains(r#"const audioDevice = "BlackHole \"2ch\"";"#));
        assert!(script.contains(&format!("const work = \"{AUDIO_PLACEHOLDER}\";")));
    }
}
