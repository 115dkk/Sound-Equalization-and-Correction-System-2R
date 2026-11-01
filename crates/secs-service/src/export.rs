use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use secs_camilla::{DesignLabels, Device, DeviceValue, Options as CamillaOptions};
use secs_export::ExportRun;
use secs_filters::FiltersError;
use secs_types::ipc::{
    CamillaCaptureDevice, CamillaDevices, CamillaPlaybackDevice, ErrorCode, ExportKind,
    ExportParams, ExportResult, IpcError, JobKind, JobResult, JobStarted, Notice, OutputRateChoice,
};

use crate::{EventSink, Service, lock, results::MimoView, session};

fn fallback_notice(rate: Option<f64>) -> Notice {
    let mut params = BTreeMap::new();
    if let Some(rate) = rate {
        params.insert("rate".into(), rate.to_string());
    }
    Notice {
        code: "output_rate_fallback".into(),
        params,
    }
}

fn resolve_output_rate(
    choice: OutputRateChoice,
    backend: &dyn Fn() -> Box<dyn secs_audio::AudioBackend>,
) -> (u32, Vec<Notice>) {
    match choice {
        OutputRateChoice::Fixed(rate) => return (rate, Vec::new()),
        OutputRateChoice::Auto => {}
    }
    let endpoints = match backend().enumerate() {
        Ok(endpoints) => endpoints,
        Err(_) => return (48_000, vec![fallback_notice(None)]),
    };
    let Some(rate) = endpoints
        .iter()
        .find(|endpoint| endpoint.is_default_output)
        .map(|endpoint| endpoint.default_samplerate)
    else {
        return (48_000, vec![fallback_notice(None)]);
    };
    if rate.is_finite()
        && rate.fract() == 0.0
        && (0.0..=f64::from(u32::MAX)).contains(&rate)
        && secs_dsp::rate::is_output_rate(rate as u32)
    {
        (rate as u32, Vec::new())
    } else {
        (48_000, vec![fallback_notice(Some(rate))])
    }
}

/// The extra convolution rates to write. A rate equal to the main rate is
/// refused when the user chose that rate, and dropped when `Auto` resolved to
/// it: the screen cannot know the device's rate before the export starts.
fn extra_rates_for(rates: &[u32], output_rate: u32, auto: bool) -> Result<Vec<u32>, IpcError> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for &rate in rates {
        if !secs_dsp::rate::is_output_rate(rate) {
            return Err(IpcError::new(
                ErrorCode::InvalidRequest,
                "unsupported extra output sample rate",
            )
            .with_param("rate", rate.to_string()));
        }
        if !seen.insert(rate) || (rate == output_rate && !auto) {
            return Err(IpcError::new(
                ErrorCode::InvalidRequest,
                "extra output sample rates must be distinct from the main rate and each other",
            )
            .with_param("rate", rate.to_string()));
        }
        if rate != output_rate {
            out.push(rate);
        }
    }
    Ok(out)
}

impl Service {
    pub(crate) fn start_export(
        &self,
        params: ExportParams,
        events: Arc<dyn EventSink>,
    ) -> Result<JobStarted, IpcError> {
        self.start_export_with(params, events, JobKind::Export, true, |result, _| {
            Ok(JobResult::Export(result))
        })
    }

    pub(crate) fn start_export_with<F>(
        &self,
        params: ExportParams,
        events: Arc<dyn EventSink>,
        job_kind: JobKind,
        record_export: bool,
        complete: F,
    ) -> Result<JobStarted, IpcError>
    where
        F: FnOnce(ExportResult, Option<String>) -> Result<JobResult, IpcError> + Send + 'static,
    {
        let compute_key = crate::compute::ComputeIdentity::from(&self.acceleration().status);
        let mut session = lock(&self.session);
        let ticket = session.ticket()?;
        let open = session.require_mut().map_err(IpcError::from)?;
        if open.tutorial {
            return Err(IpcError::new(
                ErrorCode::TutorialBlocked,
                "tutorial sessions cannot export",
            ));
        }
        let requested_design_name = crate::design_export::validate_design_name(
            params.design_name.as_deref(),
        )
        .map_err(|reason| {
            IpcError::new(ErrorCode::InvalidRequest, "invalid design name")
                .with_param("reason", reason)
        })?;
        if requested_design_name.is_some()
            && matches!(
                params.kind,
                ExportKind::BeforeAfterMdat | ExportKind::StagedMdat
            )
        {
            return Err(IpcError::new(
                ErrorCode::InvalidRequest,
                "design_name only applies to filter exports",
            )
            .with_param("reason", "design_name"));
        }
        if params.camilla_devices.is_some() && params.kind != ExportKind::CamillaDsp {
            return Err(IpcError::new(
                ErrorCode::InvalidRequest,
                "camilla_devices only applies to CamillaDSP",
            )
            .with_param("reason", "camilla_devices"));
        }
        if let Some(devices) = &params.camilla_devices {
            let invalid_asio = matches!(&devices.capture, CamillaCaptureDevice::Asio { device } if device.trim().is_empty())
                || matches!(&devices.playback, CamillaPlaybackDevice::Asio { device } if device.trim().is_empty());
            let invalid_loopback = matches!(
                &devices.capture,
                CamillaCaptureDevice::Wasapi {
                    exclusive: true,
                    loopback: true,
                    ..
                }
            );
            if invalid_asio || invalid_loopback {
                return Err(IpcError::new(
                    ErrorCode::InvalidRequest,
                    "invalid CamillaDSP device options",
                )
                .with_param(
                    "reason",
                    if invalid_asio {
                        "camilla_asio_device"
                    } else {
                        "camilla_wasapi_loopback_exclusive"
                    },
                ));
            }
        }
        if params.mdat_scope.is_some() && params.kind != ExportKind::BeforeAfterMdat {
            return Err(IpcError::new(
                ErrorCode::InvalidRequest,
                "mdat_scope only applies to REW mdat",
            )
            .with_param("reason", "mdat_scope"));
        }
        if !params.extra_rates.is_empty() && params.kind != ExportKind::Convolution {
            return Err(IpcError::new(
                ErrorCode::InvalidRequest,
                "extra_rates only applies to convolution export",
            )
            .with_param("reason", "extra_rates"));
        }
        let mdat_scope = params.mdat_scope.unwrap_or_default();
        let now = (self.deps.clock)();
        let project_mode = secs_types::calibration::mode_tag(
            secs_project::calibration_from_project(open.store.project()).mode,
        )
        .to_owned();
        let output_rate_auto = open.store.project().dsp.output_rate == OutputRateChoice::Auto;
        let (output_rate, output_rate_notes) = if matches!(
            params.kind,
            ExportKind::BeforeAfterMdat | ExportKind::StagedMdat
        ) {
            (48_000, Vec::new())
        } else {
            resolve_output_rate(
                open.store.project().dsp.output_rate,
                self.deps.audio_backend.as_ref(),
            )
        };
        let mut params = params;
        params.extra_rates = extra_rates_for(&params.extra_rates, output_rate, output_rate_auto)?;
        if params.kind == ExportKind::BeforeAfterMdat
            && mdat_scope == secs_export::MdatScope::Measurements
        {
            let originals = session::original_measurements(open).map_err(IpcError::from)?;
            if originals.is_empty() {
                return Err(IpcError::new(
                    ErrorCode::EngineInput,
                    "there are no measurements to export",
                )
                .with_param("reason", "no_measurements"));
            }
            let uuid = self.deps.uuid.clone();
            let now_ms = (self.deps.unix_ms)();
            let export_session = self.session.clone();
            let completion_clock = self.deps.clock.clone();
            let completion_offset = self.deps.clock_offset_seconds.clone();
            drop(session);
            let destination = if params.folder.is_empty() {
                crate::export_folder::measurement_mdat_folder(&self.resolved_export_base()?, now)
            } else {
                Path::new(&params.folder).to_path_buf()
            };
            return self.jobs.start(JobKind::Export, events, move |job| {
                job.cancel()
                    .check()
                    .map_err(|_| IpcError::new(ErrorCode::Cancelled, "cancelled"))?;
                job.begin_commit()?;
                let folder = destination.as_path();
                let mut uuid = lock(&uuid);
                let destination = secs_export::export_rew_mdat_using(
                    None,
                    mdat_scope,
                    &originals,
                    folder,
                    None,
                    None,
                    None,
                    now_ms,
                    uuid.as_mut(),
                )
                .map_err(rew_export_error)?;
                let mut paths = vec![destination];
                if let Some(path) = write_compute_provenance(folder, None)? {
                    paths.push(path);
                }
                let result = ExportResult {
                    files: paths
                        .into_iter()
                        .map(|path| path.to_string_lossy().into_owned())
                        .collect(),
                    output_rate: 48_000,
                    notes: Vec::new(),
                    folder: folder.to_string_lossy().into_owned(),
                    stale: false,
                };
                lock(&export_session).record_export(secs_types::local_api::LastExport {
                    kind: ExportKind::BeforeAfterMdat,
                    folder: result.folder.clone(),
                    mimo: false,
                    mode: project_mode,
                    design_name: None,
                    design_id: None,
                    time: completion_clock()
                        .shift_seconds(completion_offset().saturating_neg())
                        .rfc3339_utc(),
                });
                Ok(JobResult::Export(result))
            });
        }
        let stale = {
            let calibration = open.results.calibration(open.store.project());
            if calibration.is_none() {
                return Err(IpcError::new(
                    ErrorCode::EngineInput,
                    "calibration must run before export",
                ));
            }
            calibration.is_some_and(crate::results::RunView::stale)
        };
        let originals = if params.kind == ExportKind::BeforeAfterMdat
            && mdat_scope == secs_export::MdatScope::All
        {
            session::original_measurements(open).map_err(IpcError::from)?
        } else {
            Vec::new()
        };
        let project = open.store.project();
        let project_name =
            (!project.name.trim().is_empty()).then(|| project.name.trim().to_owned());
        let app_version = self.deps.app_version.to_string();
        let live = open.results.live_calibration();
        let cached = open.results.cached();
        let cached_sample_rate_policy = cached.map(|cache| cache.labels.sample_rate_policy);
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
        let completion_clock = self.deps.clock.clone();
        let completion_offset = self.deps.clock_offset_seconds.clone();
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
            ExportKind::FilterText | ExportKind::EqualizerApo | ExportKind::Convolution
        )
        .then(|| (export_chain.clone(), export_mimo.take()));
        // One export reads the clock once; every file in the bundle gets that date.
        let now_ms = (self.deps.unix_ms)();
        let is_filter_export = matches!(
            params.kind,
            ExportKind::CamillaDsp
                | ExportKind::FilterText
                | ExportKind::EqualizerApo
                | ExportKind::Convolution
        );
        let manifest_chain = is_filter_export.then(|| export_chain.clone());
        let manifest_mimo = if is_filter_export {
            filter_values
                .as_ref()
                .and_then(|(_, mimo)| mimo.clone())
                .or_else(|| export_mimo.clone())
        } else {
            None
        };
        let created_utc = is_filter_export
            .then(|| crate::design_export::created_utc(now_ms).map_err(export_error))
            .transpose()?;
        let display_design_name = requested_design_name
            .clone()
            .unwrap_or_else(|| description.label().to_owned());
        let design_id = is_filter_export
            .then(|| {
                secs_graph::design::design_id(
                    export_chain,
                    manifest_mimo.as_ref(),
                    secs_graph::LfeChoice::DOLBY,
                )
                .map_err(export_error)
            })
            .transpose()?;
        let camilla_devices = params.camilla_devices.clone();
        let explicit = (!params.folder.is_empty()).then(|| Path::new(&params.folder).to_path_buf());
        let explicit_unnamed = explicit.is_some() && requested_design_name.is_none();
        drop(session);
        // An explicit folder is the base of a named bundle too, so the default
        // base (and the host's home directory behind it) is only read without one.
        let default_base = if explicit.is_none() {
            Some(self.resolved_export_base()?)
        } else {
            None
        };
        let base = explicit
            .clone()
            .or(default_base.clone())
            .unwrap_or_default();
        let final_destination = if let Some(name) = requested_design_name.as_deref() {
            crate::design_export::named_folder(&base, params.kind, name)
        } else if let Some(explicit) = explicit {
            explicit
        } else {
            let base = default_base.as_deref().expect("resolved export base");
            match params.kind {
                ExportKind::CamillaDsp => crate::export_folder::camilladsp_folder(
                    base,
                    folder_includes_mimo,
                    mode_name,
                    now,
                ),
                ExportKind::BeforeAfterMdat | ExportKind::StagedMdat => {
                    crate::export_folder::mdat_folder(base, folder_includes_mimo, mode_name, now)
                }
                ExportKind::FilterText => {
                    crate::export_folder::text_folder(base, includes_mimo, mode_name, now)
                }
                ExportKind::EqualizerApo => {
                    crate::export_folder::eqapo_folder(base, includes_mimo, mode_name, now)
                }
                ExportKind::Convolution => {
                    crate::export_folder::convolution_folder(base, includes_mimo, mode_name, now)
                }
            }
        };
        let destination = if requested_design_name.is_some() {
            crate::design_export::staging_folder(&final_destination)
        } else {
            final_destination.clone()
        };
        let registry_base = if is_filter_export {
            if requested_design_name.is_some() {
                Some(base)
            } else if let Some(base) = default_base {
                Some(base)
            } else {
                final_destination.parent().map(Path::to_path_buf)
            }
        } else {
            None
        };
        self.jobs.start(job_kind, events, move |job| {
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
                    let (capture, playback) = camilla_options(
                        camilla_devices.as_ref(),
                        chain.mains.len() + usize::from(!chain.subs.is_empty()),
                        chain.mains.len() + chain.subs.len(),
                    );
                    let options = CamillaOptions {
                        capture,
                        playback,
                        output_rate,
                        design: Some(DesignLabels {
                            project_name: project_name.clone(),
                            design_name: display_design_name.clone(),
                            design_id: design_id.clone().expect("filter export design id"),
                            bypass: false,
                        }),
                        headroom: true,
                        labels: true,
                        bypass: true,
                        ..CamillaOptions::default()
                    };
                    job.begin_commit()?;
                    secs_camilla::export_with_options(
                        chain,
                        folder,
                        &description.camilla(),
                        export_mimo.as_ref(),
                        options,
                    )
                    .map_err(export_error)?
                }
                ExportKind::FilterText | ExportKind::EqualizerApo | ExportKind::Convolution => {
                    let values = filter_values.as_ref().ok_or_else(|| {
                        IpcError::new(ErrorCode::Internal, "filter export values disappeared")
                    })?;
                    let run = description.filters(&values.0, values.1.as_ref());
                    let options = secs_filters::Options {
                        output_rate,
                        extra_rates: params.extra_rates.clone(),
                    };
                    match params.kind {
                        ExportKind::FilterText => {
                            job.begin_commit()?;
                            secs_filters::export_filter_text_with_options(&run, folder, &options)
                        }
                        ExportKind::EqualizerApo => {
                            job.begin_commit()?;
                            secs_filters::export_eqapo_with_options(&run, folder, &options)
                        }
                        ExportKind::Convolution => {
                            let prepared = secs_filters::prepare_convolution_named_with_options(
                                &run,
                                &options,
                                requested_design_name.as_deref(),
                                &|| job.cancel().is_cancelled(),
                                &|done, total| {
                                    job.report_progress("export.convolution", done, total);
                                },
                            )
                            .map_err(filter_error)?;
                            job.begin_commit()?;
                            prepared.install(folder)
                        }
                        _ => unreachable!(),
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
                        let ctx = super::calibration::compute_context(&job, memory, false);
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
                        .map_err(export_error)?
                    } else {
                        secs_export::export_rew_mdat_using(
                            Some(&run),
                            mdat_scope,
                            &originals,
                            folder,
                            mimo.as_ref(),
                            Some(&sources),
                            spl_offset,
                            now_ms,
                            uuid.as_mut(),
                        )
                        .map_err(rew_export_error)?
                    };
                    vec![destination]
                }
            };
            if let Some(path) = write_compute_provenance(folder, compute_report.as_ref())? {
                paths.push(path);
            }
            let mut design_id = None;
            if is_filter_export {
                let metadata = crate::design_export::DesignMetadata {
                    design_name: display_design_name.clone(),
                    project_name: project_name.clone(),
                    app_version: app_version.clone(),
                    created_utc: created_utc.clone().expect("filter export timestamp"),
                    export_kind: params.kind,
                    mode: mode_name.to_owned(),
                    latency_ms: description.latency_ms(),
                    output_rate,
                    output_rate_auto,
                    chain: manifest_chain.clone().expect("filter export chain"),
                    mimo: manifest_mimo.clone(),
                    bypass_config: (params.kind == ExportKind::CamillaDsp)
                        .then(|| format!("configs/secs_{}_bypass.yml", description.camilla().mode)),
                };
                let (manifest, id) = crate::design_export::write_manifest(folder, &metadata)
                    .map_err(export_error)?;
                paths.push(manifest);
                design_id = Some(id);
            }
            if requested_design_name.is_some() {
                crate::design_export::replace_folder(folder, &final_destination)
                    .map_err(export_error)?;
                for path in &mut paths {
                    if let Ok(relative) = path.strip_prefix(folder) {
                        *path = final_destination.join(relative);
                    }
                }
            }
            if let Some(base) = &registry_base {
                if explicit_unnamed {
                    crate::design_export::rebuild_index_selected(base, &final_destination)
                        .map_err(export_error)?;
                } else {
                    crate::design_export::rebuild_index(base).map_err(export_error)?;
                }
            }
            let result = ExportResult {
                files: paths
                    .into_iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect(),
                output_rate,
                notes: output_rate_notes.clone(),
                folder: final_destination.to_string_lossy().into_owned(),
                stale,
            };
            if record_export {
                lock(&session_for_design).record_export(secs_types::local_api::LastExport {
                    kind: params.kind,
                    folder: result.folder.clone(),
                    mimo: folder_includes_mimo,
                    mode: mode_name.to_owned(),
                    design_name: requested_design_name.clone(),
                    design_id: design_id.clone(),
                    time: completion_clock()
                        .shift_seconds(completion_offset().saturating_neg())
                        .rfc3339_utc(),
                });
            }
            complete(result, design_id)
        })
    }
}

fn camilla_options(
    devices: Option<&CamillaDevices>,
    inputs: usize,
    outputs: usize,
) -> (Option<Device>, Option<Device>) {
    let Some(devices) = devices else {
        return (None, None);
    };
    let capture = match &devices.capture {
        CamillaCaptureDevice::File => None,
        CamillaCaptureDevice::Wasapi {
            device,
            format,
            exclusive,
            loopback,
        } => {
            let mut values = vec![
                ("type".into(), DeviceValue::Str("Wasapi".into())),
                ("channels".into(), DeviceValue::Int(inputs as i64)),
            ];
            if let Some(device) = device {
                values.push(("device".into(), DeviceValue::Str(device.clone())));
            }
            values.extend([
                ("format".into(), DeviceValue::Str(format.name().into())),
                ("exclusive".into(), DeviceValue::Bool(*exclusive)),
                ("loopback".into(), DeviceValue::Bool(*loopback)),
            ]);
            Some(Device(values))
        }
        CamillaCaptureDevice::Asio { device } => Some(Device(vec![
            ("type".into(), DeviceValue::Str("Asio".into())),
            ("channels".into(), DeviceValue::Int(inputs as i64)),
            ("device".into(), DeviceValue::Str(device.clone())),
        ])),
    };
    let playback = match &devices.playback {
        CamillaPlaybackDevice::File => None,
        CamillaPlaybackDevice::Wasapi {
            device,
            format,
            exclusive,
        } => {
            let mut values = vec![
                ("type".into(), DeviceValue::Str("Wasapi".into())),
                ("channels".into(), DeviceValue::Int(outputs as i64)),
            ];
            if let Some(device) = device {
                values.push(("device".into(), DeviceValue::Str(device.clone())));
            }
            values.extend([
                ("format".into(), DeviceValue::Str(format.name().into())),
                ("exclusive".into(), DeviceValue::Bool(*exclusive)),
            ]);
            Some(Device(values))
        }
        CamillaPlaybackDevice::Asio { device } => Some(Device(vec![
            ("type".into(), DeviceValue::Str("Asio".into())),
            ("channels".into(), DeviceValue::Int(outputs as i64)),
            ("device".into(), DeviceValue::Str(device.clone())),
        ])),
    };
    (capture, playback)
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

fn rew_export_error(error: secs_export::ExportError) -> IpcError {
    match error {
        secs_export::ExportError::NoMeasurements => IpcError::new(
            ErrorCode::EngineInput,
            "there are no measurements to export",
        )
        .with_param("reason", "no_measurements"),
        secs_export::ExportError::NoResult => {
            IpcError::new(ErrorCode::EngineInput, "calibration must run before export")
        }
        error => export_error(error),
    }
}

fn export_error(error: impl std::fmt::Display) -> IpcError {
    IpcError::new(ErrorCode::ExportFailed, error.to_string())
}

fn filter_error(error: FiltersError) -> IpcError {
    match error {
        FiltersError::Scope(block) => IpcError::new(ErrorCode::ExportScope, error.to_string())
            .with_param("reason", block.name()),
        FiltersError::Cancelled => IpcError::new(ErrorCode::Cancelled, "cancelled"),
        _ => export_error(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secs_audio::{
        AudioBackend, AudioError, Direction, Endpoint, InputSession, OutputSession, ProbeResult,
        ShareMode, StreamSpec,
    };

    struct FakeBackend {
        endpoints: Vec<Endpoint>,
        fail: bool,
    }

    impl AudioBackend for FakeBackend {
        fn name(&self) -> &'static str {
            "output-rate-test"
        }

        fn enumerate(&self) -> Result<Vec<Endpoint>, AudioError> {
            if self.fail {
                Err(AudioError::Backend("test failure".into()))
            } else {
                Ok(self.endpoints.clone())
            }
        }

        fn probe(
            &self,
            _: &Endpoint,
            _: Direction,
            _: StreamSpec,
            _: ShareMode,
        ) -> Result<ProbeResult, AudioError> {
            unreachable!("the output-rate resolver only enumerates endpoints")
        }

        fn open_output(
            &self,
            _: &Endpoint,
            _: StreamSpec,
            _: ShareMode,
        ) -> Result<Box<dyn OutputSession>, AudioError> {
            unreachable!("the output-rate resolver only enumerates endpoints")
        }

        fn open_input(
            &self,
            _: &Endpoint,
            _: StreamSpec,
            _: ShareMode,
        ) -> Result<Box<dyn InputSession>, AudioError> {
            unreachable!("the output-rate resolver only enumerates endpoints")
        }
    }

    fn endpoint(rate: f64, is_default_output: bool) -> Endpoint {
        Endpoint {
            id: rate.to_string(),
            name: "test".into(),
            host_api: "test".into(),
            max_input_channels: 0,
            max_output_channels: 2,
            default_samplerate: rate,
            is_default_input: false,
            is_default_output,
        }
    }

    fn resolved(
        choice: OutputRateChoice,
        endpoints: Vec<Endpoint>,
        fail: bool,
    ) -> (u32, Vec<Notice>) {
        resolve_output_rate(choice, &move || {
            Box::new(FakeBackend {
                endpoints: endpoints.clone(),
                fail,
            })
        })
    }

    #[test]
    fn output_rate_resolver_uses_fixed_and_supported_default_rates() {
        assert_eq!(
            resolved(OutputRateChoice::Fixed(192_000), Vec::new(), true),
            (192_000, Vec::new())
        );
        for rate in [44_100.0, 96_000.0] {
            assert_eq!(
                resolved(OutputRateChoice::Auto, vec![endpoint(rate, true)], false),
                (rate as u32, Vec::new())
            );
        }
    }

    #[test]
    fn extra_rates_drop_the_resolved_auto_rate_and_refuse_a_chosen_one() {
        // Auto resolved to 44.1 kHz: the screen could not know, so it is dropped.
        assert_eq!(
            extra_rates_for(&[44_100, 96_000], 44_100, true).unwrap(),
            vec![96_000]
        );
        // A fixed main rate repeated as an extra rate is the user's mistake.
        assert!(extra_rates_for(&[48_000], 48_000, false).is_err());
        assert!(extra_rates_for(&[96_000, 96_000], 48_000, true).is_err());
        assert!(extra_rates_for(&[32_000], 48_000, false).is_err());
        assert_eq!(
            extra_rates_for(&[], 48_000, false).unwrap(),
            Vec::<u32>::new()
        );
    }

    #[test]
    fn output_rate_resolver_falls_back_for_bad_missing_and_failed_defaults() {
        let (rate, notes) = resolved(
            OutputRateChoice::Auto,
            vec![endpoint(32_000.0, true)],
            false,
        );
        assert_eq!(rate, 48_000);
        assert_eq!(notes[0].code, "output_rate_fallback");
        assert_eq!(
            notes[0].params.get("rate").map(String::as_str),
            Some("32000")
        );

        for (endpoints, fail) in [(vec![endpoint(96_000.0, false)], false), (Vec::new(), true)] {
            let (rate, notes) = resolved(OutputRateChoice::Auto, endpoints, fail);
            assert_eq!(rate, 48_000);
            assert_eq!(notes[0].code, "output_rate_fallback");
            assert!(notes[0].params.is_empty());
        }
    }

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
