use crate::{Project, PyJson, ordered_speakers};
use std::collections::{BTreeMap, BTreeSet};

pub const SWEEP_LENGTHS: [&str; 7] = ["64k", "128k", "256k", "512k", "1M", "2M", "4M"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingReference {
    Acoustic,
    Loopback,
    None,
}

impl TimingReference {
    pub fn as_disk_str(self) -> &'static str {
        match self {
            Self::Acoustic => "acoustic",
            Self::Loopback => "loopback",
            Self::None => "none",
        }
    }

    fn from_disk(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "loopback" => Self::Loopback,
            "none" => Self::None,
            _ => Self::Acoustic,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeasurementPlan {
    pub roles: Vec<String>,
    pub outputs: BTreeMap<String, i64>,
    pub positions: Vec<i64>,
    pub output_device: String,
    pub input_device: String,
    pub input_channel: i64,
    pub sweep_level_db: f64,
    pub sweep_length: String,
    pub repetitions: i64,
    pub timing_reference: TimingReference,
    pub timing_ref_output: Option<i64>,
    pub naming: String,
    pub start_frequency: f64,
    pub end_frequency: f64,
    pub sample_rate: i64,
}

impl Default for MeasurementPlan {
    fn default() -> Self {
        Self {
            roles: Vec::new(),
            outputs: BTreeMap::new(),
            positions: Vec::new(),
            output_device: String::new(),
            input_device: String::new(),
            input_channel: 1,
            sweep_level_db: -20.0,
            sweep_length: "256k".into(),
            repetitions: 1,
            timing_reference: TimingReference::Acoustic,
            timing_ref_output: None,
            naming: "{role} P{pos}".into(),
            start_frequency: 20.0,
            end_frequency: 20_000.0,
            sample_rate: 48_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanValidationError {
    pub code: &'static str,
    pub params: BTreeMap<String, String>,
}

impl PlanValidationError {
    fn new(code: &'static str) -> Self {
        Self {
            code,
            params: BTreeMap::new(),
        }
    }

    fn role(code: &'static str, role: &str) -> Self {
        Self {
            code,
            params: BTreeMap::from([("role".into(), role.into())]),
        }
    }
}

impl MeasurementPlan {
    pub fn name_for(&self, role: &str, pos: i64) -> String {
        let (base, band) = split_role(role);
        let mut out = String::new();
        let mut rest = self.naming.as_str();
        while let Some(open) = rest.find('{') {
            out.push_str(&rest[..open]);
            let Some(close) = rest[open + 1..].find('}') else {
                return format!("{role} P{pos}");
            };
            let key = &rest[open + 1..open + 1 + close];
            match key {
                "role" => out.push_str(role),
                "pos" => out.push_str(&pos.to_string()),
                "base" => out.push_str(&base),
                "band" => out.push_str(band.as_deref().unwrap_or("")),
                _ => return format!("{role} P{pos}"),
            }
            rest = &rest[open + close + 2..];
        }
        out.push_str(rest);
        out
    }

    pub fn output_for(&self, role: &str) -> i64 {
        self.outputs.get(role).copied().unwrap_or(1)
    }

    pub fn validate(&self) -> Vec<PlanValidationError> {
        let mut errors = Vec::new();
        if self.roles.is_empty() {
            errors.push(PlanValidationError::new("measurement.roles.empty"));
        }
        if self.roles.iter().collect::<BTreeSet<_>>().len() != self.roles.len() {
            errors.push(PlanValidationError::new("measurement.roles.duplicate"));
        }
        if self.positions.is_empty() {
            errors.push(PlanValidationError::new("measurement.positions.empty"));
        }
        if self.positions.iter().collect::<BTreeSet<_>>().len() != self.positions.len() {
            errors.push(PlanValidationError::new("measurement.positions.duplicate"));
        }
        for role in &self.roles {
            if !self.outputs.contains_key(role) {
                errors.push(PlanValidationError::role(
                    "measurement.output.missing",
                    role,
                ));
            }
        }
        if self.repetitions != 1 && self.timing_reference == TimingReference::Acoustic {
            errors.push(PlanValidationError::new(
                "measurement.acoustic_repetitions.invalid",
            ));
        }
        if !SWEEP_LENGTHS.contains(&self.sweep_length.as_str()) {
            let mut error = PlanValidationError::new("measurement.sweep_length.invalid");
            error
                .params
                .insert("allowed".into(), SWEEP_LENGTHS.join(" / "));
            errors.push(error);
        }
        errors
    }

    pub fn total_steps(&self) -> usize {
        self.roles.len() * self.positions.len()
    }
}

pub fn split_role(role: &str) -> (String, Option<String>) {
    if let Some((base, band)) = role.split_once(':') {
        let band = band.trim();
        (base.trim().into(), (!band.is_empty()).then(|| band.into()))
    } else {
        (role.trim().into(), None)
    }
}

pub fn sweep_length_samples(length: &str) -> Result<i64, std::num::ParseFloatError> {
    let text = length.trim().to_ascii_lowercase();
    let (number, scale) = if let Some(value) = text.strip_suffix('k') {
        (value, 1024.0)
    } else if let Some(value) = text.strip_suffix('m') {
        (value, 1024.0 * 1024.0)
    } else {
        (text.as_str(), 1.0)
    };
    Ok((number.parse::<f64>()? * scale) as i64)
}

pub fn plan_from_project(
    project: &Project,
    positions: impl IntoIterator<Item = i64>,
) -> MeasurementPlan {
    let mut plan = MeasurementPlan {
        positions: positions.into_iter().collect(),
        output_device: project.config.device.clone(),
        ..MeasurementPlan::default()
    };
    for speaker in ordered_speakers(&project.config.speakers, true) {
        for band in &speaker.bands {
            let role = if speaker.bands.len() <= 1 {
                speaker.role.clone()
            } else {
                format!("{}:{}", speaker.role, band.band)
            };
            plan.outputs.insert(role.clone(), band.out);
            plan.roles.push(role);
        }
    }
    plan
}

pub fn measurement_roles(project: &Project) -> Vec<(String, i64)> {
    let mut result = Vec::new();
    for speaker in ordered_speakers(&project.config.speakers, false) {
        if speaker.bands.len() <= 1 {
            result.push((
                speaker.role.clone(),
                speaker.bands.first().map_or(1, |band| band.out),
            ));
        } else {
            result.extend(
                speaker
                    .bands
                    .iter()
                    .map(|band| (format!("{}:{}", speaker.role, band.band), band.out)),
            );
        }
    }
    result
}

pub fn default_ref_role(project: &Project) -> String {
    let roles = measurement_roles(project);
    let mains = roles
        .iter()
        .filter(|(role, _)| !role.to_ascii_uppercase().starts_with("SW"))
        .collect::<Vec<_>>();
    let mut wanted = mains
        .iter()
        .find(|(role, _)| role.split(':').next() == Some("R"))
        .copied()
        .or_else(|| mains.first().copied())
        .map(|(role, _)| role.clone())
        .or_else(|| roles.first().map(|(role, _)| role.clone()))
        .unwrap_or_default();
    if let Some((base, _)) = wanted.split_once(':')
        && let Some((role, _)) = roles
            .iter()
            .find(|(role, _)| role.starts_with(&format!("{base}:")) && role.ends_with(":HF"))
    {
        wanted = role.clone();
    }
    wanted
}

pub fn timing_settings(project: &Project) -> (TimingReference, Option<i64>) {
    let devices = devices(project);
    let kind = TimingReference::from_disk(&py_string(
        devices.and_then(|value| value.get("timing_reference")),
        "acoustic",
    ));
    match kind {
        TimingReference::Acoustic => {
            let table = measurement_roles(project)
                .into_iter()
                .collect::<BTreeMap<_, _>>();
            let stored = py_string(devices.and_then(|value| value.get("timing_ref_role")), "");
            let role = if table.contains_key(&stored) {
                stored
            } else {
                default_ref_role(project)
            };
            (kind, table.get(&role).copied())
        }
        TimingReference::Loopback => {
            let channel = devices.and_then(|value| value.get("timing_ref_output"));
            let output = channel
                .filter(|value| value.is_truthy())
                .map(|value| py_i64(Some(value), 0));
            (kind, output)
        }
        TimingReference::None => (kind, None),
    }
}

pub fn start_delay_seconds(text: &str) -> i64 {
    let digits = text
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>();
    digits.parse().unwrap_or(0)
}

pub fn sweep_seconds(project: &Project) -> f64 {
    let devices = devices(project);
    let rate = py_f64(devices.and_then(|value| value.get("sample_rate")), 48_000.0).max(1.0);
    let length = py_string(devices.and_then(|value| value.get("sweep_length")), "256k");
    let samples = sweep_length_samples(&length).unwrap_or(262_144) as f64;
    let sweep = samples / rate;
    if py_string(
        devices.and_then(|value| value.get("timing_reference")),
        "acoustic",
    )
    .to_ascii_lowercase()
    .starts_with("acoustic")
    {
        2.0 * sweep + 0.7
    } else {
        sweep + 2.5
    }
}

pub fn position_seconds(project: &Project) -> f64 {
    let bands = project
        .config
        .speakers
        .iter()
        .map(|speaker| speaker.bands.len())
        .sum::<usize>() as f64;
    let delay = project
        .extra
        .iter()
        .find(|(key, _)| key == "start_delay")
        .map(|(_, value)| py_string(Some(value), ""))
        .map_or(0, |value| start_delay_seconds(&value));
    bands * sweep_seconds(project) * normalized_repetitions(project) as f64 + delay as f64
}

fn normalized_repetitions(project: &Project) -> i64 {
    let devices = devices(project);
    let mut repetitions = py_i64(devices.and_then(|value| value.get("repetitions")), 1);
    if ![1, 2, 4, 8].contains(&repetitions) {
        repetitions = 1;
    }
    if py_string(
        devices.and_then(|value| value.get("timing_reference")),
        "acoustic",
    )
    .to_ascii_lowercase()
    .starts_with("acoustic")
    {
        repetitions = 1;
    }
    repetitions
}

fn devices(project: &Project) -> Option<&PyJson> {
    project
        .extra
        .iter()
        .find(|(key, _)| key == "devices")
        .map(|(_, value)| value)
        .filter(|value| value.object().is_some())
}

fn py_string(value: Option<&PyJson>, default: &str) -> String {
    crate::project::string(value, default)
}

fn py_i64(value: Option<&PyJson>, default: i64) -> i64 {
    crate::project::integer(value, default, "measurement option").unwrap_or(default)
}

fn py_f64(value: Option<&PyJson>, default: f64) -> f64 {
    value
        .and_then(|value| crate::project::float(value).ok())
        .unwrap_or(default)
}
