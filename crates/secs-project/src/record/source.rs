use super::*;

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

pub(super) fn load_source_checked(wav: &Path) -> Result<Option<SourceInfo>, ProjectError> {
    let path = source_path(wav);
    let bytes = match crate::irs::read_bounded(&path, MAX_SOURCE_BYTES, "measurement source") {
        Ok(bytes) => bytes,
        Err(ProjectError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let text = String::from_utf8(bytes).map_err(|error| {
        ProjectError::Invalid(format!(
            "measurement source {} is not UTF-8: {error}",
            path.display()
        ))
    })?;
    Ok(source_from_json(&loads(&text)?))
}

pub(super) const MAX_PROJECT_SOURCE_BYTES: usize = 64 * 1024 * 1024;

pub(super) fn source_info_bytes(info: &SourceInfo) -> usize {
    let mut bytes = std::mem::size_of::<SourceInfo>();
    bytes = bytes.saturating_add(
        info.strings
            .iter()
            .map(|(key, value)| std::mem::size_of::<(String, String)>() + key.len() + value.len())
            .sum::<usize>(),
    );
    bytes = bytes.saturating_add(
        info.boxed
            .iter()
            .map(|(key, _)| std::mem::size_of::<(String, Boxed)>() + key.len())
            .sum::<usize>(),
    );
    for (key, cal) in &info.cal {
        bytes = bytes
            .saturating_add(std::mem::size_of::<(String, CalInfo)>())
            .saturating_add(key.len())
            .saturating_add(cal.source_name.len())
            .saturating_add(cal.source_path.len())
            .saturating_add(cal.freq.as_ref().map_or(0, |values| {
                values.len().saturating_mul(std::mem::size_of::<f32>())
            }))
            .saturating_add(cal.gain.as_ref().map_or(0, |values| {
                values.len().saturating_mul(std::mem::size_of::<f32>())
            }))
            .saturating_add(cal.phase.as_ref().map_or(0, |values| {
                values.len().saturating_mul(std::mem::size_of::<f32>())
            }))
            .saturating_add(
                cal.boxed
                    .iter()
                    .map(|(name, _)| std::mem::size_of::<(String, Boxed)>() + name.len())
                    .sum::<usize>(),
            );
    }
    bytes
        .saturating_add(info.notes.as_ref().map_or(0, String::len))
        .saturating_add(info.format.as_ref().map_or(0, String::len))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceSkipReason {
    TooLarge,
    Unreadable,
    Invalid,
    Budget,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SourceSkip {
    pub(crate) pos: u32,
    pub(crate) role: String,
    pub(crate) path: PathBuf,
    pub(crate) reason: SourceSkipReason,
}

#[derive(Debug, Default)]
pub(crate) struct LoadedSources {
    pub(crate) by_entry: BTreeMap<(i64, String), Arc<SourceInfo>>,
    pub(crate) exhausted: bool,
    pub(crate) skipped: Vec<SourceSkip>,
    pub(crate) parse_count: usize,
}

#[derive(Clone, Debug)]
pub(super) struct SourceBudget {
    limit: usize,
    used: usize,
    exhausted: bool,
}

impl SourceBudget {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            limit,
            used: 0,
            exhausted: false,
        }
    }

    pub(super) fn from_sources(limit: usize, sources: &BTreeMap<(i64, String), Arc<SourceInfo>>) -> Self {
        let mut budget = Self::new(limit);
        for source in sources.values() {
            let spent = budget.spend(source_info_bytes(source));
            debug_assert!(spent, "loaded sources exceed their map budget");
        }
        budget
    }

    pub(super) fn spend(&mut self, bytes: usize) -> bool {
        let Some(used) = self.used.checked_add(bytes) else {
            self.exhausted = true;
            return false;
        };
        if used > self.limit {
            self.exhausted = true;
            false
        } else {
            self.used = used;
            true
        }
    }

    pub(super) fn release(&mut self, bytes: usize) {
        self.used = self.used.saturating_sub(bytes);
        self.exhausted = false;
    }
}

pub(super) fn source_error_reason(error: &ProjectError) -> SourceSkipReason {
    match error {
        ProjectError::Io(_) => SourceSkipReason::Unreadable,
        ProjectError::Invalid(detail) if detail.contains(" exceeds ") => SourceSkipReason::TooLarge,
        ProjectError::Json(_)
        | ProjectError::Invalid(_)
        | ProjectError::Wav(_)
        | ProjectError::IrPath { .. } => SourceSkipReason::Invalid,
    }
}

pub(super) fn source_warning(skip: &SourceSkip) -> MigrationWarning {
    let code = match skip.reason {
        SourceSkipReason::Budget => "measurement.source.budget",
        SourceSkipReason::TooLarge | SourceSkipReason::Unreadable | SourceSkipReason::Invalid => {
            "measurement.source.skipped"
        }
    };
    let entry = if skip.path.as_os_str().is_empty() {
        format!("position {}, role {}", skip.pos, skip.role)
    } else {
        skip.path
            .components()
            .collect::<PathBuf>()
            .to_string_lossy()
            .into_owned()
    };
    MigrationWarning { code, entry }
}

pub(super) fn skip(
    skipped: &mut Vec<SourceSkip>,
    pos: u32,
    role: &str,
    path: &Path,
    reason: SourceSkipReason,
) {
    skipped.push(SourceSkip {
        pos,
        role: role.to_owned(),
        path: path.to_owned(),
        reason,
    });
}

pub(crate) fn load_project_sources_within(
    project: &Project,
    project_path: &Path,
    budget: usize,
) -> LoadedSources {
    let base = project_path.parent().unwrap_or_else(|| Path::new("."));
    let mut loaded: HashMap<PathBuf, Option<(Arc<SourceInfo>, usize)>> = HashMap::new();
    let mut budget = SourceBudget::new(budget);
    let mut by_entry = BTreeMap::new();
    let mut skipped = Vec::new();
    let mut parse_count = 0;
    for record in measurement_values(project)
        .iter()
        .filter_map(MeasurementRecord::from_json)
    {
        let Ok(pos) = u32::try_from(record.pos) else {
            continue;
        };
        for (role, raw) in record.ir {
            if raw.is_empty() || budget.exhausted {
                continue;
            }
            let path = PathBuf::from(raw);
            let path = if path.is_absolute() {
                path
            } else {
                base.join(path)
            };
            let sidecar = source_path(&path);
            // F5: neither the IR nor its sidecar may lead outside the `.irs`.
            if !path.is_file() || confined_ir_path(project_path, &path).is_err() {
                continue;
            }
            let metadata = match fs::symlink_metadata(&sidecar) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => {
                    skip(
                        &mut skipped,
                        pos,
                        &role,
                        &sidecar,
                        SourceSkipReason::Unreadable,
                    );
                    continue;
                }
                Ok(metadata) => metadata,
            };
            if !metadata.file_type().is_file() {
                skip(
                    &mut skipped,
                    pos,
                    &role,
                    &sidecar,
                    SourceSkipReason::Invalid,
                );
                continue;
            }
            if metadata.len() > MAX_SOURCE_BYTES {
                skip(
                    &mut skipped,
                    pos,
                    &role,
                    &sidecar,
                    SourceSkipReason::TooLarge,
                );
                continue;
            }
            if confined_ir_path(project_path, &sidecar).is_err() {
                skip(
                    &mut skipped,
                    pos,
                    &role,
                    &sidecar,
                    SourceSkipReason::Invalid,
                );
                continue;
            }
            let key = sidecar.canonicalize().unwrap_or_else(|_| sidecar.clone());
            let info = match loaded.get(&key) {
                Some(info) => info.clone(),
                None => {
                    // The cache slot and its key spend the budget too, so
                    // sidecars that hold no source info cannot grow the cache
                    // past it (audit #155 TD-112).
                    let entry_bytes =
                        std::mem::size_of::<(PathBuf, Option<(Arc<SourceInfo>, usize)>)>()
                            .saturating_add(key.as_os_str().len());
                    if !budget.spend(entry_bytes) {
                        skip(&mut skipped, pos, &role, &sidecar, SourceSkipReason::Budget);
                        continue;
                    }
                    parse_count += 1;
                    let info = match load_source_checked(&path) {
                        Ok(Some(info)) => {
                            let bytes = source_info_bytes(&info);
                            if !budget.spend(bytes) {
                                skip(&mut skipped, pos, &role, &sidecar, SourceSkipReason::Budget);
                                None
                            } else {
                                Some((Arc::new(info), bytes))
                            }
                        }
                        Ok(None) => None,
                        Err(error) => {
                            skip(
                                &mut skipped,
                                pos,
                                &role,
                                &sidecar,
                                source_error_reason(&error),
                            );
                            None
                        }
                    };
                    loaded.insert(key, info.clone());
                    info
                }
            };
            if let Some((info, bytes)) = info {
                if !budget.spend(bytes) {
                    skip(&mut skipped, pos, &role, &sidecar, SourceSkipReason::Budget);
                    continue;
                }
                by_entry.insert((i64::from(pos), role), info);
            }
        }
    }
    LoadedSources {
        by_entry,
        exhausted: budget.exhausted,
        skipped,
        parse_count,
    }
}

pub(super) const SOURCE_STRING_FIELDS: [&str; 7] = [
    "outputName",
    "sourceFileName",
    "timingRefName",
    "osName",
    "osVersion",
    "jre",
    "sigGenLevelSt",
];
pub(super) const SOURCE_BOXED_FIELDS: [&str; 8] = [
    "inputVolume",
    "outputVolume",
    "micSensitivity",
    "splCalOffset",
    "dBVInputOffset",
    "dBVOutputOffset",
    "usesEmbeddedCalData",
    "calDataLimitApplied",
];
pub(super) const SOURCE_CAL_FIELDS: [&str; 2] = ["meterCal", "scCal"];
pub(super) const SOURCE_CAL_BOXED: [&str; 4] = ["sensitivitydB", "sensitivitydBFS", "serialNum", "revision"];

pub(super) fn source_to_json(info: &SourceInfo) -> PyJson {
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

pub(super) fn boxed_is_serializable(value: Boxed) -> bool {
    match value {
        Boxed::Double(value) => value.is_finite(),
        Boxed::Integer(value) => i32::try_from(value).is_ok(),
        Boxed::Boolean(_) => true,
    }
}

pub(super) fn serializable_date(value: SourceDate) -> Option<PyJson> {
    match value {
        SourceDate::Int(value) => Some(PyJson::Int(value.to_string())),
        SourceDate::Float(value) if value.is_finite() => Some(PyJson::Float(value)),
        SourceDate::Float(_) => None,
    }
}

pub(super) fn boxed_to_json(value: Boxed) -> PyJson {
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

pub(super) fn cal_to_json(cal: &CalInfo) -> PyJson {
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

pub(super) fn float_array_to_json(values: Option<&[f32]>) -> PyJson {
    values.map_or(PyJson::Null, |values| {
        PyJson::Array(
            values
                .iter()
                .map(|value| PyJson::Float(f64::from(*value)))
                .collect(),
        )
    })
}

pub(super) fn source_from_json(value: &PyJson) -> Option<SourceInfo> {
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
                    inverse_c_selected: value_for(cal, "inverseCSelected")
                        .is_some_and(PyJson::is_truthy),
                    source_name: value_for(cal, "sourceName")
                        .filter(|value| value.is_truthy())
                        .map_or_else(String::new, python_str),
                    source_path: value_for(cal, "sourcePath")
                        .filter(|value| value.is_truthy())
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
        .filter(|value| value.is_truthy())
        .map(python_str);
    info.format = value_for(object, "format")
        .filter(|value| value.is_truthy())
        .map(python_str);
    Some(info)
}

pub(super) fn boxed_from_json(value: &PyJson) -> Option<Boxed> {
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
        "java.lang.Boolean" => Some(Boxed::Boolean(raw.is_truthy())),
        _ => None,
    }
}

pub(super) fn date_from_json(value: &PyJson) -> Option<SourceDate> {
    match value {
        PyJson::Bool(value) => Some(SourceDate::Int(i64::from(*value))),
        PyJson::Int(value) => value.parse().ok().map(SourceDate::Int),
        PyJson::Float(value) if value.is_finite() => Some(SourceDate::Float(*value)),
        _ => None,
    }
}

pub(super) fn float_array_from_json(value: &PyJson) -> Option<Vec<f32>> {
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

pub(super) fn python_float_value(value: &PyJson) -> Option<f64> {
    match value {
        PyJson::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
        PyJson::Int(value) => value.parse().ok(),
        PyJson::Float(value) => Some(*value),
        PyJson::Str(value) => value.trim().parse().ok(),
        _ => None,
    }
}

pub(super) fn python_int(value: &PyJson) -> Option<i64> {
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

pub(super) fn python_str(value: &PyJson) -> String {
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

pub(super) fn python_repr(value: &PyJson) -> String {
    match value {
        PyJson::Str(value) => format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'")),
        _ => python_str(value),
    }
}

