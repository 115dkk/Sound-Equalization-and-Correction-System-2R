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

use crate::{EventSink, Service, calibration::calibration_stale, lock, session};

impl Service {
    pub(crate) fn start_export(
        &self,
        params: ExportParams,
        events: Arc<dyn EventSink>,
    ) -> Result<JobStarted, IpcError> {
        let compute_key = crate::compute::ComputeIdentity::from(&self.acceleration().status);
        let session = lock(&self.session);
        let state = lock(&self.results_state);
        let open = session.require().map_err(IpcError::from)?;
        let ticket = session.ticket()?;
        let project = open.store.project();
        if open.tutorial {
            return Err(IpcError::new(
                ErrorCode::TutorialBlocked,
                "tutorial sessions cannot export",
            ));
        }
        let live = state.calibration.as_ref();
        let cached = open.cached.as_ref();
        let cached_sample_rate_policy = cached.map(|cache| cache.labels.sample_rate_policy);
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
        let mdat = matches!(
            params.kind,
            ExportKind::BeforeAfterMdat | ExportKind::StagedMdat
        );
        let cached_mimo = cached
            .filter(|cache| {
                live.is_none()
                    && cache.mimo_compute_key == compute_key
                    && mimo_eligible(
                        &settings,
                        stale,
                        secs_project::measured_positions(project).len(),
                        cache.mimo_key.as_deref(),
                        cache.mimo_calibration_key.as_deref(),
                        cache.run_key.as_deref(),
                    )
            })
            .and_then(|cache| cache.mimo_paths.as_ref())
            .filter(|paths| !paths.paths.is_empty());
        let live_mimo = live.and_then(|calibration| {
            state.mimo.as_ref().filter(|result| {
                mimo_eligible(
                    &settings,
                    stale,
                    calibration.design.spectra.positions.len(),
                    Some(&result.settings_key),
                    Some(&result.calibration_key),
                    Some(&calibration.run_key),
                ) && result.compute_key == compute_key
                    && !result.run.paths.paths.is_empty()
            })
        });
        // Capture only what this export format consumes. In particular the
        // convolution writers never clone the large applied-response matrices.
        let mut live_values = live.filter(|_| mdat).map(|calibration| {
            (
                calibration.design.clone(),
                calibration.measurements.clone(),
                calibration.config.clone(),
                calibration.settings.clone(),
            )
        });
        let live_camilla =
            live.filter(|_| params.kind == ExportKind::CamillaDsp)
                .map(|calibration| {
                    (
                        calibration.design.clone(),
                        labels(&calibration.settings, &calibration.design),
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
            .map(|calibration| &calibration.design.chain)
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
                    let run = ExportRun {
                        design: &calibration.design,
                        measurements: &calibration.measurements,
                        config: &calibration.config,
                        settings: &calibration.settings,
                    };
                    FilterValues {
                        chain: calibration.design.chain.clone(),
                        mimo: export_mimo.take(),
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
                    let (design, measurements, config, settings) = if let Some(values) =
                        live_values.take()
                    {
                        values
                    } else {
                        job.report_progress("design", 0, 1);
                        // Reads the measurements of the project the export
                        // started from; the job stays cancellable.
                        let mut prepared = session::apply(&session_for_design, ticket, |open| {
                            let project = open.store.project().clone();
                            let mut settings = secs_project::calibration_from_project(&project);
                            if let Some(policy) = cached_sample_rate_policy {
                                settings.sample_rate_policy = policy;
                            }
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
                            Ok((
                                (settings, inputs.measurements, inputs.config, inputs.notes),
                                false,
                            ))
                        })?;
                        let ctx = super::calibration::compute_context(&job, memory);
                        prepared.1 = secs_engine::normalize_measurements(
                            &prepared.1,
                            prepared.0.sample_rate_policy,
                            &ctx,
                        )
                        .map_err(super::calibration::engine_error)?;
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
                        if let Some(paths) = cached_mimo_paths.as_ref() {
                            mimo = Some(cached_mimo_responses(
                                &design,
                                &prepared.1,
                                &prepared.2,
                                paths,
                                secs_engine::MimoKey::from_settings(&settings).max_supports,
                            )?);
                        }
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

fn cached_mimo_responses(
    design: &secs_engine::Design,
    measurements: &secs_engine::MeasurementSet,
    config: &secs_engine::EngineConfig,
    paths: &MimoPaths,
    max_supports: usize,
) -> Result<MimoResponses, IpcError> {
    let view = secs_engine::export_view(design, measurements, config)
        .map_err(super::calibration::engine_error)?;
    let frequencies = secs_engine::grid::design_freqs();
    let mut applied = Vec::new();
    for main in &design.chain.mains {
        let base = secs_engine::stage_responses(
            design,
            &main.role,
            &view.spectra,
            design.spectra.main_position,
        )
        .ok_or_else(|| IpcError::new(ErrorCode::EngineInput, "missing MIMO final response"))?
        .output;
        let mut response = base;
        for path in paths.paths.iter().filter(|path| path.input == main.role) {
            let support = view
                .spectra
                .get(design.spectra.main_position, &path.support)
                .ok_or_else(|| {
                    IpcError::new(
                        ErrorCode::EngineInput,
                        format!("missing MIMO support response {}", path.support),
                    )
                })?;
            let delay_ms = path.delay_ms - design.chain.main_extra_ms;
            let delay = delay_response(delay_ms, frequencies);
            let high_pass = secs_dsp::response::lr4(frequencies, path.low_hz, "HP");
            let electrical = cached_mimo_filter_response(&path.filter, frequencies, paths.fsiso)?;
            for ((((output, acoustic), delay), high_pass), electrical) in response
                .iter_mut()
                .zip(support.iter())
                .zip(delay)
                .zip(high_pass)
                .zip(electrical)
            {
                *output += acoustic * delay * high_pass * electrical;
            }
        }
        applied.push((main.role.clone(), response));
    }
    Ok(MimoResponses {
        kind: if paths.paths.iter().any(|path| path.filter.is_fir()) {
            MimoKind::Fir
        } else {
            MimoKind::Iir
        },
        fsiso: paths.fsiso,
        cap_text: secs_engine::mimo::cap_text(max_supports),
        paths: paths.clone(),
        applied,
    })
}

fn delay_response(ms: f64, frequencies: &[f64]) -> Vec<secs_engine::C> {
    frequencies
        .iter()
        .map(|&frequency| {
            secs_engine::C::from_polar(1.0, (-2.0 * std::f64::consts::PI * frequency) * ms * 0.001)
        })
        .collect()
}

fn cached_mimo_filter_response(
    filter: &secs_types::chain::MimoFilter,
    frequencies: &[f64],
    fsiso: f64,
) -> Result<Vec<secs_engine::C>, IpcError> {
    use secs_types::chain::MimoFilter;

    Ok(match filter {
        MimoFilter::Iir {
            gain_db,
            delay_ms,
            polarity,
            peq,
            allpass,
        } => {
            let gain = f64::from(*polarity) * 10_f64.powf(gain_db / 20.0);
            let mut response: Vec<_> = delay_response(*delay_ms, frequencies)
                .into_iter()
                .map(|value| value * gain)
                .collect();
            for &(frequency, q, gain_db) in peq {
                for (value, filter) in
                    response
                        .iter_mut()
                        .zip(secs_dsp::response::peaking_response(
                            frequencies,
                            frequency,
                            q,
                            gain_db,
                            secs_engine::grid::SR_F,
                        ))
                {
                    *value *= filter;
                }
            }
            for &(frequency, q) in allpass {
                for (value, filter) in
                    response
                        .iter_mut()
                        .zip(secs_dsp::response::allpass2(frequencies, frequency, q))
                {
                    *value *= filter;
                }
            }
            for (value, filter) in
                response
                    .iter_mut()
                    .zip(secs_dsp::response::lr4(frequencies, fsiso, "LP"))
            {
                *value *= filter;
            }
            response
        }
        MimoFilter::Fir { taps_48k } => {
            let n = 2 * frequencies.len().saturating_sub(1);
            let mut padded = vec![0.0; n];
            let count = n.min(taps_48k.len());
            padded[..count].copy_from_slice(&taps_48k[..count]);
            secs_dsp::fft::rfft(&padded)
        }
        MimoFilter::FirLowRate { taps_low, fs_low } => {
            if !fs_low.is_finite() || *fs_low <= 0.0 || taps_low.iter().any(|tap| !tap.is_finite())
            {
                return Err(IpcError::new(
                    ErrorCode::ExportFailed,
                    "invalid cached MIMO FIR recipe",
                ));
            }
            secs_dsp::fir::lowrate_response(taps_low, *fs_low, frequencies)
        }
    })
}

fn mimo_eligible(
    settings: &secs_types::calibration::CalibrationSettings,
    stale: bool,
    measured_positions: usize,
    mimo_key: Option<&str>,
    mimo_calibration_key: Option<&str>,
    run_key: Option<&str>,
) -> bool {
    settings.mimo_on
        && !stale
        && measured_positions >= 3
        && mimo_key == Some(settings.mimo_key().as_str())
        && mimo_calibration_key == run_key
        && run_key.is_some()
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
            Some(&engine)
        ));
        for count in [0, 1, 2] {
            assert!(!mimo_eligible(
                &settings,
                false,
                count,
                Some(&mimo),
                Some(&engine),
                Some(&engine)
            ));
        }
        assert!(!mimo_eligible(
            &settings,
            true,
            3,
            Some(&mimo),
            Some(&engine),
            Some(&engine)
        ));
        assert!(!mimo_eligible(
            &settings,
            false,
            3,
            None,
            Some(&engine),
            Some(&engine)
        ));
        assert!(!mimo_eligible(
            &settings,
            false,
            3,
            Some("old"),
            Some(&engine),
            Some(&engine)
        ));
        assert!(!mimo_eligible(
            &settings,
            false,
            3,
            Some(&mimo),
            Some("old"),
            Some(&engine)
        ));
        settings.mimo_on = false;
        assert!(!mimo_eligible(
            &settings,
            false,
            3,
            Some(&mimo),
            Some(&engine),
            Some(&engine)
        ));
    }
}
