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
    assert_success(&calibrated);
    assert!(project.exists());
    assert!(project.with_extension("mdat").exists());
    eprintln!(
        "run1={first_time:?} run2={second_time:?} create={create_time:?} sample={sample_time:?} calibrate={calibrate_time:?}"
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
