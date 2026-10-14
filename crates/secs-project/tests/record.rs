#![forbid(unsafe_code)]

use secs_oracle::Fixture;
use secs_project::{
    IrCache, LoadedIr, MeasurementResult, MeasurementStore, Project, ProjectError, PyJson,
    load_project_irs, save_ir, write_wav_float32,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "secs-project-record-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn setup() -> (TempDir, PathBuf, MeasurementStore, Vec<String>) {
    let directory = TempDir::new();
    let path = directory.0.join("측정.secs");
    let mut project = Project::from_preset("2.1").unwrap();
    for speaker in &mut project.config.speakers {
        speaker.bands.truncate(1);
    }
    project.config.renumber();
    fs::write(&path, project.save_text()).unwrap();
    let roles = vec!["L".into(), "R".into(), "SW1".into()];
    (
        directory,
        path.clone(),
        MeasurementStore::new(path, project),
        roles,
    )
}

fn result(role: &str, pos: i64, uuid: &str) -> MeasurementResult {
    MeasurementResult {
        role: role.into(),
        pos,
        ir: vec![-1.0, -0.25, 0.0, 0.5, 1.0],
        sample_rate: 48_000,
        t0_s: -0.25,
        spl_db: Some(75.0),
        spl_offset_db: Some(116.1),
        snr_db: Some(24.0),
        uuid: Some(uuid.into()),
        source: Some("native".into()),
    }
}

fn oracle_result(role: &str, pos: i64, uuid: &str, offset: Option<f64>) -> MeasurementResult {
    MeasurementResult {
        role: role.into(),
        pos,
        ir: vec![0.25, -0.5],
        sample_rate: 48_000,
        t0_s: -0.001 * pos as f64,
        spl_db: Some(72.5 + pos as f64),
        spl_offset_db: offset,
        snr_db: Some(31.0),
        uuid: Some(uuid.into()),
        source: Some("must-not-be-written".into()),
    }
}

#[test]
fn measurement_session_saved_bytes_match_upstream_after_every_step() {
    let roles = vec!["L".into(), "R".into()];
    for fx in Fixture::all("service_plan", "session_") {
        let directory = TempDir::new();
        let path = directory.0.join("session.secs");
        let project = Project::load_text(&fx.inputs()["project"].to_string()).unwrap();
        fs::write(&path, project.save_text()).unwrap();
        let mut store = MeasurementStore::new(&path, project);
        let steps = fx.outputs()["steps"].as_array().unwrap();
        let mut step = 0;
        let mut check = |name: &str, store: &mut MeasurementStore| {
            store.save().unwrap();
            assert_eq!(steps[step]["name"], name, "{} step {step}", fx.id);
            assert_eq!(
                fs::read_to_string(&path).unwrap(),
                steps[step]["text"].as_str().unwrap(),
                "{} step {name}",
                fx.id
            );
            step += 1;
        };

        let repaired = store
            .record(
                oracle_result("L", 2, "l-2-repair", Some(114.5)),
                Some(&roles),
            )
            .unwrap();
        assert_eq!(repaired.snr_db.get("L"), Some(&31.0), "{}", fx.id);
        check("repair_2", &mut store);
        store.start_position(1);
        check("start_1", &mut store);
        store
            .record(oracle_result("L", 1, "l-1", None), Some(&roles))
            .unwrap();
        check("record_1_l", &mut store);
        store
            .record(oracle_result("R", 1, "r-1", Some(116.25)), Some(&roles))
            .unwrap();
        check("record_1_r_offset", &mut store);
        store.start_position(3);
        check("start_3", &mut store);
        store.start_position(2);
        check("start_2", &mut store);
        store
            .record(oracle_result("L", 2, "l-2", None), Some(&roles))
            .unwrap();
        check("record_2_l", &mut store);
        store
            .record(oracle_result("R", 2, "r-2", None), Some(&roles))
            .unwrap();
        check("record_2_r", &mut store);
        store
            .record(oracle_result("L", 2, "l-2-new", Some(115.75)), Some(&roles))
            .unwrap();
        check("record_2_l_again", &mut store);
        store.start_position(1);
        check("restart_1", &mut store);
        store
            .record(oracle_result("R", 1, "r-1-new", None), Some(&roles))
            .unwrap();
        check("remeasure_1_r", &mut store);
        assert!(store.clear_position(2));
        check("clear_2", &mut store);
        assert_eq!(step, steps.len(), "{}", fx.id);

        let saved = fs::read_to_string(&path).unwrap();
        assert!(!saved.contains("snr_db"), "{}", fx.id);
        assert!(!saved.contains("must-not-be-written"), "{}", fx.id);
    }
}

#[test]
fn test_run_plan_writes_secs_and_ir_files() {
    let (_directory, path, mut store, roles) = setup();
    for pos in [1, 2] {
        store.start_position(pos);
        for role in &roles {
            store
                .record(result(role, pos, &format!("{role}-{pos}")), Some(&roles))
                .unwrap();
        }
    }
    let saved: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let measurements = saved["measurements"].as_array().unwrap();
    assert_eq!(measurements.len(), 2);
    assert_eq!(measurements[0]["label"], "위치 1");
    assert_eq!(measurements[0]["done"], true);
    assert_eq!(measurements[0]["ir"].as_object().unwrap().len(), 3);
    assert_eq!(store.done_positions(Some(&roles)), vec![1, 2]);
    assert_eq!(store.pending_positions(&[1, 2, 3], &roles), vec![3]);
    assert!(path.with_extension("irs").join("SW1_P2.wav").is_file());
}

#[test]
fn test_saved_ir_matches_measured_ir_and_ir_saver_paths_and_round_trip() {
    let (_directory, path, mut store, roles) = setup();
    store
        .record(result("L:LF", 3, "one"), Some(&["L:LF".into()]))
        .unwrap();
    let (samples, rate) = store.ir_of("L:LF", 3).unwrap();
    assert_eq!(samples, result("L:LF", 3, "one").ir);
    assert_eq!(rate, 48_000);
    assert_eq!(
        store.ir_path_of("L:LF", 3).unwrap().file_name().unwrap(),
        "L_LF_P3.wav"
    );
    assert!(path.with_extension("irs").join("L_LF_P3.wav").is_file());
    assert_eq!(roles.len(), 3);
}

#[test]
fn test_t0_is_stored_in_secs_because_wav_cannot_hold_it_and_defaults_to_zero() {
    let (_directory, _path, mut store, roles) = setup();
    store.record(result("L", 1, "one"), Some(&roles)).unwrap();
    assert_eq!(store.t0_of("L", 1), -0.25);
    assert_eq!(store.t0_of("R", 1), 0.0);
    assert_eq!(store.t0_by_role(1).get("L"), Some(&-0.25));
    assert_eq!(store.t0_of("L", 99), 0.0);
}

#[test]
fn test_reload_project_restores_irs_by_role_and_load_project_irs_roundtrip() {
    let (_directory, path, mut store, roles) = setup();
    for role in &roles {
        store.record(result(role, 1, role), Some(&roles)).unwrap();
    }
    let project = Project::load_text(&fs::read_to_string(&path).unwrap()).unwrap();
    let mut cache = IrCache::default();
    let loaded = load_project_irs(&project, &path, &mut cache).unwrap();
    assert_eq!(loaded[&1].len(), 3);
    assert!(matches!(&loaded[&1]["L"], LoadedIr::Timed(_, 48_000, value) if *value == -0.25));
    let mut reopened = MeasurementStore::new(path, project);
    assert_eq!(reopened.irs_by_role(1).unwrap().len(), 3);
}

#[test]
fn test_load_project_irs_without_measurements() {
    let directory = TempDir::new();
    let path = directory.0.join("empty.secs");
    let project = Project::from_preset("2.0").unwrap();
    let mut cache = IrCache::default();
    assert!(
        load_project_irs(&project, &path, &mut cache)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn test_cancel_then_resume_skips_done_positions_and_partial_position_is_not_done_f2() {
    let (_directory, _path, mut store, roles) = setup();
    store.start_position(1);
    for role in &roles {
        store.record(result(role, 1, role), Some(&roles)).unwrap();
    }
    store.start_position(2);
    store.record(result("L", 2, "kept"), Some(&roles)).unwrap();
    assert_eq!(store.done_positions(Some(&roles)), vec![1]);
    assert_eq!(store.pending_positions(&[1, 2, 3], &roles), vec![2, 3]);
    assert_eq!(
        store.entry_for(2).unwrap().ir.keys().collect::<Vec<_>>(),
        vec!["L"]
    );
    assert!(!store.entry_for(2).unwrap().done);
}

#[test]
fn clear_position_updates_the_project_view() {
    let (_directory, _path, mut store, roles) = setup();
    store.record(result("L", 1, "one"), Some(&roles)).unwrap();
    assert!(store.clear_position(1));
    assert!(store.records().is_empty());
    assert_eq!(store.project().measurements, PyJson::Array(Vec::new()));
    assert!(!store.clear_position(1));
}

#[test]
fn test_remeasure_replaces_entry() {
    let (_directory, _path, mut store, roles) = setup();
    for role in &roles {
        store.record(result(role, 1, role), Some(&roles)).unwrap();
    }
    store.start_position(1);
    for role in &roles {
        store
            .record(result(role, 1, &format!("new-{role}")), Some(&roles))
            .unwrap();
    }
    assert_eq!(store.records().len(), 1);
    assert_eq!(
        store.entry_for(1).unwrap().uuid["L"].as_deref(),
        Some("new-L")
    );
}

#[test]
fn interrupted_save_leaves_old_secs_intact_f1() {
    let (directory, path, mut store, _roles) = setup();
    let old = fs::read(&path).unwrap();
    let blocker = directory.0.join("blocker");
    fs::write(&blocker, b"not a directory").unwrap();
    let inaccessible = blocker.join("project.secs");
    let project = store.project().clone();
    let mut failing = MeasurementStore::new(inaccessible, project);
    assert!(failing.save().is_err());
    assert_eq!(fs::read(path).unwrap(), old);
    store.save().unwrap();
}

#[test]
fn f5_absolute_parent_and_symlink_escapes_are_rejected_but_in_tree_reads() {
    let (directory, path, _store, _roles) = setup();
    let irs = path.with_extension("irs");
    fs::create_dir_all(&irs).unwrap();
    let good = irs.join("good.wav");
    fs::write(&good, write_wav_float32(&[0.25], 48_000)).unwrap();
    let outside = directory.0.join("outside.wav");
    fs::write(&outside, write_wav_float32(&[0.5], 48_000)).unwrap();

    for raw in [
        good.strip_prefix(path.parent().unwrap())
            .unwrap()
            .to_string_lossy()
            .to_string(),
        outside.to_string_lossy().to_string(),
        "x.irs/../outside.wav".into(),
    ] {
        let mut project = Project::from_preset("2.0").unwrap();
        project.measurements = PyJson::Array(vec![entry_with_path(&raw)]);
        let mut store = MeasurementStore::new(&path, project);
        if raw.ends_with("good.wav") {
            assert_eq!(store.ir_of("L", 1).unwrap().0, vec![0.25]);
        } else {
            assert!(matches!(
                store.ir_of("L", 1),
                Err(ProjectError::IrPath { .. })
            ));
            assert_eq!(store.warnings()[0].code, "measurement.ir_path.outside");
        }
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, irs.join("link.wav")).unwrap();
        let mut project = Project::from_preset("2.0").unwrap();
        project.measurements = PyJson::Array(vec![entry_with_path("측정.irs/link.wav")]);
        let mut store = MeasurementStore::new(&path, project);
        assert!(matches!(
            store.ir_of("L", 1),
            Err(ProjectError::IrPath { .. })
        ));
    }
}

#[test]
fn ir_cache_rereads_changed_file_and_evicts_to_cap() {
    let directory = TempDir::new();
    let a = directory.0.join("a.wav");
    let b = directory.0.join("b.wav");
    fs::write(&a, write_wav_float32(&[1.0], 48_000)).unwrap();
    fs::write(&b, write_wav_float32(&[2.0, 2.0], 48_000)).unwrap();
    let mut cache = IrCache::new(8);
    assert_eq!(cache.load(&a).unwrap().0, vec![1.0]);
    fs::write(&a, write_wav_float32(&[3.0, 4.0, 5.0], 48_000)).unwrap();
    assert_eq!(cache.load(&a).unwrap().0, vec![3.0, 4.0, 5.0]);
    cache.load(&b).unwrap();
    assert_eq!(cache.len(), 1);
    let loaded = cache.load(&b).unwrap();
    let mut caller_copy = loaded.0;
    caller_copy[0] = 99.0;
    assert_eq!(cache.load(&b).unwrap().0, vec![2.0, 2.0]);
}

fn entry_with_path(path: &str) -> PyJson {
    PyJson::Object(vec![
        ("pos".into(), PyJson::Int("1".into())),
        (
            "ir".into(),
            PyJson::Object(vec![("L".into(), PyJson::Str(path.into()))]),
        ),
    ])
}

#[allow(dead_code)]
fn save(path: &Path, role: &str, pos: i64) {
    save_ir(path, role, pos, &[0.0], 48_000).unwrap();
}
