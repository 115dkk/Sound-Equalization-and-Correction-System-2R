use crate::{
    MAX_IR_BYTES, Project, ProjectError, PyJson, dumps_compact, ir_directory, ir_file, loads,
    read_mono_float_wav, save_ir,
};
use secs_mdat::{Boxed, CalInfo, SourceDate, SourceInfo};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

pub const IR_CACHE_LIMIT_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_PROJECT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct MeasurementResult {
    pub role: String,
    pub pos: i64,
    pub ir: Vec<f32>,
    pub sample_rate: u32,
    pub t0_s: f64,
    pub spl_db: Option<f64>,
    pub spl_offset_db: Option<f64>,
    pub snr_db: Option<f64>,
    pub uuid: Option<String>,
    pub source: Option<String>,
    pub source_info: Option<SourceInfo>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeasurementRecord {
    pub pos: i64,
    pub label: String,
    pub uuid: BTreeMap<String, Option<String>>,
    pub mdat: Option<String>,
    pub ir: BTreeMap<String, String>,
    pub spl_db: BTreeMap<String, Option<f64>>,
    pub t0: BTreeMap<String, f64>,
    pub spl_offset: BTreeMap<String, f64>,
    pub snr_db: BTreeMap<String, f64>,
    pub source: BTreeMap<String, String>,
    pub done: bool,
    pub sample_rate: Option<u32>,
}

impl MeasurementRecord {
    fn from_json(value: &PyJson) -> Option<Self> {
        let object = value.object()?;
        let get = |key: &str| {
            object
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value)
        };
        let pos = crate::project::integer(Some(get("pos")?), -1, "measurement.pos").ok()?;
        Some(Self {
            pos,
            label: crate::project::string(get("label"), &format!("위치 {pos}")),
            uuid: optional_string_map(get("uuid")),
            mdat: get("mdat").and_then(PyJson::str).map(str::to_owned),
            ir: string_map(get("ir")),
            spl_db: optional_float_map(get("spl_db")),
            t0: float_map(get("t0")),
            spl_offset: float_map(get("spl_offset")),
            snr_db: float_map(get("snr_db")),
            source: string_map(get("source")),
            done: get("done").and_then(PyJson::bool).unwrap_or(false),
            sample_rate: get("sample_rate")
                .and_then(|value| crate::project::integer(Some(value), 0, "sample_rate").ok())
                .and_then(|value| u32::try_from(value).ok()),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationWarning {
    pub code: &'static str,
    pub entry: String,
}

#[derive(Debug)]
pub struct MeasurementStore {
    path: PathBuf,
    project: Project,
    snr_db: BTreeMap<(i64, String), f64>,
    warnings: Vec<MigrationWarning>,
    cache: IrCache,
}

impl MeasurementStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, ProjectError> {
        let path = path.into();
        let bytes = crate::irs::read_bounded(&path, MAX_PROJECT_BYTES, "project")?;
        let text = String::from_utf8(bytes).map_err(|error| {
            ProjectError::Invalid(format!("project {} is not UTF-8: {error}", path.display()))
        })?;
        let (project, warnings) = Project::from_json_with_warnings(loads(&text)?)?;
        let mut store = Self::new(path, project);
        store.warnings.extend(warnings);
        Ok(store)
    }

    pub fn new(path: impl Into<PathBuf>, project: Project) -> Self {
        Self {
            path: path.into(),
            project,
            snr_db: BTreeMap::new(),
            warnings: Vec::new(),
            cache: IrCache::default(),
        }
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn project_mut(&mut self) -> &mut Project {
        &mut self.project
    }

    pub fn records(&self) -> Vec<MeasurementRecord> {
        measurement_values(&self.project)
            .iter()
            .filter_map(MeasurementRecord::from_json)
            .map(|mut record| {
                record.snr_db.extend(
                    self.snr_db
                        .iter()
                        .filter(|((pos, _), _)| *pos == record.pos)
                        .map(|((_, role), value)| (role.clone(), *value)),
                );
                record
            })
            .collect()
    }

    pub fn entry_for(&self, pos: i64) -> Option<MeasurementRecord> {
        let mut record = measurement_values(&self.project)
            .iter()
            .find(|entry| pos_of(entry) == Some(pos))
            .and_then(MeasurementRecord::from_json)?;
        record.snr_db.extend(
            self.snr_db
                .iter()
                .filter(|((entry_pos, _), _)| *entry_pos == pos)
                .map(|((_, role), value)| (role.clone(), *value)),
        );
        Some(record)
    }

    pub fn done_positions(&self, roles: Option<&[String]>) -> Vec<i64> {
        let needed = roles.map(|roles| roles.iter().map(String::as_str).collect::<BTreeSet<_>>());
        let mut out = self
            .records()
            .into_iter()
            .filter(|record| match &needed {
                Some(needed) => needed
                    .iter()
                    .all(|role| record.ir.get(*role).is_some_and(|path| !path.is_empty())),
                None => record.done,
            })
            .map(|record| record.pos)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        out.sort_unstable();
        out
    }

    pub fn pending_positions(&self, positions: &[i64], roles: &[String]) -> Vec<i64> {
        let done = self
            .done_positions(Some(roles))
            .into_iter()
            .collect::<BTreeSet<_>>();
        positions
            .iter()
            .copied()
            .filter(|pos| !done.contains(pos))
            .collect()
    }

    pub fn label_for(&self, pos: i64) -> String {
        format!("위치 {pos}")
    }

    pub fn start_position(&mut self, pos: i64) -> MeasurementRecord {
        let entry = new_entry(pos, self.label_for(pos));
        let records = measurement_values_mut(&mut self.project);
        if let Some(index) = records.iter().position(|entry| pos_of(entry) == Some(pos)) {
            records[index] = entry;
        } else {
            records.push(entry);
        }
        self.snr_db.retain(|(entry_pos, _), _| *entry_pos != pos);
        self.entry_for(pos).expect("new entry has a valid position")
    }

    pub fn record(
        &mut self,
        result: MeasurementResult,
        plan_roles: Option<&[String]>,
    ) -> Result<MeasurementRecord, ProjectError> {
        let path = save_ir(
            &self.path,
            &result.role,
            result.pos,
            &result.ir,
            result.sample_rate,
        )?;
        save_source(&path, result.source_info.as_ref())?;
        self.cache.remember(&path, &result.ir, result.sample_rate)?;
        let relative = relative_to_project(&self.path, &path);
        let label = self.label_for(result.pos);
        let role = result.role.clone();
        let records = measurement_values_mut(&mut self.project);
        let index = records
            .iter()
            .position(|entry| pos_of(entry) == Some(result.pos))
            .unwrap_or_else(|| {
                records.push(new_entry(result.pos, label.clone()));
                records.len() - 1
            });
        let entry = records[index]
            .object_mut()
            .expect("new and matched measurement entries are objects");
        repair_entry(entry, result.pos, label);
        set_nested(
            entry,
            "uuid",
            &role,
            result.uuid.map_or(PyJson::Null, PyJson::Str),
        );
        set_nested(entry, "ir", &role, PyJson::Str(relative));
        set_nested(
            entry,
            "spl_db",
            &role,
            result.spl_db.map_or(PyJson::Null, PyJson::Float),
        );
        set_nested(entry, "t0", &role, PyJson::Float(result.t0_s));
        if let Some(value) = result.spl_offset_db {
            setdefault_nested(entry, "spl_offset", &role, PyJson::Float(value));
        }
        set_value(
            entry,
            "sample_rate",
            PyJson::Int(result.sample_rate.to_string()),
        );
        let done = plan_roles.map_or_else(
            || nested_object(entry, "ir").is_some_and(|values| !values.is_empty()),
            |roles| {
                roles.iter().all(|role| {
                    nested_object(entry, "ir")
                        .and_then(|values| value_for(values, role))
                        .is_some_and(py_truthy)
                })
            },
        );
        set_value(entry, "done", PyJson::Bool(done));
        if let Some(value) = result.snr_db {
            self.snr_db.insert((result.pos, role), value);
        }
        self.save()?;
        Ok(self
            .entry_for(result.pos)
            .expect("recorded entry has a valid position"))
    }

    pub fn clear_position(&mut self, pos: i64) -> bool {
        let records = measurement_values_mut(&mut self.project);
        let before = records.len();
        records.retain(|entry| pos_of_or_minus_one(entry) != pos);
        let changed = before != records.len();
        if changed {
            self.snr_db.retain(|(entry_pos, _), _| *entry_pos != pos);
        }
        changed
    }

    pub fn ir_path_of(&mut self, role: &str, pos: i64) -> Result<PathBuf, ProjectError> {
        let raw = self
            .entry_for(pos)
            .and_then(|entry| entry.ir.get(role).cloned())
            .filter(|path| !path.is_empty());
        let candidate = raw.as_ref().map_or_else(
            || ir_file(&self.path, role, pos),
            |raw| {
                let raw = PathBuf::from(raw);
                if raw.is_absolute() {
                    raw
                } else {
                    self.path
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .join(raw)
                }
            },
        );
        if confined_ir_path(&self.path, &candidate).is_err() {
            let entry = format!("position {pos}, role {role}");
            if !self.warnings.iter().any(|warning| warning.entry == entry) {
                self.warnings.push(MigrationWarning {
                    code: "measurement.ir_path.outside",
                    entry: entry.clone(),
                });
            }
            return Err(ProjectError::IrPath {
                entry,
                path: candidate,
            });
        }
        Ok(candidate)
    }

    pub fn ir_of(&mut self, role: &str, pos: i64) -> Result<(Vec<f32>, u32), ProjectError> {
        let path = self.ir_path_of(role, pos)?;
        self.cache.load(&path)
    }

    pub fn t0_of(&self, role: &str, pos: i64) -> f64 {
        self.entry_for(pos)
            .and_then(|entry| entry.t0.get(role).copied())
            .unwrap_or(0.0)
    }

    pub fn t0_by_role(&self, pos: i64) -> BTreeMap<String, f64> {
        self.entry_for(pos).map_or_else(BTreeMap::new, |entry| {
            entry
                .ir
                .iter()
                .filter(|(_, path)| !path.is_empty())
                .map(|(role, _)| (role.clone(), entry.t0.get(role).copied().unwrap_or(0.0)))
                .collect()
        })
    }

    pub fn irs_by_role(
        &mut self,
        pos: i64,
    ) -> Result<BTreeMap<String, (Vec<f32>, u32)>, ProjectError> {
        let roles = self.entry_for(pos).map_or_else(Vec::new, |entry| {
            entry
                .ir
                .iter()
                .filter(|(_, path)| !path.is_empty())
                .map(|(role, _)| role.clone())
                .collect()
        });
        roles
            .into_iter()
            .map(|role| self.ir_of(&role, pos).map(|value| (role, value)))
            .collect()
    }

    pub fn warnings(&self) -> &[MigrationWarning] {
        &self.warnings
    }

    pub fn save(&mut self) -> Result<(), ProjectError> {
        atomic_write(&self.path, self.project.save_text().as_bytes())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoadedIr {
    Samples(Vec<f32>, u32),
    Timed(Vec<f32>, u32, f64),
}

pub fn source_path(wav: &Path) -> PathBuf {
    wav.with_extension("src.json")
}

pub fn save_source(wav: &Path, info: Option<&SourceInfo>) -> Result<(), ProjectError> {
    let path = source_path(wav);
    let Some(info) = info else {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        return Ok(());
    };
    atomic_write(&path, dumps_compact(&source_to_json(info)).as_bytes())
}

pub fn load_source(wav: &Path) -> Option<SourceInfo> {
    let text = fs::read_to_string(source_path(wav)).ok()?;
    let value = loads(&text).ok()?;
    source_from_json(&value)
}

pub fn load_project_sources(
    project: &Project,
    project_path: &Path,
) -> BTreeMap<u32, BTreeMap<String, SourceInfo>> {
    let base = project_path.parent().unwrap_or_else(|| Path::new("."));
    let mut out = BTreeMap::new();
    for record in measurement_values(project)
        .iter()
        .filter_map(MeasurementRecord::from_json)
    {
        let Ok(pos) = u32::try_from(record.pos) else {
            continue;
        };
        for (role, raw) in record.ir {
            if raw.is_empty() {
                continue;
            }
            let path = PathBuf::from(raw);
            let path = if path.is_absolute() {
                path
            } else {
                base.join(path)
            };
            // F5: neither the IR nor its sidecar may lead outside the `.irs`.
            if confined_ir_path(project_path, &path).is_err()
                || confined_ir_path(project_path, &source_path(&path)).is_err()
            {
                continue;
            }
            if let Some(info) = load_source(&path) {
                out.entry(pos)
                    .or_insert_with(BTreeMap::new)
                    .insert(role, info);
            }
        }
    }
    out
}

pub fn load_project_irs(
    project: &Project,
    project_path: &Path,
    cache: &mut IrCache,
) -> Result<BTreeMap<i64, BTreeMap<String, LoadedIr>>, ProjectError> {
    let mut store = MeasurementStore::new(project_path, project.clone());
    std::mem::swap(&mut store.cache, cache);
    let mut out = BTreeMap::new();
    let positions = store
        .records()
        .iter()
        .map(|record| record.pos)
        .collect::<Vec<_>>();
    for pos in positions {
        let roles = store.entry_for(pos).map_or_else(Vec::new, |entry| {
            entry
                .ir
                .iter()
                .filter(|(_, path)| !path.is_empty())
                .map(|(role, _)| role.clone())
                .collect()
        });
        for role in roles {
            let (samples, rate) = store.ir_of(&role, pos)?;
            let t0 = store.t0_of(&role, pos);
            let value = if t0 == 0.0 {
                LoadedIr::Samples(samples, rate)
            } else {
                LoadedIr::Timed(samples, rate, t0)
            };
            out.entry(pos)
                .or_insert_with(BTreeMap::new)
                .insert(role, value);
        }
    }
    std::mem::swap(&mut store.cache, cache);
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CacheKey {
    path: PathBuf,
    modified_ns: u128,
    size: u64,
}

#[derive(Debug, Clone)]
struct CacheValue {
    samples: Vec<f32>,
    sample_rate: u32,
}

#[derive(Debug)]
pub struct IrCache {
    entries: BTreeMap<CacheKey, CacheValue>,
    order: VecDeque<CacheKey>,
    bytes: usize,
    cap: usize,
}

impl Default for IrCache {
    fn default() -> Self {
        Self::new(IR_CACHE_LIMIT_BYTES)
    }
}

impl IrCache {
    pub fn new(cap: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            cap,
        }
    }

    pub fn load(&mut self, path: &Path) -> Result<(Vec<f32>, u32), ProjectError> {
        let key = cache_key(path)?;
        self.remove_old_versions(&key.path, Some(&key));
        if let Some(value) = self.entries.get(&key) {
            let hit = (value.samples.clone(), value.sample_rate);
            if let Some(index) = self.order.iter().position(|old_key| old_key == &key) {
                let touched = self.order.remove(index).expect("index from position");
                self.order.push_back(touched);
            }
            return Ok(hit);
        }
        let (samples, sample_rate) =
            read_mono_float_wav(&crate::irs::read_bounded(path, MAX_IR_BYTES, "WAV")?)
                .map_err(|error| ProjectError::Wav(format!("{}: {error}", path.display())))?;
        self.insert(key, &samples, sample_rate);
        Ok((samples, sample_rate))
    }

    pub fn remember(
        &mut self,
        path: &Path,
        samples: &[f32],
        sample_rate: u32,
    ) -> Result<(), ProjectError> {
        let key = cache_key(path)?;
        self.remove_old_versions(&key.path, Some(&key));
        self.insert(key, samples, sample_rate);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Sample bytes the cache holds; never more than its cap.
    pub fn retained_bytes(&self) -> usize {
        self.bytes
    }

    /// Stores a copy unless it alone exceeds the cap, then evicts from the
    /// least recently used end until the cache fits.
    fn insert(&mut self, key: CacheKey, samples: &[f32], sample_rate: u32) {
        let size = samples.len().saturating_mul(std::mem::size_of::<f32>());
        if let Some(old) = self.entries.remove(&key) {
            self.bytes = self
                .bytes
                .saturating_sub(old.samples.len().saturating_mul(4));
            self.order.retain(|old_key| old_key != &key);
        }
        if size > self.cap {
            return;
        }
        self.bytes = self.bytes.saturating_add(size);
        self.order.push_back(key.clone());
        self.entries.insert(
            key,
            CacheValue {
                samples: samples.to_vec(),
                sample_rate,
            },
        );
        while self.bytes > self.cap
            && let Some(old_key) = self.order.pop_front()
        {
            if let Some(old) = self.entries.remove(&old_key) {
                self.bytes = self
                    .bytes
                    .saturating_sub(old.samples.len().saturating_mul(4));
            }
        }
    }

    fn remove_old_versions(&mut self, path: &Path, keep: Option<&CacheKey>) {
        let old = self
            .entries
            .keys()
            .filter(|key| key.path == path && keep != Some(*key))
            .cloned()
            .collect::<Vec<_>>();
        for key in old {
            if let Some(value) = self.entries.remove(&key) {
                self.bytes = self
                    .bytes
                    .saturating_sub(value.samples.len().saturating_mul(4));
            }
            self.order.retain(|old_key| old_key != &key);
        }
    }
}

const SOURCE_STRING_FIELDS: [&str; 7] = [
    "outputName",
    "sourceFileName",
    "timingRefName",
    "osName",
    "osVersion",
    "jre",
    "sigGenLevelSt",
];
const SOURCE_BOXED_FIELDS: [&str; 8] = [
    "inputVolume",
    "outputVolume",
    "micSensitivity",
    "splCalOffset",
    "dBVInputOffset",
    "dBVOutputOffset",
    "usesEmbeddedCalData",
    "calDataLimitApplied",
];
const SOURCE_CAL_FIELDS: [&str; 2] = ["meterCal", "scCal"];
const SOURCE_CAL_BOXED: [&str; 4] = ["sensitivitydB", "sensitivitydBFS", "serialNum", "revision"];

fn source_to_json(info: &SourceInfo) -> PyJson {
    let mut fields = vec![
        (
            "strings".into(),
            PyJson::Object(
                info.strings
                    .iter()
                    .map(|(name, value)| (name.clone(), PyJson::Str(value.clone())))
                    .collect(),
            ),
        ),
        (
            "boxed".into(),
            PyJson::Object(
                info.boxed
                    .iter()
                    .filter(|(_, value)| boxed_is_serializable(*value))
                    .map(|(name, value)| (name.clone(), boxed_to_json(*value)))
                    .collect(),
            ),
        ),
        (
            "cal".into(),
            PyJson::Object(
                info.cal
                    .iter()
                    .map(|(name, cal)| (name.clone(), cal_to_json(cal)))
                    .collect(),
            ),
        ),
    ];
    if let Some(date) = info.date.and_then(serializable_date) {
        fields.push(("date".into(), date));
    }
    if let Some(notes) = &info.notes {
        fields.push(("notes".into(), PyJson::Str(notes.clone())));
    }
    if let Some(format) = &info.format {
        fields.push(("format".into(), PyJson::Str(format.clone())));
    }
    PyJson::Object(fields)
}

fn boxed_is_serializable(value: Boxed) -> bool {
    match value {
        Boxed::Double(value) => value.is_finite(),
        Boxed::Integer(value) => i32::try_from(value).is_ok(),
        Boxed::Boolean(_) => true,
    }
}

fn serializable_date(value: SourceDate) -> Option<PyJson> {
    match value {
        SourceDate::Int(value) => Some(PyJson::Int(value.to_string())),
        SourceDate::Float(value) if value.is_finite() => Some(PyJson::Float(value)),
        SourceDate::Float(_) => None,
    }
}

fn boxed_to_json(value: Boxed) -> PyJson {
    let (kind, value) = match value {
        Boxed::Double(value) => ("java.lang.Double", PyJson::Float(value)),
        Boxed::Integer(value) => ("java.lang.Integer", PyJson::Int(value.to_string())),
        Boxed::Boolean(value) => ("java.lang.Boolean", PyJson::Bool(value)),
    };
    PyJson::Object(vec![
        ("type".into(), PyJson::Str(kind.into())),
        ("value".into(), value),
    ])
}

fn cal_to_json(cal: &CalInfo) -> PyJson {
    let mut fields = vec![
        (
            "inverseCSelected".into(),
            PyJson::Bool(cal.inverse_c_selected),
        ),
        ("sourceName".into(), PyJson::Str(cal.source_name.clone())),
        ("sourcePath".into(), PyJson::Str(cal.source_path.clone())),
        ("freqArray".into(), float_array_to_json(cal.freq.as_deref())),
        ("gainArray".into(), float_array_to_json(cal.gain.as_deref())),
        (
            "phaseArray".into(),
            float_array_to_json(cal.phase.as_deref()),
        ),
    ];
    fields.extend(
        cal.boxed
            .iter()
            .filter(|(_, value)| boxed_is_serializable(*value))
            .map(|(name, value)| (name.clone(), boxed_to_json(*value))),
    );
    PyJson::Object(fields)
}

fn float_array_to_json(values: Option<&[f32]>) -> PyJson {
    values.map_or(PyJson::Null, |values| {
        PyJson::Array(
            values
                .iter()
                .map(|value| PyJson::Float(f64::from(*value)))
                .collect(),
        )
    })
}

fn source_from_json(value: &PyJson) -> Option<SourceInfo> {
    let object = value.object()?;
    let mut info = SourceInfo::default();
    if let Some(strings) = value_for(object, "strings").and_then(PyJson::object) {
        for (name, value) in strings {
            if SOURCE_STRING_FIELDS.contains(&name.as_str()) && !matches!(value, PyJson::Null) {
                info.strings.push((name.clone(), python_str(value)));
            }
        }
    }
    if let Some(boxed) = value_for(object, "boxed").and_then(PyJson::object) {
        for (name, value) in boxed {
            if SOURCE_BOXED_FIELDS.contains(&name.as_str())
                && let Some(value) = boxed_from_json(value)
            {
                info.boxed.push((name.clone(), value));
            }
        }
    }
    if let Some(calibrations) = value_for(object, "cal").and_then(PyJson::object) {
        for (name, value) in calibrations {
            if !SOURCE_CAL_FIELDS.contains(&name.as_str()) {
                continue;
            }
            let Some(cal) = value.object() else {
                continue;
            };
            let mut boxed = Vec::new();
            for key in SOURCE_CAL_BOXED {
                if let Some(value) = value_for(cal, key).and_then(boxed_from_json) {
                    boxed.push((key.into(), value));
                }
            }
            info.cal.push((
                name.clone(),
                CalInfo {
                    inverse_c_selected: value_for(cal, "inverseCSelected").is_some_and(py_truthy),
                    source_name: value_for(cal, "sourceName")
                        .filter(|value| py_truthy(value))
                        .map_or_else(String::new, python_str),
                    source_path: value_for(cal, "sourcePath")
                        .filter(|value| py_truthy(value))
                        .map_or_else(String::new, python_str),
                    freq: value_for(cal, "freqArray").and_then(float_array_from_json),
                    gain: value_for(cal, "gainArray").and_then(float_array_from_json),
                    phase: value_for(cal, "phaseArray").and_then(float_array_from_json),
                    boxed,
                },
            ));
        }
    }
    info.date = value_for(object, "date").and_then(date_from_json);
    info.notes = value_for(object, "notes")
        .filter(|value| py_truthy(value))
        .map(python_str);
    info.format = value_for(object, "format")
        .filter(|value| py_truthy(value))
        .map(python_str);
    Some(info)
}

fn boxed_from_json(value: &PyJson) -> Option<Boxed> {
    let object = value.object()?;
    let kind = value_for(object, "type")?.str()?;
    let raw = value_for(object, "value").filter(|value| !matches!(value, PyJson::Null))?;
    match kind {
        "java.lang.Double" => python_float_value(raw)
            .filter(|value| value.is_finite())
            .map(Boxed::Double),
        "java.lang.Integer" => python_int(raw)
            .filter(|value| i32::try_from(*value).is_ok())
            .map(Boxed::Integer),
        "java.lang.Boolean" => Some(Boxed::Boolean(py_truthy(raw))),
        _ => None,
    }
}

fn date_from_json(value: &PyJson) -> Option<SourceDate> {
    match value {
        PyJson::Bool(value) => Some(SourceDate::Int(i64::from(*value))),
        PyJson::Int(value) => value.parse().ok().map(SourceDate::Int),
        PyJson::Float(value) if value.is_finite() => Some(SourceDate::Float(*value)),
        _ => None,
    }
}

fn float_array_from_json(value: &PyJson) -> Option<Vec<f32>> {
    value
        .array()?
        .iter()
        .map(|value| match value {
            PyJson::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            PyJson::Int(value) => value.parse().ok(),
            PyJson::Float(value) => Some(*value),
            PyJson::Str(value) => value.trim().parse().ok(),
            _ => None,
        })
        .map(|value: Option<f64>| {
            value
                .filter(|value| value.is_finite())
                .map(|value| value as f32)
                .filter(|value| value.is_finite())
        })
        .collect()
}

fn python_float_value(value: &PyJson) -> Option<f64> {
    match value {
        PyJson::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
        PyJson::Int(value) => value.parse().ok(),
        PyJson::Float(value) => Some(*value),
        PyJson::Str(value) => value.trim().parse().ok(),
        _ => None,
    }
}

fn python_int(value: &PyJson) -> Option<i64> {
    match value {
        PyJson::Bool(value) => Some(i64::from(*value)),
        PyJson::Int(value) => value.parse().ok(),
        PyJson::Float(value) if value.is_finite() => {
            let value = value.trunc();
            (value >= i64::MIN as f64 && value < i64::MAX as f64).then_some(value as i64)
        }
        PyJson::Str(value) => value.trim().parse().ok(),
        _ => None,
    }
}

fn python_str(value: &PyJson) -> String {
    match value {
        PyJson::Null => "None".into(),
        PyJson::Bool(value) => if *value { "True" } else { "False" }.into(),
        PyJson::Int(value) => value.clone(),
        PyJson::Float(value) => {
            let text = dumps_compact(&PyJson::Float(*value));
            if text == "Infinity" {
                "inf".into()
            } else if text == "-Infinity" {
                "-inf".into()
            } else if text == "NaN" {
                "nan".into()
            } else {
                text
            }
        }
        PyJson::Str(value) => value.clone(),
        PyJson::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(python_repr)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        PyJson::Object(values) => format!(
            "{{{}}}",
            values
                .iter()
                .map(|(key, value)| format!(
                    "{}: {}",
                    python_repr(&PyJson::Str(key.clone())),
                    python_repr(value)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn python_repr(value: &PyJson) -> String {
    match value {
        PyJson::Str(value) => format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'")),
        _ => python_str(value),
    }
}

fn measurement_values(project: &Project) -> &[PyJson] {
    match &project.measurements {
        PyJson::Array(records) => records,
        _ => &[],
    }
}

fn measurement_values_mut(project: &mut Project) -> &mut Vec<PyJson> {
    if !matches!(project.measurements, PyJson::Array(_)) {
        project.measurements = PyJson::Array(Vec::new());
    }
    match &mut project.measurements {
        PyJson::Array(records) => records,
        _ => unreachable!("measurements was replaced with an array"),
    }
}

fn pos_of(value: &PyJson) -> Option<i64> {
    crate::project::integer(Some(value.get("pos")?), -1, "measurement.pos").ok()
}

fn pos_of_or_minus_one(value: &PyJson) -> i64 {
    let Some(object) = value.object() else {
        return -1;
    };
    let Some(value) = value_for(object, "pos") else {
        return -1;
    };
    crate::project::integer(Some(value), -1, "measurement.pos").unwrap_or(-1)
}

fn new_entry(pos: i64, label: String) -> PyJson {
    PyJson::Object(vec![
        ("pos".into(), PyJson::Int(pos.to_string())),
        ("label".into(), PyJson::Str(label)),
        ("uuid".into(), PyJson::Object(Vec::new())),
        ("mdat".into(), PyJson::Null),
        ("ir".into(), PyJson::Object(Vec::new())),
        ("spl_db".into(), PyJson::Object(Vec::new())),
        ("t0".into(), PyJson::Object(Vec::new())),
        ("done".into(), PyJson::Bool(false)),
        ("sample_rate".into(), PyJson::Null),
    ])
}

fn repair_entry(object: &mut Vec<(String, PyJson)>, pos: i64, label: String) {
    set_value(object, "pos", PyJson::Int(pos.to_string()));
    setdefault_value(object, "label", PyJson::Str(label));
    setdefault_value(object, "mdat", PyJson::Null);
    for key in ["uuid", "ir", "spl_db", "t0"] {
        if !value_for(object, key).is_some_and(|value| matches!(value, PyJson::Object(_))) {
            set_value(object, key, PyJson::Object(Vec::new()));
        }
    }
    if value_for(object, "spl_offset").is_some_and(|value| !matches!(value, PyJson::Object(_))) {
        set_value(object, "spl_offset", PyJson::Object(Vec::new()));
    }
    setdefault_value(object, "done", PyJson::Bool(false));
    setdefault_value(object, "sample_rate", PyJson::Null);
}

fn value_for<'a>(object: &'a [(String, PyJson)], key: &str) -> Option<&'a PyJson> {
    object
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value)
}

fn value_for_mut<'a>(object: &'a mut [(String, PyJson)], key: &str) -> Option<&'a mut PyJson> {
    object
        .iter_mut()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value)
}

fn set_value(object: &mut Vec<(String, PyJson)>, key: &str, value: PyJson) {
    if let Some(old) = value_for_mut(object, key) {
        *old = value;
    } else {
        object.push((key.into(), value));
    }
}

fn setdefault_value(object: &mut Vec<(String, PyJson)>, key: &str, value: PyJson) {
    if value_for(object, key).is_none() {
        object.push((key.into(), value));
    }
}

fn nested_object<'a>(object: &'a [(String, PyJson)], key: &str) -> Option<&'a [(String, PyJson)]> {
    value_for(object, key).and_then(PyJson::object)
}

fn set_nested(object: &mut [(String, PyJson)], key: &str, role: &str, value: PyJson) {
    let nested = value_for_mut(object, key)
        .and_then(PyJson::object_mut)
        .expect("repair_entry made the field an object");
    set_value(nested, role, value);
}

fn setdefault_nested(object: &mut Vec<(String, PyJson)>, key: &str, role: &str, value: PyJson) {
    setdefault_value(object, key, PyJson::Object(Vec::new()));
    let nested = value_for_mut(object, key)
        .and_then(PyJson::object_mut)
        .expect("upstream setdefault requires an object");
    set_value(nested, role, value);
}

fn py_truthy(value: &PyJson) -> bool {
    match value {
        PyJson::Null => false,
        PyJson::Bool(value) => *value,
        PyJson::Int(value) => value != "0",
        PyJson::Float(value) => *value != 0.0,
        PyJson::Str(value) => !value.is_empty(),
        PyJson::Array(value) => !value.is_empty(),
        PyJson::Object(value) => !value.is_empty(),
    }
}

fn cache_key(path: &Path) -> Result<CacheKey, ProjectError> {
    let metadata = fs::metadata(path)?;
    let modified_ns = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Ok(CacheKey {
        path: path.canonicalize()?,
        modified_ns,
        size: metadata.len(),
    })
}

pub(crate) fn atomic_write(path: &Path, data: &[u8]) -> Result<(), ProjectError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    secs_types::fs::write_atomic_bytes(path, data)?;
    Ok(())
}

fn confined_ir_path(project: &Path, candidate: &Path) -> Result<(), ()> {
    if candidate
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err(());
    }
    let root = ir_directory(project);
    let root_canonical = canonicalize_existing_ancestor(&root)?;
    let candidate_canonical = canonicalize_existing_ancestor(candidate)?;
    if candidate_canonical.starts_with(root_canonical) {
        Ok(())
    } else {
        Err(())
    }
}

fn canonicalize_existing_ancestor(path: &Path) -> Result<PathBuf, ()> {
    let mut existing = path;
    let mut missing = Vec::new();
    while !existing.exists() {
        let name = existing.file_name().ok_or(())?;
        missing.push(name.to_owned());
        existing = existing.parent().ok_or(())?;
    }
    let mut canonical = existing.canonicalize().map_err(|_| ())?;
    for name in missing.into_iter().rev() {
        canonical.push(name);
    }
    Ok(canonical)
}

fn relative_to_project(project: &Path, path: &Path) -> String {
    path.strip_prefix(project.parent().unwrap_or_else(|| Path::new(".")))
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn string_map(value: Option<&PyJson>) -> BTreeMap<String, String> {
    value
        .and_then(PyJson::object)
        .map_or_else(BTreeMap::new, |object| {
            object
                .iter()
                .filter_map(|(key, value)| value.str().map(|value| (key.clone(), value.into())))
                .collect()
        })
}

fn optional_string_map(value: Option<&PyJson>) -> BTreeMap<String, Option<String>> {
    value
        .and_then(PyJson::object)
        .map_or_else(BTreeMap::new, |object| {
            object
                .iter()
                .map(|(key, value)| (key.clone(), value.str().map(str::to_owned)))
                .collect()
        })
}

fn float_map(value: Option<&PyJson>) -> BTreeMap<String, f64> {
    value
        .and_then(PyJson::object)
        .map_or_else(BTreeMap::new, |object| {
            object
                .iter()
                .filter_map(|(key, value)| {
                    crate::project::float(value)
                        .ok()
                        .map(|value| (key.clone(), value))
                })
                .collect()
        })
}

fn optional_float_map(value: Option<&PyJson>) -> BTreeMap<String, Option<f64>> {
    value
        .and_then(PyJson::object)
        .map_or_else(BTreeMap::new, |object| {
            object
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        if matches!(value, PyJson::Null) {
                            None
                        } else {
                            crate::project::float(value).ok()
                        },
                    )
                })
                .collect()
        })
}
