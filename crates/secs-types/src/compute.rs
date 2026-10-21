//! Application-local compute selection and actual-run provenance.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ComputePreference {
    /// Prefer a capable, supported discrete GPU; otherwise use the CPU.
    #[default]
    Auto,
    Cpu,
    Gpu,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ComputeBackend {
    #[default]
    Cpu,
    Cuda,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ComputeReason {
    Recommended,
    UserCpu,
    UserGpu,
    #[default]
    NoDevice,
    RuntimeUnavailable,
    UnsupportedPlatform,
    Integrated,
    Legacy,
    LowPerformance,
    InsufficientMemory,
    DeviceFailure,
    SmallProblem,
    /// IIR realization keeps the CPU target because its optimizer amplifies tiny differences.
    IirReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GpuDeviceInfo {
    pub id: u32,
    pub name: String,
    pub compute_major: u32,
    pub compute_minor: u32,
    pub multiprocessors: u32,
    #[ts(type = "number")]
    pub memory_bytes: u64,
    pub integrated: bool,
    /// The runtime can execute this device's f64 path.
    pub supported: bool,
    /// Hardware suitability for the automatic preference.
    pub recommended: bool,
    pub reason: ComputeReason,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ComputeStatus {
    pub preference: ComputePreference,
    pub selected: ComputeBackend,
    pub reason: ComputeReason,
    pub gpu_available: bool,
    pub device: Option<GpuDeviceInfo>,
    pub devices: Vec<GpuDeviceInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ComputeReport {
    pub requested: ComputePreference,
    /// The backend that actually produced accelerated systems, not a preference.
    pub backend: ComputeBackend,
    pub device: Option<String>,
    /// Identifies the accelerated stage; other correction stages remain on the CPU.
    pub stage: Option<ComputeStage>,
    pub accelerated_systems: u32,
    pub cpu_systems: u32,
    pub fallbacks: u32,
    pub reason: Option<ComputeReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ComputeStage {
    IdealFirSweep,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_defaults_are_automatic_with_cpu_provenance() {
        assert_eq!(ComputePreference::default(), ComputePreference::Auto);
        assert_eq!(ComputeReport::default().backend, ComputeBackend::Cpu);
        assert!(!ComputeStatus::default().gpu_available);
    }
}
