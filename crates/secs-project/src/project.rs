use crate::pyjson::{Shape, merge_with_file};
use crate::{
    DspProfile, ProjectError, PyJson, SpeakerConfig, TargetCurve, TargetJson, dumps, loads,
};

pub const PROJECT_VERSION: i64 = 3;
pub const FOCUS_STEPS: [i64; 5] = [0, 25, 50, 75, 100];

const PROJECT_KEYS: [&str; 14] = [
    "version",
    "name",
    "config",
    "dsp",
    "target",
    "curtain",
    "xo",
    "bc",
    "mso",
    "measurements",
    "verify",
    "tesira",
    "snapshots",
    "extra",
];

pub(crate) fn integer(
    value: Option<&PyJson>,
    default: i64,
    field: &str,
) -> Result<i64, ProjectError> {
    match value {
        None => Ok(default),
        Some(PyJson::Int(value)) => value
            .parse()
            .map_err(|_| ProjectError::Invalid(format!("{field} must fit in an integer"))),
        Some(PyJson::Float(value)) if value.is_finite() => {
            let truncated = value.trunc();
            if truncated < i64::MIN as f64 || truncated >= -(i64::MIN as f64) {
                Err(ProjectError::Invalid(format!(
                    "{field} must fit in an integer"
                )))
            } else {
                Ok(truncated as i64)
            }
        }
        Some(PyJson::Bool(value)) => Ok(i64::from(*value)),
        Some(PyJson::Str(value)) => value
            .parse()
            .map_err(|_| ProjectError::Invalid(format!("{field} must be an integer"))),
        Some(_) => Err(ProjectError::Invalid(format!("{field} must be an integer"))),
    }
}

pub(crate) fn float(value: &PyJson) -> Result<f64, ProjectError> {
    match value {
        PyJson::Int(value) => {
            let parsed = value
                .parse::<f64>()
                .map_err(|_| ProjectError::Invalid("expected number".into()))?;
            if !parsed.is_finite() {
                Err(ProjectError::Invalid(
                    "integer is too large for a Python float".into(),
                ))
            } else {
                Ok(parsed)
            }
        }
        PyJson::Float(value) => Ok(*value),
        PyJson::Bool(value) => Ok(if *value { 1.0 } else { 0.0 }),
        PyJson::Str(value) => value
            .parse()
            .map_err(|_| ProjectError::Invalid("expected number".into())),
        _ => Err(ProjectError::Invalid("expected number".into())),
    }
}

pub(crate) fn string(value: Option<&PyJson>, default: &str) -> String {
    match value {
        None => default.into(),
        Some(PyJson::Str(value) | PyJson::Int(value)) => value.clone(),
        Some(PyJson::Float(value)) => crate::pyjson::dumps(&PyJson::Float(*value)),
        Some(PyJson::Bool(true)) => "True".into(),
        Some(PyJson::Bool(false)) => "False".into(),
        Some(PyJson::Null) => "None".into(),
        Some(value) => crate::pyjson::dumps(value),
    }
}

pub(crate) fn optional_bool(value: Option<&PyJson>) -> Option<bool> {
    value.map(PyJson::is_truthy)
}

fn get<'a>(object: &'a [(String, PyJson)], key: &str) -> Option<&'a PyJson> {
    object
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value)
}

fn set(object: &mut Vec<(String, PyJson)>, key: &str, value: PyJson) {
    if let Some((_, old)) = object.iter_mut().find(|(name, _)| name == key) {
        *old = value;
    } else {
        object.push((key.into(), value));
    }
}

fn object_or_empty(
    value: Option<&PyJson>,
    field: &str,
) -> Result<Vec<(String, PyJson)>, ProjectError> {
    match value {
        None | Some(PyJson::Null) => Ok(Vec::new()),
        Some(PyJson::Object(value)) => Ok(value.clone()),
        Some(_) => Err(ProjectError::Invalid(format!("{field} must be an object"))),
    }
}

fn array_or_empty(value: Option<&PyJson>, field: &str) -> Result<Vec<PyJson>, ProjectError> {
    match value {
        None | Some(PyJson::Null) => Ok(Vec::new()),
        Some(PyJson::Array(value)) => Ok(value.clone()),
        Some(_) => Err(ProjectError::Invalid(format!("{field} must be an array"))),
    }
}

pub fn snap_focus(value: &PyJson) -> i64 {
    let x = float(value).unwrap_or(50.0);
    let mut best = FOCUS_STEPS[0];
    let mut distance = (best as f64 - x).abs();
    for step in FOCUS_STEPS.into_iter().skip(1) {
        let candidate = (step as f64 - x).abs();
        if candidate < distance {
            best = step;
            distance = candidate;
        }
    }
    best
}

pub fn rename_keys(value: &PyJson, table: &[(&str, &str)]) -> PyJson {
    let PyJson::Object(object) = value else {
        return value.clone();
    };
    if !table
        .iter()
        .any(|(old, _)| object.iter().any(|(key, _)| key == old))
    {
        return value.clone();
    }
    let mut out = Vec::new();
    for (key, value) in object {
        if let Some((_, new)) = table.iter().find(|(old, _)| key == *old) {
            if !object.iter().any(|(key, _)| key == new) {
                out.push(((*new).into(), value.clone()));
            }
        } else {
            out.push((key.clone(), value.clone()));
        }
    }
    PyJson::Object(out)
}

pub fn migrate_group_keys(value: &PyJson) -> PyJson {
    rename_keys(value, &[("하이트", "탑")])
}

pub fn migrate_calibration_keys(value: &PyJson) -> PyJson {
    let mut out = rename_keys(
        value,
        &[
            ("art_on", "mimo_on"),
            ("art_low", "mimo_low"),
            ("art_max_supports", "mimo_max_supports"),
        ],
    );
    if let Some(PyJson::Object(manual)) = out.get("subs_manual").cloned() {
        let migrated = PyJson::Object(
            manual
                .into_iter()
                .map(|(role, entry)| {
                    (
                        role,
                        rename_keys(&entry, &[("art_support", "mimo_support")]),
                    )
                })
                .collect(),
        );
        set(
            out.object_mut()
                .expect("renaming an object returns an object"),
            "subs_manual",
            migrated,
        );
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub version: i64,
    pub name: String,
    pub config: SpeakerConfig,
    pub dsp: DspProfile,
    pub target: TargetCurve,
    pub curtain: (f64, f64),
    pub xo: Vec<(String, f64)>,
    pub bc: PyJson,
    pub mso: PyJson,
    pub measurements: PyJson,
    pub verify: PyJson,
    pub tesira: PyJson,
    pub snapshots: PyJson,
    pub extra: Vec<(String, PyJson)>,
    /// Fingerprints of calibration attached to recorded measurements, keyed by
    /// position and role. This runtime index is rebuilt from source sidecars and
    /// is never written to the project file (ADR 0015).
    pub measurement_calibration: std::collections::BTreeMap<(i64, String), u64>,
    /// The `config`, `dsp`, `target` and `calibration` values as the file
    /// held them (after migration). Saving rebuilds those from the typed
    /// fields and [`crate::store_calibration`], then keeps the keys this
    /// version does not know from these (ADR 0004, docs/DEVIATIONS.md F16).
    /// Empty for a project that was not read from a file.
    pub loaded: Vec<(String, PyJson)>,
}

/// The fields rebuilt on save and the shape each meets its loaded value with.
const REBUILT: [(&str, &Shape); 4] = [
    ("config", &crate::config::CONFIG_SHAPE),
    ("dsp", &crate::dsp_profile::DSP_SHAPE),
    ("target", &crate::target::TARGET_SHAPE),
    ("calibration", &crate::calibration::CALIBRATION_SHAPE),
];

impl Default for Project {
    fn default() -> Self {
        Self {
            version: PROJECT_VERSION,
            name: String::new(),
            config: SpeakerConfig::default(),
            dsp: DspProfile::default(),
            target: TargetCurve::default(),
            curtain: (24.0, 300.0),
            xo: Vec::new(),
            bc: PyJson::Object(vec![
                ("mode".into(), PyJson::Str("full".into())),
                ("phase".into(), PyJson::Str("minphase".into())),
                ("target_preset".into(), PyJson::Str("flat".into())),
            ]),
            mso: PyJson::Object(vec![
                ("lo".into(), PyJson::Int("20".into())),
                ("hi".into(), PyJson::Int("150".into())),
                ("hp".into(), PyJson::Int("25".into())),
                ("maxGain".into(), PyJson::Int("-6".into())),
                ("maxDelay".into(), PyJson::Int("20".into())),
                ("maxPer".into(), PyJson::Int("4".into())),
                ("focus".into(), PyJson::Int("50".into())),
                ("active".into(), PyJson::Object(vec![])),
                ("level".into(), PyJson::Object(vec![])),
                ("decay".into(), PyJson::Int("1".into())),
            ]),
            measurements: PyJson::Array(vec![]),
            verify: PyJson::Object(vec![("A".into(), PyJson::Null), ("B".into(), PyJson::Null)]),
            tesira: PyJson::Object(vec![
                ("ip".into(), PyJson::Str(String::new())),
                ("method".into(), PyJson::Str("ttp".into())),
                ("slot".into(), PyJson::Int("1".into())),
            ]),
            snapshots: PyJson::Array(vec![]),
            extra: Vec::new(),
            measurement_calibration: std::collections::BTreeMap::new(),
            loaded: Vec::new(),
        }
    }
}

impl Project {
    pub fn from_preset(name: &str) -> Result<Self, ProjectError> {
        Ok(Self {
            config: SpeakerConfig::from_preset(name, "", false)?,
            ..Self::default()
        })
    }

    pub fn from_json(value: PyJson) -> Result<Self, ProjectError> {
        Self::from_json_with_warnings(value).map(|(project, _)| project)
    }

    pub(crate) fn from_json_with_warnings(
        value: PyJson,
    ) -> Result<(Self, Vec<crate::MigrationWarning>), ProjectError> {
        let object = value
            .object()
            .ok_or_else(|| ProjectError::Invalid("project root must be an object".into()))?;
        let mut project = Self {
            version: integer(get(object, "version"), 1, "version")?,
            name: string(get(object, "name"), ""),
            config: SpeakerConfig::from_json(
                get(object, "config")
                    .cloned()
                    .unwrap_or(PyJson::Object(vec![])),
            )?,
            dsp: DspProfile::from_json(
                get(object, "dsp")
                    .cloned()
                    .unwrap_or(PyJson::Object(vec![])),
            )?,
            target: TargetCurve::from_json(
                get(object, "target")
                    .cloned()
                    .unwrap_or(PyJson::Object(vec![])),
            )?,
            ..Self::default()
        };
        if let Some(curtain) = get(object, "curtain") {
            let values = curtain
                .array()
                .ok_or_else(|| ProjectError::Invalid("curtain must be an array".into()))?;
            if values.len() < 2 {
                return Err(ProjectError::Invalid("curtain needs two numbers".into()));
            }
            project.curtain = (float(&values[0])?, float(&values[1])?);
        }
        project.xo = object_or_empty(get(object, "xo"), "xo")?
            .into_iter()
            .map(|(key, value)| Ok((string(Some(&PyJson::Str(key)), ""), float(&value)?)))
            .collect::<Result<_, ProjectError>>()?;
        for (field, target) in [
            ("bc", &mut project.bc),
            ("mso", &mut project.mso),
            ("verify", &mut project.verify),
            ("tesira", &mut project.tesira),
        ] {
            if let Some(value) = get(object, field) {
                *target = PyJson::Object(object_or_empty(Some(value), field)?);
            }
        }
        for (field, target) in [
            ("measurements", &mut project.measurements),
            ("snapshots", &mut project.snapshots),
        ] {
            if let Some(value) = get(object, field) {
                *target = PyJson::Array(array_or_empty(Some(value), field)?);
            }
        }
        project.extra = object_or_empty(get(object, "extra"), "extra")?;
        for (key, value) in object {
            if !PROJECT_KEYS.contains(&key.as_str()) {
                set(&mut project.extra, key, value.clone());
            }
        }
        project.migrate_legacy()?;
        project.loaded = REBUILT
            .iter()
            .filter_map(|(key, _)| {
                let value = if *key == "calibration" {
                    get(&project.extra, key)
                } else {
                    get(object, key)
                };
                value.map(|value| ((*key).to_owned(), value.clone()))
            })
            .collect();
        let warnings = project.config.repair_output_numbers();
        Ok((project, warnings))
    }

    /// `value` merged with the loaded value of `key`, if the file had one.
    fn keep_unknown(&self, key: &str, value: PyJson) -> PyJson {
        match (
            REBUILT.iter().find(|(name, _)| *name == key),
            get(&self.loaded, key),
        ) {
            (Some((_, shape)), Some(old)) => merge_with_file(value, old, shape),
            _ => value,
        }
    }

    pub fn to_json(&self) -> PyJson {
        let mut out = self
            .extra
            .iter()
            .filter(|(key, _)| !PROJECT_KEYS.contains(&key.as_str()))
            .map(|(key, value)| (key.clone(), self.keep_unknown(key, value.clone())))
            .collect::<Vec<_>>();
        out.extend([
            ("version".into(), PyJson::Int(self.version.to_string())),
            ("name".into(), PyJson::Str(self.name.clone())),
            (
                "config".into(),
                self.keep_unknown("config", self.config.to_json()),
            ),
            ("dsp".into(), self.keep_unknown("dsp", self.dsp.to_json())),
            (
                "target".into(),
                self.keep_unknown("target", self.target.to_json()),
            ),
            (
                "curtain".into(),
                PyJson::Array(vec![
                    PyJson::Float(self.curtain.0),
                    PyJson::Float(self.curtain.1),
                ]),
            ),
            (
                "xo".into(),
                PyJson::Object(
                    self.xo
                        .iter()
                        .map(|(key, value)| (key.clone(), PyJson::Float(*value)))
                        .collect(),
                ),
            ),
            ("bc".into(), self.bc.clone()),
            ("mso".into(), self.mso.clone()),
            ("measurements".into(), self.measurements.clone()),
            ("verify".into(), self.verify.clone()),
            ("tesira".into(), self.tesira.clone()),
            ("snapshots".into(), self.snapshots.clone()),
        ]);
        PyJson::Object(out)
    }

    pub fn load_text(text: &str) -> Result<Self, ProjectError> {
        Self::from_json(loads(text)?)
    }
    pub fn save_text(&self) -> String {
        let mut text = dumps(&self.to_json());
        text.push('\n');
        text
    }

    fn migrate_legacy(&mut self) -> Result<(), ProjectError> {
        if self.bc.get("phase").and_then(PyJson::str) == Some("fir") {
            set(
                self.bc.object_mut().expect("bc is checked object"),
                "phase",
                PyJson::Str("minphase".into()),
            );
            set(&mut self.extra, "_migrated_fir_phase", PyJson::Bool(true));
        }
        if self.dsp.fir_taps_auto.is_none() || self.dsp.support_taps_auto.is_none() {
            let phase = self
                .bc
                .get("phase")
                .and_then(PyJson::str)
                .unwrap_or("minphase");
            let (channel, support) =
                self.dsp
                    .allocate_taps(&self.config, phase, self.bass_path_count() as i64, 256);
            if self.dsp.fir_taps_auto.is_none() {
                self.dsp.fir_taps_auto = Some(self.dsp.fir_taps == channel);
            }
            if self.dsp.support_taps_auto.is_none() {
                self.dsp.support_taps_auto = Some(self.dsp.support_taps == support);
            }
        }
        if let Some(object) = self.mso.object_mut() {
            let focus = object
                .iter()
                .find(|(key, _)| key == "focus")
                .map(|(_, value)| snap_focus(value))
                .unwrap_or(50);
            set(object, "focus", PyJson::Int(focus.to_string()));
        }
        if self.version < 2 {
            self.xo = match migrate_group_keys(&PyJson::Object(
                self.xo
                    .iter()
                    .map(|(key, value)| (key.clone(), PyJson::Float(*value)))
                    .collect(),
            )) {
                PyJson::Object(values) => values
                    .into_iter()
                    .map(|(key, value)| Ok((key, float(&value)?)))
                    .collect::<Result<_, ProjectError>>()?,
                _ => unreachable!(),
            };
            if let Some(calibration) = self
                .extra
                .iter_mut()
                .find(|(key, _)| key == "calibration")
                .map(|(_, value)| value)
                .and_then(PyJson::object_mut)
            {
                for key in ["xo", "xo_rec"] {
                    if let Some((_, value)) = calibration.iter_mut().find(|(name, _)| name == key) {
                        *value = migrate_group_keys(value);
                    }
                }
            }
        }
        if self.version < 3
            && let Some((_, calibration)) =
                self.extra.iter_mut().find(|(key, _)| key == "calibration")
        {
            *calibration = migrate_calibration_keys(calibration);
        }
        if self.version < PROJECT_VERSION {
            self.version = PROJECT_VERSION;
        }
        Ok(())
    }

    pub fn bass_path_count(&self) -> usize {
        self.extra
            .iter()
            .find(|(key, _)| key == "bass")
            .map(|(_, value)| value)
            .and_then(|value| value.get("paths"))
            .map_or(0, |paths| match paths {
                PyJson::Array(value) => value.len(),
                PyJson::Object(value) => value.len(),
                _ => 0,
            })
    }

    pub fn apply_recommended_taps(&mut self) -> bool {
        let phase = if self.bc.get("phase").and_then(PyJson::str) == Some("mixed") {
            "mixed"
        } else {
            "minphase"
        };
        let (channel, support) =
            self.dsp
                .allocate_taps(&self.config, phase, self.bass_path_count() as i64, 256);
        let mut changed = false;
        if self.dsp.fir_taps_auto == Some(true) && self.dsp.fir_taps != channel {
            self.dsp.fir_taps = channel;
            changed = true;
        }
        if self.dsp.support_taps_auto == Some(true) && self.dsp.support_taps != support {
            self.dsp.support_taps = support;
            changed = true;
        }
        changed
    }

    pub fn validate(&self) -> Vec<String> {
        self.config.validate()
    }
}
