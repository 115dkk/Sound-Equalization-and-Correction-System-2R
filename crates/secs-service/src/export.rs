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

use crate::{EventSink, Service, lock, results::MimoView, session};

impl Service {
    pub(crate) fn start_export(
        &self,
        params: ExportParams,
        events: Arc<dyn EventSink>,
    ) -> Result<JobStarted, IpcError> {
        let compute_key = crate::compute::ComputeIdentity::from(&self.acceleration().status);
        let session = lock(&self.session);
        let open = session.require().map_err(IpcError::from)?;
        let ticket = session.ticket()?;
        let project = open.store.project();
        if open.tutorial {
            return Err(IpcError::new(
                ErrorCode::TutorialBlocked,
                "tutorial sessions cannot export",
            ));
        }
        let live = open.results.live_calibration();
        let cached = open.results.cached();
        let cached_sample_rate_policy = cached.map(|cache| cache.labels.sample_rate_policy);
        let calibration = open.results.calibration(project);
        if calibration.is_none() {
            return Err(IpcError::new(
                ErrorCode::EngineInput,
                "calibration must run before export",
            ));
        }
        let stale = calibration.is_some_and(crate::results::RunView::stale);
        let settings = secs_project::calibration_from_project(project);
        let mdat = matches!(
            params.kind,
            ExportKind::BeforeAfterMdat | ExportKind::StagedMdat
        );
        let (live_mimo, cached_mimo) = match open.results.mimo(project, &compute_key).exported() {
            Some(MimoView::Live(result)) => (Some(result), None),
            Some(MimoView::Cached(cache)) => (None, cache.mimo_paths.as_ref()),
            None => (None, None),
        };
        // Capture only what this export format consumes. In particular the
        // convolution writers never clone the large applied-response matrices.
        let mut live_values = live
            .filter(|_| mdat)
            .map(|calibration| (calibration.shared_design(), calibration.settings.clone()));
        let live_camilla =
            live.filter(|_| params.kind == ExportKind::CamillaDsp)
                .map(|calibration| {
                    (
                        calibration.shared_design(),
                        labels(&calibration.settings, calibration.design()),
                    )
                });
        let cached_camilla = cached
            .filter(|_| live.is_none() && params.kind == ExportKind::CamillaDsp)
            .map(|cache| {
                (
                    cache.calibration_chain.clone(),
                    RunLabels {
                        mode: cache.labels.mode.clone(),
                        ultra_ms: cache.labels.ultra_ms,
                        latency_ms: cache.labels.latency_ms,
                        rc_hi_hz: cache.labels.rc_hi_hz,
                        mimo_max_supports: cache.labels.mimo_max_supports,
                    },
                )
            });
        let spl_offset = secs_project::project_spl_offset(project);
        let sources = if mdat {
            secs_project::load_project_sources(project, &open.path)
        } else {
            Default::default()
        };
        let uuid = self.deps.uuid.clone();
        let session_for_design = self.session.clone();
        let memory = self.deps.memory.available_bytes();
        let mode_name = live
            .map(|calibration| mode_name(calibration.settings.mode).to_owned())
            .or_else(|| cached.map(|cache| cache.labels.mode.clone()))
            .unwrap_or_else(|| "mixed".into());
        let mut mimo = live_mimo.filter(|_| mdat).map(|result| MimoResponses {
            kind: if result.run.fir {
                MimoKind::Fir
            } else {
                MimoKind::Iir
            },
            fsiso: result.run.paths.fsiso,
            cap_text: secs_engine::mimo::cap_text(result.run.key.max_supports),
            paths: result.run.paths.clone(),
            applied: result.run.applied.clone(),
        });
        let cached_mimo_paths = cached_mimo.cloned();
        let mut export_mimo = if mdat {
            None
        } else {
            live_mimo
                .map(|result| &result.run.paths)
                .or(cached_mimo)
                .cloned()
        };
        let includes_mimo = export_mimo.is_some();
        let folder_includes_mimo = mimo.is_some() || includes_mimo;
        let compute_report = if let Some(result) = live_mimo {
            Some(result.summary.compute.clone())
        } else if cached_mimo.is_some() {
            cached
                .and_then(|cache| cache.mimo.as_ref())
                .map(|result| result.compute.clone())
        } else {
            None
        };
        let export_chain = live
            .map(|calibration| &calibration.design().chain)
            .or_else(|| cached.map(|cache| &cache.calibration_chain))
            .expect("live or cached calibration checked above");
        if let Some(block) = secs_types::ipc::export_block(
            params.kind,
            !export_chain.subs.is_empty(),
            includes_mimo,
            live.is_none() && stale,
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
                    let design = calibration.design();
                    let run = ExportRun {
                        design,
                        measurements: design.replay.measurements(),
                        config: design.replay.config(),
                        settings: &calibration.settings,
                    };
                    FilterValues {
                        chain: design.chain.clone(),
                        mimo: export_mimo.take(),
                        mode: mode_name.clone(),
                        ultra_ms: calibration.settings.ultra_ms,
                        label: calibration.settings.label(),
                        latency_ms: design.chain.latency_ms,
                        rc_hi_hz: design.rc_hi,
                        rc_pieces: secs_export::rc_pieces(&run),
                        stamp: now.minute_stamp(),
                    }
                } else {
                    cached_filter_values(
                        cached.expect("live or cached calibration checked above"),
                        &settings,
                        export_mimo.take(),
                        now.minute_stamp(),
                    )
                })
            }
            _ => None,
        };
        // One export reads the clock once; every mdat it builds gets that date.
        let now_ms = (self.deps.unix_ms)();
        drop(session);
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
            let mut paths = match params.kind {
                ExportKind::CamillaDsp => {
                    let (chain, labels) = if let Some((design, labels)) = &live_camilla {
                        (&design.chain, labels)
                    } else {
                        let (chain, labels) = cached_camilla.as_ref().ok_or_else(|| {
                            IpcError::new(ErrorCode::Internal, "run cache disappeared")
                        })?;
                        (chain, labels)
                    };
                    job.begin_commit()?;
                    secs_camilla::export(chain, folder, labels, export_mimo.as_ref())
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
                    let (design, settings) = if let Some(values) = live_values.take() {
                        values
                    } else {
                        job.report_progress("design", 0, 1);
                        // Reads the measurements of the project the export
                        // started from; the job stays cancellable.
                        let (project, settings, measurements) =
                            session::apply(&session_for_design, ticket, |open| {
                                let project = open.store.project().clone();
                                let mut settings = secs_project::calibration_from_project(&project);
                                if let Some(policy) = cached_sample_rate_policy {
                                    settings.sample_rate_policy = policy;
                                }
                                let measurements =
                                    super::calibration::load_measurements(&mut open.store)?;
                                Ok(((project, settings, measurements), false))
                            })?;
                        let ctx = super::calibration::compute_context(&job, memory);
                        let groups = super::calibration::project_groups(&project);
                        let view = secs_engine::ProjectView {
                            groups: &groups,
                            project_xo: &project.xo,
                            target: &project.target,
                        };
                        let calibration = secs_engine::calibrate(
                            &secs_engine::CalibrationRequest {
                                view: &view,
                                irs: &measurements,
                                settings: &settings,
                                subs_only: false,
                            },
                            &ctx,
                        )
                        .map_err(super::calibration::engine_error)?;
                        let design = calibration.design;
                        job.report_progress("design", 1, 1);
                        if let Some(paths) = cached_mimo_paths.as_ref() {
                            mimo = Some(MimoResponses {
                                kind: if paths.paths.iter().any(|path| path.filter.is_fir()) {
                                    MimoKind::Fir
                                } else {
                                    MimoKind::Iir
                                },
                                fsiso: paths.fsiso,
                                cap_text: secs_engine::mimo::cap_text(
                                    secs_engine::MimoKey::from_settings(&settings).max_supports,
                                ),
                                paths: paths.clone(),
                                applied: design
                                    .replay
                                    .mimo_responses(&design, paths)
                                    .map_err(super::calibration::engine_error)?,
                            });
                        }
                        (Arc::new(design), settings)
                    };
                    job.begin_commit()?;
                    let run = ExportRun {
                        design: &design,
                        measurements: design.replay.measurements(),
                        config: design.replay.config(),
                        settings: &settings,
                    };
                    let mut uuid = lock(&uuid);
                    let destination = if kind == ExportKind::StagedMdat {
                        secs_export::export_mdat_with_mimo_using(
                            &run,
                            folder,
                            mimo.as_ref(),
                            spl_offset,
                            now_ms,
                            uuid.as_mut(),
                        )
                    } else {
                        secs_export::export_before_after_mdat_with_mimo_using(
                            &run,
                            folder,
                            mimo.as_ref(),
                            Some(&sources),
                            spl_offset,
                            now_ms,
                            uuid.as_mut(),
                        )
                    }
                    .map_err(export_error)?;
                    vec![destination]
                }
            };
            if let Some(path) = write_compute_provenance(folder, compute_report.as_ref())? {
                paths.push(path);
            }
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
        chain: cache.calibration_chain.clone(),
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

fn write_compute_provenance(
    folder: &Path,
    report: Option<&secs_types::compute::ComputeReport>,
) -> Result<Option<std::path::PathBuf>, IpcError> {
    let path = folder.join("compute-provenance.json");
    if let Some(report) =
        report.filter(|report| report.accelerated_systems > 0 || report.fallbacks > 0)
    {
        let bytes = serde_json::to_vec_pretty(report).map_err(export_error)?;
        std::fs::write(&path, bytes).map_err(export_error)?;
        Ok(Some(path))
    } else {
        // Reusing an export directory must not retain an earlier GPU run's metadata.
        match std::fs::remove_file(&path) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(export_error(error)),
        }
        Ok(None)
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
    fn provenance_records_actual_acceleration_and_clears_previous_gpu_metadata() {
        use secs_types::compute::{ComputeBackend, ComputeReport, ComputeStage};
        let folder =
            std::env::temp_dir().join(format!("secs-compute-export-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        assert!(
            write_compute_provenance(&folder, Some(&ComputeReport::default()))
                .unwrap()
                .is_none()
        );
        let mut report = ComputeReport {
            backend: ComputeBackend::Cuda,
            device: Some("Test GPU".into()),
            stage: Some(ComputeStage::IdealFirSweep),
            accelerated_systems: 2,
            ..Default::default()
        };
        let path = write_compute_provenance(&folder, Some(&report))
            .unwrap()
            .unwrap();
        let saved: ComputeReport = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved, report);
        report.backend = ComputeBackend::Cpu;
        report.accelerated_systems = 0;
        report.cpu_systems = 2;
        report.fallbacks = 2;
        write_compute_provenance(&folder, Some(&report)).unwrap();
        let fallback: ComputeReport =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(fallback, report);
        assert!(write_compute_provenance(&folder, None).unwrap().is_none());
        assert!(!path.exists());
        std::fs::remove_dir_all(folder).unwrap();
    }
}
