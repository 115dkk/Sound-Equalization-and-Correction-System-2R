use std::path::Path;
use std::sync::Arc;

use secs_camilla::RunLabels;
use secs_export::{ExportRun, MimoKind, MimoResponses};
use secs_filters::{FilterRun, FiltersError};
use secs_types::calibration::{Mode, mode_label};
use secs_types::chain::{MimoPaths, SignalChain};
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
        // A cached MIMO run is selected only when there is no live calibration,
        // matching the CamillaDSP freshness rule.
        let export_mimo = if live.is_some() {
            mimo.as_ref().map(|value| value.paths.clone())
        } else {
            cached_mimo.clone()
        };
        let includes_mimo = export_mimo.is_some();
        // Cached mdat exports rebuild calibration only, not MIMO responses.
        let folder_includes_mimo = mimo.is_some()
            || (live.is_none() && params.kind == ExportKind::CamillaDsp && cached_mimo.is_some());
        let export_chain = live
            .map(|calibration| &calibration.design.chain)
            .or_else(|| cached.map(|cache| &cache.calibration.chain))
            .expect("live or cached calibration checked above");
        if let Some(block) = secs_types::ipc::filter_export_block(
            params.kind,
            !export_chain.subs.is_empty(),
            includes_mimo,
        ) {
            return Err(IpcError::new(
                ErrorCode::ExportScope,
                "the chosen filter export cannot carry this result",
            )
            .with_param("reason", block.name()));
        }
        let now = (self.deps.clock)();
        let filter_values = match params.kind {
            ExportKind::FilterText | ExportKind::EqualizerApo => {
                Some(if let Some(calibration) = live {
                    let run = ExportRun {
                        design: &calibration.design,
                        measurements: &calibration.measurements,
                        config: &calibration.config,
                        settings: &calibration.settings,
                    };
                    FilterValues {
                        chain: calibration.design.chain.clone(),
                        mimo: export_mimo.clone(),
                        mode: mode_name.clone(),
                        ultra_ms: calibration.settings.ultra_ms,
                        label: calibration.settings.label(),
                        latency_ms: calibration.design.chain.latency_ms,
                        rc_hi_hz: calibration.design.rc_hi,
                        rc_pieces: secs_export::rc_pieces(&run),
                        stamp: now.minute_stamp(),
                    }
                } else {
                    cached_filter_values(
                        cached.expect("live or cached calibration checked above"),
                        &settings,
                        export_mimo.clone(),
                        now.minute_stamp(),
                    )
                })
            }
            _ => None,
        };
        drop(session);
        drop(state);
        let destination = if params.folder.is_empty() {
            let base = self.resolved_export_base()?;
            match params.kind {
                ExportKind::CamillaDsp => crate::export_folder::camilladsp_folder(
                    &base,
                    folder_includes_mimo,
                    &mode_name,
                    now,
                ),
                ExportKind::BeforeAfterMdat | ExportKind::StagedMdat => {
                    crate::export_folder::mdat_folder(&base, folder_includes_mimo, &mode_name, now)
                }
                ExportKind::FilterText => {
                    crate::export_folder::text_folder(&base, includes_mimo, &mode_name, now)
                }
                ExportKind::EqualizerApo => {
                    crate::export_folder::eqapo_folder(&base, includes_mimo, &mode_name, now)
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
                ExportKind::FilterText | ExportKind::EqualizerApo => {
                    let values = filter_values.as_ref().ok_or_else(|| {
                        IpcError::new(ErrorCode::Internal, "filter export values disappeared")
                    })?;
                    let run = values.run();
                    job.begin_commit()?;
                    if params.kind == ExportKind::FilterText {
                        secs_filters::export_filter_text(&run, folder)
                    } else {
                        secs_filters::export_eqapo(&run, folder)
                    }
                    .map_err(filter_error)?
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

#[derive(Debug, Clone)]
struct FilterValues {
    chain: SignalChain,
    mimo: Option<MimoPaths>,
    mode: String,
    ultra_ms: f64,
    label: String,
    latency_ms: f64,
    rc_hi_hz: f64,
    rc_pieces: Vec<String>,
    stamp: String,
}

impl FilterValues {
    fn run(&self) -> FilterRun<'_> {
        FilterRun {
            chain: &self.chain,
            mimo: self.mimo.as_ref(),
            mode: &self.mode,
            ultra_ms: self.ultra_ms,
            label: &self.label,
            latency_ms: self.latency_ms,
            rc_hi_hz: self.rc_hi_hz,
            rc_pieces: &self.rc_pieces,
            stamp: &self.stamp,
        }
    }
}

fn cached_filter_values(
    cache: &crate::cache::CachedResults,
    settings: &secs_types::calibration::CalibrationSettings,
    mimo: Option<MimoPaths>,
    stamp: String,
) -> FilterValues {
    let rc_note = cache
        .calibration
        .notes
        .iter()
        .find(|notice| notice.code == "rc_limit");
    let rc_values = rc_note.and_then(|notice| {
        Some((
            notice.params.get("hz")?.parse::<f64>().ok()?,
            notice.params.get("auto")?.parse::<bool>().ok()?,
            notice
                .params
                .get("schroeder_hz")
                .and_then(|value| value.parse::<f64>().ok()),
        ))
    });
    let (rc_hi_hz, rc_auto, schroeder_hz, legacy_fallback) = match rc_values {
        Some((hz, auto, schroeder)) => (hz, auto, schroeder, false),
        None => (cache.labels.rc_hi_hz, settings.rc_hi.is_none(), None, true),
    };
    FilterValues {
        chain: cache.calibration.chain.clone(),
        mimo,
        mode: cache.labels.mode.clone(),
        ultra_ms: cache.labels.ultra_ms,
        label: mode_label(cache.calibration.mode, Some(cache.calibration.ultra_ms)),
        latency_ms: cache.labels.latency_ms,
        rc_hi_hz,
        rc_pieces: secs_export::rc_pieces_from_values(
            rc_hi_hz,
            rc_auto,
            schroeder_hz,
            settings.room_volume_m3,
            legacy_fallback,
        ),
        stamp,
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

fn filter_error(error: FiltersError) -> IpcError {
    match error {
        FiltersError::Scope(block) => IpcError::new(ErrorCode::ExportScope, error.to_string())
            .with_param("reason", block.name()),
        _ => export_error(error),
    }
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
