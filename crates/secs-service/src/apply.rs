use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use secs_types::ipc::*;

use crate::{ApplyAdapter, EventSink, Service, lock, results::MimoView};

const TIMEOUT: Duration = Duration::from_secs(2);
const EQAPO_FOLDER: &str = "SECS2R";
const EQAPO_BACKUP: &str = "config.txt.secs2r-backup";
const CAMILLA_CURRENT: &str = "camilladsp-current.txt";
const RETAINED_GENERATIONS: &str = "secs-retained-generations.txt";

impl Service {
    pub(crate) fn apply_status(&self) -> ApplyStatus {
        let current = self.current_design_identity();
        let address = lock(&self.app_settings).apply.camilla.address.clone();
        let camilla = match self.deps.apply.camilla_status(&address, TIMEOUT) {
            Ok(status) => {
                let design = match (&current, &status.title, &status.description) {
                    (Some((design_id, expected_title)), Some(title), Some(description))
                        if title == expected_title
                            && description.contains(&format!("secs-design {design_id}")) =>
                    {
                        DesignMatch::This {
                            design_id: design_id.clone(),
                        }
                    }
                    (_, Some(title), _) => DesignMatch::Other {
                        title: title.clone(),
                    },
                    _ => DesignMatch::Unknown,
                };
                CamillaApplyStatus {
                    reachable: true,
                    error: None,
                    status: Some(status),
                    design,
                }
            }
            Err(error) => CamillaApplyStatus {
                reachable: false,
                error: Some(error),
                status: None,
                design: DesignMatch::Unknown,
            },
        };
        let host = self.deps.managed_host.service_state();
        let host_status = (host == ServiceState::Running)
            .then(|| self.deps.managed_host.call(&HostRequest::Status).ok())
            .flatten()
            .and_then(|response| match response {
                HostResponse::Status(status) => Some(status),
                HostResponse::Ok | HostResponse::Log(_) | HostResponse::Error(_) => None,
            });
        ApplyStatus {
            camilla,
            eqapo: self.eqapo_status(),
            host,
            host_status,
            current_design_id: current.map(|(design_id, _)| design_id),
        }
    }

    pub(crate) fn start_apply_camilla(
        &self,
        params: ApplyCamillaParams,
        events: Arc<dyn EventSink>,
    ) -> Result<JobStarted, IpcError> {
        let settings = lock(&self.app_settings).apply.camilla.clone();
        let managed_exe = if settings.level == ApplyLevel::Manage {
            Some(
                settings
                    .exe_path
                    .clone()
                    .filter(|path| !path.trim().is_empty())
                    .ok_or_else(|| {
                        IpcError::new(
                            ErrorCode::InvalidRequest,
                            "managed CamillaDSP needs an executable path",
                        )
                        .with_param("field", "apply.camilla.exe_path")
                    })?,
            )
        } else {
            None
        };
        check_level(settings.level, "camilladsp")?;
        let managed_port = port_from_address(&settings.address)?;
        let final_folder = self.app_data_path().join("applied/camilladsp");
        let stage = crate::design_export::staging_folder(&final_folder);
        remove_stage(&stage)?;
        let address = settings.address;
        let final_for_job = final_folder.clone();
        let current_marker = final_folder.with_file_name(CAMILLA_CURRENT);
        let apply = self.deps.apply.clone();
        let managed_host = self.deps.managed_host.clone();
        self.start_export_with(
            ExportParams {
                kind: ExportKind::CamillaDsp,
                folder: stage.to_string_lossy().into_owned(),
                mdat_scope: None,
                design_name: None,
                camilla_devices: params.camilla_devices,
                // Applying writes one rate: the profile's output rate.
                extra_rates: Vec::new(),
            },
            events,
            JobKind::Apply,
            false,
            move |result, design_id| {
                let design_id = design_id.ok_or_else(|| {
                    IpcError::new(ErrorCode::Internal, "CamillaDSP export has no design id")
                })?;
                let stage = PathBuf::from(&result.folder);
                let relative = result
                    .files
                    .iter()
                    .map(PathBuf::from)
                    .find_map(|path| {
                        let relative = path.strip_prefix(&stage).ok()?;
                        let name = relative.file_name()?.to_str()?;
                        (path.extension().and_then(|value| value.to_str()) == Some("yml")
                            && !name.ends_with("_bypass.yml"))
                        .then(|| relative.to_owned())
                    })
                    .ok_or_else(|| {
                        IpcError::new(ErrorCode::Internal, "CamillaDSP export has no main config")
                    })?;
                let text = fs::read_to_string(stage.join(&relative))
                    .map_err(|error| io_error(stage.join(&relative), error))?;
                // CamillaDSP identifies a convolution filter by its absolute file
                // name, so every attempt gets another generation even when the
                // design id is unchanged.
                if let Err(error) = apply.before_generation_install(&final_for_job, &stage) {
                    let _ = remove_stage(&stage);
                    return Err(error);
                }
                let generation =
                    crate::deploy::install(&final_for_job, design_folder(&design_id), &stage)?;
                let config = generation.dir.join(relative);
                let marker_before = marker_generation(&final_for_job, &current_marker);
                let (outcome, reported_previous) =
                    match apply.camilla_apply_report(&address, &config, &text, TIMEOUT) {
                        Ok(result) => result,
                        Err(error)
                            if managed_exe.is_some()
                                && error
                                    .params
                                    .get("reason")
                                    .is_some_and(|reason| reason == "connection_refused") =>
                        {
                            (ApplyOutcome::HandedOver, None)
                        }
                        Err(error) => {
                            let mut retained = vec![generation.name.as_str()];
                            if let Some(previous) = marker_before.as_deref() {
                                retained.push(previous);
                            }
                            remember_retained(&final_for_job, &retained);
                            mark_generation_uncertain(&current_marker, &generation.name);
                            return Err(error);
                        }
                    };
                let reported_generation = reported_previous
                    .as_deref()
                    .and_then(|path| applied_generation(&final_for_job, Path::new(path)));
                if matches!(&outcome, ApplyOutcome::RolledBack { .. }) {
                    // Reload may have opened this generation before the old path was
                    // restored. Keep it through the next successful apply.
                    remember_retained(&final_for_job, &[generation.name.as_str()]);
                } else {
                    if let Err(error) =
                        apply
                            .before_camilla_marker_write(&current_marker)
                            .and_then(|()| {
                                crate::deploy::write_entry(
                                    &current_marker,
                                    generation.name.as_bytes(),
                                )
                            })
                    {
                        eprintln!(
                            "could not record active CamillaDSP generation: {}",
                            error.detail
                        );
                    }
                    if let Some(exe_path) = managed_exe
                        && let Err(error) = managed_host
                            .call(&HostRequest::SetManaged(HostManaged {
                                camilla: Some(ManagedCamilla {
                                    exe_path,
                                    config_path: config.to_string_lossy().into_owned(),
                                    port: managed_port,
                                    design_id: design_id.clone(),
                                }),
                                eqapo: None,
                                ..HostManaged::default()
                            }))
                            .and_then(expect_host_ok)
                    {
                        let mut retained = Vec::new();
                        if let Some(previous) = reported_generation.as_deref() {
                            retained.push(previous);
                        }
                        remember_retained(&final_for_job, &retained);
                        return Err(error);
                    }
                    // The marker is diagnostic only. Retirement follows the path
                    // CamillaDSP reported during this apply (audit #155 TD-108).
                    let mut keep = vec![generation.name.as_str()];
                    if let Some(previous) = reported_generation.as_deref() {
                        keep.push(previous);
                    }
                    retire_after_success(&final_for_job, &keep, reported_generation.is_some());
                }
                Ok(JobResult::Apply(ApplyResult {
                    outcome,
                    design_id,
                    folder: generation.dir.to_string_lossy().into_owned(),
                    stale: result.stale,
                }))
            },
        )
    }

    pub(crate) fn start_apply_eqapo(
        &self,
        events: Arc<dyn EventSink>,
    ) -> Result<JobStarted, IpcError> {
        let settings = lock(&self.app_settings).apply.eqapo.clone();
        check_level(settings.level, "equalizer_apo")?;
        let install = (self.deps.eqapo_install)().ok_or_else(|| {
            IpcError::new(ErrorCode::Unsupported, "Equalizer APO is not installed")
                .with_param("target", "equalizer_apo")
        })?;
        let config_root = PathBuf::from(&install.config_path);
        let final_folder = config_root.join(EQAPO_FOLDER);
        let stage = crate::design_export::staging_folder(&final_folder);
        remove_stage(&stage)?;
        let final_for_job = final_folder.clone();
        let apply = self.deps.apply.clone();
        let managed = settings.level == ApplyLevel::Manage;
        let managed_host = self.deps.managed_host.clone();
        self.start_export_with(
            ExportParams {
                kind: ExportKind::EqualizerApo,
                folder: stage.to_string_lossy().into_owned(),
                mdat_scope: None,
                design_name: None,
                camilla_devices: None,
                // Applying writes one rate: the profile's output rate.
                extra_rates: Vec::new(),
            },
            events,
            JobKind::Apply,
            false,
            move |result, design_id| {
                let design_id = design_id.ok_or_else(|| {
                    IpcError::new(ErrorCode::Internal, "Equalizer APO export has no design id")
                })?;
                let stage = PathBuf::from(&result.folder);
                let entry = final_for_job.join("SECS.txt");
                let previous_entry = match fs::read(&entry) {
                    Ok(bytes) => Some(bytes),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => return Err(io_error(&entry, error)),
                };
                let previous_generation = previous_entry.as_deref().and_then(entry_generation);
                if let Err(error) = apply.before_generation_install(&final_for_job, &stage) {
                    let _ = remove_stage(&stage);
                    return Err(error);
                }
                let generation =
                    crate::deploy::install(&final_for_job, design_folder(&design_id), &stage)?;
                let entry_text = secs_filters::generation_entry(&generation.name);
                let config = config_root.join("config.txt");
                let switched = crate::deploy::write_entry(&entry, entry_text.as_bytes())
                    .and_then(|()| ensure_eqapo_include(&config, apply.as_ref()));
                if let Err(error) = switched {
                    let restore = match previous_entry {
                        Some(bytes) => crate::deploy::write_entry(&entry, &bytes),
                        None => match fs::remove_file(&entry) {
                            Ok(()) => Ok(()),
                            Err(remove) if remove.kind() == std::io::ErrorKind::NotFound => Ok(()),
                            Err(remove) => Err(io_error(&entry, remove)),
                        },
                    };
                    let mut retained = vec![generation.name.as_str()];
                    if let Some(previous) = previous_generation.as_deref() {
                        retained.push(previous);
                    }
                    remember_retained(&final_for_job, &retained);
                    if let Err(rollback) = restore {
                        return Err(IpcError::new(
                            ErrorCode::ApplyFailed,
                            format!(
                                "entry switch failed ({}); rollback failed ({})",
                                error.detail, rollback.detail
                            ),
                        ));
                    }
                    // SECS.txt may already have pointed at this generation. A reader that
                    // opened it before the rollback can still resolve the nested include
                    // (audit #155 TD-105).
                    return Err(error);
                }
                let mut keep = vec!["SECS.txt", generation.name.as_str()];
                if let Some(previous) = previous_generation.as_deref() {
                    keep.push(previous);
                }
                retire_after_success(&final_for_job, &keep, previous_generation.is_some());
                if managed {
                    expect_host_ok(managed_host.call(&HostRequest::SetManaged(HostManaged {
                        camilla: None,
                        eqapo: Some(ManagedEqApo {
                            config_path: config_root.to_string_lossy().into_owned(),
                            design_id: design_id.clone(),
                        }),
                        ..HostManaged::default()
                    }))?)?;
                }
                Ok(JobResult::Apply(ApplyResult {
                    outcome: ApplyOutcome::Applied,
                    design_id,
                    folder: generation.dir.to_string_lossy().into_owned(),
                    stale: result.stale,
                }))
            },
        )
    }

    pub(crate) fn revert_eqapo(&self) -> Result<(), IpcError> {
        let install = (self.deps.eqapo_install)().ok_or_else(|| {
            IpcError::new(ErrorCode::Unsupported, "Equalizer APO is not installed")
                .with_param("target", "equalizer_apo")
        })?;
        let path = PathBuf::from(install.config_path).join("config.txt");
        let text = fs::read_to_string(&path).map_err(|error| io_error(path.clone(), error))?;
        if let Some(updated) = self.deps.apply.eqapo_without_include(&text)? {
            write_changed(&path, updated.as_bytes())?;
        }
        Ok(())
    }

    pub(crate) fn apply_settings_set(
        &self,
        settings: ApplySettings,
    ) -> Result<ApplySettings, IpcError> {
        validate_address(&settings.camilla.address)?;
        let mut app = lock(&self.app_settings).clone();
        app.apply = settings.clone();
        self.app_settings_set(AppSettingsParams { settings: app })?;
        Ok(settings)
    }

    pub(crate) fn host_install(&self) -> Result<(), IpcError> {
        self.deps.managed_host.install()
    }

    pub(crate) fn host_uninstall(&self) -> Result<(), IpcError> {
        self.deps.managed_host.uninstall()
    }

    pub(crate) fn host_stop_managing(&self) -> Result<(), IpcError> {
        expect_host_ok(self.deps.managed_host.call(&HostRequest::StopManaging)?)
    }

    fn app_data_path(&self) -> PathBuf {
        self.deps.app_data_path.clone()
    }

    fn eqapo_status(&self) -> EqApoApplyStatus {
        let Some(install) = (self.deps.eqapo_install)() else {
            return EqApoApplyStatus {
                installed: false,
                variant: EqApoVariant::Unknown,
                config_path: None,
                include: IncludeState::Absent,
                applied_design_id: None,
            };
        };
        let root = PathBuf::from(&install.config_path);
        let include = fs::read_to_string(root.join("config.txt"))
            .ok()
            .map_or(IncludeState::Absent, |text| {
                self.deps.apply.eqapo_include_state(&text)
            });
        let bundle = root.join(EQAPO_FOLDER);
        let applied_manifest = fs::read_to_string(bundle.join("SECS.txt"))
            .ok()
            .and_then(|entry| {
                let generation = entry_generation(entry.as_bytes())?;
                fs::read(bundle.join(generation).join("secs-design.json")).ok()
            })
            .or_else(|| fs::read(bundle.join("secs-design.json")).ok());
        let applied_design_id = applied_manifest
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| value.get("design_id")?.as_str().map(str::to_owned));
        EqApoApplyStatus {
            installed: true,
            variant: install.variant,
            config_path: Some(install.config_path),
            include,
            applied_design_id,
        }
    }

    fn current_design_identity(&self) -> Option<(String, String)> {
        let compute = crate::compute::ComputeIdentity::from(&self.acceleration().status);
        let session = lock(&self.session);
        let open = session.require().ok()?;
        let project = open.store.project();
        let chain = open
            .results
            .live_calibration()
            .map(|run| &run.design().chain)
            .or_else(|| open.results.cached().map(|cache| &cache.calibration_chain))?;
        let mimo = match open.results.mimo(project, &compute).exported() {
            Some(MimoView::Live(result)) => Some(&result.run.paths),
            Some(MimoView::Cached(cache)) => cache.mimo_paths.as_ref(),
            None => None,
        };
        let design_id =
            secs_graph::design::design_id(chain, mimo, secs_graph::LfeChoice::DOLBY).ok()?;
        let description =
            crate::export_run::describe(&open.results, project, &compute, (self.deps.clock)())
                .ok()?;
        let project_name = (!project.name.trim().is_empty()).then(|| project.name.trim());
        let title = project_name.map_or_else(
            || format!("SECS 2R · {}", description.label()),
            |name| format!("SECS 2R · {name} · {}", description.label()),
        );
        Some((design_id, title))
    }
}

fn check_level(level: ApplyLevel, target: &str) -> Result<(), IpcError> {
    if level == ApplyLevel::Files {
        return Err(IpcError::new(
            ErrorCode::Unsupported,
            "the target is configured for file export only",
        )
        .with_param("target", target));
    }
    if level == ApplyLevel::Manage && !cfg!(target_os = "windows") {
        return Err(IpcError::new(
            ErrorCode::Unsupported,
            "managed application is available only on Windows",
        )
        .with_param("target", target));
    }
    Ok(())
}

fn port_from_address(address: &str) -> Result<u16, IpcError> {
    validate_address(address)?;
    address
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse().ok())
        .ok_or_else(|| {
            IpcError::new(
                ErrorCode::InvalidRequest,
                "CamillaDSP address must end in a port",
            )
            .with_param("field", "camilla.address")
        })
}

fn expect_host_ok(response: HostResponse) -> Result<(), IpcError> {
    match response {
        HostResponse::Ok => Ok(()),
        HostResponse::Error(error) => Err(error),
        HostResponse::Status(_) | HostResponse::Log(_) => Err(IpcError::new(
            ErrorCode::ApplyFailed,
            "managed host returned the wrong response",
        )),
    }
}

fn validate_address(address: &str) -> Result<(), IpcError> {
    let valid = if let Some(rest) = address.strip_prefix('[') {
        rest.split_once("]:").is_some_and(|(host, port)| {
            !host.is_empty() && port.parse::<u16>().is_ok_and(|port| port != 0)
        })
    } else {
        address.rsplit_once(':').is_some_and(|(host, port)| {
            !host.is_empty()
                && !host.contains(':')
                && !host.chars().any(char::is_whitespace)
                && port.parse::<u16>().is_ok_and(|port| port != 0)
        })
    };
    if valid {
        Ok(())
    } else {
        Err(IpcError::new(
            ErrorCode::InvalidRequest,
            "CamillaDSP address must be host:port",
        )
        .with_param("field", "camilla.address"))
    }
}

fn io_error(path: impl AsRef<Path>, error: impl std::fmt::Display) -> IpcError {
    let detail = error.to_string();
    let denied = detail.to_ascii_lowercase().contains("permission denied")
        || detail.to_ascii_lowercase().contains("access is denied");
    IpcError::new(
        if denied {
            ErrorCode::PermissionDenied
        } else {
            ErrorCode::ApplyFailed
        },
        detail,
    )
    .with_param("path", path.as_ref().to_string_lossy())
}

/// The applied folder of one design: the start of its id, short enough for
/// Windows paths and unique among the designs a user applies.
fn design_folder(design_id: &str) -> &str {
    design_id.get(..16).unwrap_or(design_id)
}

fn entry_generation(bytes: &[u8]) -> Option<String> {
    let entry = std::str::from_utf8(bytes).ok()?;
    let include = entry
        .lines()
        .find_map(|line| line.trim().strip_prefix("Include: "))?;
    include.split(['\\', '/']).next().map(str::to_owned)
}

fn applied_generation(root: &Path, config: &Path) -> Option<String> {
    let relative = config.strip_prefix(root).ok()?;
    let mut components = relative.components();
    let std::path::Component::Normal(name) = components.next()? else {
        return None;
    };
    if !matches!(components.next(), Some(std::path::Component::Normal(_)))
        || !components.all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        return None;
    }
    let generation = root.join(name);
    let metadata = fs::symlink_metadata(&generation).ok()?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return None;
    }
    let manifest = fs::symlink_metadata(generation.join("secs-design.json")).ok()?;
    if manifest.file_type().is_symlink() || !manifest.is_file() {
        return None;
    }
    name.to_str().map(str::to_owned)
}

fn mark_generation_uncertain(path: &Path, generation: &str) {
    let value = format!("?{generation}");
    if let Err(error) = crate::deploy::write_entry(path, value.as_bytes()) {
        eprintln!(
            "could not record uncertain CamillaDSP generation: {}",
            error.detail
        );
    }
}

fn marker_generation(root: &Path, marker: &Path) -> Option<String> {
    let value = fs::read_to_string(marker).ok()?;
    let name = value.trim().strip_prefix('?').unwrap_or(value.trim());
    if !valid_generation_name(name) {
        return None;
    }
    let folder = root.join(name);
    let metadata = fs::symlink_metadata(&folder).ok()?;
    let manifest = fs::symlink_metadata(folder.join("secs-design.json")).ok()?;
    (!metadata.file_type().is_symlink()
        && metadata.is_dir()
        && !manifest.file_type().is_symlink()
        && manifest.is_file())
    .then(|| name.to_owned())
}

fn valid_generation_name(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(components.next(), Some(std::path::Component::Normal(_)))
        && components.next().is_none()
}

fn read_retained(root: &Path) -> Result<Vec<String>, IpcError> {
    let path = root.join(RETAINED_GENERATIONS);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_error(&path, error)),
    };
    let text = std::str::from_utf8(&bytes).map_err(|error| io_error(&path, error))?;
    let mut names = Vec::new();
    for name in text.lines() {
        if name.is_empty() || !valid_generation_name(name) {
            return Err(IpcError::new(
                ErrorCode::ApplyFailed,
                "retained generation record contains an invalid folder name",
            )
            .with_param("path", path.to_string_lossy()));
        }
        names.push(name.to_owned());
    }
    names.sort();
    names.dedup();
    Ok(names)
}

fn remember_retained(root: &Path, names: &[&str]) {
    if names.is_empty() {
        return;
    }
    let mut retained = match read_retained(root) {
        Ok(retained) => retained,
        Err(error) => {
            eprintln!(
                "could not read retained applied generations; keeping the existing record: {}",
                error.detail
            );
            return;
        }
    };
    retained.extend(
        names
            .iter()
            .filter(|name| valid_generation_name(name))
            .map(|name| (*name).to_owned()),
    );
    retained.sort();
    retained.dedup();
    let mut bytes = retained.join("\n").into_bytes();
    bytes.push(b'\n');
    let path = root.join(RETAINED_GENERATIONS);
    if let Err(error) = crate::deploy::write_entry(&path, &bytes) {
        eprintln!(
            "could not record retained applied generations: {}",
            error.detail
        );
    }
}

fn retire_after_success(root: &Path, keep: &[&str], has_previous: bool) {
    let retained = match read_retained(root) {
        Ok(retained) => retained,
        Err(error) => {
            eprintln!(
                "could not read retained applied generations; skipping retirement: {}",
                error.detail
            );
            return;
        }
    };
    if !has_previous && retained.is_empty() {
        return;
    }
    let mut keep = keep
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    keep.extend(retained);
    keep.push(RETAINED_GENERATIONS.to_owned());
    keep.sort();
    keep.dedup();
    let keep_refs = keep.iter().map(String::as_str).collect::<Vec<_>>();
    if let Err(error) = crate::deploy::prune(root, &keep_refs) {
        eprintln!("could not retire old applied generations: {}", error.detail);
        return;
    }
    let path = root.join(RETAINED_GENERATIONS);
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => eprintln!(
            "could not clear retained applied generations: {}",
            io_error(&path, error).detail
        ),
    }
}

fn remove_stage(path: &Path) -> Result<(), IpcError> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(path, error)),
    }
}

fn ensure_eqapo_include(config: &Path, apply: &dyn ApplyAdapter) -> Result<(), IpcError> {
    let text = fs::read_to_string(config).map_err(|error| io_error(config, error))?;
    let Some(updated) = apply.eqapo_with_include(&text)? else {
        return Ok(());
    };
    let backup = config.with_file_name(EQAPO_BACKUP);
    if !backup.exists() {
        write_changed(&backup, text.as_bytes())?;
    }
    write_changed(config, updated.as_bytes())
}

fn write_changed(path: &Path, bytes: &[u8]) -> Result<(), IpcError> {
    if fs::read(path).ok().as_deref() == Some(bytes) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error(path, error))?;
    }
    secs_types::fs::write_atomic_bytes(path, bytes).map_err(|error| io_error(path, error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_require_a_nonzero_port() {
        for value in ["127.0.0.1:1234", "localhost:1", "[::1]:1234"] {
            assert!(validate_address(value).is_ok(), "{value}");
        }
        for value in ["localhost", ":1234", "host:0", "host:no", "a:b:1"] {
            assert!(validate_address(value).is_err(), "{value}");
        }
    }

    #[test]
    fn permission_mapping_carries_the_path() {
        let error = io_error(
            Path::new("config.txt"),
            std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        );
        assert_eq!(error.code, ErrorCode::PermissionDenied);
        assert_eq!(
            error.params.get("path").map(String::as_str),
            Some("config.txt")
        );
    }
}
