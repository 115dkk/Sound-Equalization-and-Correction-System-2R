#![forbid(unsafe_code)]
use secs_project::{MeasurementStore, Project, PyJson};
use secs_service::mdat_import::*;
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::PathBuf};
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
        } else {
            let source = root().join(input["path"].as_str().unwrap());
            let scan = scan_for(&source, &strings(&input["roles"]), None, 1).unwrap();
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
            let temp = root().join(format!("target/service-mdat-{}-{name}", std::process::id()));
            std::fs::create_dir_all(&temp).unwrap();
            let target = temp.join("import.secs");
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
            std::fs::remove_dir_all(temp).unwrap();
        }
        count += 1;
    }
    assert_eq!(count, 23);
}
#[test]
fn test_load_mdat_assigns_roles_by_title_and_preserves_other_positions() {
    let dir = root().join(format!("target/mdat-merge-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("merge.secs");
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
    std::fs::remove_dir_all(dir).unwrap();
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
    let mut store = MeasurementStore::new(
        root().join("target/missing.secs"),
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
