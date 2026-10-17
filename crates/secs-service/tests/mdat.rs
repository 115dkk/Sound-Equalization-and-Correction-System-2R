#![forbid(unsafe_code)]
use secs_mdat::{Boxed, SourceDate};
use secs_project::{
    MeasurementResult, MeasurementStore, Project, PyJson, load_source, save_source, source_path,
};
use secs_service::mdat_import::*;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{collections::BTreeSet, path::PathBuf};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "secs-service-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().into())
        .collect()
}
fn optional(v: &Value) -> Vec<Option<String>> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().map(str::to_owned))
        .collect()
}
#[test]
fn service_mdat_every_oracle_value_and_imported_secs_bytes() {
    let dir = root().join("oracle/goldens/service_mdat");
    let mut count = 0;
    for file in std::fs::read_dir(dir).unwrap() {
        let path = file.unwrap().path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let (input, out) = (&v["inputs"], &v["outputs"]);
        let name = path.file_stem().unwrap().to_str().unwrap();
        if name.starts_with("titles") {
            let titles = strings(&input["titles"]);
            let roles = strings(&input["roles"]);
            let flags = input["flags"]
                .as_array()
                .unwrap()
                .iter()
                .map(Value::as_bool)
                .collect::<Vec<_>>();
            assert_eq!(
                json!(titles.iter().map(|t| split_position(t)).collect::<Vec<_>>()),
                out["split"],
                "{name}"
            );
            assert_eq!(
                json!(
                    titles
                        .iter()
                        .map(|t| match_role(t, &roles, &BTreeSet::new()))
                        .collect::<Vec<_>>()
                ),
                out["match"],
                "{name}"
            );
            assert_eq!(
                json!(
                    titles
                        .iter()
                        .map(|t| split_position(t).0 != t.trim())
                        .collect::<Vec<_>>()
                ),
                out["position_tagged"]
            );
            let used = roles.iter().take(1).cloned().collect();
            assert_eq!(
                json!(
                    titles
                        .iter()
                        .map(|t| match_role(t, &roles, &used))
                        .collect::<Vec<_>>()
                ),
                out["match_used"],
                "{name}"
            );
            assert_eq!(
                json!(suggest_assignment(&titles, &roles)),
                out["suggest"],
                "{name}"
            );
            assert_eq!(
                json!(auto_assign(&titles, &roles, &flags)),
                out["auto"],
                "{name}"
            );
        } else if name.starts_with("problems") {
            let positions = input["positions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                json!(problems(&optional(&input["assignment"]), &positions)),
                *out
            );
        } else if name == "source_generated" {
            let directory = TempDir::new("mdat-source");
            let wav = directory.0.join("L_P1.wav");
            for case in ["rew", "missing_cal"] {
                std::fs::write(source_path(&wav), out["json"][case].as_str().unwrap()).unwrap();
                let info = load_source(&wav).unwrap();
                match case {
                    "rew" => {
                        assert_eq!(
                            info.strings,
                            vec![
                                ("outputName".into(), "출력 장치, out 1".into()),
                                ("sourceFileName".into(), "입력 장치".into()),
                            ]
                        );
                        assert_eq!(info.date, Some(SourceDate::Int(1_704_164_645_000)));
                        assert_eq!(
                            info.boxed,
                            vec![("usesEmbeddedCalData".into(), Boxed::Boolean(true))]
                        );
                        let cal = &info.cal[0].1;
                        assert_eq!(cal.source_name, "마이크.cal");
                        assert_eq!(cal.freq, Some(vec![20.0, 1000.0]));
                        assert_eq!(cal.gain, Some(vec![-1.25, 0.25]));
                        assert_eq!(cal.phase, Some(vec![0.5, 0.0]));
                    }
                    "missing_cal" => {
                        let cal = &info.cal[0].1;
                        assert_eq!(cal.source_name, "없음.cal");
                        assert_eq!(cal.freq, None);
                        assert_eq!(cal.gain, None);
                        assert_eq!(cal.phase, None);
                    }
                    _ => unreachable!(),
                }
                save_source(&wav, Some(&info)).unwrap();
            }
        } else if name.starts_with("file_") {
            let source = root().join(input["path"].as_str().unwrap());
            let scan = scan_for(&source, &strings(&input["roles"]), None, 1).unwrap();
            let service = secs_service::Service::new(secs_service::Deps::default());
            let secs_types::ipc::Envelope::Ok(response) = service.call(
                secs_types::ipc::Request::MdatScan(secs_types::ipc::MdatScanParams {
                    path: source.to_string_lossy().into_owned(),
                }),
                std::sync::Arc::new(secs_service::NoEvents),
            ) else {
                panic!("scan IPC failed")
            };
            let secs_types::ipc::Response::MdatScan(result) = *response else {
                panic!()
            };
            assert_eq!(
                json!(
                    result
                        .measurements
                        .iter()
                        .map(|m| m.position_tagged)
                        .collect::<Vec<_>>()
                ),
                out["position_tagged"]
            );
            assert_eq!(
                json!(
                    scan.measurements
                        .iter()
                        .map(|m| &m.info.title)
                        .collect::<Vec<_>>()
                ),
                out["titles"]
            );
            assert_eq!(
                json!(
                    scan.measurements
                        .iter()
                        .map(|m| m.looks_like_sub)
                        .collect::<Vec<_>>()
                ),
                out["flags"]
            );
            assert_eq!(
                json!(
                    scan.measurements
                        .iter()
                        .map(|m| (&m.title_base, m.position))
                        .collect::<Vec<_>>()
                ),
                out["split"]
            );
            assert_eq!(
                json!(
                    scan.measurements
                        .iter()
                        .map(|m| &m.role)
                        .collect::<Vec<_>>()
                ),
                out["assignment"]
            );
            let temp = TempDir::new(&format!("mdat-{name}"));
            let target = temp.0.join("import.secs");
            let mut project = Project::load_text(&input["project"].to_string()).unwrap();
            project
                .extra
                .push(("array".into(), PyJson::Int("9".into())));
            let mut store = MeasurementStore::new(&target, project);
            assert_eq!(
                import(&mut store, &source, None, None).unwrap() as u64,
                out["import_count"].as_u64().unwrap()
            );
            if let Some(text) = out["secs"].as_str() {
                assert_eq!(std::fs::read_to_string(&target).unwrap(), text, "{name}");
            }
        }
        count += 1;
    }
    assert_eq!(count, 43);
}

#[test]
fn side_mdat_bytes_match_upstream_for_every_import_case() {
    for fixture in secs_oracle::Fixture::all("service_mdat", "file_") {
        let directory = TempDir::new(&format!("side-mdat-{}", fixture.id.replace('/', "-")));
        let source = root().join(fixture.inputs()["path"].as_str().unwrap());
        let target = directory.0.join("import.secs");
        std::fs::write(&target, fixture.outputs()["secs"].as_str().unwrap()).unwrap();
        for (relative, text) in fixture.outputs()["source_sidecars"].as_object().unwrap() {
            let path = directory.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text.as_str().unwrap()).unwrap();
        }
        // Import once through Rust as the product path does, then save with the
        // oracle's fixed UUID sequence and clock.
        let mut project = Project::load_text(&fixture.inputs()["project"].to_string()).unwrap();
        project
            .extra
            .push(("array".into(), PyJson::Int("9".into())));
        let mut store = MeasurementStore::new(&target, project);
        assert_eq!(
            import(&mut store, &source, None, None).unwrap() as u64,
            fixture.outputs()["import_count"].as_u64().unwrap()
        );
        let mut next = 1_u128;
        let deps = secs_service::Deps {
            uuid: std::sync::Arc::new(std::sync::Mutex::new(Box::new(move || {
                let value = next;
                next += 1;
                Ok(value)
            }))),
            unix_ms: std::sync::Arc::new(|| 1_704_164_645_000),
            ..Default::default()
        };
        let service = secs_service::Service::new(deps);
        let secs_types::ipc::Envelope::Ok(_) = service.call(
            secs_types::ipc::Request::ProjectOpen(secs_types::ipc::ProjectPathParams {
                path: target.to_string_lossy().into_owned(),
            }),
            std::sync::Arc::new(secs_service::NoEvents),
        ) else {
            panic!("{} open", fixture.id)
        };
        let secs_types::ipc::Envelope::Ok(_) = service.call(
            secs_types::ipc::Request::ProjectSave(Default::default()),
            std::sync::Arc::new(secs_service::NoEvents),
        ) else {
            panic!("{} save", fixture.id)
        };
        assert_eq!(
            std::fs::read(target.with_extension("mdat")).unwrap(),
            fixture.blob(&fixture.outputs()["side_mdat"]),
            "{}",
            fixture.id
        );
    }
}

#[test]
fn side_mdat_orders_multiple_positions_and_unplanned_roles_like_upstream() {
    let fixture = secs_oracle::Fixture::load("service_mdat", "side_multi_unplanned");
    let directory = TempDir::new("side-multi");
    let target = directory.0.join("multi.secs");
    let project = Project::from_preset("7.1.4").unwrap();
    let mut store = MeasurementStore::new(&target, project);
    for (pos, role, samples, rate, t0, offset) in [
        (2, "Z", vec![0.25, -0.5, 1.0], 44_100, 0.002, None),
        (
            1,
            "SW1",
            vec![1.0, 0.0, -0.25],
            48_000,
            -0.001,
            Some(116.25),
        ),
        (1, "Tfl", vec![0.5, 0.25, 0.0], 48_000, 0.0, Some(117.5)),
    ] {
        store
            .record(
                MeasurementResult {
                    role: role.into(),
                    pos,
                    ir: samples,
                    sample_rate: rate,
                    t0_s: t0,
                    spl_db: None,
                    spl_offset_db: offset,
                    snr_db: None,
                    uuid: Some(format!("{role}-{pos}")),
                    source: None,
                    source_info: None,
                },
                Some(&["Tfl".into(), "SW1".into()]),
            )
            .unwrap();
    }
    let mut next = 1_u128;
    let deps = secs_service::Deps {
        uuid: std::sync::Arc::new(std::sync::Mutex::new(Box::new(move || {
            let value = next;
            next += 1;
            Ok(value)
        }))),
        unix_ms: std::sync::Arc::new(|| 1_704_164_645_000),
        ..Default::default()
    };
    let service = secs_service::Service::new(deps);
    assert!(matches!(
        service.call(
            secs_types::ipc::Request::ProjectOpen(secs_types::ipc::ProjectPathParams {
                path: target.to_string_lossy().into_owned()
            }),
            std::sync::Arc::new(secs_service::NoEvents)
        ),
        secs_types::ipc::Envelope::Ok(_)
    ));
    assert!(matches!(
        service.call(
            secs_types::ipc::Request::ProjectSave(Default::default()),
            std::sync::Arc::new(secs_service::NoEvents)
        ),
        secs_types::ipc::Envelope::Ok(_)
    ));
    assert_eq!(
        std::fs::read(target.with_extension("mdat")).unwrap(),
        fixture.blob(&fixture.outputs()["side_mdat"])
    );
}
#[test]
fn test_load_mdat_assigns_roles_by_title_and_preserves_other_positions() {
    let dir = TempDir::new("mdat-merge");
    let path = dir.0.join("merge.secs");
    let mut store = MeasurementStore::new(&path, Project::from_preset("2.1").unwrap());
    let source = root().join("oracle/goldens/mdat/build_3_96000_bytes.bin");
    let picks = [
        Assignment {
            role: Some("L".into()),
            position: Some(1),
        },
        Assignment {
            role: Some("R".into()),
            position: Some(2),
        },
        Assignment {
            role: Some("SW1".into()),
            position: Some(1),
        },
    ];
    assert_eq!(import(&mut store, &source, Some(&picks), None).unwrap(), 3);
    let previous = store.entry_for(2).unwrap();
    let picks = [Assignment {
        role: Some("L".into()),
        position: Some(1),
    }];
    assert_eq!(import(&mut store, &source, Some(&picks), None).unwrap(), 1);
    assert_eq!(store.entry_for(2).unwrap(), previous);
    assert!(store.entry_for(1).unwrap().ir.contains_key("SW1"));
    assert_eq!(mdat_sources(&store).len(), 2);
    assert_eq!(
        store.entry_for(1).unwrap().source["L"],
        "build_3_96000_bytes.bin"
    );
    assert!(store.clear_position(1));
    store.save().unwrap();
    assert_eq!(store.records().len(), 1);
}
#[test]
fn test_mdat_titles_match_biamp_roles() {
    let roles = ["L:LF", "L:HF", "R:LF", "R:HF", "SW1"].map(String::from);
    assert_eq!(
        match_role("L Fin", &roles, &BTreeSet::new()).as_deref(),
        Some("L:LF")
    );
    assert_eq!(
        match_role("L Fin 2", &roles, &BTreeSet::from(["L:LF".into()])).as_deref(),
        Some("L:HF")
    );
    assert_eq!(
        match_role("R HF sweep", &roles, &BTreeSet::new()).as_deref(),
        Some("R:HF")
    );
}
#[test]
fn test_mdat_cancel_and_missing_file_leave_measurements_unchanged() {
    let directory = TempDir::new("mdat-missing");
    let mut store = MeasurementStore::new(
        directory.0.join("missing.secs"),
        Project::from_preset("2.1").unwrap(),
    );
    let before = store.project().save_text();
    assert!(import(&mut store, &root().join("missing.mdat"), None, None).is_err());
    assert_eq!(store.project().save_text(), before);
}
#[test]
fn test_mdat_dialog_repeated_duplicate_skip_and_cancel() {
    for _ in 0..3 {
        assert!(!problems(&[None, None], &[1, 1]).is_empty());
        assert!(!problems(&[Some("L".into()), Some("L".into())], &[1, 1]).is_empty());
        assert!(problems(&[Some("L".into()), Some("L".into())], &[1, 2]).is_empty());
    }
}
