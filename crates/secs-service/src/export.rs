use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use secs_camilla::{DesignLabels, Device, DeviceValue, Options as CamillaOptions};
use secs_export::ExportRun;
use secs_filters::FiltersError;
use secs_types::ipc::{
    CamillaCaptureDevice, CamillaDevices, CamillaPlaybackDevice, ErrorCode, ExportKind,
    ExportParams, ExportResult as IpcExportResult, IpcError, JobResult, JobStarted, Notice,
    OutputRateChoice,
};

use crate::{
    EventSink, Service,
    export_plan::{
        DesignSource, Destination, ExportPlan, ExportPurpose, LastExportRecord, ManifestMeta,
        MimoSelection,
    },
    lock,
    results::{MimoView, ProjectKeys},
    session,
};

#[derive(Debug)]
pub(crate) struct ExportResult {
    pub files: Vec<String>,
    pub output_rate: u32,
    pub notes: Vec<Notice>,
    pub folder: String,
    pub stale: bool,
    pub main_config: Option<std::path::PathBuf>,
    pub design_id: Option<String>,
}

impl ExportResult {
    fn into_ipc(self) -> IpcExportResult {
        IpcExportResult {
            files: self.files,
            output_rate: self.output_rate,
            notes: self.notes,
            folder: self.folder,
            stale: self.stale,
        }
    }
}

struct PreparedFolder {
    prepared: std::path::PathBuf,
    destination: std::path::PathBuf,
    relative_files: Vec<std::path::PathBuf>,
    moved: bool,
}

impl PreparedFolder {
    fn path_for(destination: &Path, job: secs_types::ipc::JobId) -> std::path::PathBuf {
        let name = destination
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "export".into());
        destination.with_file_name(format!(".{name}.prepare-{}", job.0))
    }

    fn write(mut self, log: &crate::LogSink) -> Result<Vec<std::path::PathBuf>, IpcError> {
        crate::design_export::replace_folder_with_log(&self.prepared, &self.destination, log)
            .map_err(export_error)?;
        self.moved = true;
        Ok(self
            .relative_files
            .iter()
            .map(|relative| self.destination.join(relative))
            .collect())
    }
}

impl Drop for PreparedFolder {
    fn drop(&mut self) {
        if !self.moved {
            let _ = std::fs::remove_dir_all(&self.prepared);
        }
    }
}

struct EqapoWriter {
    prepared: PreparedFolder,
}

impl EqapoWriter {
    fn prepare(
        run: &secs_filters::FilterRun<'_>,
        destination: &Path,
        options: &secs_filters::Options,
        job: &crate::jobs::JobContext,
    ) -> Result<Self, IpcError> {
        let prepared = PreparedFolder::path_for(destination, job.id());
        match std::fs::remove_dir_all(&prepared) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(export_error(error)),
        }
        let installed = secs_filters::export_eqapo_observed_with_options(run, &prepared, options)
            .map_err(filter_error)?;
        let relative_files = installed
            .files
            .iter()
            .map(|path| {
                path.strip_prefix(&prepared)
                    .expect("prepared Equalizer APO files stay below their folder")
                    .to_owned()
            })
            .collect();
        Ok(Self {
            prepared: PreparedFolder {
                prepared,
                destination: destination.to_owned(),
                relative_files,
                moved: false,
            },
        })
    }

    fn write(self, log: &crate::LogSink) -> Result<Vec<std::path::PathBuf>, IpcError> {
        self.prepared.write(log)
    }
}

struct FilterTextWriter {
    prepared: PreparedFolder,
}

impl FilterTextWriter {
    fn prepare(
        run: &secs_filters::FilterRun<'_>,
        destination: &Path,
        options: &secs_filters::Options,
        job: &crate::jobs::JobContext,
    ) -> Result<Self, IpcError> {
        let prepared = PreparedFolder::path_for(destination, job.id());
        match std::fs::remove_dir_all(&prepared) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(export_error(error)),
        }
        let installed =
            secs_filters::export_filter_text_observed_with_options(run, &prepared, options)
                .map_err(filter_error)?;
        let relative_files = installed
            .files
            .iter()
            .map(|path| {
                path.strip_prefix(&prepared)
                    .expect("prepared filter text stays below its folder")
                    .to_owned()
            })
            .collect();
        Ok(Self {
            prepared: PreparedFolder {
                prepared,
                destination: destination.to_owned(),
                relative_files,
                moved: false,
            },
        })
    }

    fn write(self, log: &crate::LogSink) -> Result<Vec<std::path::PathBuf>, IpcError> {
        self.prepared.write(log)
    }
}

struct BeforeAfterMdatWriter {
    destination: std::path::PathBuf,
    bytes: Vec<u8>,
}

impl BeforeAfterMdatWriter {
    #[allow(clippy::too_many_arguments)]
    fn prepare(
        run: Option<&ExportRun<'_>>,
        scope: secs_export::MdatScope,
        originals: &[secs_export::Original],
        folder: &Path,
        mimo: Option<&secs_export::MimoResponses>,
        sources: Option<&secs_export::Sources>,
        spl_offset: Option<f64>,
        now_ms: i64,
        uuid: &crate::UuidSource,
    ) -> Result<Self, IpcError> {
        let items = secs_export::rew_mdat_items(run, scope, originals, mimo, sources)
            .map_err(rew_export_error)?;
        let label = run.map(|run| run.settings.label()).unwrap_or_default();
        let mut uuid = lock(uuid);
        let uuids = (0..items.len())
            .map(|_| uuid.as_mut()())
            .collect::<Result<Vec<_>, _>>()
            .map_err(export_error)?;
        let mut uuids = uuids.into_iter();
        let bytes = secs_mdat::build(&items, secs_mdat::TEMPLATE, spl_offset, now_ms, &mut || {
            uuids.next().expect("one UUID per before-after mdat item")
        })
        .map_err(export_error)?;
        Ok(Self {
            destination: folder.join(secs_export::rew_mdat_filename(scope, &label)),
            bytes,
        })
    }

    fn write(self) -> Result<std::path::PathBuf, IpcError> {
        if let Some(parent) = self.destination.parent() {
            std::fs::create_dir_all(parent).map_err(export_error)?;
        }
        secs_types::fs::write_atomic_bytes(&self.destination, &self.bytes).map_err(export_error)?;
        Ok(self.destination)
    }
}

struct StagedMdatWriter {
    destination: std::path::PathBuf,
    bytes: Vec<u8>,
}

impl StagedMdatWriter {
    fn prepare(
        run: &ExportRun<'_>,
        folder: &Path,
        mimo: Option<&secs_export::MimoResponses>,
        spl_offset: Option<f64>,
        now_ms: i64,
        uuid: &crate::UuidSource,
    ) -> Result<Self, IpcError> {
        let label = run.settings.label();
        let items = match mimo {
            Some(mimo) => secs_export::stage_items_mimo(run, mimo, Some(&label)),
            None => secs_export::stage_items(run, Some(&label)),
        }
        .map_err(export_error)?;
        let mut uuid = lock(uuid);
        let uuids = (0..items.len())
            .map(|_| uuid.as_mut()())
            .collect::<Result<Vec<_>, _>>()
            .map_err(export_error)?;
        let mut uuids = uuids.into_iter();
        let bytes = secs_mdat::build(&items, secs_mdat::TEMPLATE, spl_offset, now_ms, &mut || {
            uuids.next().expect("one UUID per staged mdat item")
        })
        .map_err(export_error)?;
        Ok(Self {
            destination: folder.join(format!("SECS 보정 설계 {label}.mdat")),
            bytes,
        })
    }

    fn write(self) -> Result<std::path::PathBuf, IpcError> {
        if let Some(parent) = self.destination.parent() {
            std::fs::create_dir_all(parent).map_err(export_error)?;
        }
        secs_types::fs::write_atomic_bytes(&self.destination, &self.bytes).map_err(export_error)?;
        Ok(self.destination)
    }
}

struct CamillaWriter<'a> {
    chain: &'a secs_types::chain::SignalChain,
    folder: &'a Path,
    devices: Option<&'a CamillaDevices>,
    output_rate: u32,
    project_name: Option<String>,
    design_name: String,
    design_id: String,
    labels: secs_camilla::RunLabels,
    mimo: Option<&'a secs_types::chain::MimoPaths>,
    prepared: Option<secs_camilla::PreparedConfig>,
    main_config: std::path::PathBuf,
}

impl CamillaWriter<'_> {
    fn prepare(&mut self, job: &crate::jobs::JobContext) -> Result<(), IpcError> {
        let (capture, playback) = camilla_options(
            self.devices,
            self.chain.mains.len() + usize::from(!self.chain.subs.is_empty()),
            self.chain.mains.len() + self.chain.subs.len(),
        );
        let mut options = CamillaOptions {
            capture,
            playback,
            output_rate: self.output_rate,
            design: Some(DesignLabels {
                project_name: self.project_name.clone(),
                design_name: self.design_name.clone(),
                design_id: self.design_id.clone(),
                bypass: false,
            }),
            headroom: true,
            labels: true,
            bypass: true,
            ..CamillaOptions::default()
        };
        options.note = self.labels.note.clone();
        options.latency_ms = Some(self.labels.latency_ms);
        options.mode = self.labels.mode.clone();
        let name = format!("secs_{}", options.mode);
        self.main_config = self.folder.join("configs").join(format!("{name}.yml"));
        job.report_progress("export.camilla.prepare", 0, 1);
        self.prepared = Some(
            secs_camilla::prepare_config(self.chain, self.folder, &name, &options, self.mimo)
                .map_err(export_error)?,
        );
        Ok(())
    }

    fn write(
        self,
        log: &crate::LogSink,
    ) -> Result<(Vec<std::path::PathBuf>, std::path::PathBuf), IpcError> {
        let installed = secs_camilla::install_prepared(
            self.folder,
            self.prepared.expect("CamillaDSP writer was prepared"),
            &|_, _| Ok(()),
            &|_, _| Ok(()),
        )
        .map_err(export_error)?;
        log_leftover_backup(log, installed.leftover_backup);
        Ok((installed.files, self.main_config))
    }
}

fn log_leftover_backup(log: &crate::LogSink, leftover: Option<(std::path::PathBuf, String)>) {
    if let Some((path, error)) = leftover {
        log(
            crate::LogLevel::Warn,
            "export",
            &format!(
                "backup folder {} could not be removed after install: {error}",
                path.display()
            ),
        );
    }
}

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
    pub(crate) fn plan_export(
        &self,
        mut params: ExportParams,
        purpose: ExportPurpose,
    ) -> Result<ExportPlan, IpcError> {
        let (ticket, output_rate_choice) = {
            let session = lock(&self.session);
            let ticket = session.ticket()?;
            let open = session.require().map_err(IpcError::from)?;
            (ticket, open.project().dsp.output_rate)
        };
        let output_rate_auto = output_rate_choice == OutputRateChoice::Auto;
        let (output_rate, output_rate_notes) = if matches!(
            params.kind,
            ExportKind::BeforeAfterMdat | ExportKind::StagedMdat
        ) {
            (48_000, Vec::new())
        } else {
            resolve_output_rate(output_rate_choice, self.deps.audio_backend.as_ref())
        };
        let now = (self.deps.clock)();
        let now_ms = (self.deps.unix_ms)();
        self.with_open_ticket(ticket, |open, compute_key| {
            if open.tutorial {
                return Err(IpcError::new(
                    ErrorCode::TutorialBlocked,
                    "tutorial sessions cannot export",
                )
                .with_param("reason", "tutorial"));
            }
            if !params.folder.is_empty() && !Path::new(&params.folder).is_absolute() {
                return Err(IpcError::new(
                    ErrorCode::InvalidRequest,
                    "export folder must be an absolute path",
                )
                .with_param("reason", "relative_folder")
                .with_param("field", "folder"));
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
            params.extra_rates =
                extra_rates_for(&params.extra_rates, output_rate, output_rate_auto)?;
            let mdat_scope = params.mdat_scope.unwrap_or_default();
            let project = open.project();
            let project_mode = secs_types::calibration::mode_tag(
                secs_project::calibration_from_project(project).mode,
            )
            .to_owned();

            if params.kind == ExportKind::BeforeAfterMdat
                && mdat_scope == secs_export::MdatScope::Measurements
            {
                if open
                    .store()
                    .records()
                    .iter()
                    .all(|record| record.ir.values().all(String::is_empty))
                {
                    return Err(IpcError::new(
                        ErrorCode::EngineInput,
                        "there are no measurements to export",
                    )
                    .with_param("reason", "no_measurements"));
                }
                let snapshot = session::ProjectSnapshot::of(ticket, open);
                let final_folder = if params.folder.is_empty() {
                    crate::export_folder::measurement_mdat_folder(
                        &self.resolved_export_base_for(open)?,
                        now,
                    )
                } else {
                    Path::new(&params.folder).to_path_buf()
                };
                return Ok(ExportPlan {
                    kind: params.kind,
                    purpose,
                    source: DesignSource::Measurements {
                        snapshot: Arc::new(snapshot),
                    },
                    mimo: None,
                    description: crate::export_run::ExportDescription::measurements(
                        secs_project::calibration_from_project(project).mode,
                        now,
                    ),
                    destination: Destination {
                        staging: final_folder.clone(),
                        final_folder,
                        index_root: None,
                        explicit_unnamed: !params.folder.is_empty(),
                    },
                    manifest: ManifestMeta {
                        project_name: None,
                        app_version: self.deps.app_version.to_string(),
                        created_utc: None,
                        display_design_name: String::new(),
                        output_rate_auto: false,
                        design_id: None,
                    },
                    record: (purpose == ExportPurpose::Export).then_some(LastExportRecord {
                        mode: project_mode,
                        includes_mimo: false,
                        design_name: None,
                    }),
                    output_rate: 48_000,
                    output_rate_notes: Vec::new(),
                    extra_rates: Vec::new(),
                    camilla_devices: None,
                    mdat_scope,
                    originals: None,
                    source_sidecars: None,
                    ticket,
                    stale: false,
                    spl_offset: None,
                    now_ms,
                });
            }

            let stale = {
                let calibration = open.results.calibration(project);
                if calibration.is_none() {
                    return Err(IpcError::new(
                        ErrorCode::EngineInput,
                        "calibration must run before export",
                    ));
                }
                calibration.is_some_and(crate::results::RunView::stale)
            };
            let live = open.results.live_calibration();
            let cached = open.results.cached();
            let mdat = matches!(
                params.kind,
                ExportKind::BeforeAfterMdat | ExportKind::StagedMdat
            );
            let source = if let Some(run) = live {
                DesignSource::Live {
                    design: run.shared_design(),
                    settings: Arc::new(run.settings.clone()),
                }
            } else {
                let cached = cached.ok_or_else(|| {
                    IpcError::new(ErrorCode::Internal, "run cache disappeared")
                })?;
                let chain = Arc::new(cached.calibration_chain.clone());
                if mdat {
                    DesignSource::Replay {
                        snapshot: Arc::new(session::ProjectSnapshot::of(ticket, open)),
                        chain,
                        sample_rate_policy: cached.labels.sample_rate_policy,
                    }
                } else {
                    DesignSource::Cached { chain }
                }
            };
            let description = crate::export_run::describe(&open.results, project, compute_key, now)?;
            let mimo = match open.results.mimo(project, compute_key).exported() {
                Some(MimoView::Live(result)) => Some(MimoSelection {
                    paths: Arc::new(result.run.paths.clone()),
                    applied: mdat.then(|| Arc::new(result.run.applied.clone())),
                    compute: result.summary.compute.clone(),
                }),
                Some(MimoView::Cached(cache)) => Some(MimoSelection {
                    paths: Arc::new(
                        cache
                            .mimo_paths
                            .clone()
                            .expect("exported cached MIMO has paths"),
                    ),
                    applied: None,
                    compute: cache
                        .mimo
                        .as_ref()
                        .expect("exported cached MIMO has summary")
                        .compute
                        .clone(),
                }),
                None => None,
            };
            let includes_mimo = mimo.is_some() && !mdat;
            let export_chain = source
                .chain()
                .expect("a design export has a live or cached chain");
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
            let project_name =
                (!project.name.trim().is_empty()).then(|| project.name.trim().to_owned());
            let display_design_name = requested_design_name
                .clone()
                .unwrap_or_else(|| description.label().to_owned());
            let is_filter_export = matches!(
                params.kind,
                ExportKind::CamillaDsp
                    | ExportKind::FilterText
                    | ExportKind::EqualizerApo
                    | ExportKind::Convolution
            );
            let created_utc = is_filter_export
                .then(|| crate::design_export::created_utc(now_ms).map_err(export_error))
                .transpose()?;
            let design_id = is_filter_export
                .then(|| {
                    secs_graph::design::design_id(
                        export_chain,
                        mimo.as_ref().map(|selection| selection.paths.as_ref()),
                        secs_graph::LfeChoice::DOLBY,
                    )
                    .map_err(export_error)
                })
                .transpose()?;
            let explicit =
                (!params.folder.is_empty()).then(|| Path::new(&params.folder).to_path_buf());
            let explicit_unnamed = explicit.is_some() && requested_design_name.is_none();
            let default_base = if explicit.is_none() {
                Some(self.resolved_export_base_for(open)?)
            } else {
                None
            };
            let base = explicit
                .clone()
                .or(default_base.clone())
                .unwrap_or_default();
            let final_folder = if let Some(name) = requested_design_name.as_deref() {
                crate::design_export::named_folder(&base, params.kind, name)
            } else if let Some(explicit) = explicit {
                explicit
            } else {
                let base = default_base.as_deref().expect("resolved export base");
                match params.kind {
                    ExportKind::CamillaDsp => crate::export_folder::camilladsp_folder(
                        base,
                        mimo.is_some(),
                        description.mode(),
                        now,
                    ),
                    ExportKind::BeforeAfterMdat | ExportKind::StagedMdat => {
                        crate::export_folder::mdat_folder(
                            base,
                            mimo.is_some(),
                            description.mode(),
                            now,
                        )
                    }
                    ExportKind::FilterText => crate::export_folder::text_folder(
                        base,
                        includes_mimo,
                        description.mode(),
                        now,
                    ),
                    ExportKind::EqualizerApo => crate::export_folder::eqapo_folder(
                        base,
                        includes_mimo,
                        description.mode(),
                        now,
                    ),
                    ExportKind::Convolution => crate::export_folder::convolution_folder(
                        base,
                        includes_mimo,
                        description.mode(),
                        now,
                    ),
                }
            };
            let staging = if requested_design_name.is_some() {
                crate::design_export::staging_folder(&final_folder)
            } else {
                final_folder.clone()
            };
            let index_root = if is_filter_export && purpose == ExportPurpose::Export {
                if requested_design_name.is_some() {
                    Some(base)
                } else if let Some(base) = default_base {
                    Some(base)
                } else {
                    final_folder.parent().map(Path::to_path_buf)
                }
            } else {
                None
            };
            let snapshot = mdat.then(|| session::ProjectSnapshot::of(ticket, open));
            Ok(ExportPlan {
                kind: params.kind,
                purpose,
                source,
                mimo,
                description,
                destination: Destination {
                    staging,
                    final_folder,
                    index_root,
                    explicit_unnamed,
                },
                manifest: ManifestMeta {
                    project_name,
                    app_version: self.deps.app_version.to_string(),
                    created_utc,
                    display_design_name,
                    output_rate_auto,
                    design_id,
                },
                record: (purpose == ExportPurpose::Export).then_some(LastExportRecord {
                    mode: project_mode,
                    includes_mimo: open.results.mimo(project, compute_key).exported().is_some(),
                    design_name: requested_design_name,
                }),
                output_rate,
                output_rate_notes: output_rate_notes.clone(),
                extra_rates: params.extra_rates.clone(),
                camilla_devices: params.camilla_devices.clone(),
                mdat_scope,
                originals: (params.kind == ExportKind::BeforeAfterMdat
                    && mdat_scope == secs_export::MdatScope::All)
                    .then(|| session::ProjectSnapshot::of(ticket, open)),
                source_sidecars: snapshot,
                ticket,
                stale,
                spl_offset: secs_project::project_spl_offset(project),
                now_ms,
            })
        })
    }
    pub(crate) fn start_export(
        &self,
        params: ExportParams,
        events: Arc<dyn EventSink>,
    ) -> Result<JobStarted, IpcError> {
        self.start_export_with(params, events, ExportPurpose::Export, |result| {
            Ok(JobResult::Export(result.into_ipc()))
        })
    }

    pub(crate) fn start_export_with<F>(
        &self,
        params: ExportParams,
        events: Arc<dyn EventSink>,
        purpose: ExportPurpose,
        complete: F,
    ) -> Result<JobStarted, IpcError>
    where
        F: FnOnce(ExportResult) -> Result<JobResult, IpcError> + Send + 'static,
    {
        let plan = self.plan_export(params, purpose)?;
        self.start_export_plan(plan, events, complete)
    }

    fn start_export_plan<F>(
        &self,
        plan: ExportPlan,
        events: Arc<dyn EventSink>,
        complete: F,
    ) -> Result<JobStarted, IpcError>
    where
        F: FnOnce(ExportResult) -> Result<JobResult, IpcError> + Send + 'static,
    {
        let job_kind = plan.purpose.job_kind();
        let uuid = self.deps.uuid.clone();
        let session_for_design = self.session.clone();
        let completion_clock = self.deps.clock.clone();
        let completion_offset = self.deps.clock_offset_seconds.clone();
        let log = self.deps.log.clone();
        self.start_job(job_kind, events, move |job| {
            let ExportPlan {
                kind,
                purpose: _,
                source,
                mimo: selection,
                description,
                destination,
                manifest,
                record,
                output_rate,
                output_rate_notes,
                extra_rates,
                camilla_devices,
                mdat_scope,
                originals,
                source_sidecars,
                ticket,
                stale,
                spl_offset,
                now_ms,
            } = plan;
            job.cancel()
                .check()
                .map_err(|_| IpcError::new(ErrorCode::Cancelled, "cancelled"))?;
            let ctx = super::calibration::compute_context(&job, false);

            if let DesignSource::Measurements { snapshot } = source {
                let originals = snapshot.load_originals(&job, ctx.budget())?;
                if originals.values.is_empty() {
                    return Err(IpcError::new(
                        ErrorCode::EngineInput,
                        "there are no measurements to export",
                    )
                    .with_param("reason", "no_measurements"));
                }
                let folder = destination.staging.as_path();
                let writer = BeforeAfterMdatWriter::prepare(
                    None,
                    mdat_scope,
                    &originals.values,
                    folder,
                    None,
                    None,
                    None,
                    now_ms,
                    &uuid,
                )?;
                job.begin_commit()?;
                let mut paths = vec![writer.write()?];
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
                    main_config: None,
                    design_id: None,
                };
                if let Some(record) = record {
                    lock(&session_for_design).record_export(secs_types::local_api::LastExport {
                        kind,
                        folder: result.folder.clone(),
                        mimo: record.includes_mimo,
                        mode: record.mode,
                        design_name: record.design_name,
                        design_id: None,
                        time: completion_clock()
                            .shift_seconds(completion_offset().saturating_neg())
                            .rfc3339_utc(),
                    });
                }
                return complete(result);
            }

            let mdat = matches!(kind, ExportKind::BeforeAfterMdat | ExportKind::StagedMdat);
            let replay = matches!(source, DesignSource::Replay { .. });
            let (
                mut live_values,
                live_camilla,
                cached_camilla,
                replay_snapshot,
                cached_sample_rate_policy,
                export_chain,
            ) = match source {
                DesignSource::Live { design, settings } => {
                    let chain = Arc::new(design.signal_chain().clone());
                    (
                        mdat.then(|| (design.clone(), (*settings).clone())),
                        (kind == ExportKind::CamillaDsp).then_some(design),
                        None,
                        None,
                        None,
                        chain,
                    )
                }
                DesignSource::Cached { chain } => {
                    let camilla = (kind == ExportKind::CamillaDsp).then(|| (*chain).clone());
                    (None, None, camilla, None, None, chain)
                }
                DesignSource::Replay {
                    snapshot,
                    chain,
                    sample_rate_policy,
                } => (
                    None,
                    None,
                    None,
                    Some(snapshot),
                    Some(sample_rate_policy),
                    chain,
                ),
                DesignSource::Measurements { .. } => unreachable!(),
            };
            let sources = source_sidecars
                .as_ref()
                .map(|snapshot| {
                    secs_project::load_project_sources(&snapshot.project, &snapshot.path)
                })
                .unwrap_or_default();
            for warning in &sources.warnings {
                job.log(Notice {
                    code: warning.code.to_owned(),
                    params: [("entry".into(), warning.entry.clone())].into(),
                });
            }

            let (mimo_paths, mimo_applied, compute_report) =
                selection.map_or((None, None, None), |selection| {
                    (
                        Some(selection.paths),
                        selection.applied,
                        Some(selection.compute),
                    )
                });
            let mut mimo = if mdat {
                match (mimo_paths.as_ref(), mimo_applied) {
                    (Some(paths), Some(applied)) => Some(
                        description
                            .mimo_responses((**paths).clone(), Arc::unwrap_or_clone(applied)),
                    ),
                    _ => None,
                }
            } else {
                None
            };
            let cached_mimo_paths = replay
                .then(|| mimo_paths.as_ref().map(|paths| (**paths).clone()))
                .flatten();
            let mut export_mimo = (!mdat)
                .then(|| mimo_paths.map(Arc::unwrap_or_clone))
                .flatten();
            let includes_mimo = export_mimo.is_some();
            let folder_includes_mimo =
                mimo.is_some() || includes_mimo || cached_mimo_paths.is_some();
            let filter_values = matches!(
                kind,
                ExportKind::FilterText | ExportKind::EqualizerApo | ExportKind::Convolution
            )
            .then(|| ((*export_chain).clone(), export_mimo.take()));
            let manifest_mimo = if matches!(
                kind,
                ExportKind::CamillaDsp
                    | ExportKind::FilterText
                    | ExportKind::EqualizerApo
                    | ExportKind::Convolution
            ) {
                filter_values
                    .as_ref()
                    .and_then(|(_, mimo)| mimo.clone())
                    .or_else(|| export_mimo.clone())
            } else {
                None
            };
            let requested_design_name = record
                .as_ref()
                .and_then(|record| record.design_name.clone());
            let folder = destination.staging.as_path();
            let final_destination = destination.final_folder;
            let registry_base = destination.index_root;
            let explicit_unnamed = destination.explicit_unnamed;
            let mut main_config = None;
            let mut paths = match kind {
                ExportKind::CamillaDsp => {
                    let chain = if let Some(design) = &live_camilla {
                        design.signal_chain()
                    } else {
                        cached_camilla.as_ref().ok_or_else(|| {
                            IpcError::new(ErrorCode::Internal, "run cache disappeared")
                        })?
                    };
                    let mut writer = CamillaWriter {
                        chain,
                        folder,
                        devices: camilla_devices.as_ref(),
                        output_rate,
                        project_name: manifest.project_name.clone(),
                        design_name: manifest.display_design_name.clone(),
                        design_id: manifest.design_id.clone().expect("filter export design id"),
                        labels: description.camilla(),
                        mimo: export_mimo.as_ref(),
                        prepared: None,
                        main_config: std::path::PathBuf::new(),
                    };
                    writer.prepare(&job)?;
                    job.begin_commit()?;
                    let (files, config) = writer.write(&log)?;
                    main_config = Some(config);
                    files
                }
                ExportKind::FilterText | ExportKind::EqualizerApo | ExportKind::Convolution => {
                    let values = filter_values.as_ref().ok_or_else(|| {
                        IpcError::new(ErrorCode::Internal, "filter export values disappeared")
                    })?;
                    let run = description.filters(&values.0, values.1.as_ref());
                    let options = secs_filters::Options {
                        output_rate,
                        extra_rates,
                    };
                    match kind {
                        ExportKind::FilterText => {
                            let writer = FilterTextWriter::prepare(&run, folder, &options, &job)?;
                            job.begin_commit()?;
                            writer.write(&log)?
                        }
                        ExportKind::EqualizerApo => {
                            let writer = EqapoWriter::prepare(&run, folder, &options, &job)?;
                            job.begin_commit()?;
                            writer.write(&log)?
                        }
                        ExportKind::Convolution => {
                            let prepared = secs_filters::prepare_convolution_named_with_options(
                                &run,
                                &options,
                                requested_design_name.as_deref(),
                                &ctx,
                            )
                            .map_err(filter_error)?;
                            job.begin_commit()?;
                            let installed =
                                prepared.install_observed(folder).map_err(filter_error)?;
                            log_leftover_backup(&log, installed.leftover_backup);
                            installed.files
                        }
                        _ => unreachable!(),
                    }
                }
                kind => {
                    let (design, settings) = if let Some(values) = live_values.take() {
                        values
                    } else {
                        job.report_progress("design", 0, 1);
                        let loaded = replay_snapshot
                            .ok_or_else(|| {
                                IpcError::new(ErrorCode::Internal, "replay snapshot disappeared")
                            })?
                            .load_measurements(&job, ctx.budget())?;
                        job.report_progress("read_measurements", 1, 1);
                        job.cancel()
                            .check()
                            .map_err(|_| IpcError::new(ErrorCode::Cancelled, "cancelled"))?;
                        session::commit(
                            &session_for_design,
                            ticket,
                            &job,
                            session::Finality::Streaming,
                            |open| {
                                let project = open.project();
                                let settings = secs_project::calibration_from_project(project);
                                let keys = ProjectKeys::of(project);
                                if !open.results.inputs_unchanged_since(&keys, &settings) {
                                    return Err(IpcError::new(
                                        ErrorCode::StaleInputs,
                                        "project changed while computing",
                                    ));
                                }
                                Ok(((), false))
                            },
                        )?;
                        let mut settings = secs_project::calibration_from_project(&loaded.project);
                        if let Some(policy) = cached_sample_rate_policy {
                            settings.sample_rate_policy = policy;
                        }
                        let groups = super::calibration::project_groups(&loaded.project);
                        let inverted = loaded.project.config.inverted_mains();
                        let view = secs_engine::ProjectView {
                            groups: &groups,
                            project_xo: &loaded.project.xo,
                            target: &loaded.project.target,
                            inverted: &inverted,
                        };
                        let calibration = secs_engine::calibrate(
                            &secs_engine::CalibrationRequest {
                                view: &view,
                                irs: &loaded.values,
                                settings: &settings,
                                subs_only: false,
                            },
                            &ctx,
                        )
                        .map_err(super::calibration::engine_error)?;
                        let design = calibration.design;
                        drop(loaded);
                        job.report_progress("design", 1, 1);
                        if let Some(paths) = cached_mimo_paths.as_ref() {
                            mimo = Some(
                                description.mimo_responses(
                                    paths.clone(),
                                    design
                                        .mimo_responses(paths)
                                        .map_err(super::calibration::engine_error)?,
                                ),
                            );
                        }
                        (Arc::new(design), settings)
                    };
                    let originals = originals
                        .map(|snapshot| snapshot.load_originals(&job, ctx.budget()))
                        .transpose()?;
                    let original_values = originals
                        .as_ref()
                        .map_or(&[][..], |loaded| loaded.values.as_slice());
                    let run = ExportRun {
                        design: &design,
                        settings: &settings,
                    };
                    if kind == ExportKind::StagedMdat {
                        let writer = StagedMdatWriter::prepare(
                            &run,
                            folder,
                            mimo.as_ref(),
                            spl_offset,
                            now_ms,
                            &uuid,
                        )?;
                        job.begin_commit()?;
                        vec![writer.write()?]
                    } else {
                        let writer = BeforeAfterMdatWriter::prepare(
                            Some(&run),
                            mdat_scope,
                            original_values,
                            folder,
                            mimo.as_ref(),
                            Some(&sources.values),
                            spl_offset,
                            now_ms,
                            &uuid,
                        )?;
                        job.begin_commit()?;
                        vec![writer.write()?]
                    }
                }
            };
            if let Some(path) = write_compute_provenance(folder, compute_report.as_ref())? {
                paths.push(path);
            }
            let mut written_design_id = None;
            if manifest.design_id.is_some() {
                let metadata = crate::design_export::DesignMetadata {
                    design_name: manifest.display_design_name.clone(),
                    project_name: manifest.project_name.clone(),
                    app_version: manifest.app_version.clone(),
                    created_utc: manifest
                        .created_utc
                        .clone()
                        .expect("filter export timestamp"),
                    export_kind: kind,
                    mode: description.mode().to_owned(),
                    latency_ms: description.latency_ms(),
                    output_rate,
                    output_rate_auto: manifest.output_rate_auto,
                    chain: (*export_chain).clone(),
                    mimo: manifest_mimo,
                    bypass_config: (kind == ExportKind::CamillaDsp)
                        .then(|| format!("configs/secs_{}_bypass.yml", description.camilla().mode)),
                };
                let (manifest_path, id) = crate::design_export::write_manifest(folder, &metadata)
                    .map_err(export_error)?;
                paths.push(manifest_path);
                written_design_id = Some(id);
            }
            if requested_design_name.is_some() {
                crate::design_export::replace_folder_with_log(folder, &final_destination, &log)
                    .map_err(export_error)?;
                for path in &mut paths {
                    if let Ok(relative) = path.strip_prefix(folder) {
                        *path = final_destination.join(relative);
                    }
                }
                if let Some(path) = &mut main_config
                    && let Ok(relative) = path.strip_prefix(folder)
                {
                    *path = final_destination.join(relative);
                }
            }
            if let Some(base) = &registry_base {
                if explicit_unnamed {
                    crate::design_export::rebuild_index_selected_with_log(
                        base,
                        &final_destination,
                        &log,
                    )
                    .map_err(export_error)?;
                } else {
                    crate::design_export::rebuild_index_with_log(base, &log)
                        .map_err(export_error)?;
                }
            }
            let result = ExportResult {
                files: paths
                    .into_iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect(),
                output_rate,
                notes: output_rate_notes,
                folder: final_destination.to_string_lossy().into_owned(),
                stale,
                main_config,
                design_id: written_design_id.clone(),
            };
            if let Some(record) = record {
                lock(&session_for_design).record_export(secs_types::local_api::LastExport {
                    kind,
                    folder: result.folder.clone(),
                    mimo: record.includes_mimo || folder_includes_mimo,
                    mode: record.mode,
                    design_name: record.design_name,
                    design_id: written_design_id,
                    time: completion_clock()
                        .shift_seconds(completion_offset().saturating_neg())
                        .rfc3339_utc(),
                });
            }
            complete(result)
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
        secs_types::fs::write_atomic_bytes(&path, &bytes).map_err(export_error)?;
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
    fn eqapo_writer_stores_the_same_files_main_config_and_design_id() {
        let root = std::env::temp_dir().join(format!("secs-eqapo-writer-{}", std::process::id()));
        let prepared = root.join("prepared");
        let destination = root.join("destination");
        std::fs::create_dir_all(&prepared).unwrap();
        std::fs::write(prepared.join("config.txt"), b"config").unwrap();
        let writer = EqapoWriter {
            prepared: PreparedFolder {
                prepared,
                destination: destination.clone(),
                relative_files: vec![std::path::PathBuf::from("config.txt")],
                moved: false,
            },
        };
        let log: crate::LogSink = Arc::new(|_, _, _| {});
        let files = writer.write(&log).unwrap();
        assert_eq!(files, [destination.join("config.txt")]);
        assert_eq!(std::fs::read(&files[0]).unwrap(), b"config");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn filter_text_writer_stores_the_same_files_main_config_and_design_id() {
        let root =
            std::env::temp_dir().join(format!("secs-filter-text-writer-{}", std::process::id()));
        let prepared = root.join("prepared");
        let destination = root.join("destination");
        std::fs::create_dir_all(&prepared).unwrap();
        std::fs::write(prepared.join("filters.txt"), b"filters").unwrap();
        let writer = FilterTextWriter {
            prepared: PreparedFolder {
                prepared,
                destination: destination.clone(),
                relative_files: vec![std::path::PathBuf::from("filters.txt")],
                moved: false,
            },
        };
        let log: crate::LogSink = Arc::new(|_, _, _| {});
        let files = writer.write(&log).unwrap();
        assert_eq!(files, [destination.join("filters.txt")]);
        assert_eq!(std::fs::read(&files[0]).unwrap(), b"filters");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn before_after_mdat_writer_stores_the_same_files_main_config_and_design_id() {
        let folder =
            std::env::temp_dir().join(format!("secs-before-after-writer-{}", std::process::id()));
        let destination = folder.join("SECS 원 측정.mdat");
        let writer = BeforeAfterMdatWriter {
            destination: destination.clone(),
            bytes: vec![4, 5, 6],
        };
        assert_eq!(writer.write().unwrap(), destination);
        assert_eq!(std::fs::read(&destination).unwrap(), [4, 5, 6]);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn staged_mdat_writer_stores_the_same_files_main_config_and_design_id() {
        let folder =
            std::env::temp_dir().join(format!("secs-staged-writer-{}", std::process::id()));
        let destination = folder.join("SECS 보정 설계 mixed.mdat");
        let writer = StagedMdatWriter {
            destination: destination.clone(),
            bytes: vec![1, 2, 3],
        };
        assert_eq!(writer.write().unwrap(), destination);
        assert_eq!(std::fs::read(&destination).unwrap(), [1, 2, 3]);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn bundle_backup_removal_failure_is_an_export_warning_td198() {
        let lines = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = lines.clone();
        let log: crate::LogSink = Arc::new(move |level, scope, line| {
            sink.lock()
                .unwrap()
                .push((level, scope.to_owned(), line.to_owned()));
        });
        let backup = std::path::PathBuf::from("leftover-backup");

        log_leftover_backup(&log, Some((backup.clone(), "backup is busy".into())));

        let lines = lines.lock().unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(
            (lines[0].0, lines[0].1.as_str()),
            (crate::LogLevel::Warn, "export")
        );
        assert_eq!(
            lines[0].2,
            format!(
                "backup folder {} could not be removed after install: backup is busy",
                backup.display()
            )
        );
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
