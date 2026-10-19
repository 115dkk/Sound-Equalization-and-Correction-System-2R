//! Compute policy injection and cache identity, independent of host hardware.
use secs_engine::mimo::AccelerationSelection;
use secs_types::compute::{ComputeBackend, ComputePreference, ComputeReason, ComputeStatus};
use secs_types::ipc::{ErrorCode, IpcError};
use serde::{Deserialize, Serialize};

use crate::{Service, lock};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ComputeIdentity {
    preference: ComputePreference,
    backend: ComputeBackend,
    device: Option<(u32, String)>,
}

// Caches written before acceleration always contain CPU calculations.
impl Default for ComputeIdentity {
    fn default() -> Self {
        Self {
            preference: ComputePreference::Cpu,
            backend: ComputeBackend::Cpu,
            device: None,
        }
    }
}

impl From<&ComputeStatus> for ComputeIdentity {
    fn from(status: &ComputeStatus) -> Self {
        Self {
            preference: status.preference,
            backend: status.selected,
            device: (status.selected == ComputeBackend::Cuda)
                .then(|| {
                    status
                        .device
                        .as_ref()
                        .map(|device| (device.id, device.name.clone()))
                })
                .flatten(),
        }
    }
}

pub(crate) fn cpu_selection(preference: ComputePreference) -> AccelerationSelection {
    AccelerationSelection {
        status: ComputeStatus {
            preference,
            reason: if preference == ComputePreference::Cpu {
                ComputeReason::UserCpu
            } else {
                ComputeReason::NoDevice
            },
            ..Default::default()
        },
        backend: None,
    }
}

impl Service {
    pub(crate) fn acceleration(&self) -> AccelerationSelection {
        let preference = self
            .deps
            .compute_override
            .unwrap_or_else(|| lock(&self.app_settings).compute);
        (self.deps.acceleration)(preference)
    }

    pub(crate) fn require_acceleration(&self) -> Result<AccelerationSelection, IpcError> {
        let selection = self.acceleration();
        if selection.status.preference == ComputePreference::Gpu
            && (!selection.status.gpu_available || selection.backend.is_none())
        {
            return Err(IpcError::new(
                ErrorCode::ComputeUnavailable,
                "no supported GPU is available",
            )
            .with_param(
                "reason",
                serde_json::to_value(selection.status.reason)
                    .expect("compute reason serializes")
                    .as_str()
                    .expect("compute reason is a string"),
            ));
        }
        Ok(selection)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_identity_tracks_policy_and_device_but_not_availability_of_unused_devices() {
        let cpu = cpu_selection(ComputePreference::Cpu).status;
        assert_eq!(ComputeIdentity::from(&cpu), ComputeIdentity::default());
        let auto = cpu_selection(ComputePreference::Auto).status;
        assert_ne!(ComputeIdentity::from(&auto), ComputeIdentity::from(&cpu));
        let mut selected = auto.clone();
        selected.selected = ComputeBackend::Cuda;
        assert_ne!(
            ComputeIdentity::from(&auto),
            ComputeIdentity::from(&selected)
        );
        selected.device = Some(secs_types::compute::GpuDeviceInfo {
            id: 0,
            name: "GPU A".into(),
            compute_major: 7,
            compute_minor: 5,
            multiprocessors: 48,
            memory_bytes: 8 << 30,
            integrated: false,
            supported: true,
            recommended: true,
            reason: ComputeReason::Recommended,
        });
        let first_device = ComputeIdentity::from(&selected);
        selected.device.as_mut().unwrap().id = 1;
        assert_ne!(first_device, ComputeIdentity::from(&selected));
        selected.device.as_mut().unwrap().id = 0;
        selected.device.as_mut().unwrap().name = "GPU B".into();
        assert_ne!(first_device, ComputeIdentity::from(&selected));
        let mut cpu_with_gpu = cpu.clone();
        cpu_with_gpu.gpu_available = true;
        assert_eq!(
            ComputeIdentity::from(&cpu),
            ComputeIdentity::from(&cpu_with_gpu)
        );
    }
}
