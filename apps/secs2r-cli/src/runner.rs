//! Public API runner, event sink, and production dependencies.

use crate::apply;
use crate::args::ComputeChoice;
use crate::commands::print_json;
use secs_api::{Engine, Error, Event, Secs};
use secs_service::{Deps, EventSink, LocalTime};
use secs_types::ipc::*;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

const JOB_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug)]
pub(crate) struct PrintEvents {
    quiet: bool,
}
impl PrintEvents {
    pub(crate) fn new(quiet: bool) -> Self {
        Self { quiet }
    }
}
impl EventSink for PrintEvents {
    fn send(&self, event: JobEvent) {
        if !self.quiet {
            print_json(&serde_json::json!({"event": event}));
        }
    }
}

pub(crate) struct Runner {
    pub(crate) engine: Engine,
    pub(crate) ok: bool,
}
impl Runner {
    pub(crate) fn call(&mut self, request: Request) -> Option<Response> {
        if !self.ok {
            return None;
        }
        let side_events = matches!(request, Request::ProjectSave(_))
            .then(|| self.engine.subscribe().expect("embedded subscription"));
        let response = match self.engine.call(request) {
            Ok(response) => response,
            Err(error) => {
                self.fail(error);
                return None;
            }
        };
        if let Some(started) = response_job(&response) {
            match self.engine.wait_job(started.job, JOB_TIMEOUT) {
                Ok(event) => self.finish_job(event).then_some(response),
                Err(error) => {
                    self.fail(error);
                    None
                }
            }
        } else if matches!(response, Response::ProjectSave(_)) {
            // The save reply returns before the side mdat job has written the
            // file (TD-149); this process ends with the command, so wait for
            // that job and report its failure.
            let events = side_events.expect("project save subscription");
            loop {
                match events.receiver.recv_timeout(JOB_TIMEOUT) {
                    Ok(Event::Job(event @ JobEvent::Finished { .. })) => {
                        if self.finish_side_mdat(event) {
                            print_json(
                                &serde_json::json!({"reply": Envelope::Ok(Box::new(response.clone()))}),
                            );
                            break Some(response);
                        }
                        break None;
                    }
                    Ok(_) => {}
                    Err(error) => {
                        self.ok = false;
                        print_json(&serde_json::json!({"reply": {
                            "ok": false,
                            "error": IpcError::new(ErrorCode::Internal, error.to_string()),
                        }}));
                        break None;
                    }
                }
            }
        } else {
            print_json(&serde_json::json!({"reply": Envelope::Ok(Box::new(response.clone()))}));
            Some(response)
        }
    }

    pub(crate) fn fail(&mut self, error: Error) {
        self.ok = false;
        match error {
            Error::Api(error) => {
                print_json(&serde_json::json!({"reply": Envelope::Err(error)}));
            }
            error => {
                print_json(&serde_json::json!({"reply": {
                    "ok": false,
                    "error": secs_api::WireError::from(&error),
                }}));
            }
        }
    }

    fn finish_side_mdat(&mut self, event: JobEvent) -> bool {
        let JobEvent::Finished { status, error, .. } = event else {
            unreachable!("event subscription waits for a finished event")
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
            unreachable!("public API waits for a finished event")
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
