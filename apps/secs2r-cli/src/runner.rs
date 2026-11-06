//! Service runner, event sink, and production dependencies.

use crate::apply;
use crate::args::ComputeChoice;
use crate::commands::print_json;
use secs_service::{Deps, EventSink, LocalTime, Service};
use secs_types::ipc::*;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::SystemTime;

#[derive(Debug)]
pub(crate) struct Events {
    pub(crate) quiet: bool,
    finished: Mutex<Option<JobEvent>>,
    changed: Condvar,
}
impl Events {
    pub(crate) fn new(quiet: bool) -> Self {
        Self {
            quiet,
            finished: Mutex::new(None),
            changed: Condvar::new(),
        }
    }

    fn wait(&self) -> JobEvent {
        let mut finished = lock(&self.finished);
        loop {
            if let Some(event) = finished.take() {
                return event;
            }
            finished = self
                .changed
                .wait(finished)
                .unwrap_or_else(|error| error.into_inner());
        }
    }
}
impl EventSink for Events {
    fn send(&self, event: JobEvent) {
        if !self.quiet {
            print_json(&serde_json::json!({"event": event}));
        }
        if matches!(event, JobEvent::Finished { .. }) {
            *lock(&self.finished) = Some(event);
            self.changed.notify_all();
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

pub(crate) struct Runner {
    pub(crate) service: Service,
    pub(crate) quiet: bool,
    pub(crate) ok: bool,
}
impl Runner {
    pub(crate) fn call(&mut self, request: Request) -> Option<Response> {
        if !self.ok {
            return None;
        }
        let events = Arc::new(Events::new(self.quiet));
        let envelope = self.service.call(request, events.clone());
        match envelope {
            Envelope::Err(error) => {
                self.ok = false;
                print_json(&serde_json::json!({"reply": Envelope::Err(error)}));
                None
            }
            Envelope::Ok(response) => {
                if response_job(&response).is_some() {
                    if self.finish_job(events.wait()) {
                        Some(*response)
                    } else {
                        None
                    }
                } else if matches!(*response, Response::ProjectSave(_)) {
                    // The save reply returns before the side mdat job has
                    // written the file (TD-149); this process ends with the
                    // command, so wait for that job and report its failure.
                    if self.finish_side_mdat(events.wait()) {
                        print_json(&serde_json::json!({"reply": Envelope::Ok(response.clone())}));
                        Some(*response)
                    } else {
                        None
                    }
                } else {
                    print_json(&serde_json::json!({"reply": Envelope::Ok(response.clone())}));
                    Some(*response)
                }
            }
        }
    }

    fn finish_side_mdat(&mut self, event: JobEvent) -> bool {
        let JobEvent::Finished { status, error, .. } = event else {
            unreachable!("event sink waits for a finished event")
        };
        if status == JobStatus::Succeeded {
            return true;
        }
        self.ok = false;
        print_json(&serde_json::json!({"reply": {
            "ok": false,
            "error": error.unwrap_or_else(|| IpcError::new(
                ErrorCode::Internal,
                format!("side mdat job ended with {status:?}"),
            )),
        }}));
        false
    }

    fn finish_job(&mut self, event: JobEvent) -> bool {
        let JobEvent::Finished {
            status,
            result,
            error,
            ..
        } = event
        else {
            unreachable!("event sink waits for a finished event")
        };
        if status == JobStatus::Succeeded {
            print_json(&serde_json::json!({"reply": {
                "ok": true,
                "data": result,
            }}));
            true
        } else {
            self.ok = false;
            print_json(&serde_json::json!({"reply": {
                "ok": false,
                "error": error.unwrap_or_else(|| IpcError::new(
                    ErrorCode::Internal,
                    format!("job ended with {status:?}"),
                )),
            }}));
            false
        }
    }

    pub(crate) fn open(&mut self, path: &Path) {
        self.call(Request::ProjectOpen(ProjectPathParams {
            path: path.to_string_lossy().into_owned(),
        }));
    }

    pub(crate) fn save(&mut self) {
        self.call(Request::ProjectSave(ProjectSaveParams::default()));
    }

    pub(crate) fn configure_mock(&mut self) {
        self.call(Request::MeasureConfigure(MeasureConfigureParams {
            method: "mock".into(),
            ..Default::default()
        }));
    }

    pub(crate) fn calibrate(
        &mut self,
        settings: Option<secs_types::calibration::CalibrationSettings>,
        mimo: bool,
    ) {
        self.call(Request::Calibrate(CalibrateParams {
            settings,
            mimo,
            ..Default::default()
        }));
    }

    pub(crate) fn export(
        &mut self,
        kind: ExportKind,
        folder: &Path,
        mdat_scope: Option<MdatScope>,
        design_name: Option<String>,
        camilla_devices: Option<CamillaDevices>,
        extra_rates: Vec<u32>,
    ) {
        self.call(Request::Export(ExportParams {
            kind,
            folder: folder.to_string_lossy().into_owned(),
            mdat_scope,
            design_name,
            camilla_devices,
            extra_rates,
        }));
    }
}

fn response_job(response: &Response) -> Option<JobStarted> {
    match response {
        Response::SampleLoad(value)
        | Response::Calibrate(value)
        | Response::RecommendXo(value)
        | Response::Mimo(value)
        | Response::Export(value)
        | Response::ApplyCamilla(value)
        | Response::ApplyEqapo(value)
        | Response::MeasurePosition(value)
        | Response::MdatImport(value) => Some(value.clone()),
        _ => None,
    }
}

pub(crate) fn deps(deterministic: bool, compute: Option<ComputeChoice>) -> Deps {
    let mut deps = Deps {
        app_version: env!("CARGO_PKG_VERSION").into(),
        acceleration: Arc::new(secs_gpu::select),
        compute_override: compute.map(Into::into),
        audio_backend: Arc::new(secs_sys::audio::default_backend),
        rew_transport: Arc::new(|| Box::new(secs_rew::HttpTransport::default())),
        process: Arc::new(secs_sys::process::OsProcessProbe),
        memory: Arc::new(secs_sys::memory::OsMemoryProbe),
        app_data_path: std::env::current_dir()
            .unwrap_or_else(|_| std::env::temp_dir())
            .join(".secs2r"),
        eqapo_install: Arc::new(secs_sys::eqapo_install),
        apply: Arc::new(apply::NativeApply),
        managed_host: Arc::new(apply::NativeManagedHost),
        clock: Arc::new(|| {
            let now = SystemTime::now();
            LocalTime::from_system(now, secs_sys::clock::utc_offset_seconds(now))
        }),
        ..Deps::default()
    };
    if deterministic {
        let mut next = 1_u128;
        deps.uuid = Arc::new(Mutex::new(Box::new(move || {
            let value = next;
            next = next.wrapping_add(1);
            Ok(value)
        })));
        deps.clock = Arc::new(|| LocalTime {
            year: 2024,
            month: 1,
            day: 2,
            hour: 3,
            minute: 4,
            second: 5,
        });
        // 2024-01-02 03:04:05 UTC, the same instant as the fixed clock above.
        deps.unix_ms = Arc::new(|| 1_704_164_645_000);
    }
    deps
}
