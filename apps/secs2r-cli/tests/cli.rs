#![forbid(unsafe_code)]

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{Duration, Instant},
};

use serde_json::Value;

fn root(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../target/cli-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path.canonicalize().unwrap()
}

fn run(args: &[&str]) -> (Output, Duration) {
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_secs2r-cli"))
        .args(args)
        .output()
        .unwrap();
    (output, started.elapsed())
}

fn lines(output: &Output) -> Vec<Value> {
    std::str::from_utf8(&output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|error| panic!("{error}: {line}")))
        .collect()
}

fn assert_success(output: &Output) -> Vec<Value> {
    assert!(
        output.status.success(),
        "status={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let values = lines(output);
    assert!(!values.is_empty());
    assert!(
        values
            .iter()
            .all(|value| value.get("reply").is_some() || value.get("event").is_some())
    );
    assert!(
        values
            .iter()
            .filter_map(|value| value.get("reply"))
            .all(|reply| reply["ok"] == true)
    );
    values
}

fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(base: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        let mut entries: Vec<_> = fs::read_dir(path).unwrap().map(Result::unwrap).collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if entry.file_type().unwrap().is_dir() {
                visit(base, &entry.path(), out);
            } else {
                out.insert(
                    entry.path().strip_prefix(base).unwrap().to_owned(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}

#[test]
fn project_set_persists_low_protect_switch() {
    let root = root("low-protect");
    let project = root.join("room.secs");
    let path = project.to_string_lossy();
    assert_success(&run(&["project", "new", &path, "--preset", "2.1"]).0);
    for (value, expected) in [("on", true), ("off", false)] {
        let result = assert_success(&run(&["project", "set", &path, "--low-protect", value]).0);
        let settings = result
            .iter()
            .find(|v| v["reply"]["data"]["method"] == "calibration_settings_get")
            .unwrap();
        assert_eq!(settings["reply"]["data"]["data"]["low_protect"], expected);
        let saved: Value = serde_json::from_slice(&fs::read(&project).unwrap()).unwrap();
        assert_eq!(
            saved["calibration"].get("low_protect"),
            expected.then_some(&Value::Bool(true))
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn bootstrap_and_check_are_json_lines() {
    let (bootstrap, bootstrap_time) = run(&["bootstrap"]);
    let bootstrap = assert_success(&bootstrap);
    assert_eq!(bootstrap.len(), 1);
    assert_eq!(bootstrap[0]["reply"]["data"]["method"], "bootstrap");

    let (check, check_time) = run(&["check"]);
    let check = assert_success(&check);
    assert_eq!(check.len(), 1);
    assert_eq!(check[0]["reply"]["data"]["parallel"], "ok");
    eprintln!("bootstrap={bootstrap_time:?} check={check_time:?}");
}

#[test]
fn export_accepts_filter_kinds() {
    for kind in ["filter-text", "equalizer-apo"] {
        let root = root(kind);
        let missing = root.join("missing.secs");
        let output = root.join("export");
        let (result, _) = run(&[
            "export",
            &missing.to_string_lossy(),
            "--kind",
            kind,
            "--out",
            &output.to_string_lossy(),
        ]);
        assert_eq!(result.status.code(), Some(1));
        assert!(!String::from_utf8_lossy(&result.stderr).contains("invalid value"));
        assert_eq!(lines(&result)[0]["reply"]["error"]["code"], "project_io");
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn deterministic_run_and_sample_flow() {
    let root = root("deterministic");
    let run_root = root.join("run");
    let run_arg = run_root.to_string_lossy();
    let (first_run, first_time) = run(&[
        "--deterministic-uuids",
        "run",
        "--out",
        &run_arg,
        "--fill",
        "sample",
    ]);
    let first_lines = assert_success(&first_run);
    assert!(first_lines.iter().any(|value| {
        value["reply"]["data"]["skipped"] == "filter-text"
            && value["reply"]["data"]["reason"] == "subs"
    }));
    assert!(!run_root.join("export/filter-text").exists());
    assert!(run_root.join("export/equalizer-apo").exists());
    let expected = files(&run_root);
    fs::remove_dir_all(&run_root).unwrap();
    let (second_run, second_time) = run(&[
        "--deterministic-uuids",
        "run",
        "--out",
        &run_arg,
        "--fill",
        "sample",
    ]);
    assert_success(&second_run);
    assert_eq!(expected, files(&run_root));

    let project = root.join("manual.secs");
    let project_arg = project.to_string_lossy();
    let (created, create_time) = run(&[
        "--deterministic-uuids",
        "project",
        "new",
        &project_arg,
        "--sample",
    ]);
    assert_success(&created);
    let (sampled, sample_time) = run(&["--deterministic-uuids", "sample", &project_arg]);
    let sampled = assert_success(&sampled);
    assert!(sampled.iter().any(|value| value.get("event").is_some()));
    let (calibrated, calibrate_time) = run(&["--deterministic-uuids", "calibrate", &project_arg]);
    let calibrated = assert_success(&calibrated);
    let summaries: Vec<_> = calibrated
        .iter()
        .filter(|line| line["reply"]["data"]["method"] == "results")
        .collect();
    assert_eq!(summaries.len(), 1);
    let summary = &summaries[0]["reply"]["data"]["data"]["calibration"];
    assert!(summary.get("chain").is_none());
    assert!(
        summary["main_roles"]
            .as_array()
            .is_some_and(|roles| !roles.is_empty())
    );
    let completion = calibrated
        .iter()
        .find(|line| {
            line["event"]["event"] == "finished" && line["event"]["result"]["kind"] == "calibration"
        })
        .unwrap();
    assert_eq!(
        completion["event"]["result"],
        serde_json::json!({"kind":"calibration"})
    );
    assert!(project.exists());
    assert!(project.with_extension("mdat").exists());
    eprintln!(
        "run1={first_time:?} run2={second_time:?} create={create_time:?} sample={sample_time:?} calibrate={calibrate_time:?}"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn mimo_calibrates_in_a_fresh_process_before_computing_paths() {
    let root = root("fresh-mimo");
    let project = root.join("fresh-mimo.secs");
    let project_arg = project.to_string_lossy();
    assert_success(
        &run(&[
            "--deterministic-uuids",
            "project",
            "new",
            &project_arg,
            "--sample",
        ])
        .0,
    );
    let mut project_json: Value = serde_json::from_slice(&fs::read(&project).unwrap()).unwrap();
    project_json["calibration"] = serde_json::json!({
        "mimo_on": true,
        "mimo_max_supports": 1,
    });
    fs::write(&project, serde_json::to_vec_pretty(&project_json).unwrap()).unwrap();
    assert_success(
        &run(&[
            "--deterministic-uuids",
            "sample",
            &project_arg,
            "--positions",
            "3",
        ])
        .0,
    );
    assert_success(&run(&["--deterministic-uuids", "calibrate", &project_arg]).0);

    let (mimo, _) = run(&["--deterministic-uuids", "mimo", &project_arg]);
    let mimo = assert_success(&mimo);
    assert!(mimo.iter().any(|line| {
        line["event"]["event"] == "finished" && line["event"]["result"]["kind"] == "mimo"
    }));
    let results = mimo
        .iter()
        .find(|line| line["reply"]["data"]["method"] == "results")
        .expect("mimo command must print results");
    assert!(results["reply"]["data"]["data"]["mimo"].is_object());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_set_persists_analytic_derivative_choices() {
    let root = root("project-set-derivatives");
    let project = root.join("derivatives.secs");
    let project_arg = project.to_string_lossy();
    assert_success(&run(&["project", "new", &project_arg, "--sample"]).0);
    let set = assert_success(
        &run(&[
            "project",
            "set",
            &project_arg,
            "--mimo",
            "on",
            "--mimo-supports",
            "iir",
            "--mimo-iir-derivatives",
            "analytic",
            "--peq-derivatives",
            "analytic",
        ])
        .0,
    );
    let settings = set
        .iter()
        .rev()
        .find(|line| line["reply"]["data"]["method"] == "calibration_settings_get")
        .expect("project set must print settings");
    assert_eq!(
        settings["reply"]["data"]["data"]["peq_analytic_derivatives"],
        true
    );
    assert_eq!(
        settings["reply"]["data"]["data"]["mimo_iir_analytic_derivatives"],
        true
    );
    assert_eq!(settings["reply"]["data"]["data"]["mimo_on"], true);
    assert_eq!(settings["reply"]["data"]["data"]["mimo_fir"], false);
    let printed = assert_success(&run(&["project", "set", &project_arg]).0);
    let settings = printed
        .iter()
        .find(|line| line["reply"]["data"]["method"] == "calibration_settings_get")
        .expect("project set without choices must print settings");
    assert_eq!(
        settings["reply"]["data"]["data"]["peq_analytic_derivatives"],
        true
    );
    assert_eq!(
        settings["reply"]["data"]["data"]["mimo_iir_analytic_derivatives"],
        true
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn errors_exit_one_and_quiet_omits_events() {
    let root = root("errors-quiet");
    let missing = root.join("missing.secs");
    let missing_arg = missing.to_string_lossy();
    let (bad, bad_time) = run(&["project", "info", &missing_arg]);
    assert_eq!(bad.status.code(), Some(1));
    let bad = lines(&bad);
    assert_eq!(bad.len(), 1);
    assert_eq!(bad[0]["reply"]["ok"], false);

    let project = root.join("quiet.secs");
    let project_arg = project.to_string_lossy();
    let (created, _) = run(&["project", "new", &project_arg, "--sample"]);
    assert_success(&created);
    let (quiet, quiet_time) = run(&["--quiet", "sample", &project_arg]);
    let quiet = assert_success(&quiet);
    assert!(quiet.iter().all(|value| value.get("event").is_none()));
    assert_eq!(quiet.len(), 3);
    eprintln!("bad={bad_time:?} quiet={quiet_time:?}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn compute_info_and_process_overrides_are_json_and_missing_gpu_is_typed() {
    let root = root("compute-info");
    let missing_runtime = root.join("missing-cuda-runtime");
    let run_isolated = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_secs2r-cli"))
            .args(args)
            .env("SECS_CUDA_LIBRARY_PATH", &missing_runtime)
            .output()
            .unwrap()
    };
    let default = run_isolated(&["compute-info"]);
    let default = assert_success(&default);
    let info = &default[0]["reply"]["data"]["data"];
    assert_eq!(default[0]["reply"]["data"]["method"], "compute_status");
    assert_eq!(info["preference"], "auto");
    assert_eq!(info["selected"], "cpu");
    assert_eq!(info["gpu_available"], false);
    let cpu = run_isolated(&["compute-info", "--compute", "cpu"]);
    let cpu = assert_success(&cpu);
    assert_eq!(cpu[0]["reply"]["data"]["data"]["preference"], "cpu");
    assert_eq!(cpu[0]["reply"]["data"]["data"]["selected"], "cpu");
    let project = root.join("gpu.secs");
    let project_arg = project.to_string_lossy();
    assert_success(&run_isolated(&["project", "new", &project_arg, "--sample"]));
    assert_success(&run_isolated(&["sample", &project_arg]));
    let gpu = run_isolated(&["--compute", "gpu", "mimo", &project_arg]);
    assert_eq!(gpu.status.code(), Some(1));
    assert!(
        lines(&gpu)
            .iter()
            .any(|line| { line["reply"]["error"]["code"] == "compute_unavailable" })
    );
    let invalid = run_isolated(&["--compute", "other", "compute-info"]);
    assert_eq!(invalid.status.code(), Some(2));
    let reopened = fs::read_to_string(&project).unwrap();
    assert!(!reopened.contains("compute"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn import_mdat_position_applies_to_titles_without_a_position_tag() {
    let root = root("import-mdat-position");
    let project = root.join("room.secs");
    let project_arg = project.to_string_lossy();
    assert_success(
        &run(&[
            "project",
            "new",
            &project_arg,
            "--preset",
            "7.1.4",
            "--subs",
            "1",
        ])
        .0,
    );
    // Titles "L", "Tfl", "SW1": no position tag.
    let mdat = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../oracle/goldens/mdat/build_3_real_48000_bytes.bin");
    assert_success(
        &run(&[
            "import-mdat",
            &project_arg,
            &mdat.to_string_lossy(),
            "--position",
            "3",
        ])
        .0,
    );
    let saved: Value = serde_json::from_slice(&fs::read(&project).unwrap()).unwrap();
    let recorded: Vec<(i64, Vec<String>)> = saved["measurements"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| {
            let pos = entry["pos"]
                .as_i64()
                .or_else(|| entry["pos"].as_str()?.parse().ok())?;
            let roles: Vec<String> = entry["ir"]
                .as_object()?
                .iter()
                .filter(|(_, path)| path.as_str().is_some_and(|path| !path.is_empty()))
                .map(|(role, _)| role.clone())
                .collect();
            (!roles.is_empty()).then_some((pos, roles))
        })
        .collect();
    assert_eq!(
        recorded,
        [(3, vec!["L".into(), "Tfl".into(), "SW1".into()])],
        "{saved}"
    );
    fs::remove_dir_all(root).unwrap();
}
