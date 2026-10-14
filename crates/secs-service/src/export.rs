use std::path::Path;
use std::sync::Arc;

use secs_camilla::RunLabels;
use secs_export::{ExportRun, MimoKind, MimoResponses};
use secs_types::calibration::Mode;
use secs_types::ipc::{
    ErrorCode, ExportKind, ExportParams, ExportResult, IpcError, JobKind, JobResult, JobStarted,
};

use crate::{EventSink, Service, calibration::calibration_stale, lock};

impl Service {
    pub(crate) fn start_export(
        &self,
        params: ExportParams,
        events: Arc<dyn EventSink>,
    ) -> Result<JobStarted, IpcError> {
        let state = lock(&self.results_state);
        let session = lock(&self.session);
        let open = session.require().map_err(IpcError::from)?;
        let project = open.store.project();
        let live = state.calibration.as_ref();
        let cached = open.cached.as_ref();
        if live.is_none() && cached.is_none() {
            return Err(IpcError::new(
                ErrorCode::EngineInput,
                "calibration must run before export",
            ));
        }
        if let Some(calibration) = live
            && calibration_stale(calibration, project)
        {
            return Err(IpcError::new(
                ErrorCode::StaleInputs,
                "calibration result is stale",
            ));
        }
        if live.is_none()
            && cached.is_some_and(|cache| {
                cache.measurement_signature != super::calibration::signature_key(project)
                    || cache.target_key != super::calibration::target_key_string(project)
                    || cache.engine_key
                        != secs_project::calibration_from_project(project).engine_key()
            })
        {
            return Err(IpcError::new(
                ErrorCode::StaleInputs,
                "calibration result is stale",
            ));
        }
        let cached_camilla = cached.map(|cache| {
            let labels = RunLabels {
                mode: cache.labels.mode.clone(),
                ultra_ms: cache.labels.ultra_ms,
                latency_ms: cache.labels.latency_ms,
                rc_hi_hz: cache.labels.rc_hi_hz,
                mimo_max_supports: cache.labels.mimo_max_supports,
            };
            (
                cache.calibration.chain.clone(),
                labels,
                cache.mimo_paths.clone(),
            )
        });
        let live_values = live.map(|calibration| {
            (
                calibration.design.clone(),
                calibration.measurements.clone(),
                calibration.config.clone(),
                calibration.settings.clone(),
            )
        });
        let spl_offset = secs_project::project_spl_offset(project);
        let uuid = self.deps.uuid.clone();
        let session_for_design = self.session.clone();
        let memory = self.deps.memory.available_bytes();
        let mimo = live.and_then(|calibration| {
            state.mimo.as_ref().and_then(|result| {
                let fresh = result.settings_key == calibration.settings.mimo_key()
                    && result.calibration_key == calibration.settings.engine_key();
                fresh.then(|| MimoResponses {
                    kind: if result.run.fir {
                        MimoKind::Fir
                    } else {
                        MimoKind::Iir
                    },
                    fsiso: result.run.paths.fsiso,
                    cap_text: if calibration.settings.mimo_max_supports == 0 {
                        String::new()
                    } else {
                        format!("max {}", calibration.settings.mimo_max_supports)
                    },
                    paths: result.run.paths.clone(),
                    applied: result.run.applied.clone(),
                })
            })
        });
        drop(session);
        drop(state);
        self.jobs.start(JobKind::Export, events, move |job| {
            job.cancel()
                .check()
                .map_err(|_| IpcError::new(ErrorCode::Cancelled, "cancelled"))?;
            let folder = Path::new(&params.folder);
            let paths = match params.kind {
                ExportKind::CamillaDsp => {
                    let (chain, labels, paths) =
                        if let Some((design, _, _, settings)) = &live_values {
                            (
                                design.chain.clone(),
                                labels(settings, design),
                                mimo.as_ref().map(|value| value.paths.clone()),
                            )
                        } else {
                            cached_camilla.clone().ok_or_else(|| {
                                IpcError::new(ErrorCode::Internal, "run cache disappeared")
                            })?
                        };
                    job.begin_commit()?;
                    secs_camilla::export(&chain, folder, &labels, paths.as_ref())
                        .map_err(export_error)?
                }
                kind => {
                    let (design, measurements, config, settings) =
                        if let Some(values) = &live_values {
                            values.clone()
                        } else {
                            let prepared = {
                                let mut session = lock(&session_for_design);
                                let open = session.require_mut().map_err(IpcError::from)?;
                                let project = open.store.project().clone();
                                let settings = secs_project::calibration_from_project(&project);
                                let measurements =
                                    super::calibration::load_measurements(&mut open.store)?;
                                let groups = super::calibration::project_groups(&project);
                                let view = secs_engine::ProjectView {
                                    groups: &groups,
                                    project_xo: &project.xo,
                                    target: &project.target,
                                };
                                let inputs =
                                    secs_engine::engine_inputs(&view, &measurements, &settings)
                                        .map_err(super::calibration::engine_error)?;
                                (settings, inputs.measurements, inputs.config, inputs.notes)
                            };
                            job.report_progress("design", 0, 1);
                            let ctx = super::calibration::compute_context(&job, memory);
                            let request = secs_engine::DesignRequest {
                                measurements: &prepared.1,
                                config: &prepared.2,
                                tuning: &secs_engine::Tuning::default(),
                                subs_only: false,
                                adapter_notes: &prepared.3,
                            };
                            let fitters = secs_engine::production_fitters(job.cancel());
                            let design = secs_engine::design(&request, &ctx, fitters.as_ref())
                                .map_err(super::calibration::engine_error)?;
                            job.report_progress("design", 1, 1);
                            (Arc::new(design), prepared.1, prepared.2, prepared.0)
                        };
                    job.begin_commit()?;
                    let run = ExportRun {
                        design: &design,
                        measurements: &measurements,
                        config: &config,
                        settings: &settings,
                    };
                    let mut uuid = lock(&uuid);
                    let destination = if kind == ExportKind::StagedMdat {
                        secs_export::export_mdat_with_mimo_using(
                            &run,
                            folder,
                            mimo.as_ref(),
                            spl_offset,
                            uuid.as_mut(),
                        )
                    } else {
                        secs_export::export_before_after_mdat_with_mimo_using(
                            &run,
                            folder,
                            mimo.as_ref(),
                            spl_offset,
                            uuid.as_mut(),
                        )
                    }
                    .map_err(export_error)?;
                    vec![destination]
                }
            };
            Ok(JobResult::Export(ExportResult {
                files: paths
                    .into_iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect(),
            }))
        })
    }
}

fn labels(
    settings: &secs_types::calibration::CalibrationSettings,
    design: &secs_engine::Design,
) -> RunLabels {
    RunLabels {
        mode: match settings.mode {
            Mode::Zero => "zero",
            Mode::Mixed => "mixed",
            Mode::Linear => "linear",
            Mode::Ultra => "ultra",
        }
        .into(),
        ultra_ms: settings.ultra_ms,
        latency_ms: design.chain.latency_ms,
        rc_hi_hz: design.rc_hi,
        mimo_max_supports: settings.mimo_max_supports.max(0) as usize,
    }
}

fn export_error(error: impl std::fmt::Display) -> IpcError {
    IpcError::new(ErrorCode::ExportFailed, error.to_string())
}
