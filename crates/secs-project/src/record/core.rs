use crate::{
    MAX_IR_BYTES, Project, ProjectError, PyJson, dumps_compact, ir_directory, ir_file, loads,
    read_mono_float_wav, save_ir,
};
use secs_mdat::{Boxed, CalInfo, SourceDate, SourceInfo};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;
use thiserror::Error;

pub const IR_CACHE_LIMIT_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_PROJECT_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_SOURCE_BYTES: u64 = 16 * 1024 * 1024;

/// Calibration curves a measurement source sidecar attaches (ADR 0015).
/// `meterCal` precedes every `scCal`; only valid embedded curves are attached.
#[derive(Debug, Clone, PartialEq)]
pub struct AttachedCalibration<'a> {
    /// The valid `meterCal` entry, when present.
    pub meter: Option<&'a CalInfo>,
    /// Valid `scCal` entries in sidecar order.
    pub others: Vec<&'a CalInfo>,
    /// Whether the `meterCal` identifies a C-weighted SPL meter.
    pub c_weighted: bool,
}

impl AttachedCalibration<'_> {
    /// Whether neither a curve nor C weighting is attached.
    pub fn is_empty(&self) -> bool {
        self.meter.is_none() && self.others.is_empty() && !self.c_weighted
    }
}

fn has_valid_curve(cal: &CalInfo) -> bool {
    let (Some(frequencies), Some(gains)) = (&cal.freq, &cal.gain) else {
        return false;
    };
    let frequencies = frequencies
        .iter()
        .map(|&value| f64::from(value))
        .collect::<Vec<_>>();
    let gains = gains
        .iter()
        .map(|&value| f64::from(value))
        .collect::<Vec<_>>();
    secs_types::calibration::calibration_curve_is_valid(&frequencies, &gains)
}

/// Selects the sidecar calibration that ADR 0015 applies.
pub fn attached_calibration(info: &SourceInfo) -> AttachedCalibration<'_> {
    let meter_entry = info
        .cal
        .iter()
        .find(|(name, _)| name == "meterCal")
        .map(|(_, cal)| cal);
    let meter = meter_entry.filter(|cal| has_valid_curve(cal));
    let c_weighted = meter_entry.is_some_and(|cal| {
        cal.inverse_c_selected && (meter.is_some() || (cal.freq.is_none() && cal.gain.is_none()))
    });
    AttachedCalibration {
        meter,
        others: info
            .cal
            .iter()
            .filter(|(name, cal)| name == "scCal" && has_valid_curve(cal))
            .map(|(_, cal)| cal)
            .collect(),
        c_weighted,
    }
}

const FNV_OFFSET_BASIS_64: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME_64: u64 = 0x0000_0100_0000_01b3;

fn fingerprint_bytes(hash: &mut u64, bytes: &[u8]) {
    for &byte in bytes {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(FNV_PRIME_64);
    }
}

fn fingerprint_len(hash: &mut u64, len: usize) {
    fingerprint_bytes(hash, &(len as u64).to_le_bytes());
}

fn fingerprint_array(hash: &mut u64, values: &[f32]) {
    fingerprint_len(hash, values.len());
    for value in values {
        fingerprint_bytes(hash, &value.to_bits().to_le_bytes());
    }
}

fn fingerprint_curve(hash: &mut u64, name: &str, cal: &CalInfo) {
    fingerprint_len(hash, name.len());
    fingerprint_bytes(hash, name.as_bytes());
    fingerprint_array(hash, cal.freq.as_deref().unwrap_or_default());
    fingerprint_array(hash, cal.gain.as_deref().unwrap_or_default());
    match &cal.phase {
        Some(phase) => {
            fingerprint_bytes(hash, &[1]);
            fingerprint_array(hash, phase);
        }
        None => fingerprint_bytes(hash, &[0]),
    }
}

/// FNV-1a 64 fingerprint of attached curves and the C-weighting flag.
/// Returns `None` when the sidecar attaches no response correction.
pub fn calibration_fingerprint(info: &SourceInfo) -> Option<u64> {
    let attached = attached_calibration(info);
    if attached.is_empty() {
        return None;
    }
    let mut hash = FNV_OFFSET_BASIS_64;
    fingerprint_bytes(&mut hash, &[u8::from(attached.c_weighted)]);
    if let Some(meter) = attached.meter {
        fingerprint_curve(&mut hash, "meterCal", meter);
    }
    for cal in attached.others {
        fingerprint_curve(&mut hash, "scCal", cal);
    }
    Some(hash)
}

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

/// Failure before or after a measurement changed the in-memory project.
#[derive(Debug, Error)]
pub enum RecordError {
    #[error("{0}")]
    Invalid(#[source] ProjectError),
    #[error("{0}")]
    SaveFailed(#[source] ProjectError),
}

impl From<RecordError> for ProjectError {
    fn from(error: RecordError) -> Self {
        match error {
            RecordError::Invalid(error) | RecordError::SaveFailed(error) => error,
        }
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
    sources: BTreeMap<(i64, String), Arc<SourceInfo>>,
    source_budget: SourceBudget,
    source_parse_count: usize,
    warnings: Vec<MigrationWarning>,
    cache: IrCache,
    saves: u64,
}

/// A removed position: its measurement entries (with their list indices)
/// and the SNR values the store held for it.
#[derive(Clone, Debug)]
pub struct RemovedPosition {
    pos: i64,
    entries: Vec<(usize, PyJson)>,
    snr_db: Vec<(String, f64)>,
    sources: Vec<(String, Arc<SourceInfo>)>,
}

impl RemovedPosition {
    /// The measurement position that was removed.
    pub fn position(&self) -> i64 {
        self.pos
    }
}

#[allow(deprecated)]
impl MeasurementStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, ProjectError> {
        Self::open_with_source_budget(path, MAX_PROJECT_SOURCE_BYTES)
    }

    fn open_with_source_budget(
        path: impl Into<PathBuf>,
        source_budget: usize,
    ) -> Result<Self, ProjectError> {
        let path = path.into();
        let bytes = crate::irs::read_bounded(&path, MAX_PROJECT_BYTES, "project")?;
        let text = String::from_utf8(bytes).map_err(|error| {
            ProjectError::Invalid(format!("project {} is not UTF-8: {error}", path.display()))
        })?;
        let (mut project, mut warnings) = Project::from_json_with_warnings(loads(&text)?)?;
        let loaded = load_project_sources_within(&project, &path, source_budget);
        let LoadedSources {
            by_entry: sources,
            exhausted,
            skipped,
            parse_count,
        } = loaded;
        debug_assert_eq!(
            exhausted,
            skipped
                .iter()
                .any(|skip| skip.reason == SourceSkipReason::Budget)
        );
        for (key, info) in &sources {
            if let Some(fingerprint) = calibration_fingerprint(info) {
                project
                    .measurement_calibration
                    .insert(key.clone(), fingerprint);
            }
        }
        warnings.extend(skipped.iter().map(source_warning));
        let mut store = Self::new(path, project);
        store.sources = sources;
        store.source_budget = SourceBudget::from_sources(source_budget, &store.sources);
        store.source_parse_count = parse_count;
        store.warnings.extend(warnings);
        Ok(store)
    }

    pub fn new(path: impl Into<PathBuf>, project: Project) -> Self {
        Self {
            path: path.into(),
            project,
            snr_db: BTreeMap::new(),
            sources: BTreeMap::new(),
            source_budget: SourceBudget::new(MAX_PROJECT_SOURCE_BYTES),
            source_parse_count: 0,
            warnings: Vec::new(),
            cache: IrCache::default(),
            saves: 0,
        }
    }

    /// Moves an open store to another project path without reparsing source sidecars.
    pub fn relocated(&self, path: PathBuf, project: Project) -> Self {
        let mut relocated = Self::new(path, project);
        relocated.sources = self.sources.clone();
        relocated.source_budget = self.source_budget.clone();
        relocated.source_parse_count = self.source_parse_count;
        relocated
    }

    /// Overrides the source-map budget for focused tests.
    #[doc(hidden)]
    pub fn with_source_budget(mut self, cap_bytes: usize) -> Self {
        let sources = std::mem::take(&mut self.sources);
        self.source_budget = SourceBudget::new(cap_bytes);
        for ((pos, role), source) in sources {
            self.keep_source((pos, role), source);
        }
        self
    }

    /// Number of source sidecars parsed when this store opened.
    /// Relocation preserves the diagnostic count without parsing again.
    pub fn source_parse_count(&self) -> usize {
        self.source_parse_count
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

#[deprecated(since = "0.23.0", note = "no caller; removed in the next minor (audit #186 TD-219)")]
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
        let released = self
            .sources
            .iter()
            .filter(|((entry_pos, _), _)| *entry_pos == pos)
            .map(|(_, source)| source_info_bytes(source))
            .sum();
        self.sources.retain(|(entry_pos, _), _| *entry_pos != pos);
        self.source_budget.release(released);
        self.entry_for(pos).expect("new entry has a valid position")
    }

    /// Writes the IR and its source sidecar, enters the result into its
    /// position (upstream `session.record`), and saves the project once.
    ///
    /// A result with a `source` also sets `source[role]` on the entry, where
    /// upstream's importer (`step5.load_mdat`) sets it after recording: the
    /// key goes last when the entry has none, and a value that is not an
    /// object is replaced by one.
    pub fn record(
        &mut self,
        result: MeasurementResult,
        plan_roles: Option<&[String]>,
    ) -> Result<MeasurementRecord, RecordError> {
        let source_info = result.source_info.clone();
        let source_fingerprint = source_info.as_ref().and_then(calibration_fingerprint);
        let path = save_ir(
            &self.path,
            &result.role,
            result.pos,
            &result.ir,
            result.sample_rate,
        )
        .map_err(RecordError::Invalid)?;
        save_source(&path, result.source_info.as_ref()).map_err(RecordError::Invalid)?;
        self.cache
            .remember(&path, &result.ir, result.sample_rate)
            .map_err(RecordError::Invalid)?;
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
                        .is_some_and(PyJson::is_truthy)
                })
            },
        );
        set_value(entry, "done", PyJson::Bool(done));
        if let Some(source) = result.source {
            if !value_for(entry, "source").is_some_and(|value| matches!(value, PyJson::Object(_))) {
                set_value(entry, "source", PyJson::Object(Vec::new()));
            }
            set_nested(entry, "source", &role, PyJson::Str(source));
        }
        let calibration_key = (result.pos, role.clone());
        if let Some(fingerprint) = source_fingerprint {
            self.project
                .measurement_calibration
                .insert(calibration_key, fingerprint);
        } else {
            self.project
                .measurement_calibration
                .remove(&calibration_key);
        }
        let source_key = (result.pos, role.clone());
        if let Some(source) = source_info {
            self.keep_source(source_key, Arc::new(source));
        } else {
            self.remove_source(&source_key);
        }
        if let Some(value) = result.snr_db {
            self.snr_db.insert((result.pos, role), value);
        }
        self.save().map_err(RecordError::SaveFailed)?;
        Ok(self
            .entry_for(result.pos)
            .expect("recorded entry has a valid position"))
    }

    /// Removes a position's entries and SNR values; `false` if it had none.
    pub fn clear_position(&mut self, pos: i64) -> bool {
        self.remove_position(pos).is_some()
    }

    /// Takes a position's entries and SNR values out of the store, keeping
    /// them for [`Self::restore_position`]. `None` if the position has no
    /// entry. The project file is not written.
    pub fn remove_position(&mut self, pos: i64) -> Option<RemovedPosition> {
        let records = measurement_values_mut(&mut self.project);
        let entries = records
            .iter()
            .enumerate()
            .filter(|(_, entry)| pos_of_or_minus_one(entry) == pos)
            .map(|(index, entry)| (index, entry.clone()))
            .collect::<Vec<_>>();
        if entries.is_empty() {
            return None;
        }
        records.retain(|entry| pos_of_or_minus_one(entry) != pos);
        let snr_db = self
            .snr_db
            .iter()
            .filter(|((entry_pos, _), _)| *entry_pos == pos)
            .map(|((_, role), value)| (role.clone(), *value))
            .collect();
        self.snr_db.retain(|(entry_pos, _), _| *entry_pos != pos);
        let sources = self
            .sources
            .iter()
            .filter(|((entry_pos, _), _)| *entry_pos == pos)
            .map(|((_, role), source)| (role.clone(), source.clone()))
            .collect::<Vec<_>>();
        self.source_budget.release(
            sources
                .iter()
                .map(|(_, source)| source_info_bytes(source))
                .sum(),
        );
        self.sources.retain(|(entry_pos, _), _| *entry_pos != pos);
        Some(RemovedPosition {
            pos,
            entries,
            snr_db,
            sources,
        })
    }

    /// Puts a removed position back; fails if that position was recorded
    /// again meanwhile. Each entry returns to its old list index (or the
    /// end, if the list is shorter now). The project file is not written.
    pub fn restore_position(&mut self, removed: RemovedPosition) -> Result<(), ProjectError> {
        let RemovedPosition {
            pos,
            entries,
            snr_db,
            sources,
        } = removed;
        if measurement_values(&self.project)
            .iter()
            .any(|entry| pos_of_or_minus_one(entry) == pos)
        {
            return Err(ProjectError::Invalid(format!(
                "position {pos} was recorded again"
            )));
        }
        let records = measurement_values_mut(&mut self.project);
        for (index, entry) in entries {
            records.insert(index.min(records.len()), entry);
        }
        self.snr_db
            .extend(snr_db.into_iter().map(|(role, value)| ((pos, role), value)));
        for (role, source) in sources {
            self.keep_source((pos, role), source);
        }
        Ok(())
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

    pub fn ir_of(&mut self, role: &str, pos: i64) -> Result<(Arc<[f32]>, u32), ProjectError> {
        let path = self.ir_path_of(role, pos)?;
        self.cache.load(&path)
    }

    fn remove_source(&mut self, key: &(i64, String)) -> Option<Arc<SourceInfo>> {
        let source = self.sources.remove(key)?;
        self.source_budget.release(source_info_bytes(&source));
        Some(source)
    }

    fn keep_source(&mut self, key: (i64, String), source: Arc<SourceInfo>) -> bool {
        self.remove_source(&key);
        let bytes = source_info_bytes(&source);
        if bytes > MAX_SOURCE_BYTES as usize || !self.source_budget.spend(bytes) {
            self.warnings.push(MigrationWarning {
                code: "measurement.source.budget",
                entry: format!("position {}, role {}", key.0, key.1),
            });
            false
        } else {
            self.sources.insert(key, source);
            true
        }
    }

    pub fn sources_by_position(&self) -> BTreeMap<u32, BTreeMap<String, Arc<SourceInfo>>> {
        let mut by_position = BTreeMap::<u32, BTreeMap<String, Arc<SourceInfo>>>::new();
        for ((position, role), source) in &self.sources {
            let Ok(position) = u32::try_from(*position) else {
                continue;
            };
            if !self.has_recorded_ir(role, i64::from(position)) {
                continue;
            }
            by_position
                .entry(position)
                .or_default()
                .insert(role.clone(), source.clone());
        }
        by_position
    }

    /// Returns a recorded IR's source sidecar from the map built when the store opened.
    pub fn source_of(&self, role: &str, pos: i64) -> Option<Arc<SourceInfo>> {
        self.has_recorded_ir(role, pos)
            .then(|| self.sources.get(&(pos, role.to_owned())).cloned())
            .flatten()
    }

    fn has_recorded_ir(&self, role: &str, pos: i64) -> bool {
        self.entry_for(pos)
            .and_then(|entry| entry.ir.get(role).cloned())
            .is_some_and(|path| !path.is_empty())
    }

    /// Rewrites a recorded IR's source sidecar and updates its in-memory
    /// calibration fingerprint. The IR and measurement entry are unchanged.
    pub fn replace_source(
        &mut self,
        role: &str,
        pos: i64,
        info: Option<&SourceInfo>,
    ) -> Result<(), ProjectError> {
        let recorded = self
            .entry_for(pos)
            .and_then(|entry| entry.ir.get(role).cloned())
            .is_some_and(|path| !path.is_empty());
        if !recorded {
            return Err(ProjectError::Invalid(format!(
                "position {pos}, role {role} has no recorded IR"
            )));
        }
        let path = self.ir_path_of(role, pos)?;
        if !path.is_file() {
            return Err(ProjectError::Invalid(format!(
                "recorded IR does not exist: {}",
                path.display()
            )));
        }
        if confined_ir_path(&self.path, &source_path(&path)).is_err() {
            return Err(ProjectError::IrPath {
                entry: format!("position {pos}, role {role} source"),
                path: source_path(&path),
            });
        }
        save_source(&path, info)?;
        let key = (pos, role.to_owned());
        if let Some(fingerprint) = info.and_then(calibration_fingerprint) {
            self.project
                .measurement_calibration
                .insert(key.clone(), fingerprint);
        } else {
            self.project.measurement_calibration.remove(&key);
        }
        if let Some(info) = info {
            self.keep_source(key, Arc::new(info.clone()));
        } else {
            self.remove_source(&key);
        }
        Ok(())
    }

    pub fn t0_of(&self, role: &str, pos: i64) -> f64 {
        self.entry_for(pos)
            .and_then(|entry| entry.t0.get(role).copied())
            .unwrap_or(0.0)
    }

#[deprecated(since = "0.23.0", note = "no caller; removed in the next minor (audit #186 TD-219)")]
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

#[deprecated(since = "0.23.0", note = "no caller; removed in the next minor (audit #186 TD-219)")]
    pub fn irs_by_role(
        &mut self,
        pos: i64,
    ) -> Result<BTreeMap<String, (Arc<[f32]>, u32)>, ProjectError> {
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
        atomic_write(&self.path, self.project.save_text().as_bytes())?;
        self.saves += 1;
        Ok(())
    }

    /// How many times this store has written its project file. A test hook:
    /// tests count saves with it.
    pub fn save_count(&self) -> u64 {
        self.saves
    }
}

mod cache;
mod confine;
mod entries;
mod source;

pub use cache::IrCache;
use confine::*;
use entries::*;
pub use source::{save_source, source_path};
use source::*;

pub(crate) fn atomic_write(path: &Path, data: &[u8]) -> Result<(), ProjectError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    secs_types::fs::write_atomic_bytes(path, data)?;
    Ok(())
}


#[cfg(test)]
mod source_budget_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn entry(position: usize, path: String) -> PyJson {
        PyJson::Object(vec![
            ("pos".into(), PyJson::Int(position.to_string())),
            (
                "ir".into(),
                PyJson::Object(vec![("L".into(), PyJson::Str(path))]),
            ),
        ])
    }

    fn sidecar_project(name: &str, sidecar: &[u8]) -> (PathBuf, PathBuf, PathBuf) {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "secs-project-sidecar-warning-{}-{unique}",
            std::process::id()
        ));
        let project_path = root.join(format!("{name}.secs"));
        let irs = project_path.with_extension("irs");
        fs::create_dir_all(&irs).unwrap();
        let wav = irs.join("L_P1.wav");
        fs::write(&wav, []).unwrap();
        let sidecar_path = source_path(&wav);
        fs::write(&sidecar_path, sidecar).unwrap();
        let mut project = Project::from_preset("2.0").unwrap();
        project.measurements = PyJson::Array(vec![entry(1, format!("{name}.irs/L_P1.wav"))]);
        fs::write(&project_path, project.save_text()).unwrap();
        (root, project_path, sidecar_path)
    }

    fn three_source_project(name: &str) -> (PathBuf, PathBuf) {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "secs-project-source-map-{}-{name}-{unique}",
            std::process::id()
        ));
        let project_path = root.join("room.secs");
        let irs = project_path.with_extension("irs");
        fs::create_dir_all(&irs).unwrap();
        let mut entries = Vec::new();
        for (position, role) in [(1, "L"), (1, "R"), (2, "L")] {
            let wav = irs.join(format!("{role}_P{position}.wav"));
            fs::write(&wav, []).unwrap();
            save_source(
                &wav,
                Some(&SourceInfo {
                    notes: Some(format!("{role} at {position}")),
                    ..SourceInfo::default()
                }),
            )
            .unwrap();
            let relative = format!("room.irs/{role}_P{position}.wav");
            if let Some(PyJson::Object(fields)) = entries
                .iter_mut()
                .find(|entry| pos_of(entry) == Some(position))
            {
                let ir = fields.iter_mut().find(|(key, _)| key == "ir").unwrap();
                ir.1.object_mut()
                    .unwrap()
                    .push((role.into(), PyJson::Str(relative)));
            } else {
                entries.push(PyJson::Object(vec![
                    ("pos".into(), PyJson::Int(position.to_string())),
                    (
                        "ir".into(),
                        PyJson::Object(vec![(role.into(), PyJson::Str(relative))]),
                    ),
                ]));
            }
        }
        let mut project = Project::from_preset("2.0").unwrap();
        project.measurements = PyJson::Array(entries);
        fs::write(&project_path, project.save_text()).unwrap();
        (root, project_path)
    }

    #[test]
    #[cfg_attr(miri, ignore = "uses temporary files")]
    fn open_parses_each_sidecar_once_and_source_of_never_parses_again() {
        let (root, project_path) = three_source_project("parse-once");
        let store = MeasurementStore::open(&project_path).unwrap();
        assert_eq!(store.source_parse_count(), 3);
        for _ in 0..5 {
            assert!(store.source_of("L", 1).is_some());
        }
        assert_eq!(store.source_parse_count(), 3);
        let relocated = store.relocated(root.join("relocated.secs"), store.project().clone());
        assert_eq!(relocated.source_parse_count(), 3);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "uses temporary files")]
    fn unreadable_sidecar_is_reported_as_a_migration_warning_td183() {
        let (root, project_path, sidecar) = sidecar_project("malformed", b"{not json");

        let store = MeasurementStore::open(&project_path).unwrap();
        let loaded =
            load_project_sources_within(store.project(), &project_path, MAX_PROJECT_SOURCE_BYTES);
        assert!(loaded.skipped.iter().any(|skip| {
            skip.pos == 1
                && skip.role == "L"
                && skip.path == sidecar
                && skip.reason == SourceSkipReason::Invalid
        }));

        assert!(
            store.warnings().iter().any(|warning| {
                warning.code == "measurement.source.skipped"
                    && warning.entry == sidecar.to_string_lossy()
            }),
            "warnings: {:?}, sidecar: {}",
            store.warnings(),
            sidecar.display()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "uses temporary files")]
    fn sidecar_that_is_a_directory_is_reported_as_skipped_td183() {
        let (root, project_path, sidecar) = sidecar_project("directory", b"[]");
        fs::remove_file(&sidecar).unwrap();
        fs::create_dir(&sidecar).unwrap();

        let store = MeasurementStore::open(&project_path).unwrap();

        assert!(store.warnings().iter().any(|warning| {
            warning.code == "measurement.source.skipped"
                && warning.entry == sidecar.to_string_lossy()
        }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "uses temporary files")]
    fn budget_exhaustion_is_reported_td183() {
        let (root, project_path, sidecar) = sidecar_project("budget", b"[]");

        let store = MeasurementStore::open_with_source_budget(&project_path, 1).unwrap();

        assert!(store.warnings().iter().any(|warning| {
            warning.code == "measurement.source.budget"
                && warning.entry == sidecar.to_string_lossy()
        }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "uses temporary files")]
    fn distinct_large_sidecars_stop_parsing_at_the_project_budget_td112() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "secs-project-sidecar-budget-{}-{unique}",
            std::process::id()
        ));
        let project_path = root.join("many.secs");
        let irs = project_path.with_extension("irs");
        fs::create_dir_all(&irs).unwrap();
        let info = SourceInfo {
            notes: Some("x".repeat(8 * 1024 * 1024)),
            ..SourceInfo::default()
        };
        let mut entries = Vec::new();
        for position in 1..=10 {
            let wav = irs.join(format!("{position}.wav"));
            fs::write(&wav, []).unwrap();
            save_source(&wav, Some(&info)).unwrap();
            entries.push(entry(position, format!("many.irs/{position}.wav")));
        }
        let mut project = Project::from_preset("2.0").unwrap();
        project.measurements = PyJson::Array(entries);

        let loaded = load_project_sources_within(&project, &project_path, MAX_PROJECT_SOURCE_BYTES);
        assert!(loaded.exhausted);
        let parsed = loaded.parse_count;
        let sources = loaded.by_entry;
        assert!(parsed > 0);
        assert!(
            parsed < 10,
            "parsed {parsed} sidecars after exhausting the budget"
        );
        assert!(sources.keys().any(|(position, _)| *position == 1));
        assert!(!sources.keys().any(|(position, _)| *position == 10));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "uses temporary files")]
    fn sidecars_without_source_info_still_spend_the_cache_budget_td112() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "secs-project-empty-sidecars-{}-{unique}",
            std::process::id()
        ));
        let project_path = root.join("empty.secs");
        let irs = project_path.with_extension("irs");
        fs::create_dir_all(&irs).unwrap();
        let mut entries = Vec::new();
        for position in 1..=50 {
            let wav = irs.join(format!("{position}.wav"));
            fs::write(&wav, []).unwrap();
            fs::write(source_path(&wav), "[]").unwrap();
            entries.push(entry(position, format!("empty.irs/{position}.wav")));
        }
        let mut project = Project::from_preset("2.0").unwrap();
        project.measurements = PyJson::Array(entries);

        let loaded = load_project_sources_within(&project, &project_path, 4096);
        let parsed = loaded.parse_count;
        assert!(loaded.by_entry.is_empty());
        assert!(loaded.exhausted);
        assert!(parsed > 0);
        assert!(
            parsed < 50,
            "parsed {parsed} empty sidecars with a 4 KiB cache budget"
        );

        fs::remove_dir_all(root).unwrap();
    }
}
