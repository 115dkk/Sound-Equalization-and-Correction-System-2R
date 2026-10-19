#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Device policy and CUDA adapter for the ideal-FIR sweep (ADR 0008).

use secs_compute::{CancelToken, MemoryBudget, MemoryPermit};
use secs_cuda_sys::{Device, ErrorKind, Runtime, SpectralSystem};
use secs_engine::mimo::{AccelerationError, AccelerationSelection, RidgeBackend, ShiftedSystem};
use secs_types::compute::{
    ComputeBackend, ComputePreference, ComputeReason, ComputeStatus, GpuDeviceInfo,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
};

/// Small systems do not amortize device dispatch and synchronization in Auto.
pub const AUTO_MINIMUM_DIMENSION: usize = 1024;
const RECOMMENDED_MEMORY: u64 = 2 * 1024 * 1024 * 1024;
const RECOMMENDED_MULTIPROCESSORS: u32 = 16;

fn classify(device: &Device) -> GpuDeviceInfo {
    let supported = !device.integrated && (device.compute_major, device.compute_minor) >= (7, 5);
    let reason = if device.integrated {
        ComputeReason::Integrated
    } else if !supported {
        ComputeReason::Legacy
    } else if device.memory_bytes < RECOMMENDED_MEMORY {
        ComputeReason::InsufficientMemory
    } else if device.multiprocessors < RECOMMENDED_MULTIPROCESSORS {
        ComputeReason::LowPerformance
    } else {
        ComputeReason::Recommended
    };
    GpuDeviceInfo {
        id: device.id,
        name: device.name.clone(),
        compute_major: device.compute_major,
        compute_minor: device.compute_minor,
        multiprocessors: device.multiprocessors,
        memory_bytes: device.memory_bytes,
        integrated: device.integrated,
        supported,
        recommended: reason == ComputeReason::Recommended,
        reason,
    }
}

fn status(
    preference: ComputePreference,
    mut devices: Vec<GpuDeviceInfo>,
    unavailable: ComputeReason,
) -> ComputeStatus {
    devices.sort_by_key(|d| d.id);
    let best = |recommended: bool| {
        devices
            .iter()
            .filter(|d| {
                if recommended {
                    d.recommended
                } else {
                    d.supported
                }
            })
            .max_by_key(|d| {
                (
                    d.multiprocessors,
                    d.memory_bytes,
                    d.compute_major,
                    d.compute_minor,
                    std::cmp::Reverse(d.id),
                )
            })
    };
    let chosen = match preference {
        ComputePreference::Auto => best(true),
        ComputePreference::Gpu => best(false),
        ComputePreference::Cpu => None,
    };
    let gpu_available = devices.iter().any(|d| d.supported);
    let selected = if chosen.is_some() {
        ComputeBackend::Cuda
    } else {
        ComputeBackend::Cpu
    };
    let reason = match preference {
        ComputePreference::Cpu => ComputeReason::UserCpu,
        ComputePreference::Gpu if chosen.is_some() => ComputeReason::UserGpu,
        ComputePreference::Auto if chosen.is_some() => ComputeReason::Recommended,
        _ => devices
            .iter()
            .max_by_key(|d| {
                (
                    d.supported,
                    d.multiprocessors,
                    d.memory_bytes,
                    std::cmp::Reverse(d.id),
                )
            })
            .map_or(unavailable, |d| d.reason),
    };
    let device = chosen
        .or_else(|| {
            devices.iter().max_by_key(|d| {
                (
                    d.supported,
                    d.multiprocessors,
                    d.memory_bytes,
                    std::cmp::Reverse(d.id),
                )
            })
        })
        .cloned();
    ComputeStatus {
        preference,
        selected,
        reason,
        gpu_available,
        device,
        devices,
    }
}

fn reason(error: &secs_cuda_sys::Error) -> ComputeReason {
    match error.kind {
        ErrorKind::UnsupportedPlatform => ComputeReason::UnsupportedPlatform,
        ErrorKind::RuntimeUnavailable => ComputeReason::RuntimeUnavailable,
        ErrorKind::NoDevice => ComputeReason::NoDevice,
        ErrorKind::InsufficientMemory => ComputeReason::InsufficientMemory,
        _ => ComputeReason::DeviceFailure,
    }
}

fn acceleration_error(error: secs_cuda_sys::Error) -> AccelerationError {
    if error.kind == ErrorKind::Cancelled {
        AccelerationError::Cancelled
    } else {
        AccelerationError::failed(reason(&error), error.detail)
    }
}

/// Successful runtime loading is cached; failures can be retried after setup.
/// This function never runs the numerical workload merely to display status.
pub fn select(preference: ComputePreference) -> AccelerationSelection {
    static RUNTIME: OnceLock<Mutex<Option<Runtime>>> = OnceLock::new();
    static LEASES: OnceLock<Mutex<BTreeMap<u32, Arc<MemoryBudget>>>> = OnceLock::new();
    let runtime = {
        let mut cached = RUNTIME
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(runtime) = cached.as_ref() {
            Ok(runtime.clone())
        } else {
            let loaded = Runtime::load();
            if let Ok(runtime) = &loaded {
                *cached = Some(runtime.clone());
            }
            loaded
        }
    };
    let (devices, unavailable) = match &runtime {
        Ok(runtime) => match runtime.devices() {
            Ok(devices) => (
                devices.iter().map(classify).collect(),
                ComputeReason::NoDevice,
            ),
            Err(error) => (Vec::new(), reason(&error)),
        },
        Err(error) => (Vec::new(), reason(error)),
    };
    let status = status(preference, devices, unavailable);
    let backend = if status.selected == ComputeBackend::Cuda {
        let device = status.device.as_ref().expect("CUDA selection has a device");
        let mut leases = LEASES
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let lease = leases
            .entry(device.id)
            .or_insert_with(|| Arc::new(MemoryBudget::new(1)))
            .clone();
        runtime.as_ref().ok().map(|runtime| {
            Arc::new(CudaBackend {
                runtime: runtime.clone(),
                device: device.id,
                lease,
            }) as Arc<dyn RidgeBackend>
        })
    } else {
        None
    };
    AccelerationSelection { status, backend }
}

#[derive(Debug)]
struct CudaBackend {
    runtime: Runtime,
    device: u32,
    lease: Arc<MemoryBudget>,
}

impl RidgeBackend for CudaBackend {
    fn factor<'a>(
        &'a self,
        ata: &'a [f64],
        atb: &'a [f64],
        cancel: &CancelToken,
    ) -> Result<Box<dyn ShiftedSystem + 'a>, AccelerationError> {
        let permit = self
            .lease
            .acquire(1, cancel)
            .map_err(|_| AccelerationError::Cancelled)?;
        let system = self
            .runtime
            .factor(self.device, ata, atb, &|| cancel.is_cancelled())
            .map_err(acceleration_error)?;
        Ok(Box::new(Prepared {
            system,
            _permit: permit,
            ata,
            atb,
        }))
    }
    fn minimum_dimension(&self) -> usize {
        AUTO_MINIMUM_DIMENSION
    }
}

struct Prepared<'a> {
    system: SpectralSystem,
    _permit: MemoryPermit<'a>,
    ata: &'a [f64],
    atb: &'a [f64],
}
impl ShiftedSystem for Prepared<'_> {
    fn solve(&mut self, lambda: f64, cancel: &CancelToken) -> Result<Vec<f64>, AccelerationError> {
        let mut answer = self
            .system
            .solve(lambda, &|| cancel.is_cancelled())
            .map_err(acceleration_error)?;
        // The reference RidgeSweep performs one refinement in these exact CPU
        // accumulation coordinates. Reuse the GPU factor for the correction.
        let n = self.atb.len();
        let residual: Vec<_> = self
            .ata
            .chunks_exact(n)
            .enumerate()
            .map(|(i, row)| {
                let mut dot = 0.0;
                for j in 0..n {
                    dot += (row[j] + if i == j { lambda } else { 0.0 }) * answer[j];
                }
                self.atb[i] - dot
            })
            .collect();
        let correction = self
            .system
            .solve_rhs(lambda, &residual, &|| cancel.is_cancelled())
            .map_err(acceleration_error)?;
        for (x, dx) in answer.iter_mut().zip(correction) {
            *x += dx;
        }
        if answer.iter().any(|v| !v.is_finite()) {
            return Err(AccelerationError::failed(
                ComputeReason::DeviceFailure,
                "nonfinite refined CUDA solution",
            ));
        }
        Ok(answer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn device(id: u32) -> Device {
        Device {
            id,
            name: format!("GPU {id}"),
            compute_major: 7,
            compute_minor: 5,
            multiprocessors: 48,
            memory_bytes: 8 * 1024 * 1024 * 1024,
            integrated: false,
        }
    }

    #[test]
    fn device_policy_rejects_integrated_legacy_and_small_devices() {
        let mut d = device(0);
        d.integrated = true;
        assert_eq!(classify(&d).reason, ComputeReason::Integrated);
        assert!(!classify(&d).supported);
        d = device(0);
        d.compute_minor = 0;
        assert_eq!(classify(&d).reason, ComputeReason::Legacy);
        d = device(0);
        d.multiprocessors = 15;
        assert_eq!(classify(&d).reason, ComputeReason::LowPerformance);
        assert!(classify(&d).supported);
        d = device(0);
        d.memory_bytes = RECOMMENDED_MEMORY - 1;
        assert_eq!(classify(&d).reason, ComputeReason::InsufficientMemory);
        d = device(0);
        d.multiprocessors = 16;
        d.memory_bytes = RECOMMENDED_MEMORY;
        assert!(classify(&d).recommended);
    }

    #[test]
    fn preference_preserves_cpu_and_selects_recommended_before_unusable_hardware() {
        let mut bad = device(1);
        bad.compute_major = 6;
        bad.multiprocessors = 1000;
        let devices = vec![classify(&bad), classify(&device(2)), classify(&device(0))];
        let auto = status(
            ComputePreference::Auto,
            devices.clone(),
            ComputeReason::NoDevice,
        );
        assert_eq!(auto.selected, ComputeBackend::Cuda);
        assert_eq!(auto.device.unwrap().id, 0);
        let cpu = status(ComputePreference::Cpu, devices, ComputeReason::NoDevice);
        assert_eq!(cpu.selected, ComputeBackend::Cpu);
        assert_eq!(cpu.reason, ComputeReason::UserCpu);
        assert!(cpu.gpu_available);
        let missing = status(
            ComputePreference::Auto,
            Vec::new(),
            ComputeReason::RuntimeUnavailable,
        );
        assert_eq!(missing.selected, ComputeBackend::Cpu);
        assert_eq!(missing.reason, ComputeReason::RuntimeUnavailable);
    }

    #[test]
    fn explicit_gpu_can_select_supported_hardware_below_auto_performance_floor() {
        let mut d = device(0);
        d.multiprocessors = 8;
        assert_eq!(
            status(
                ComputePreference::Auto,
                vec![classify(&d)],
                ComputeReason::NoDevice
            )
            .selected,
            ComputeBackend::Cpu
        );
        assert_eq!(
            status(
                ComputePreference::Gpu,
                vec![classify(&d)],
                ComputeReason::NoDevice
            )
            .selected,
            ComputeBackend::Cuda
        );
    }
}
