//! One import flow for measurement files, over two format readers: REW
//! `.mdat` (port of SECS 2 `step5.load_mdat`, GPL-3.0-only) and Dirac Live
//! `.liveproject` (new in SECS 2R, ADR 0013).
//!
//! A request reads the file once into a reader; the assignment dialog shows
//! the reader's rows, and the import records the assigned rows into the
//! store with the file name as their source.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

use secs_mdat::SourceInfo;
use secs_project::{MeasurementResult, MeasurementStore};

use crate::measure::{MeasureError, MeasurementSettings, plan_for, position_count};
use crate::{dirac_import, mdat_import};

/// The largest measurement file a request reads, before the format
/// parser's own budgets.
const FILE_LIMIT: u64 = 256 * 1024 * 1024;

/// A row's role and position as the dialog (or a caller) assigned them.
/// Per-row positions let the dialog override the file's suggestion.
#[derive(Debug, Clone)]
pub struct Assignment {
    pub role: Option<String>,
    /// `None` keeps the row's suggested position.
    pub position: Option<i64>,
}

/// One row of a measurement file as the assignment dialog shows it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FileRow {
    pub title: String,
    /// Suggested, 1-based.
    pub position: i64,
    /// Suggested.
    pub role: Option<String>,
    /// The low-band classifier's verdict; `None` when the row has no IR,
    /// which the import then skips (upstream shows it as unreadable).
    pub looks_like_sub: Option<bool>,
    /// Whether the title itself names the position.
    pub position_tagged: bool,
}

/// The IR one row holds, ready to record.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ImportedIr {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub t0_s: f64,
    pub uuid: String,
    pub source_info: Option<SourceInfo>,
}

/// A format reader: the rows of a file and the IR of each row.
pub(crate) trait MeasurementReader {
    fn rows(&self) -> &[FileRow];
    /// The IR, its sample rate, t0 and the record's uuid and source info.
    fn take(&self, row: usize) -> Result<ImportedIr, MeasureError>;
}

/// Reads `path` whole, refusing a file over [`FILE_LIMIT`]; `what` names
/// the format in that refusal.
pub(crate) fn read(path: &Path, what: &str) -> Result<Vec<u8>, MeasureError> {
    let file = std::fs::File::open(path).map_err(|error| MeasureError::Input(error.to_string()))?;
    let mut bytes = Vec::new();
    file.take(FILE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| MeasureError::Input(error.to_string()))?;
    if bytes.len() as u64 > FILE_LIMIT {
        return Err(MeasureError::Input(format!("{what} exceeds file budget")));
    }
    Ok(bytes)
}

/// Reads the file once (256 MiB limit) and picks the reader by extension:
/// `.liveproject` is Dirac Live, anything else REW mdat. `roles` are the
/// roles to suggest; `position` puts every mdat row there, and `current`
/// is the position for mdat titles that name none (upstream
/// `current_position`). Dirac rows always carry their own position.
pub(crate) fn open(
    path: &Path,
    roles: &[String],
    position: Option<i64>,
    current: i64,
) -> Result<Box<dyn MeasurementReader>, MeasureError> {
    if dirac_import::is_liveproject(path) {
        Ok(Box::new(dirac_import::DiracReader::read(path, roles)?))
    } else {
        Ok(Box::new(mdat_import::MdatReader::read(
            path, roles, position, current,
        )?))
    }
}

/// Validates assignments (one rule: role in the plan, `1..=position_count`,
/// one record per (position, role)), records every IR with its source and
/// saves once per IR (C6), and returns the count.
///
/// There is one assignment per row; a different count is an error and
/// changes nothing. A row that breaks the rule, has no role or has no IR is
/// skipped, as upstream `step5.load_mdat` skips it (upstream lets a position
/// below 1 through: docs/DEVIATIONS.md F18); a skipped row does not claim
/// its (position, role). A failing record stops the import; the IRs
/// recorded before it stay on disk.
pub(crate) fn import(
    store: &mut MeasurementStore,
    reader: &dyn MeasurementReader,
    file_name: &str,
    assignments: &[Assignment],
) -> Result<usize, MeasureError> {
    let rows = reader.rows();
    if assignments.len() != rows.len() {
        return Err(MeasureError::Input(
            "assignment count does not match the file's measurement rows".into(),
        ));
    }
    let plan = plan_for(store.project(), &MeasurementSettings::default());
    let count = position_count(store.project());
    let mut used = BTreeSet::new();
    let mut saved = 0;
    for (index, (row, assignment)) in rows.iter().zip(assignments).enumerate() {
        if row.looks_like_sub.is_none() {
            continue;
        }
        let Some(role) = assignment
            .role
            .as_ref()
            .filter(|role| plan.roles.contains(role))
        else {
            continue;
        };
        let position = assignment.position.unwrap_or(row.position);
        if !(1..=count).contains(&position) || !used.insert((position, role.clone())) {
            continue;
        }
        let ir = reader.take(index)?;
        store.record(
            MeasurementResult {
                role: role.clone(),
                pos: position,
                ir: ir.samples,
                sample_rate: ir.sample_rate,
                t0_s: ir.t0_s,
                spl_db: None,
                spl_offset_db: None,
                snr_db: None,
                uuid: Some(ir.uuid),
                source: Some(file_name.to_owned()),
                source_info: ir.source_info,
            },
            Some(&plan.roles),
        )?;
        saved += 1;
    }
    Ok(saved)
}

/// The name a record keeps as its source: the file name of `path`.
pub(crate) fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use secs_project::Project;
    use serde_json::{Value, json};

    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "secs-service-measurement-file-{}-{}",
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

    /// Rows without a file; `take` notes which rows the import asked for.
    struct FakeReader {
        rows: Vec<FileRow>,
        taken: RefCell<Vec<usize>>,
    }

    impl FakeReader {
        /// `(title, suggested position, has an IR)` per row.
        fn new(rows: &[(&str, i64, bool)]) -> Self {
            Self {
                rows: rows
                    .iter()
                    .map(|(title, position, has_ir)| FileRow {
                        title: (*title).into(),
                        position: *position,
                        role: None,
                        looks_like_sub: has_ir.then_some(false),
                        position_tagged: true,
                    })
                    .collect(),
                taken: RefCell::default(),
            }
        }
    }

    impl MeasurementReader for FakeReader {
        fn rows(&self) -> &[FileRow] {
            &self.rows
        }

        fn take(&self, row: usize) -> Result<ImportedIr, MeasureError> {
            self.taken.borrow_mut().push(row);
            Ok(ImportedIr {
                samples: vec![1.0, -0.5, row as f32 / 10.0],
                sample_rate: 48_000,
                t0_s: -(row as f64) / 1000.0,
                uuid: format!("row-{row}"),
                source_info: None,
            })
        }
    }

    fn assign(role: Option<&str>, position: Option<i64>) -> Assignment {
        Assignment {
            role: role.map(str::to_owned),
            position,
        }
    }

    /// A 2.1 project (roles L, R, SW1) with the default nine positions.
    fn store(directory: &TempDir) -> MeasurementStore {
        let project = Project::from_preset("2.1").unwrap();
        let plan = plan_for(&project, &MeasurementSettings::default());
        assert_eq!(plan.roles, ["L", "R", "SW1"]);
        assert_eq!(position_count(&project), 9);
        MeasurementStore::new(directory.0.join("room.secs"), project)
    }

    #[test]
    fn import_skips_rows_outside_the_layout_the_plan_or_the_once_rule_f18() {
        let directory = TempDir::new();
        let mut store = store(&directory);
        let reader = FakeReader::new(&[
            ("L P1", 1, true),
            ("R P0", 1, true),
            ("R P10", 1, true),
            ("L again", 1, true),
            ("Z", 1, true),
            ("unassigned", 1, true),
            ("unreadable", 1, false),
            ("R P1", 1, true),
            ("SW1 P2", 2, true),
        ]);
        let assignments = [
            assign(Some("L"), Some(1)),
            // Below 1 (F18) and beyond the nine positions.
            assign(Some("R"), Some(0)),
            assign(Some("R"), Some(10)),
            // (1, L) is taken.
            assign(Some("L"), Some(1)),
            // Not a role of the plan.
            assign(Some("Z"), Some(1)),
            assign(None, Some(1)),
            // No IR: skipped without claiming (1, R).
            assign(Some("R"), Some(1)),
            assign(Some("R"), Some(1)),
            // No position: the row's suggestion.
            assign(Some("SW1"), None),
        ];

        let count = import(&mut store, &reader, "room.mdat", &assignments).unwrap();

        assert_eq!(count, 3);
        assert_eq!(*reader.taken.borrow(), [0, 7, 8]);
        assert_eq!(
            store
                .records()
                .iter()
                .map(|record| (record.pos, record.ir.keys().cloned().collect::<Vec<_>>()))
                .collect::<Vec<_>>(),
            [
                (1, vec!["L".to_owned(), "R".to_owned()]),
                (2, vec!["SW1".to_owned()])
            ]
        );
        assert_eq!(
            store.entry_for(1).unwrap().uuid["R"].as_deref(),
            Some("row-7")
        );
    }

    #[test]
    fn import_records_each_ir_with_its_source_and_saves_once_per_ir() {
        let directory = TempDir::new();
        let mut store = store(&directory);
        let reader = FakeReader::new(&[("L", 1, true), ("R", 1, true), ("SW1", 2, true)]);
        let assignments = [
            assign(Some("L"), Some(1)),
            assign(Some("R"), Some(1)),
            assign(Some("SW1"), Some(2)),
        ];
        let saves = store.save_count();

        assert_eq!(
            import(&mut store, &reader, "room.mdat", &assignments).unwrap(),
            3
        );

        assert_eq!(store.save_count(), saves + 3);
        let saved = MeasurementStore::open(directory.0.join("room.secs")).unwrap();
        assert_eq!(saved.project().save_text(), store.project().save_text());
        for (role, position, row) in [("L", 1, 0), ("R", 1, 1), ("SW1", 2, 2)] {
            let entry = saved.entry_for(position).unwrap();
            assert_eq!(entry.source[role], "room.mdat");
            assert_eq!(
                entry.uuid[role].as_deref(),
                Some(format!("row-{row}").as_str())
            );
            assert_eq!(entry.t0[role], -(row as f64) / 1000.0);
        }
    }

    #[test]
    fn import_refuses_an_assignment_count_that_does_not_match_the_rows() {
        let directory = TempDir::new();
        let mut store = store(&directory);
        let before = store.project().save_text();
        let reader = FakeReader::new(&[("L", 1, true), ("R", 1, true)]);

        for assignments in [
            vec![assign(Some("L"), Some(1))],
            vec![
                assign(Some("L"), Some(1)),
                assign(Some("R"), Some(1)),
                assign(Some("SW1"), Some(1)),
            ],
        ] {
            assert!(matches!(
                import(&mut store, &reader, "room.mdat", &assignments),
                Err(MeasureError::Input(_))
            ));
        }

        assert!(reader.taken.borrow().is_empty());
        assert_eq!(store.save_count(), 0);
        assert_eq!(store.project().save_text(), before);
        assert!(!directory.0.join("room.secs").exists());
    }

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn strings(value: &Value) -> Vec<String> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn mdat_reader_rows_are_the_upstream_scan_of_every_sample_file() {
        let fixtures = secs_oracle::Fixture::all("service_mdat", "file_");
        assert_eq!(fixtures.len(), 5);
        for fixture in fixtures {
            let (input, out) = (fixture.inputs(), fixture.outputs());
            let source = root().join(input["path"].as_str().unwrap());
            let reader = open(&source, &strings(&input["roles"]), None, 1).unwrap();
            let rows = reader.rows();
            let id = &fixture.id;
            assert_eq!(
                json!(rows.iter().map(|row| &row.title).collect::<Vec<_>>()),
                out["titles"],
                "{id}"
            );
            assert_eq!(
                json!(rows.iter().map(|row| row.position).collect::<Vec<_>>()),
                json!(
                    out["split"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|split| &split[1])
                        .collect::<Vec<_>>()
                ),
                "{id}"
            );
            assert_eq!(
                json!(rows.iter().map(|row| &row.role).collect::<Vec<_>>()),
                out["assignment"],
                "{id}"
            );
            assert_eq!(
                json!(
                    rows.iter()
                        .map(|row| row.looks_like_sub)
                        .collect::<Vec<_>>()
                ),
                out["flags"],
                "{id}"
            );
            assert_eq!(
                json!(
                    rows.iter()
                        .map(|row| row.position_tagged)
                        .collect::<Vec<_>>()
                ),
                out["position_tagged"],
                "{id}"
            );
        }
    }

    #[test]
    fn dirac_reader_rows_are_the_scan_of_the_sample_project() {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../secs-dirac/tests/data/minimal.liveproject");
        let roles = secs_types::role::ROLES.map(str::to_owned);
        let reader = open(&source, &roles, Some(5), 3).unwrap();
        let row = |title: &str, position, role: &str| FileRow {
            title: title.into(),
            position,
            role: Some(role.into()),
            looks_like_sub: Some(false),
            position_tagged: true,
        };
        assert_eq!(
            reader.rows(),
            [
                row("Left P1", 1, "L"),
                row("Subwoofer P1", 1, "SW1"),
                row("Left P2", 2, "L"),
            ]
        );
        let project =
            secs_dirac::parse(&std::fs::read(&source).unwrap(), &Default::default()).unwrap();
        let ir = reader.take(2).unwrap();
        assert_eq!(
            ir.samples,
            project
                .response(1, 0)
                .unwrap()
                .iter()
                .map(|sample| *sample as f32)
                .collect::<Vec<_>>()
        );
        assert_eq!((ir.sample_rate, ir.t0_s), (48_000, -0.25));
        assert_eq!(ir.source_info, None);
    }
}
