use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

/// Transport-level read snapshot, adapted to the IPC vocabulary by the service.
#[derive(Debug, Clone, Default)]
pub struct InfoSnapshot {
    pub running: bool,
    pub reachable: bool,
    pub version: Option<String>,
    pub pro: Option<bool>,
    pub blocking: Option<bool>,
    pub input_calibration_file: Option<String>,
    pub input_devices: Option<Vec<DeviceInfo>>,
    pub output_devices: Option<Vec<DeviceInfo>>,
    pub input_channel_names: Option<Vec<String>>,
    pub sample_rate: Option<f64>,
    pub sweep: Option<SweepSnapshot>,
    pub level_dbfs: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct SweepSnapshot {
    pub start_hz: Option<f64>,
    pub end_hz: Option<f64>,
    pub length: Option<String>,
}

pub const REW_API_PORT: u16 = 4735;
pub const CONNECT_TIMEOUT: Duration = Duration::from_millis(600);
pub const READ_TIMEOUT: Duration = Duration::from_secs(5);
pub const MEASURE_TIMEOUT: Duration = Duration::from_secs(180);
pub const DOWN_FAST_FAIL: Duration = Duration::from_millis(2500);
pub const PROBE_HOLD: Duration = Duration::from_secs(6);
pub const LIST_CACHE: Duration = Duration::from_secs(2);
pub const SWEEP_LENGTH_NAMES: [&str; 7] = ["64k", "128k", "256k", "512k", "1M", "2M", "4M"];
pub const PHASES: [&str; 5] = ["sweep", "running", "process", "done", "cancelled"];
pub const TIMING_REFERENCES: [&str; 5] = ["None", "Loopback", "Loopback cal", "Acoustic", "Wired"];
pub const MEASURE_COMMANDS: [&str; 8] = [
    "SPL",
    "Filtered SPL",
    "Impedance",
    "Impedance open cal",
    "Impedance short cal",
    "Impedance ref cal",
    "Check levels",
    "Cancel",
];
pub const LOW_SNR_DB: f64 = 20.0;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RewError {
    #[error("REW connection ({path}): {message}")]
    Connection { path: String, message: String },
    #[error("REW HTTP {status} ({path}): {message}")]
    Http {
        status: u16,
        path: String,
        message: String,
    },
    #[error("REW schema ({path}): {message}")]
    Schema { path: String, message: String },
    #[error("REW Pro required ({status} {path})")]
    ProRequired { status: u16, path: String },
    #[error("REW measurement cancelled")]
    Cancelled,
}
impl RewError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Connection { .. } => "rew.connection",
            Self::Http { .. } => "rew.http",
            Self::Schema { .. } => "rew.schema",
            Self::ProRequired { .. } => "rew.pro_required",
            Self::Cancelled => "rew.cancelled",
        }
    }
    pub(crate) fn schema(path: &str, message: impl ToString) -> Self {
        Self::Schema {
            path: path.into(),
            message: message.to_string(),
        }
    }
    pub(crate) fn connection(path: &str, message: impl ToString) -> Self {
        Self::Connection {
            path: path.into(),
            message: message.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);
impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub(crate) fn wait(&self, duration: Duration) -> bool {
        let start = std::time::Instant::now();
        while !self.is_cancelled() && start.elapsed() < duration {
            std::thread::sleep(
                (duration - start.elapsed().min(duration)).min(Duration::from_millis(10)),
            );
        }
        self.is_cancelled()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasurementPlan {
    pub roles: Vec<String>,
    pub outputs: BTreeMap<String, u32>,
    pub positions: Vec<u32>,
    pub output_device: String,
    pub input_device: String,
    pub input_channel: u32,
    pub sweep_level_db: f64,
    pub sweep_length: String,
    pub repetitions: u32,
    pub timing_reference: String,
    pub timing_ref_output: Option<u32>,
    pub naming: String,
    pub start_frequency: f64,
    pub end_frequency: f64,
    pub sample_rate: u32,
}
impl Default for MeasurementPlan {
    fn default() -> Self {
        Self {
            roles: vec![],
            outputs: BTreeMap::new(),
            positions: vec![],
            output_device: String::new(),
            input_device: String::new(),
            input_channel: 1,
            sweep_level_db: -20.0,
            sweep_length: "256k".into(),
            repetitions: 1,
            timing_reference: "acoustic".into(),
            timing_ref_output: None,
            naming: "{role} P{pos}".into(),
            start_frequency: 20.0,
            end_frequency: 20000.0,
            sample_rate: 48000,
        }
    }
}
impl MeasurementPlan {
    pub fn name_for(&self, role: &str, pos: u32) -> String {
        let (base, band) = split_role(role);
        let name = self
            .naming
            .replace("{role}", role)
            .replace("{pos}", &pos.to_string())
            .replace("{base}", base)
            .replace("{band}", band.unwrap_or(""));
        if name.contains(['{', '}']) {
            format!("{role} P{pos}")
        } else {
            name
        }
    }
    pub fn output_for(&self, role: &str) -> u32 {
        self.outputs.get(role).copied().unwrap_or(1)
    }
    pub fn total_steps(&self) -> usize {
        self.roles.len() * self.positions.len()
    }
    pub fn validate(&self) -> Vec<&'static str> {
        let mut errors = vec![];
        if self.roles.is_empty() {
            errors.push("rew.plan.no_roles");
        }
        if self.positions.is_empty() {
            errors.push("rew.plan.no_positions");
        }
        if self
            .roles
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != self.roles.len()
        {
            errors.push("rew.plan.duplicate_roles");
        }
        if self
            .positions
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != self.positions.len()
        {
            errors.push("rew.plan.duplicate_positions");
        }
        if self.roles.iter().any(|r| !self.outputs.contains_key(r)) {
            errors.push("rew.plan.missing_output");
        }
        if self.repetitions != 1 && self.timing_reference.to_lowercase().starts_with("acoustic") {
            errors.push("rew.plan.acoustic_repetitions");
        }
        if !SWEEP_LENGTH_NAMES.contains(&self.sweep_length.as_str()) {
            errors.push("rew.plan.sweep_length");
        }
        errors
    }
}
pub fn split_role(role: &str) -> (&str, Option<&str>) {
    match role.split_once(':') {
        Some((base, band)) => (
            base.trim(),
            (!band.trim().is_empty()).then_some(band.trim()),
        ),
        None => (role.trim(), None),
    }
}
pub fn sweep_length_samples(length: &str) -> Result<u64, RewError> {
    let text = length.trim().to_lowercase();
    let (number, scale) = if let Some(n) = text.strip_suffix('k') {
        (n, 1024.0)
    } else if let Some(n) = text.strip_suffix('m') {
        (n, 1048576.0)
    } else {
        (text.as_str(), 1.0)
    };
    let value = number
        .parse::<f64>()
        .map_err(|e| RewError::schema("sweep_length", e))?
        * scale;
    if !value.is_finite() || value < 0.0 || value >= u64::MAX as f64 {
        return Err(RewError::schema("sweep_length", "invalid length"));
    }
    Ok(value as u64)
}
pub fn sweep_length_name(length: impl ToString) -> Result<&'static str, RewError> {
    let n = sweep_length_samples(&length.to_string())?;
    Ok(SWEEP_LENGTH_NAMES
        .into_iter()
        .min_by_key(|name| sweep_length_samples(name).unwrap_or(0).abs_diff(n))
        .unwrap_or("256k"))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpulseResponse {
    pub ir: Vec<f32>,
    pub sample_rate: u32,
    pub t0_s: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasurementResult {
    pub role: String,
    pub pos: u32,
    pub uuid: Option<String>,
    pub ir: Vec<f32>,
    pub sample_rate: u32,
    pub name: String,
    pub spl_db: Option<f64>,
    pub t0_s: f64,
    pub spl_offset_db: Option<f64>,
    pub snr_db: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub channels: usize,
    pub kind: String,
    pub extra: Value,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Level {
    pub input_dbfs: f64,
    pub peak_dbfs: f64,
    pub spl_db: Option<f64>,
    pub clip: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    pub role: String,
    pub pos: u32,
    pub phase: String,
    pub percent: f64,
    pub message: String,
}
pub fn progress(role: &str, pos: u32, phase: &str, percent: f64, message: &str) -> Progress {
    Progress {
        role: role.into(),
        pos,
        phase: phase.into(),
        percent: percent.clamp(0.0, 100.0),
        message: message.into(),
    }
}
pub type ProgressCallback = Arc<dyn Fn(Progress) + Send + Sync>;
pub type LevelCallback = Arc<dyn Fn(Level) + Send + Sync>;
pub(crate) fn emit<T>(callback: &Option<Arc<dyn Fn(T) + Send + Sync>>, value: T) {
    if let Some(callback) = callback {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(value)));
    }
}
