use std::path::Path;
use std::sync::Arc;

use secs_export::ExportRun;
use secs_filters::FiltersError;
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
        let now = (self.deps.clock)();
        let description = crate::export_run::describe(&open.results, project, &compute_key, now)?;
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
        let live_camilla = live
            .filter(|_| params.kind == ExportKind::CamillaDsp)
            .map(|calibration| calibration.shared_design());
        let cached_camilla = cached
            .filter(|_| live.is_none() && params.kind == ExportKind::CamillaDsp)
            .map(|cache| cache.calibration_chain.clone());
        let spl_offset = secs_project::project_spl_offset(project);
        let sources = if mdat {
            secs_project::load_project_sources(project, &open.path)
        } else {
            Default::default()
        };
        let uuid = self.deps.uuid.clone();
        let session_for_design = self.session.clone();
        let memory = self.deps.memory.available_bytes();
        let mode_name = description.mode();
        let mut mimo = live_mimo.filter(|_| mdat).map(|result| {
            description.mimo_responses(result.run.paths.clone(), result.run.applied.clone())
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
        let filter_values = matches!(
            params.kind,
            ExportKind::FilterText | ExportKind::EqualizerApo
        )
        .then(|| (export_chain.clone(), export_mimo.take()));
        // One export reads the clock once; every mdat it builds gets that date.
        let now_ms = (self.deps.unix_ms)();
        drop(session);
        let destination = if params.folder.is_empty() {
            let base = self.resolved_export_base()?;
            match params.kind {
                ExportKind::CamillaDsp => crate::export_folder::camilladsp_folder(
                    &base,
                    folder_includes_mimo,
                    mode_name,
                    now,
                ),
                ExportKind::BeforeAfterMdat | ExportKind::StagedMdat => {
                    crate::export_folder::mdat_folder(&base, folder_includes_mimo, mode_name, now)
                }
                ExportKind::FilterText => {
                    crate::export_folder::text_folder(&base, includes_mimo, mode_name, now)
                }
                ExportKind::EqualizerApo => {
                    crate::export_folder::eqapo_folder(&base, includes_mimo, mode_name, now)
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
                    let chain = if let Some(design) = &live_camilla {
                        &design.chain
                    } else {
                        cached_camilla.as_ref().ok_or_else(|| {
                            IpcError::new(ErrorCode::Internal, "run cache disappeared")
                        })?
                    };
                    job.begin_commit()?;
                    secs_camilla::export(
                        chain,
                        folder,
                        &description.camilla(),
                        export_mimo.as_ref(),
                    )
                    .map_err(export_error)?
                }
                ExportKind::FilterText | ExportKind::EqualizerApo => {
                    let values = filter_values.as_ref().ok_or_else(|| {
                        IpcError::new(ErrorCode::Internal, "filter export values disappeared")
                    })?;
                    let run = description.filters(&values.0, values.1.as_ref());
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
                            mimo = Some(
                                description.mimo_responses(
                                    paths.clone(),
                                    design
                                        .replay
                                        .mimo_responses(&design, paths)
                                        .map_err(super::calibration::engine_error)?,
                                ),
                            );
                        }
                        (Arc::new(design), settings)
                    };
                    job.begin_commit()?;
                    let run = ExportRun {
                        design: &design,
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
