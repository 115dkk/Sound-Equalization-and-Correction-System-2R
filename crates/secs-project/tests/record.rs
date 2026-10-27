#![forbid(unsafe_code)]

use secs_mdat::{Boxed, CalInfo, SourceDate, SourceInfo};
use secs_oracle::Fixture;
use secs_project::{
    IrCache, LoadedIr, MAX_IR_BYTES, MAX_PROJECT_BYTES, MeasurementResult, MeasurementStore,
    Project, ProjectError, PyJson, TargetCurve, calibration_fingerprint, calibration_from_project,
    dumps, load_project_irs, load_project_sources, load_source, loads, save_ir, save_source,
    source_path, store_calibration, write_wav_float32,
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
        source_info: None,
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
        // Upstream `session.record` results carry no source; one that has
        // a source is written by `record` (C6).
        source: None,
        source_info: None,
    }
}

#[test]
fn out_of_range_output_is_reset_with_a_migration_warning_td31() {
    let directory = TempDir::new();
    let path = directory.0.join("untrusted.secs");
    let mut project = Project::from_preset("2.0").unwrap();
    project.config.speakers[0].bands[0].out = 1_000_000_000;
    fs::write(&path, project.save_text()).unwrap();

    let store = MeasurementStore::open(&path).unwrap();

    assert_eq!(store.project().config.speakers[0].bands[0].out, 1);
    assert_eq!(store.warnings().len(), 1);
    assert_eq!(store.warnings()[0].code, "config.band.out.outside");
    assert_eq!(
        store.warnings()[0].entry,
        "role L, band FULL, output 1000000000"
    );
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
    }
}

#[test]
fn malformed_spl_offset_is_repaired_before_recording_td20() {
    let (_directory, _path, mut store, _roles) = setup();
    let mut project = Project::from_preset("2.0").unwrap();
    project.measurements = PyJson::Array(vec![PyJson::Object(vec![
        ("pos".into(), PyJson::Int("1".into())),
        ("spl_offset".into(), PyJson::Null),
    ])]);
    *store.project_mut() = project;

    let recorded = store.record(result("L", 1, "offset"), None).unwrap();
    assert_eq!(recorded.spl_offset.get("L"), Some(&116.1));
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
fn measurement_source_writes_exact_bytes_reads_and_deletes_stale_file() {
    let (_directory, path, mut store, roles) = setup();
    let info = SourceInfo {
        strings: vec![("outputName".into(), "장치, out 1".into())],
        boxed: vec![
            ("inputVolume".into(), Boxed::Double(-12.5)),
            ("usesEmbeddedCalData".into(), Boxed::Boolean(true)),
        ],
        cal: vec![(
            "meterCal".into(),
            CalInfo {
                inverse_c_selected: false,
                source_name: "마이크.cal".into(),
                source_path: "/캘/마이크.cal".into(),
                freq: Some(vec![20.0, 1000.0]),
                gain: Some(vec![-1.25, 0.5]),
                phase: None,
                boxed: vec![("serialNum".into(), Boxed::Integer(7))],
            },
        )],
        date: Some(SourceDate::Int(1_704_164_645_000)),
        notes: Some("원 측정".into()),
        format: Some("REW".into()),
    };
    let mut measured = result("L", 1, "one");
    measured.source_info = Some(info.clone());
    store.record(measured, Some(&roles)).unwrap();
    let wav = path.with_extension("irs").join("L_P1.wav");
    let source = source_path(&wav);
    let expected = concat!(
        "{\"strings\": {\"outputName\": \"장치, out 1\"}, ",
        "\"boxed\": {\"inputVolume\": {\"type\": \"java.lang.Double\", \"value\": -12.5}, ",
        "\"usesEmbeddedCalData\": {\"type\": \"java.lang.Boolean\", \"value\": true}}, ",
        "\"cal\": {\"meterCal\": {\"inverseCSelected\": false, \"sourceName\": \"마이크.cal\", ",
        "\"sourcePath\": \"/캘/마이크.cal\", \"freqArray\": [20.0, 1000.0], ",
        "\"gainArray\": [-1.25, 0.5], \"phaseArray\": null, ",
        "\"serialNum\": {\"type\": \"java.lang.Integer\", \"value\": 7}}}, ",
        "\"date\": 1704164645000, \"notes\": \"원 측정\", \"format\": \"REW\"}"
    );
    assert_eq!(fs::read_to_string(&source).unwrap(), expected);
    assert_eq!(load_source(&wav), Some(info.clone()));
    assert_eq!(load_project_sources(store.project(), &path)[&1]["L"], info);

    store.record(result("L", 1, "two"), Some(&roles)).unwrap();
    assert!(!source.exists());
}

fn meter_source(gain_db: f32) -> SourceInfo {
    SourceInfo {
        cal: vec![(
            "meterCal".into(),
            CalInfo {
                inverse_c_selected: false,
                source_name: "mic.cal".into(),
                source_path: "mic.cal".into(),
                freq: Some(vec![20.0, 20_000.0]),
                gain: Some(vec![gain_db, gain_db]),
                phase: None,
                boxed: Vec::new(),
            },
        )],
        ..SourceInfo::default()
    }
}

#[test]
fn store_reads_and_updates_calibration_fingerprints() {
    let (directory, path, mut store, roles) = setup();
    let first = meter_source(3.0);
    let first_fingerprint = calibration_fingerprint(&first).unwrap();
    let mut measured = result("L", 1, "curve-one");
    measured.source_info = Some(first.clone());
    store.record(measured, Some(&roles)).unwrap();
    assert_eq!(
        store
            .project()
            .measurement_calibration
            .get(&(1, "L".into())),
        Some(&first_fingerprint)
    );

    let mut reopened = MeasurementStore::open(&path).unwrap();
    assert_eq!(
        reopened
            .project()
            .measurement_calibration
            .get(&(1, "L".into())),
        Some(&first_fingerprint)
    );

    reopened
        .record(result("L", 1, "without-curve"), Some(&roles))
        .unwrap();
    assert!(
        !reopened
            .project()
            .measurement_calibration
            .contains_key(&(1, "L".into()))
    );

    let mut measured = result("L", 1, "curve-two");
    measured.source_info = Some(first);
    reopened.record(measured, Some(&roles)).unwrap();
    let replacement = meter_source(-4.0);
    let replacement_fingerprint = calibration_fingerprint(&replacement).unwrap();
    reopened.replace_source("L", 1, Some(&replacement)).unwrap();
    assert_eq!(
        reopened
            .project()
            .measurement_calibration
            .get(&(1, "L".into())),
        Some(&replacement_fingerprint)
    );
    reopened.replace_source("L", 1, None).unwrap();
    assert!(
        !reopened
            .project()
            .measurement_calibration
            .contains_key(&(1, "L".into()))
    );

    reopened.replace_source("L", 1, Some(&replacement)).unwrap();
    let outside = directory.0.join("outside.wav");
    fs::write(&outside, write_wav_float32(&[0.5], 48_000)).unwrap();
    save_source(&outside, Some(&meter_source(12.0))).unwrap();
    let PyJson::Array(entries) = &mut reopened.project_mut().measurements else {
        panic!("measurements must be an array")
    };
    entries.push(entry_at(2, &outside.to_string_lossy()));
    reopened.save().unwrap();

    let reopened = MeasurementStore::open(&path).unwrap();
    assert_eq!(
        reopened.project().measurement_calibration,
        [((1, "L".into()), replacement_fingerprint)].into()
    );
}

#[test]
fn measurement_source_tolerates_partial_values_and_rejects_unsafe_narrowing() {
    let directory = TempDir::new();
    let wav = directory.0.join("L_P1.wav");
    fs::write(
        source_path(&wav),
        r#"{"strings":{"outputName":7,"osName":null},"boxed":{"inputVolume":{"type":"java.lang.Double","value":"1.5"},"bad":{"type":"java.lang.Boolean","value":1},"splCalOffset":{"type":"java.lang.Integer","value":2147483648}},"cal":{"meterCal":{"inverseCSelected":1,"sourceName":false,"sourcePath":null,"freqArray":[20,true,"30"],"gainArray":"bad","serialNum":{"type":"java.lang.Integer","value":"42"}}},"date":NaN,"notes":0,"format":true}"#,
    )
    .unwrap();
    let info = load_source(&wav).unwrap();
    assert_eq!(info.strings, vec![("outputName".into(), "7".into())]);
    assert_eq!(info.boxed, vec![("inputVolume".into(), Boxed::Double(1.5))]);
    assert_eq!(info.date, None);
    assert_eq!(info.notes, None);
    assert_eq!(info.format.as_deref(), Some("True"));
    let cal = &info.cal[0].1;
    assert!(cal.inverse_c_selected);
    assert_eq!(cal.source_name, "");
    assert_eq!(cal.source_path, "");
    assert_eq!(cal.freq, Some(vec![20.0, 1.0, 30.0]));
    assert_eq!(cal.gain, None);
    assert_eq!(cal.boxed, vec![("serialNum".into(), Boxed::Integer(42))]);

    let unsafe_values = SourceInfo {
        boxed: vec![
            (
                "splCalOffset".into(),
                Boxed::Integer(i64::from(i32::MAX) + 1),
            ),
            ("inputVolume".into(), Boxed::Double(f64::INFINITY)),
        ],
        cal: vec![(
            "meterCal".into(),
            CalInfo {
                inverse_c_selected: false,
                source_name: String::new(),
                source_path: String::new(),
                freq: None,
                gain: None,
                phase: None,
                boxed: vec![("serialNum".into(), Boxed::Integer(i64::MIN))],
            },
        )],
        date: Some(SourceDate::Float(f64::NAN)),
        ..SourceInfo::default()
    };
    save_source(&wav, Some(&unsafe_values)).unwrap();
    let saved = fs::read_to_string(source_path(&wav)).unwrap();
    assert_eq!(
        saved,
        concat!(
            "{\"strings\": {}, \"boxed\": {}, \"cal\": {\"meterCal\": {",
            "\"inverseCSelected\": false, \"sourceName\": \"\", \"sourcePath\": \"\", ",
            "\"freqArray\": null, \"gainArray\": null, \"phaseArray\": null}}}"
        )
    );
    assert_eq!(load_source(&wav).unwrap().date, None);

    fs::write(source_path(&wav), "[]").unwrap();
    assert_eq!(load_source(&wav), None);
    fs::write(source_path(&wav), "not json").unwrap();
    assert_eq!(load_source(&wav), None);
    save_source(&wav, None).unwrap();
}

#[test]
fn measurement_source_builds_upstream_mdat_bytes_and_skips_upstream_exceptions_f10() {
    let directory = TempDir::new();
    let wav = directory.0.join("L_P1.wav");
    let mut checked = 0;
    for fixture in Fixture::all("service_mdat", "source_") {
        if fixture.id.ends_with("source_generated") {
            continue;
        }
        fs::write(
            source_path(&wav),
            fixture.inputs()["text"].as_str().unwrap(),
        )
        .unwrap();
        let info = load_source(&wav);
        assert_eq!(
            info.is_some(),
            fixture.outputs()["loaded"].as_bool().unwrap(),
            "{}",
            fixture.id
        );
        let item = secs_mdat::MeasurementInput {
            title: "L_P1".into(),
            notes: "SECS · 위치 1 · 원 측정".into(),
            sample_rate_hz: 48_000,
            samples: vec![1.0, -0.5, 0.25],
            start_time_s: -0.001,
            spl_offset_db: Some(116.25),
            source_info: info,
            computed: false,
        };
        let mut uuid = || 1;
        let bytes = secs_mdat::build(
            &[item],
            secs_mdat::TEMPLATE,
            None,
            1_704_164_645_000,
            &mut uuid,
        )
        .unwrap();
        if let Some(expected) = fixture.outputs().get("bytes") {
            assert_eq!(bytes, fixture.blob(expected), "{}", fixture.id);
        } else {
            assert!(fixture.outputs()["exception"].is_string(), "{}", fixture.id);
        }
        checked += 1;
    }
    assert_eq!(checked, 18);
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

/// What upstream `step5.load_mdat` does to the entry `session.record`
/// returned: `entry['source'] = {}` unless it is a dict, then
/// `entry['source'][role] = name`.
fn upstream_source(project: &mut Project, pos: i64, role: &str, name: &str) {
    let PyJson::Array(entries) = &mut project.measurements else {
        panic!("measurements are a list")
    };
    let entry = entries
        .iter_mut()
        .find(|entry| {
            matches!(entry, PyJson::Object(fields)
                if fields.contains(&("pos".into(), PyJson::Int(pos.to_string()))))
        })
        .unwrap();
    let object = members(entry);
    let index = object
        .iter()
        .position(|(key, _)| key == "source")
        .unwrap_or_else(|| {
            object.push(("source".into(), PyJson::Object(Vec::new())));
            object.len() - 1
        });
    if !matches!(object[index].1, PyJson::Object(_)) {
        object[index].1 = PyJson::Object(Vec::new());
    }
    let sources = members(&mut object[index].1);
    match sources.iter_mut().find(|(key, _)| key == role) {
        Some((_, value)) => *value = PyJson::Str(name.into()),
        None => sources.push((role.into(), PyJson::Str(name.into()))),
    }
}

#[test]
fn record_writes_its_source_and_saves_the_project_once_c6() {
    let (_directory, path, mut store, roles) = setup();
    // Position 3 holds a malformed `source` before other keys: it is
    // replaced in place, where upstream's `entry['source'] = {}` puts it.
    store.project_mut().measurements = PyJson::Array(vec![PyJson::Object(vec![
        ("pos".into(), PyJson::Int("3".into())),
        ("source".into(), PyJson::Str("stale".into())),
        ("label".into(), PyJson::Str("kept".into())),
    ])]);
    // The reference records without a source, then sets it like upstream
    // and saves again: the bytes the importers wrote before C6.
    let reference_directory = TempDir::new();
    let reference_path = reference_directory.0.join("측정.secs");
    let mut reference = MeasurementStore::new(&reference_path, store.project().clone());
    for (role, pos, name) in [
        ("L", 1, "room.mdat"),
        ("R", 1, "room.mdat"),
        ("L", 1, "again.mdat"),
        ("SW1", 3, "room.liveproject"),
    ] {
        let mut imported = result(role, pos, &format!("{role}-{pos}"));
        imported.source = Some(name.into());
        let saves = store.save_count();
        store.record(imported.clone(), Some(&roles)).unwrap();
        assert_eq!(store.save_count(), saves + 1, "{role} P{pos}");

        imported.source = None;
        reference.record(imported, Some(&roles)).unwrap();
        upstream_source(reference.project_mut(), pos, role, name);
        reference.save().unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            fs::read_to_string(&reference_path).unwrap(),
            "{role} P{pos}"
        );
    }
    let first = store.entry_for(1).unwrap();
    assert_eq!(first.source["L"], "again.mdat");
    assert_eq!(first.source["R"], "room.mdat");
    assert_eq!(
        store.entry_for(3).unwrap().source["SW1"],
        "room.liveproject"
    );
    let mut saved = loads(&fs::read_to_string(&path).unwrap()).unwrap();
    let PyJson::Array(entries) = member(&mut saved, "measurements") else {
        panic!("measurements are a list")
    };
    assert_eq!(keys(&entries[0])[..2], ["pos", "source"]);
    assert_eq!(keys(&entries[1]).last().unwrap(), "source");

    // A result without a source (a measurement) leaves the entry without one.
    let mut measured = result("L", 2, "measured");
    measured.source = None;
    store.record(measured, Some(&roles)).unwrap();
    assert!(store.entry_for(2).unwrap().source.is_empty());
    let PyJson::Array(entries) = &store.project().measurements else {
        panic!("measurements are a list")
    };
    assert!(!keys(&entries[2]).contains(&"source".into()));
}

#[test]
fn removed_position_comes_back_with_its_entries_and_snr_c6() {
    let (_directory, _path, mut store, roles) = setup();
    for pos in [1, 2, 3] {
        for (index, role) in roles.iter().enumerate() {
            let mut measured = result(role, pos, &format!("{role}-{pos}"));
            measured.snr_db = Some(20.0 + pos as f64 + index as f64 / 10.0);
            store.record(measured, Some(&roles)).unwrap();
        }
    }
    let json = store.project().save_text();
    let middle = store.entry_for(2).unwrap();
    assert_eq!(middle.snr_db.len(), roles.len());
    let saves = store.save_count();

    let removed = store.remove_position(2).unwrap();
    assert_eq!(removed.position(), 2);
    assert!(store.entry_for(2).is_none());
    assert_eq!(
        store
            .records()
            .iter()
            .map(|record| record.pos)
            .collect::<Vec<_>>(),
        [1, 3]
    );
    assert!(store.remove_position(2).is_none());

    store.restore_position(removed).unwrap();
    assert_eq!(store.project().save_text(), json);
    assert_eq!(store.entry_for(2).unwrap(), middle);
    assert_eq!(store.save_count(), saves, "neither writes the project file");

    for pos in [1, 2, 3] {
        assert!(store.clear_position(pos));
    }
    assert!(store.records().is_empty());
    assert_eq!(store.project().measurements, PyJson::Array(Vec::new()));
    assert!(!store.clear_position(1));
}

#[test]
fn restoring_a_position_recorded_again_fails_c6() {
    let (_directory, _path, mut store, roles) = setup();
    store.record(result("L", 1, "first"), Some(&roles)).unwrap();
    let removed = store.remove_position(1).unwrap();
    let mut again = result("R", 1, "again");
    again.snr_db = Some(12.5);
    store.record(again, Some(&roles)).unwrap();
    let json = store.project().save_text();
    let entry = store.entry_for(1).unwrap();

    assert!(matches!(
        store.restore_position(removed.clone()),
        Err(ProjectError::Invalid(_))
    ));
    assert_eq!(store.project().save_text(), json);
    assert_eq!(store.entry_for(1).unwrap(), entry);

    assert!(store.clear_position(1));
    store.restore_position(removed).unwrap();
    let restored = store.entry_for(1).unwrap();
    assert_eq!(restored.uuid["L"].as_deref(), Some("first"));
    assert_eq!(restored.snr_db.get("L"), Some(&24.0));
    assert!(!restored.ir.contains_key("R"));
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
    let no_temporary = || {
        fs::read_dir(&directory.0).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".tmp-")
        })
    };
    // A save never writes into the existing file: a second name for the old
    // bytes keeps them while the project name gets the new ones.
    let backup = directory.0.join("old.secs");
    fs::hard_link(&path, &backup).unwrap();
    store.project_mut().name = "바뀐 이름".into();
    store.save().unwrap();
    assert!(
        fs::read(&backup).unwrap() == old,
        "the save wrote into the existing file"
    );
    assert!(fs::read(&path).unwrap() != old, "the save did not land");
    assert!(no_temporary());

    // A save of this same project that fails at the last step (the name is
    // taken by a folder holding the old bytes) leaves them and no temporary file.
    let saved = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    fs::write(path.join("old"), &saved).unwrap();
    store.project_mut().name = "저장하지 못한 이름".into();
    assert!(store.save().is_err());
    assert!(
        fs::read(path.join("old")).unwrap() == saved,
        "the failed save touched the old bytes"
    );
    assert!(no_temporary(), "the failed save left its temporary file");
}

#[test]
fn ir_wav_replacement_is_atomic_and_failed_rename_leaves_no_temporary_file_td15() {
    let (directory, path, _store, _roles) = setup();
    let irs = path.with_extension("irs");
    fs::create_dir_all(&irs).unwrap();
    let wav = irs.join("L_P1.wav");
    let backup = directory.0.join("old.wav");
    let old = write_wav_float32(&[0.25], 48_000);
    fs::write(&wav, &old).unwrap();
    fs::hard_link(&wav, &backup).unwrap();

    save_ir(&path, "L", 1, &[0.5], 48_000).unwrap();
    assert_eq!(fs::read(&backup).unwrap(), old);
    assert_ne!(fs::read(&wav).unwrap(), old);

    fs::remove_file(&wav).unwrap();
    fs::create_dir(&wav).unwrap();
    fs::write(wav.join("old"), &old).unwrap();
    assert!(save_ir(&path, "L", 1, &[1.0], 48_000).is_err());
    assert_eq!(fs::read(wav.join("old")).unwrap(), old);
    assert!(fs::read_dir(&irs).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".tmp-")
    }));
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
fn project_sources_are_read_only_inside_the_irs_folder_td43() {
    let directory = TempDir::new();
    let project_directory = directory.0.join("project");
    fs::create_dir_all(&project_directory).unwrap();
    let path = project_directory.join("측정.secs");
    let irs = path.with_extension("irs");
    fs::create_dir_all(&irs).unwrap();
    let info = SourceInfo {
        strings: vec![("outputName".into(), "원 측정".into())],
        ..SourceInfo::default()
    };
    let good = irs.join("good.wav");
    fs::write(&good, write_wav_float32(&[0.25], 48_000)).unwrap();
    save_source(&good, Some(&info)).unwrap();
    let outside = directory.0.join("outside.wav");
    fs::write(&outside, write_wav_float32(&[0.5], 48_000)).unwrap();
    save_source(&outside, Some(&info)).unwrap();

    // Only Unix adds the symlink rows below.
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut entries = vec![
        entry_at(1, "측정.irs/good.wav"),
        entry_at(2, "../outside.wav"),
        entry_at(3, &outside.to_string_lossy()),
    ];
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        // An in-tree IR whose sidecar links outside.
        let linked_source = irs.join("linked.wav");
        fs::write(&linked_source, write_wav_float32(&[0.75], 48_000)).unwrap();
        symlink(source_path(&outside), source_path(&linked_source)).unwrap();
        entries.push(entry_at(4, "측정.irs/linked.wav"));
        // An IR that links outside, with an in-tree sidecar.
        let linked_ir = irs.join("link.wav");
        symlink(&outside, &linked_ir).unwrap();
        save_source(&linked_ir, Some(&info)).unwrap();
        entries.push(entry_at(5, "측정.irs/link.wav"));
    }
    let mut project = Project::from_preset("2.0").unwrap();
    project.measurements = PyJson::Array(entries);

    let sources = load_project_sources(&project, &path);
    assert_eq!(sources.keys().copied().collect::<Vec<_>>(), vec![1]);
    assert_eq!(sources[&1]["L"], info);
}

#[cfg(unix)]
#[test]
fn ir_writes_reject_symlinked_store_and_destination_td16() {
    use std::os::unix::fs::symlink;

    let directory = TempDir::new();
    let path = directory.0.join("project.secs");
    let project = Project::from_preset("2.0").unwrap();
    fs::write(&path, project.save_text()).unwrap();
    let outside = directory.0.join("outside");
    fs::create_dir(&outside).unwrap();
    let outside_wav = outside.join("L_P1.wav");
    let old = write_wav_float32(&[0.25], 48_000);
    fs::write(&outside_wav, &old).unwrap();
    symlink(&outside, path.with_extension("irs")).unwrap();

    let mut store = MeasurementStore::new(&path, project.clone());
    let error = store
        .record(result("L", 1, "linked-store"), None)
        .unwrap_err();
    assert!(error.to_string().contains("symlink"));
    assert_eq!(fs::read(&outside_wav).unwrap(), old);

    fs::remove_file(path.with_extension("irs")).unwrap();
    fs::create_dir(path.with_extension("irs")).unwrap();
    symlink(&outside_wav, path.with_extension("irs").join("L_P1.wav")).unwrap();
    let mut store = MeasurementStore::new(&path, project);
    let error = store
        .record(result("L", 1, "linked-destination"), None)
        .unwrap_err();
    assert!(error.to_string().contains("symlink"));
    assert_eq!(fs::read(&outside_wav).unwrap(), old);
}

#[test]
fn oversized_project_and_wav_reads_are_rejected_td19() {
    let directory = TempDir::new();
    let project_path = directory.0.join("oversized.secs");
    let project_file = fs::File::create(&project_path).unwrap();
    project_file.set_len(MAX_PROJECT_BYTES + 1).unwrap();
    let error = MeasurementStore::open(&project_path).unwrap_err();
    let message = error.to_string();
    assert!(message.contains(&project_path.display().to_string()));
    assert!(message.contains(&MAX_PROJECT_BYTES.to_string()));

    let wav_path = directory.0.join("oversized.wav");
    let mut wav = write_wav_float32(&[], 48_000);
    wav[40..44].copy_from_slice(&u32::try_from(MAX_IR_BYTES + 1).unwrap().to_le_bytes());
    fs::write(&wav_path, wav).unwrap();
    let error = IrCache::default().load(&wav_path).unwrap_err();
    let message = error.to_string();
    assert!(message.contains(&wav_path.display().to_string()));
    assert!(message.contains(&MAX_IR_BYTES.to_string()));
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

#[test]
fn ir_cache_keeps_its_byte_cap_and_evicts_least_recently_used_td44() {
    let directory = TempDir::new();
    let wav = |name: &str, samples: &[f32]| {
        let path = directory.0.join(name);
        fs::write(&path, write_wav_float32(samples, 48_000)).unwrap();
        path
    };

    // 12 bytes of samples under an 8-byte cap: returned, not retained.
    let large = wav("large.wav", &[1.0, 2.0, 3.0]);
    let mut cache = IrCache::new(8);
    assert_eq!(cache.load(&large).unwrap().0, vec![1.0, 2.0, 3.0]);
    assert_eq!(cache.retained_bytes(), 0);
    assert!(cache.is_empty());
    cache.remember(&large, &[1.0, 2.0, 3.0], 48_000).unwrap();
    assert_eq!(cache.retained_bytes(), 0);

    // A (4 bytes), B (8 bytes), hit A, then C (4 bytes) under a 12-byte cap:
    // B is the least recently used, so A and C (8 bytes) remain.
    let a = wav("a.wav", &[1.0]);
    let b = wav("b.wav", &[2.0, 2.0]);
    let c = wav("c.wav", &[3.0]);
    let mut cache = IrCache::new(12);
    cache.load(&a).unwrap();
    cache.load(&b).unwrap();
    assert_eq!(cache.retained_bytes(), 12);
    cache.load(&a).unwrap();
    cache.load(&c).unwrap();
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.retained_bytes(), 8);
}

/// The member `key` of an object.
fn member<'a>(value: &'a mut PyJson, key: &str) -> &'a mut PyJson {
    &mut members(value)
        .iter_mut()
        .find(|(name, _)| name == key)
        .unwrap_or_else(|| panic!("no member {key}"))
        .1
}

fn members(value: &mut PyJson) -> &mut Vec<(String, PyJson)> {
    match value {
        PyJson::Object(fields) => fields,
        _ => panic!("not an object"),
    }
}

fn first(value: &mut PyJson) -> &mut PyJson {
    match value {
        PyJson::Array(items) => &mut items[0],
        _ => panic!("not an array"),
    }
}

fn keys(value: &PyJson) -> Vec<String> {
    match value {
        PyJson::Object(fields) => fields.iter().map(|(key, _)| key.clone()).collect(),
        _ => panic!("not an object"),
    }
}

fn followed_by(mut known: Vec<String>, unknown: &[&str]) -> Vec<String> {
    known.extend(unknown.iter().map(|key| (*key).to_owned()));
    known
}

#[test]
fn unknown_nested_keys_survive_open_change_and_save_f16() {
    let directory = TempDir::new();
    let path = directory.0.join("future.secs");
    let mut project = Project::from_preset("2.1").unwrap();
    project.extra.push((
        "calibration".into(),
        loads(r#"{"subs_manual":{"SW1":{"mode":"fixed"}},"bundle_layouts":{"A":{"kind":"dba"}}}"#)
            .unwrap(),
    ));
    let settings = calibration_from_project(&project);
    store_calibration(&mut project, &settings);

    // What SECS 2R writes, then what a newer SECS 2 adds to it.
    let mut file = project.to_json();
    let mut writer = file.clone();
    let calibration = member(&mut file, "calibration");
    members(calibration).insert(0, ("future_first".into(), PyJson::Str("a".into())));
    members(calibration).push((
        "future_last".into(),
        loads(r#"{"nested":[1,{"deeper":true}]}"#).unwrap(),
    ));
    members(member(member(calibration, "subs_manual"), "SW1"))
        .push(("future_sub".into(), PyJson::Bool(true)));
    members(member(member(calibration, "bundle_layouts"), "A"))
        .push(("future_layout".into(), PyJson::Int("7".into())));
    let config = member(&mut file, "config");
    members(config).push(("future_config".into(), PyJson::Str("c".into())));
    let speaker = first(member(config, "speakers"));
    members(speaker).insert(1, ("future_speaker".into(), loads(r#"{"x":1}"#).unwrap()));
    members(first(member(speaker, "bands"))).push(("future_band".into(), PyJson::Str("b".into())));
    members(member(&mut file, "dsp")).insert(0, ("future_dsp".into(), PyJson::Float(1.5)));
    members(member(&mut file, "target")).push(("future_target".into(), loads("[[1, 2]]").unwrap()));
    fs::write(&path, format!("{}\n", dumps(&file))).unwrap();

    // Open, change a setting in each object, save.
    let mut store = MeasurementStore::open(&path).unwrap();
    let mut settings = calibration_from_project(store.project());
    settings.rc_lo = 30.0;
    store_calibration(store.project_mut(), &settings);
    store.project_mut().config.speakers[0].size = "large".into();
    store.project_mut().dsp.fir_taps = 4096;
    store.project_mut().target = TargetCurve::new(vec![(20.0, 1.0), (20_000.0, -1.0)]);
    store.save().unwrap();

    let saved_text = fs::read_to_string(&path).unwrap();
    let mut saved = loads(&saved_text).unwrap();
    let calibration = member(&mut saved, "calibration");
    let written_calibration = member(&mut writer, "calibration");
    assert_eq!(
        keys(calibration),
        followed_by(keys(written_calibration), &["future_first", "future_last"])
    );
    assert_eq!(*member(calibration, "rc_lo"), PyJson::Float(30.0));
    assert_eq!(
        *member(calibration, "future_last"),
        loads(r#"{"nested":[1,{"deeper":true}]}"#).unwrap()
    );
    let sub = member(member(calibration, "subs_manual"), "SW1");
    assert_eq!(
        keys(sub),
        followed_by(
            keys(member(member(written_calibration, "subs_manual"), "SW1")),
            &["future_sub"]
        )
    );
    let layout = member(member(calibration, "bundle_layouts"), "A");
    assert_eq!(keys(layout), ["kind", "rear_trim_ms", "future_layout"]);
    let config = member(&mut saved, "config");
    assert_eq!(
        keys(config),
        [
            "speakers",
            "device",
            "preset",
            "biamp_default",
            "future_config"
        ]
    );
    let speaker = first(member(config, "speakers"));
    assert_eq!(keys(speaker), ["role", "size", "bands", "future_speaker"]);
    assert_eq!(*member(speaker, "size"), PyJson::Str("large".into()));
    assert_eq!(
        keys(first(member(speaker, "bands"))),
        ["band", "out", "pol", "future_band"]
    );
    let dsp = member(&mut saved, "dsp");
    assert_eq!(
        keys(dsp),
        followed_by(keys(member(&mut writer, "dsp")), &["future_dsp"])
    );
    assert_eq!(*member(dsp, "fir_taps"), PyJson::Int("4096".into()));
    let target = member(&mut saved, "target");
    assert_eq!(keys(target), ["points", "future_target"]);
    assert_eq!(
        *member(target, "points"),
        loads("[[20.0, 1.0], [20000.0, -1.0]]").unwrap()
    );

    // Reopening and saving again changes nothing.
    let mut reopened = MeasurementStore::open(&path).unwrap();
    reopened.save().unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), saved_text);

    // Entries the writer dropped stay dropped; the rest is still kept.
    let mut settings = calibration_from_project(reopened.project());
    settings.subs_manual.clear();
    store_calibration(reopened.project_mut(), &settings);
    reopened.save().unwrap();
    let mut saved = loads(&fs::read_to_string(&path).unwrap()).unwrap();
    let calibration = member(&mut saved, "calibration");
    assert_eq!(*member(calibration, "subs_manual"), PyJson::Object(vec![]));
    assert!(keys(calibration).ends_with(&["future_first".into(), "future_last".into()]));
}

fn entry_with_path(path: &str) -> PyJson {
    entry_at(1, path)
}

fn entry_at(pos: i64, path: &str) -> PyJson {
    PyJson::Object(vec![
        ("pos".into(), PyJson::Int(pos.to_string())),
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
