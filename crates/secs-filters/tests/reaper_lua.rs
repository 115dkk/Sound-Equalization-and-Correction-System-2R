#![forbid(unsafe_code)]
//! Optional Lua checks live outside src: only integration tests spawn tools.

use secs_filters::{FilterRun, export_convolution_named};
use secs_oracle::Fixture;
use secs_types::chain::SignalChain;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("secs_reaper_lua_{}_{id}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn bundle(temp: &TempDir, name: &str) -> (PathBuf, Value) {
    let fixture = Fixture::load("filters", "computed_dolby");
    let mut value = fixture.inputs()["chain"].clone();
    for main in value["mains"].as_array_mut().unwrap() {
        main["fir"] = serde_json::to_value(fixture.f64s(&main["fir"])).unwrap();
    }
    let mut chain: SignalChain = serde_json::from_value(value).unwrap();
    // Keep an odd output count so the mock checks REAPER's even-channel rounding.
    chain.mains.truncate(2);
    chain.subs.truncate(1);
    let run = FilterRun {
        chain: &chain,
        mimo: None,
        mode: "mixed",
        note: "",
        label: "Lua checks",
        latency_ms: chain.latency_ms,
        rc_pieces: &[],
        stamp: "fixed",
    };
    let folder = temp.0.join("bundle");
    export_convolution_named(&run, &folder, Some(name)).unwrap();
    let routing: Value =
        serde_json::from_slice(&fs::read(folder.join("routing.json")).unwrap()).unwrap();
    // Two mains plus the LFE input; both input and output counts round up.
    assert_eq!(routing["inputs"].as_array().unwrap().len(), 3);
    assert_eq!(routing["outputs"].as_array().unwrap().len(), 3);
    let script = folder.join("secs2r_reaper.lua");
    assert!(script.is_file());
    (script, routing)
}

fn executable(variable: &str, names: &[&str]) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(variable) {
        return Some(PathBuf::from(path));
    }
    names.iter().find_map(|name| {
        Command::new(name)
            .arg("-v")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|_| PathBuf::from(name))
    })
}

#[test]
fn reaper_script_compiles_when_luac_is_available() {
    let temp = TempDir::new();
    let (script, _) = bundle(&temp, "\"\\\n\r\t\0한글9 ]=]");
    let Some(compiler) = executable("SECS_LUAC", &["luac5.4", "luac"]) else {
        eprintln!("SKIP: no Lua compiler; set SECS_LUAC (required by Linux CI)");
        return;
    };
    let output = Command::new(compiler)
        .arg("-p")
        .arg(script)
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/reaper/mock.lua"
        ))
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/reaper/harness.lua"
        ))
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/reaper/playback.lua"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn reaper_script_replaces_tracks_and_restores_failed_builds() {
    let temp = TempDir::new();
    let (script, routing) = bundle(&temp, "거실");
    let inputs = routing["inputs"].as_array().unwrap();
    let outputs = routing["outputs"].as_array().unwrap();
    // The mock's expected routing comes from the public JSON contract, not Lua.
    let paths = routing["paths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|path| {
            let input = inputs
                .iter()
                .position(|name| name == &path["input"])
                .unwrap()
                + 1;
            let output = outputs
                .iter()
                .position(|name| name == &path["output"])
                .unwrap()
                + 1;
            format!("{input}:{output}")
        })
        .collect::<Vec<_>>()
        .join(",");
    assert!(!paths.is_empty());
    let Some(lua) = executable("SECS_LUA", &["lua5.4", "lua"]) else {
        eprintln!("SKIP: no Lua interpreter; set SECS_LUA (required by Linux CI)");
        return;
    };
    let output = Command::new(lua)
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/reaper/mock.lua"
        ))
        .arg(script)
        .arg(inputs.len().to_string())
        .arg(outputs.len().to_string())
        .arg(paths)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
