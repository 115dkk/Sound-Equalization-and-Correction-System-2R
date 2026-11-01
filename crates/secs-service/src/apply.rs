use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use secs_types::ipc::*;

use crate::{ApplyAdapter, EventSink, Service, lock, results::MimoView};

const TIMEOUT: Duration = Duration::from_secs(2);
const EQAPO_FOLDER: &str = "SECS2R";
const EQAPO_BACKUP: &str = "config.txt.secs2r-backup";

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
                // CamillaDSP's Reload rebuilds only the filters whose definition
                // changed, and a Conv filter's definition is its absolute file
                // name: in one shared folder a new design kept every FIR of the
                // first one applied. Each design gets its own folder; applying
                // the same design again reuses it.
                let destination = final_for_job.join(design_folder(&design_id));
                fs::create_dir_all(&final_for_job)
                    .map_err(|error| io_error(final_for_job.clone(), error))?;
                crate::design_export::replace_folder(&stage, &destination)
                    .map_err(|error| io_error(destination.clone(), error))?;
                let config = destination.join(relative);
                let outcome = match apply.camilla_apply(&address, &config, &text, TIMEOUT) {
                    Ok(outcome) => outcome,
                    Err(error)
                        if managed_exe.is_some()
                            && error
                                .params
                                .get("reason")
                                .is_some_and(|reason| reason == "connection_refused") =>
                    {
                        ApplyOutcome::HandedOver
                    }
                    Err(error) => return Err(error),
                };
                if matches!(outcome, ApplyOutcome::Applied | ApplyOutcome::HandedOver) {
                    // A rolled-back apply keeps the folder CamillaDSP still reads.
                    prune_applied(&final_for_job, &destination);
                }
                if matches!(outcome, ApplyOutcome::Applied | ApplyOutcome::HandedOver)
                    && let Some(exe_path) = managed_exe
                {
                    expect_host_ok(managed_host.call(&HostRequest::SetManaged(HostManaged {
                        camilla: Some(ManagedCamilla {
                            exe_path,
                            config_path: config.to_string_lossy().into_owned(),
                            port: managed_port,
                            design_id: design_id.clone(),
                        }),
                        eqapo: None,
                        ..HostManaged::default()
                    }))?)?;
                }
                Ok(JobResult::Apply(ApplyResult {
                    outcome,
                    design_id,
                    folder: destination.to_string_lossy().into_owned(),
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
                let generation =
                    crate::deploy::install(&final_for_job, design_folder(&design_id), &stage)?;
                let entry_bytes =
                    format!("Include: {}\\config.txt\r\n", generation.name).into_bytes();
                let config = config_root.join("config.txt");
                let switched = crate::deploy::write_entry(&entry, &entry_bytes)
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
                    let remove = fs::remove_dir_all(&generation.dir)
                        .map_err(|remove| io_error(&generation.dir, remove));
                    if let Err(rollback) = restore.and(remove) {
                        return Err(IpcError::new(
                            ErrorCode::ApplyFailed,
                            format!(
                                "entry switch failed ({}); rollback failed ({})",
                                error.detail, rollback.detail
                            ),
                        ));
                    }
                    return Err(error);
                }
                crate::deploy::prune(&final_for_job, &["SECS.txt", &generation.name])?;
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
        let applied_design_id = fs::read_to_string(root.join(EQAPO_FOLDER).join("SECS.txt"))
            .ok()
            .and_then(|entry| {
                let include = entry
                    .lines()
                    .find_map(|line| line.trim().strip_prefix("Include: "))?;
                let generation = include.split(['\\', '/']).next()?;
                fs::read(
                    root.join(EQAPO_FOLDER)
                        .join(generation)
                        .join("secs-design.json"),
                )
                .ok()
            })
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

/// Removes everything under the CamillaDSP apply root except `keep`: earlier
/// designs and the shared folder of older releases. Best effort: a folder left
/// behind is removed by the next apply.
fn prune_applied(root: &Path, keep: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        if path == keep {
            continue;
        }
        let _ = if path.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
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
