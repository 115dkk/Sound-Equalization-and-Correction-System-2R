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
        if open.tutorial {
            return Err(IpcError::new(
                ErrorCode::TutorialBlocked,
                "tutorial sessions cannot export",
            ));
        }
        let live = state.calibration.as_ref();
        let cached = open.cached.as_ref();
        if live.is_none() && cached.is_none() {
            return Err(IpcError::new(
                ErrorCode::EngineInput,
                "calibration must run before export",
            ));
        }
        let stale = live.map_or_else(
            || {
                cached.is_some_and(|cache| {
                    super::calibration::cached_stale_reason(cache, project).is_some()
                })
            },
            |calibration| calibration_stale(calibration, project),
        );
        let settings = secs_project::calibration_from_project(project);
        let cached_mimo = cached
            .filter(|cache| {
                mimo_eligible(
                    &settings,
                    stale,
                    secs_project::measured_positions(project).len(),
                    cache.mimo_key.as_deref(),
                    cache.mimo_calibration_key.as_deref(),
                    &cache.engine_key,
                )
            })
            .and_then(|cache| cache.mimo_paths.clone())
            .filter(|paths| !paths.paths.is_empty());
        let cached_camilla = cached.map(|cache| {
            let labels = RunLabels {
                mode: cache.labels.mode.clone(),
                ultra_ms: cache.labels.ultra_ms,
                latency_ms: cache.labels.latency_ms,
                rc_hi_hz: cache.labels.rc_hi_hz,
                mimo_max_supports: cache.labels.mimo_max_supports,
            };
            (cache.calibration.chain.clone(), labels, cached_mimo.clone())
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
        let mode_name = live
            .map(|calibration| mode_name(calibration.settings.mode).to_owned())
            .or_else(|| cached.map(|cache| cache.labels.mode.clone()))
            .unwrap_or_else(|| "mixed".into());
        let mimo = live.and_then(|calibration| {
            state.mimo.as_ref().and_then(|result| {
                let fresh = mimo_eligible(
                    &settings,
                    stale,
                    calibration.design.spectra.positions.len(),
                    Some(&result.settings_key),
                    Some(&result.calibration_key),
                    &calibration.settings.engine_key(),
                ) && !result.run.paths.paths.is_empty();
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
        // Cached mdat exports rebuild calibration only, not MIMO responses.
        let includes_mimo = mimo.is_some()
            || (live.is_none() && params.kind == ExportKind::CamillaDsp && cached_mimo.is_some());
        drop(session);
        drop(state);
        let destination = if params.folder.is_empty() {
            let base = self.resolved_export_base()?;
            let now = (self.deps.clock)();
            match params.kind {
                ExportKind::CamillaDsp => {
                    crate::export_folder::camilladsp_folder(&base, includes_mimo, &mode_name, now)
                }
                ExportKind::BeforeAfterMdat | ExportKind::StagedMdat => {
                    crate::export_folder::mdat_folder(&base, includes_mimo, &mode_name, now)
                }
            }
        } else {
            Path::new(&params.folder).to_path_buf()
        };
        self.jobs.start(JobKind::Export, events, move |job| {
            job.cancel()
                .check()
                .map_err(|_| IpcError::new(ErrorCode::Cancelled, "cancelled"))?;
            let folder = destination.as_path();
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
                folder: folder.to_string_lossy().into_owned(),
                stale,
            }))
        })
    }
}

fn mimo_eligible(
    settings: &secs_types::calibration::CalibrationSettings,
    stale: bool,
    measured_positions: usize,
    mimo_key: Option<&str>,
    mimo_calibration_key: Option<&str>,
    engine_key: &str,
) -> bool {
    settings.mimo_on
        && !stale
        && measured_positions >= 3
        && mimo_key == Some(settings.mimo_key().as_str())
        && mimo_calibration_key == Some(engine_key)
}

fn labels(
    settings: &secs_types::calibration::CalibrationSettings,
    design: &secs_engine::Design,
) -> RunLabels {
    RunLabels {
        mode: mode_name(settings.mode).into(),
        ultra_ms: settings.ultra_ms,
        latency_ms: design.chain.latency_ms,
        rc_hi_hz: design.rc_hi,
        mimo_max_supports: settings.mimo_max_supports.max(0) as usize,
    }
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Zero => "zero",
        Mode::Mixed => "mixed",
        Mode::Linear => "linear",
        Mode::Ultra => "ultra",
    }
}

fn export_error(error: impl std::fmt::Display) -> IpcError {
    IpcError::new(ErrorCode::ExportFailed, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mimo_requires_enabled_current_calibration_and_three_measured_positions() {
        let mut settings = secs_types::calibration::CalibrationSettings {
            mimo_on: true,
            ..Default::default()
        };
        let engine = settings.engine_key();
        let mimo = settings.mimo_key();
        assert!(mimo_eligible(
            &settings,
            false,
            3,
            Some(&mimo),
            Some(&engine),
            &engine
        ));
        for count in [0, 1, 2] {
            assert!(!mimo_eligible(
                &settings,
                false,
                count,
                Some(&mimo),
                Some(&engine),
                &engine
            ));
        }
        assert!(!mimo_eligible(
            &settings,
            true,
            3,
            Some(&mimo),
            Some(&engine),
            &engine
        ));
        assert!(!mimo_eligible(
            &settings,
            false,
            3,
            None,
            Some(&engine),
            &engine
        ));
        assert!(!mimo_eligible(
            &settings,
            false,
            3,
            Some("old"),
            Some(&engine),
            &engine
        ));
        assert!(!mimo_eligible(
            &settings,
            false,
            3,
            Some(&mimo),
            Some("old"),
            &engine
        ));
        settings.mimo_on = false;
        assert!(!mimo_eligible(
            &settings,
            false,
            3,
            Some(&mimo),
            Some(&engine),
            &engine
        ));
    }
}
