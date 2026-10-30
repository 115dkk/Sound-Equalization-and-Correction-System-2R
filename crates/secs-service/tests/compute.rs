#![forbid(unsafe_code)]

use secs_engine::mimo::{AccelerationSelection, CpuRidgeBackend};
use secs_service::{Deps, NoEvents, Service};
use secs_types::{compute::*, ipc::*};
use std::{fs, sync::Arc};

fn call(service: &Service, request: Request) -> Response {
    match service.call(request, Arc::new(NoEvents)) {
        Envelope::Ok(response) => *response,
        Envelope::Err(error) => panic!("{error:?}"),
    }
}

fn status(service: &Service) -> ComputeStatus {
    let Response::ComputeStatus(status) = call(service, Request::ComputeStatus) else {
        panic!()
    };
    status
}

fn fake_gpu(preference: ComputePreference) -> AccelerationSelection {
    let device = GpuDeviceInfo {
        id: 0,
        name: "Test discrete GPU".into(),
        compute_major: 7,
        compute_minor: 5,
        multiprocessors: 48,
        memory_bytes: 8 << 30,
        integrated: false,
        supported: true,
        recommended: true,
        reason: ComputeReason::Recommended,
    };
    let gpu = preference != ComputePreference::Cpu;
    AccelerationSelection {
        status: ComputeStatus {
            preference,
            selected: if gpu {
                ComputeBackend::Cuda
            } else {
                ComputeBackend::Cpu
            },
            reason: match preference {
                ComputePreference::Auto => ComputeReason::Recommended,
                ComputePreference::Cpu => ComputeReason::UserCpu,
                ComputePreference::Gpu => ComputeReason::UserGpu,
            },
            gpu_available: true,
            device: Some(device.clone()),
            devices: vec![device],
        },
        backend: gpu.then(|| Arc::new(CpuRidgeBackend) as Arc<dyn secs_engine::mimo::RidgeBackend>),
    }
}

#[test]
fn supported_gpu_defaults_to_auto_and_explicit_cpu_survives_reload() {
    let root = std::env::temp_dir().join(format!("secs-compute-settings-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.json");
    let deps = Deps {
        acceleration: Arc::new(fake_gpu),
        app_settings_path: Some(path.clone()),
        ..Deps::default()
    };
    let service = Service::new(deps.clone());
    assert_eq!(status(&service).preference, ComputePreference::Auto);
    assert_eq!(status(&service).selected, ComputeBackend::Cuda);
    call(
        &service,
        Request::AppSettingsSet(AppSettingsParams {
            settings: AppSettings {
                compute: ComputePreference::Cpu,
                ..Default::default()
            },
        }),
    );
    let saved = fs::read(&path).unwrap();
    let reopened = Service::new(deps.clone());
    assert_eq!(status(&reopened).selected, ComputeBackend::Cpu);
    assert_eq!(status(&reopened).reason, ComputeReason::UserCpu);
    let overridden = Service::new(Deps {
        compute_override: Some(ComputePreference::Gpu),
        ..deps
    });
    assert_eq!(status(&overridden).selected, ComputeBackend::Cuda);
    assert_eq!(status(&overridden).preference, ComputePreference::Gpu);
    let Response::AppSettingsGet(settings) = call(&overridden, Request::AppSettingsGet) else {
        panic!()
    };
    assert_eq!(settings.compute, ComputePreference::Cpu);
    assert_eq!(saved, fs::read(&path).unwrap());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unsupported_auto_is_normal_cpu_and_explicit_gpu_has_typed_error() {
    for reason in [
        ComputeReason::Integrated,
        ComputeReason::Legacy,
        ComputeReason::RuntimeUnavailable,
        ComputeReason::NoDevice,
    ] {
        let service = Service::new(Deps {
            acceleration: Arc::new(move |preference| AccelerationSelection {
                status: ComputeStatus {
                    preference,
                    reason,
                    ..Default::default()
                },
                backend: None,
            }),
            ..Deps::default()
        });
        assert_eq!(status(&service).selected, ComputeBackend::Cpu);
        assert_eq!(status(&service).reason, reason);
        call(
            &service,
            Request::AppSettingsSet(AppSettingsParams {
                settings: AppSettings {
                    compute: ComputePreference::Gpu,
                    ..Default::default()
                },
            }),
        );
        for request in [
            Request::Mimo(MimoParams::default()),
            Request::Calibrate(CalibrateParams {
                mimo: true,
                ..Default::default()
            }),
        ] {
            let Envelope::Err(error) = service.call(request, Arc::new(NoEvents)) else {
                panic!()
            };
            assert_eq!(error.code, ErrorCode::ComputeUnavailable);
            assert_eq!(
                error.params["reason"],
                serde_json::to_value(reason).unwrap().as_str().unwrap()
            );
        }
    }
}

#[test]
fn default_service_is_cpu_only_and_does_not_probe_the_host() {
    let service = Service::new(Deps::default());
    assert_eq!(status(&service), ComputeStatus::default());
}
