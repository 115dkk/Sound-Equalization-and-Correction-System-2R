#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! `secs2r-host`: the Windows service for ADR 0018's managed apply level.

#[cfg(any(target_os = "windows", test))]
// The service runs only on Windows; elsewhere main is a stub and only the
// rules' unit tests use this module.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
mod rules;
#[cfg(any(target_os = "windows", test))]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
mod transitions;

#[cfg(target_os = "windows")]
use std::path::{Path, PathBuf};
use std::process::ExitCode;
#[cfg(target_os = "windows")]
use std::sync::mpsc;
#[cfg(target_os = "windows")]
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
#[cfg(target_os = "windows")]
use rules::Core;
#[cfg(target_os = "windows")]
use secs_types::ipc::{
    ErrorCode, HostManaged, HostRequest, HostResponse, IncludeState, IpcError, ManagedProcessState,
};
#[cfg(target_os = "windows")]
use transitions::{
    ConsoleEvent as TransitionConsoleEvent, Input as TransitionInput,
    Platform as TransitionPlatform, SessionEvent as TransitionSessionEvent,
    State as TransitionState,
};

#[derive(Debug, Parser)]
#[command(name = "secs2r-host", version, about = "SECS 2R managed host service")]
struct Cli {
    #[command(subcommand)]
    action: Action,
}

#[derive(Debug, Clone, Subcommand, PartialEq, Eq)]
enum Action {
    /// Register the automatic LocalSystem Windows service.
    Install,
    /// Stop and delete the Windows service.
    Uninstall,
    /// Enter the Windows Service Control Manager dispatcher.
    Run,
    /// Print the current host status as one JSON value.
    Status,
    /// Repair Equalizer APO as the interactive user. Service-internal.
    #[command(hide = true)]
    RepairEqapo { config_folder: std::path::PathBuf },
    /// Finish replacing the stopped service executable. Service-internal.
    #[command(hide = true)]
    FinishUpdate,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone)]
struct Paths {
    state: PathBuf,
    log: PathBuf,
    pipe: String,
    program_data_root: Option<PathBuf>,
}

#[cfg(target_os = "windows")]
impl Paths {
    fn from_env() -> Self {
        let base = program_data_root();
        let state_override = std::env::var_os("SECS2R_HOST_STATE").map(PathBuf::from);
        let log_override = std::env::var_os("SECS2R_HOST_LOG").map(PathBuf::from);
        let fully_overridden = state_override.is_some() && log_override.is_some();
        Self {
            state: state_override.unwrap_or_else(|| base.join("host.json")),
            log: log_override.unwrap_or_else(|| base.join("host.log")),
            pipe: std::env::var("SECS2R_HOST_PIPE")
                .unwrap_or_else(|_| secs_sys::host::DEFAULT_PIPE_NAME.to_owned()),
            program_data_root: (!fully_overridden).then_some(base),
        }
    }
}

#[cfg(target_os = "windows")]
fn program_data_root() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("SECS2R")
}

#[cfg(target_os = "windows")]
fn program_files_root() -> Result<PathBuf, IpcError> {
    std::env::var_os("ProgramW6432")
        .or_else(|| std::env::var_os("ProgramFiles"))
        .map(PathBuf::from)
        .ok_or_else(|| {
            IpcError::new(
                ErrorCode::ApplyFailed,
                "ProgramW6432 and ProgramFiles are unavailable",
            )
        })
}

#[cfg(not(target_os = "windows"))]
fn main() -> ExitCode {
    let _ = Cli::parse();
    eprintln!("secs2r-host is available only on Windows");
    ExitCode::from(2)
}

#[cfg(target_os = "windows")]
fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("secs2r-host: {}", error.detail);
            ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "windows")]
fn run(cli: Cli) -> Result<(), IpcError> {
    match cli.action {
        Action::Install => {
            prepare_state_directory(&program_data_root())?;
            let source = std::env::current_exe().map_err(|error| {
                IpcError::new(
                    ErrorCode::ApplyFailed,
                    format!("find current executable: {error}"),
                )
            })?;
            let paths = rules::host_install_paths(&program_files_root()?);
            prepare_service_directory(&paths.directory)?;
            let existed = secs_sys::host::stop_service_for_update()?;
            let action = rules::install_action(existed);
            rules::replace_host_executable(&source, &paths).map_err(|error| {
                IpcError::new(
                    ErrorCode::ApplyFailed,
                    format!("install {}: {error}", paths.executable.display()),
                )
            })?;
            secs_sys::host::register_service(
                &paths.executable,
                action == rules::InstallAction::ReplaceAndUpdate,
            )
        }
        Action::Uninstall => {
            let paths = rules::host_install_paths(&program_files_root()?);
            validate_service_directory_if_present(&paths.directory)?;
            secs_sys::host::unregister_service()?;
            // A scanner or the exiting service can hold the file briefly.
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                match rules::remove_host_copy(&paths) {
                    Ok(()) => return Ok(()),
                    Err(error)
                        if error.kind() == std::io::ErrorKind::PermissionDenied
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(250));
                    }
                    Err(error) => {
                        return Err(IpcError::new(
                            ErrorCode::ApplyFailed,
                            format!("remove {}: {error}", paths.executable.display()),
                        ));
                    }
                }
            }
        }
        Action::Run => {
            let paths = Paths::from_env();
            if let Some(root) = &paths.program_data_root {
                prepare_state_directory(root)?;
            }
            secs_sys::host::run_service(move |events| {
                let result = service_loop(&paths, &events);
                if let Err(error) = &result {
                    eprintln!("secs2r-host service: {}", error.detail);
                    let line = rules::log_line(
                        "service_error",
                        [("detail".to_owned(), error.detail.clone())],
                    );
                    append_log_nonfatal(&paths.log, &line);
                }
                result
            })
        }
        Action::Status => {
            let paths = Paths::from_env();
            let response = secs_sys::host::pipe_call_at(&paths.pipe, &HostRequest::Status)?;
            match response {
                HostResponse::Status(status) => println_json(&status),
                HostResponse::Error(error) => return Err(error),
                other => {
                    return Err(IpcError::new(
                        ErrorCode::ApplyFailed,
                        format!("unexpected host status response: {other:?}"),
                    ));
                }
            }
            Ok(())
        }
        Action::RepairEqapo { config_folder } => repair_eqapo_as_user(&config_folder),
        Action::FinishUpdate => finish_update(),
    }
}

#[cfg(target_os = "windows")]
fn finish_update() -> Result<(), IpcError> {
    let current = std::env::current_exe().map_err(|error| {
        IpcError::new(
            ErrorCode::ApplyFailed,
            format!("find update helper executable: {error}"),
        )
    })?;
    let directory = current.parent().ok_or_else(|| {
        IpcError::new(
            ErrorCode::ApplyFailed,
            "update helper executable has no parent directory",
        )
    })?;
    let update_paths = rules::host_update_paths(directory);
    let paths = Paths::from_env();
    // The service starts the verified copy as this helper, so its own version
    // is the one being installed.
    let version = || ("version".to_owned(), env!("CARGO_PKG_VERSION").to_owned());
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if secs_sys::host::service_state() == secs_types::ipc::ServiceState::Stopped {
            break;
        }
        if Instant::now() >= deadline {
            let line = rules::log_line("update_timeout", [version()]);
            append_log_nonfatal(&paths.log, &line);
            if secs_sys::host::service_state() == secs_types::ipc::ServiceState::Stopped {
                secs_sys::host::start_service()?;
            }
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let replace_result = rules::replace_staged_update(&mut rules::RealUpdateFileOps, &update_paths)
        .map_err(|error| {
            IpcError::new(
                ErrorCode::ApplyFailed,
                format!("finish host update: {error}"),
            )
        });
    let result = match replace_result {
        Ok(()) => secs_sys::host::start_service(),
        Err(error) => {
            let _ = secs_sys::host::start_service();
            Err(error)
        }
    };
    match &result {
        Ok(()) => {
            let line = rules::log_line("update_done", [version()]);
            append_log_nonfatal(&paths.log, &line);
        }
        Err(error) => {
            let line = rules::log_line(
                "update_failed",
                [version(), ("reason".to_owned(), error.detail.clone())],
            );
            append_log_nonfatal(&paths.log, &line);
        }
    }
    result
}

#[cfg(target_os = "windows")]
fn prepare_state_directory(path: &Path) -> Result<(), IpcError> {
    match std::fs::create_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(IpcError::new(
                ErrorCode::PermissionDenied,
                format!("create {}: {error}", path.display()),
            ));
        }
    }
    let facts = secs_sys::host::inspect_state_directory(path)?;
    if !rules::state_directory_allowed(
        facts.is_directory,
        facts.is_reparse_point,
        facts.owner_is_system_or_administrators,
    ) {
        return Err(IpcError::new(
            ErrorCode::PermissionDenied,
            format!(
                "{} must be a real directory owned by SYSTEM or Administrators",
                path.display()
            ),
        ));
    }
    secs_sys::host::secure_state_directory(path)
}

#[cfg(target_os = "windows")]
fn prepare_service_directory(path: &Path) -> Result<(), IpcError> {
    match std::fs::create_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(IpcError::new(
                ErrorCode::PermissionDenied,
                format!("create {}: {error}", path.display()),
            ));
        }
    }
    validate_service_directory(path)
}

#[cfg(target_os = "windows")]
fn validate_service_directory_if_present(path: &Path) -> Result<(), IpcError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => validate_service_directory(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(IpcError::new(
            ErrorCode::PermissionDenied,
            format!("inspect {}: {error}", path.display()),
        )),
    }
}

#[cfg(target_os = "windows")]
fn validate_service_directory(path: &Path) -> Result<(), IpcError> {
    let facts = secs_sys::host::inspect_service_directory(path)?;
    if rules::service_directory_allowed(
        facts.is_directory,
        facts.is_reparse_point,
        facts.owner_is_system_administrators_or_trusted_installer,
    ) {
        Ok(())
    } else {
        Err(IpcError::new(
            ErrorCode::PermissionDenied,
            format!(
                "{} must be a real directory owned by SYSTEM, Administrators, or TrustedInstaller",
                path.display()
            ),
        ))
    }
}

#[cfg(target_os = "windows")]
fn append_log_nonfatal(path: &Path, line: &secs_types::ipc::HostLogLine) {
    if let Err(error) = rules::append_log(path, line, rules::LOG_LIMIT) {
        static LAST_REPORT: std::sync::OnceLock<std::sync::Mutex<Option<Instant>>> =
            std::sync::OnceLock::new();
        let reports = LAST_REPORT.get_or_init(|| std::sync::Mutex::new(None));
        let mut last = reports.lock().unwrap_or_else(|error| error.into_inner());
        let now = Instant::now();
        if last.is_none_or(|reported| now.duration_since(reported) >= Duration::from_secs(60)) {
            eprintln!("secs2r-host could not append its log: {}", error.detail);
            *last = Some(now);
        }
    }
}

#[cfg(target_os = "windows")]
fn remove_old_host_copy(paths: &Paths) {
    let old = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(rules::host_update_paths))
        .map(|paths| paths.old);
    let Some(old) = old else {
        return;
    };
    match std::fs::remove_file(&old) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            let line = rules::log_line(
                "old_cleanup_failed",
                [
                    ("path".to_owned(), old.to_string_lossy().into_owned()),
                    ("reason".to_owned(), error.to_string()),
                ],
            );
            append_log_nonfatal(&paths.log, &line);
        }
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
enum HostWorkResult {
    Takeover {
        pending: transitions::PendingTakeover,
        result: Result<TakeoverWorkResult, IpcError>,
    },
    Repair {
        target: Option<secs_types::ipc::ManagedEqApo>,
        result: Result<RepairWorkResult, IpcError>,
    },
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
enum TakeoverWorkResult {
    Started {
        child: secs_sys::host::ManagedChild,
        pid: u32,
        takeover: PortTakeover,
    },
    Busy {
        reason: &'static str,
        detail: String,
    },
    NoSession,
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
struct RepairWorkResult {
    include_state: IncludeState,
    repaired: Option<RepairRecord>,
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
struct RepairRecord {
    path: String,
    design_id: String,
    timestamp: String,
    helper_pid: u32,
}

#[cfg(target_os = "windows")]
fn service_loop(
    paths: &Paths,
    service_events: &mpsc::Receiver<secs_sys::host::ServiceEvent>,
) -> Result<(), IpcError> {
    remove_old_host_copy(paths);
    let managed = match rules::load_state(&paths.state) {
        Ok(managed) => managed,
        Err(error) => {
            let line = rules::log_line(
                "state_corrupt",
                [("detail".to_owned(), error.detail.clone())],
            );
            append_log_nonfatal(&paths.log, &line);
            HostManaged::default()
        }
    };
    let mut core = Core::new(env!("CARGO_PKG_VERSION"), managed);
    for line in rules::load_logs(&paths.log, 1000) {
        core.record(line);
    }
    let (config_sender, config_events) = mpsc::channel();
    let (work_sender, work_events) = mpsc::channel();
    let (mut watcher, watcher_failure) = match configure_watcher(&core, &config_sender) {
        Ok(watcher) => (watcher, None),
        Err(error) => (None, Some(error.detail)),
    };
    let mut child = None;
    let active_session = managing_session_or_none(&mut core, paths);
    let mut transition_state = TransitionState::new(core.managed.clone(), None);
    if active_session.is_none()
        && (core.managed.camilla.is_some() || core.managed.eqapo.is_some())
        && let Some(session) = secs_sys::host::active_console_session()
    {
        let (next, effects) = transitions::decide(
            &transition_state,
            TransitionInput::Console {
                event: TransitionConsoleEvent::Connect,
                session_id: session,
                owner_matches: false,
            },
        );
        transition_state = next;
        drive_transition(
            &mut transition_state,
            effects,
            &mut core,
            &mut child,
            &mut watcher,
            &config_sender,
            &work_sender,
            paths,
        )?;
    }
    if core.managed.camilla.is_some() && active_session.is_some() {
        let (next, effects) = transitions::decide(
            &transition_state,
            TransitionInput::RestartTick {
                owner_session: active_session,
            },
        );
        transition_state = next;
        drive_transition(
            &mut transition_state,
            effects,
            &mut core,
            &mut child,
            &mut watcher,
            &config_sender,
            &work_sender,
            paths,
        )?;
    }
    if active_session.is_some() {
        let (next, effects) = transitions::decide(
            &transition_state,
            TransitionInput::ConfigChanged {
                owner_session: active_session,
            },
        );
        transition_state = next;
        drive_transition(
            &mut transition_state,
            effects,
            &mut core,
            &mut child,
            &mut watcher,
            &config_sender,
            &work_sender,
            paths,
        )?;
    }
    if let Some(detail) = watcher_failure {
        let (next, effects) =
            transitions::decide(&transition_state, TransitionInput::WatcherFailed { detail });
        transition_state = next;
        drive_transition(
            &mut transition_state,
            effects,
            &mut core,
            &mut child,
            &mut watcher,
            &config_sender,
            &work_sender,
            paths,
        )?;
    }
    let mut repair_retry_at = None;
    let mut pipe_slot = None;
    let mut pipe_retry_at = Instant::now();

    let mut stop_after_response = false;
    loop {
        while let Ok(event) = service_events.try_recv() {
            match event {
                secs_sys::host::ServiceEvent::Stop => {
                    if let Some(running) = child.take() {
                        let _ = running.stop();
                    }
                    return Ok(());
                }
                secs_sys::host::ServiceEvent::SessionLogon(session) => {
                    let owner_matches = managing_session_or_none(&mut core, paths) == Some(session);
                    let (next, effects) = transitions::decide(
                        &transition_state,
                        TransitionInput::Session {
                            event: TransitionSessionEvent::Logon,
                            session_id: session,
                            owner_matches,
                        },
                    );
                    transition_state = next;
                    drive_transition(
                        &mut transition_state,
                        effects,
                        &mut core,
                        &mut child,
                        &mut watcher,
                        &config_sender,
                        &work_sender,
                        paths,
                    )?;
                }
                secs_sys::host::ServiceEvent::SessionLogoff(session) => {
                    let owner_matches = transition_state.managed.owner_session_id == Some(session);
                    let (next, effects) = transitions::decide(
                        &transition_state,
                        TransitionInput::Session {
                            event: TransitionSessionEvent::Logoff,
                            session_id: session,
                            owner_matches,
                        },
                    );
                    transition_state = next;
                    drive_transition(
                        &mut transition_state,
                        effects,
                        &mut core,
                        &mut child,
                        &mut watcher,
                        &config_sender,
                        &work_sender,
                        paths,
                    )?;
                }
                secs_sys::host::ServiceEvent::ConsoleConnect(session) => {
                    let owner_matches = managing_session_or_none(&mut core, paths) == Some(session);
                    let (next, effects) = transitions::decide(
                        &transition_state,
                        TransitionInput::Console {
                            event: TransitionConsoleEvent::Connect,
                            session_id: session,
                            owner_matches,
                        },
                    );
                    transition_state = next;
                    drive_transition(
                        &mut transition_state,
                        effects,
                        &mut core,
                        &mut child,
                        &mut watcher,
                        &config_sender,
                        &work_sender,
                        paths,
                    )?;
                }
                secs_sys::host::ServiceEvent::ConsoleDisconnect(session) => {
                    let owner_matches = transition_state.managed.owner_session_id == Some(session);
                    let (next, effects) = transitions::decide(
                        &transition_state,
                        TransitionInput::Console {
                            event: TransitionConsoleEvent::Disconnect,
                            session_id: session,
                            owner_matches,
                        },
                    );
                    transition_state = next;
                    drive_transition(
                        &mut transition_state,
                        effects,
                        &mut core,
                        &mut child,
                        &mut watcher,
                        &config_sender,
                        &work_sender,
                        paths,
                    )?;
                }
            }
        }

        while let Ok(work) = work_events.try_recv() {
            finish_host_work(
                work,
                &mut core,
                &mut child,
                &mut transition_state,
                &mut watcher,
                &config_sender,
                &work_sender,
                paths,
            )?;
        }

        while config_events.try_recv().is_ok() {
            let owner_session = managing_session_or_none(&mut core, paths);
            let (next, effects) = transitions::decide(
                &transition_state,
                TransitionInput::ConfigChanged { owner_session },
            );
            transition_state = next;
            drive_transition(
                &mut transition_state,
                effects,
                &mut core,
                &mut child,
                &mut watcher,
                &config_sender,
                &work_sender,
                paths,
            )?;
        }

        transition_state.sync_repair_retry_deadline(&mut repair_retry_at, Instant::now());
        if repair_retry_at.is_some_and(|deadline| Instant::now() >= deadline) {
            repair_retry_at = None;
            let managing_session = managing_session_or_none(&mut core, paths);
            let (next, effects) = transitions::decide(
                &transition_state,
                TransitionInput::RepairRetry { managing_session },
            );
            transition_state = next;
            drive_transition(
                &mut transition_state,
                effects,
                &mut core,
                &mut child,
                &mut watcher,
                &config_sender,
                &work_sender,
                paths,
            )?;
        }

        let child_exit = match child.as_ref() {
            Some(running) => match running.wait(Duration::ZERO) {
                Ok(Some(exit_code)) => Some(i32::try_from(exit_code).unwrap_or(i32::MAX)),
                Ok(None) => None,
                Err(error) => {
                    record(
                        &mut core,
                        paths,
                        rules::log_line("wait_failed", [("detail".to_owned(), error.detail)]),
                    )?;
                    Some(-1_i32)
                }
            },
            None => None,
        };
        if let Some(exit_code) = child_exit {
            let pid = core.camilla.pid;
            child = None;
            let (next, effects) = transitions::decide(
                &transition_state,
                TransitionInput::ChildExited {
                    pid,
                    exit_code,
                    now: Instant::now(),
                },
            );
            transition_state = next;
            drive_transition(
                &mut transition_state,
                effects,
                &mut core,
                &mut child,
                &mut watcher,
                &config_sender,
                &work_sender,
                paths,
            )?;
        }

        if transition_state
            .restart_due
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            let owner_session = managing_session_or_none(&mut core, paths);
            let (next, effects) = transitions::decide(
                &transition_state,
                TransitionInput::RestartTick { owner_session },
            );
            transition_state = next;
            drive_transition(
                &mut transition_state,
                effects,
                &mut core,
                &mut child,
                &mut watcher,
                &config_sender,
                &work_sender,
                paths,
            )?;
        }

        if pipe_slot.is_none() && Instant::now() >= pipe_retry_at {
            match secs_sys::host::PipeServerSlot::create(&paths.pipe) {
                Ok(slot) => pipe_slot = Some(slot),
                Err(error) => {
                    record(
                        &mut core,
                        paths,
                        rules::log_line(
                            "pipe_create_failed",
                            [("detail".to_owned(), error.detail)],
                        ),
                    )?;
                    pipe_retry_at = Instant::now() + Duration::from_secs(1);
                }
            }
        }
        let served = match pipe_slot.as_mut() {
            Some(slot) => secs_sys::host::pipe_server(
                slot,
                Duration::from_millis(100),
                |identity, request| {
                    handle_request(
                        identity,
                        request,
                        &mut core,
                        &mut transition_state,
                        &mut child,
                        &mut watcher,
                        &config_sender,
                        &work_sender,
                        paths,
                        &mut stop_after_response,
                    )
                    .unwrap_or_else(HostResponse::Error)
                },
            ),
            None => {
                std::thread::sleep(Duration::from_millis(100));
                Ok(false)
            }
        };
        // One client's failure (it left before the answer, a broken frame)
        // must not stop the service; the retained instance serves the next client.
        if let Err(error) = served {
            let line = rules::log_line("pipe_error", [("detail".to_owned(), error.detail)]);
            append_log_nonfatal(&paths.log, &line);
            std::thread::sleep(Duration::from_millis(100));
        }
        if stop_after_response {
            if let Some(running) = child.take() {
                let _ = running.stop();
            }
            return Ok(());
        }
        let _ = &watcher;
    }
}

#[cfg(target_os = "windows")]
struct RealTransitionPlatform<'a> {
    core: &'a mut Core,
    child: &'a mut Option<secs_sys::host::ManagedChild>,
    watcher: &'a mut Option<secs_sys::host::ConfigWatcher>,
    config_sender: &'a mpsc::Sender<()>,
    work_sender: &'a mpsc::Sender<HostWorkResult>,
    paths: &'a Paths,
}

#[cfg(target_os = "windows")]
impl TransitionPlatform for RealTransitionPlatform<'_> {
    fn save_state(&mut self, managed: &HostManaged) -> Result<(), IpcError> {
        rules::save_state(&self.paths.state, managed)
    }

    fn replace_watcher(
        &mut self,
        _target: Option<&secs_types::ipc::ManagedEqApo>,
    ) -> Result<(), IpcError> {
        *self.watcher = configure_watcher(self.core, self.config_sender)?;
        Ok(())
    }

    fn repair_eqapo(&mut self, session: u32) -> Result<(), IpcError> {
        let target = self.core.managed.eqapo.clone();
        let work_target = target.clone();
        let sender = self.work_sender.clone();
        spawn_host_work("secs2r-host-repair", move || {
            let result = repair_eqapo_work(work_target, session);
            let _ = sender.send(HostWorkResult::Repair { target, result });
        })
    }

    fn stop_child(&mut self) -> Result<(), IpcError> {
        if let Some(running) = self.child.take() {
            running.stop()?;
        }
        Ok(())
    }

    fn set_camilla_state(&mut self, state: ManagedProcessState) {
        self.core.camilla.pid = if state == ManagedProcessState::Running {
            self.child.as_ref().and_then(|child| child.pid().ok())
        } else {
            None
        };
        self.core.camilla.state = state;
    }

    fn count_restart(&mut self) {
        self.core.camilla.restarts_in_window =
            self.core.camilla.restarts_in_window.saturating_add(1);
    }

    fn clear_camilla_status(&mut self) {
        self.core.camilla = Default::default();
    }

    fn clear_eqapo_status(&mut self) {
        self.core.eqapo_include = IncludeState::Absent;
    }

    fn launch_child(
        &mut self,
        session: u32,
        target: &secs_types::ipc::ManagedCamilla,
    ) -> Result<(), IpcError> {
        let pending = transitions::PendingTakeover {
            session,
            target: target.clone(),
        };
        let work = pending.clone();
        let sender = self.work_sender.clone();
        spawn_host_work("secs2r-host-takeover", move || {
            let result = takeover_work(&work);
            let _ = sender.send(HostWorkResult::Takeover { pending, result });
        })
    }

    fn log(&mut self, line: &secs_types::ipc::HostLogLine) -> Result<(), IpcError> {
        record(self.core, self.paths, line.clone())
    }
}

#[cfg(target_os = "windows")]
#[allow(clippy::too_many_arguments)]
fn drive_transition(
    state: &mut TransitionState,
    effects: Vec<transitions::Effect>,
    core: &mut Core,
    child: &mut Option<secs_sys::host::ManagedChild>,
    watcher: &mut Option<secs_sys::host::ConfigWatcher>,
    config_sender: &mpsc::Sender<()>,
    work_sender: &mpsc::Sender<HostWorkResult>,
    paths: &Paths,
) -> Result<transitions::DriveOutcome, IpcError> {
    core.managed = state.managed.clone();
    let mut platform = RealTransitionPlatform {
        core,
        child,
        watcher,
        config_sender,
        work_sender,
        paths,
    };
    let outcome = transitions::drive(state, effects, &mut platform)?;
    if state.eqapo_error.is_some() {
        platform.core.eqapo_include = IncludeState::Damaged;
    }
    Ok(outcome)
}

#[cfg(target_os = "windows")]
#[allow(clippy::too_many_arguments)]
fn handle_request(
    identity: secs_sys::host::PipeClientIdentity,
    request: HostRequest,
    core: &mut Core,
    transition_state: &mut TransitionState,
    child: &mut Option<secs_sys::host::ManagedChild>,
    watcher: &mut Option<secs_sys::host::ConfigWatcher>,
    config_sender: &mpsc::Sender<()>,
    work_sender: &mpsc::Sender<HostWorkResult>,
    paths: &Paths,
    stop_after_response: &mut bool,
) -> Result<HostResponse, IpcError> {
    match request {
        HostRequest::SetManaged(update) => {
            authorize_mutation(identity)?;
            if let Some(eqapo) = &update.eqapo {
                validate_eqapo_path(Path::new(&eqapo.config_path))?;
            }
            let requester_sid = secs_sys::host::session_user_identity(identity.session_id)?;
            let prospective = transitions::managed_after_update(
                transition_state,
                &update,
                identity.session_id,
                &requester_sid,
            );
            let managing_session = managing_session_for(&prospective)?;
            let (next, effects) = transitions::decide(
                transition_state,
                TransitionInput::SetManaged {
                    update,
                    requester_session: identity.session_id,
                    requester_sid,
                    managing_session,
                },
            );
            *transition_state = next;
            let outcome = drive_transition(
                transition_state,
                effects,
                core,
                child,
                watcher,
                config_sender,
                work_sender,
                paths,
            )?;
            Ok(outcome
                .repair_error
                .map_or(HostResponse::Ok, HostResponse::Error))
        }
        HostRequest::StopManaging => {
            authorize_mutation(identity)?;
            let (next, effects) =
                transitions::decide(transition_state, TransitionInput::StopManaging);
            *transition_state = next;
            core.eqapo_include = IncludeState::Absent;
            drive_transition(
                transition_state,
                effects,
                core,
                child,
                watcher,
                config_sender,
                work_sender,
                paths,
            )?;
            Ok(HostResponse::Ok)
        }
        HostRequest::UpdateSelf { from_dir } => {
            authorize_mutation(identity)?;
            let install_paths = rules::host_install_paths(&program_files_root()?);
            validate_service_directory(&install_paths.directory)?;
            let update_paths = rules::host_update_paths(&install_paths.directory);
            let staged = stage_host_update(Path::new(&from_dir), &update_paths.staged);
            let manifest = match staged {
                Ok(manifest) => manifest,
                Err(rejection) => {
                    let _ = std::fs::remove_file(&update_paths.staged);
                    record_update_rejection(core, paths, &rejection);
                    return Ok(HostResponse::Error(update_rejection_error(&rejection)));
                }
            };
            if let Err(error) =
                secs_sys::host::spawn_detached(&update_paths.staged, &["finish-update"])
            {
                let rejection = rules::UpdateReject::Io(error.detail);
                let _ = std::fs::remove_file(&update_paths.staged);
                record_update_rejection(core, paths, &rejection);
                return Ok(HostResponse::Error(update_rejection_error(&rejection)));
            }
            let line = rules::log_line("update_staged", [("version".to_owned(), manifest.version)]);
            let _ = record(core, paths, line);
            *stop_after_response = true;
            Ok(HostResponse::Ok)
        }
        other => Ok(core.handle(other)),
    }
}

#[cfg(target_os = "windows")]
fn stage_host_update(
    from_dir: &Path,
    staged_executable: &Path,
) -> Result<rules::HostManifest, rules::UpdateReject> {
    rules::validate_update_source(from_dir)?;
    let manifest_bytes = rules::read_bounded(
        &from_dir.join(rules::UPDATE_MANIFEST),
        rules::MANIFEST_LIMIT,
    )?;
    let signature_bytes = rules::read_bounded(
        &from_dir.join(rules::UPDATE_SIGNATURE),
        rules::MANIFEST_LIMIT,
    )?;
    let signature_text =
        std::str::from_utf8(&signature_bytes).map_err(|_| rules::UpdateReject::BadSignature)?;
    let manifest =
        rules::verify_manifest(rules::updater_pubkey(), &manifest_bytes, signature_text)?;
    // Read, then check the version and hash, then write: the protected folder
    // must never hold bytes the signed manifest does not name.
    let executable =
        rules::read_bounded(&from_dir.join(&manifest.file), rules::HOST_EXECUTABLE_LIMIT)?;
    rules::accept_update(
        env!("CARGO_PKG_VERSION"),
        &manifest,
        &rules::sha256_hex(&executable),
    )?;
    rules::write_staged(staged_executable, &executable)?;
    Ok(manifest)
}

#[cfg(target_os = "windows")]
fn update_rejection_error(rejection: &rules::UpdateReject) -> IpcError {
    IpcError::new(ErrorCode::ApplyFailed, rejection.detail())
        .with_param("reason", rejection.log_reason())
}

#[cfg(target_os = "windows")]
fn record_update_rejection(core: &mut Core, paths: &Paths, rejection: &rules::UpdateReject) {
    let line = rules::log_line(
        "update_rejected",
        [
            ("reason".to_owned(), rejection.log_reason().to_owned()),
            ("detail".to_owned(), rejection.detail()),
        ],
    );
    let _ = record(core, paths, line);
}

#[cfg(target_os = "windows")]
fn authorize_mutation(identity: secs_sys::host::PipeClientIdentity) -> Result<(), IpcError> {
    if rules::client_session_allowed(
        secs_sys::host::active_console_session(),
        identity.session_id,
    ) {
        Ok(())
    } else {
        Err(IpcError::new(
            ErrorCode::PermissionDenied,
            "managed-host mutation requires the active console session",
        )
        .with_param("client_pid", identity.process_id.to_string())
        .with_param("client_session", identity.session_id.to_string()))
    }
}

#[cfg(target_os = "windows")]
fn managing_session_for(managed: &HostManaged) -> Result<Option<u32>, IpcError> {
    let Some(active) = secs_sys::host::active_console_session() else {
        return Ok(None);
    };
    let sid = secs_sys::host::session_user_identity(active)?;
    Ok(rules::managed_user_allowed(managed.owner_user_sid.as_deref(), &sid).then_some(active))
}

#[cfg(target_os = "windows")]
fn managing_session(core: &Core) -> Result<Option<u32>, IpcError> {
    managing_session_for(&core.managed)
}

#[cfg(target_os = "windows")]
fn managing_session_or_none(core: &mut Core, paths: &Paths) -> Option<u32> {
    match managing_session(core) {
        Ok(session) => session,
        Err(error) => {
            let _ = record(
                core,
                paths,
                rules::log_line(
                    "session_lookup_failed",
                    [("detail".to_owned(), error.detail)],
                ),
            );
            None
        }
    }
}

#[cfg(target_os = "windows")]
fn validate_eqapo_path(requested: &Path) -> Result<PathBuf, IpcError> {
    let detected = secs_sys::eqapo_install().ok_or_else(|| {
        IpcError::new(
            ErrorCode::PermissionDenied,
            "Equalizer APO machine configuration is not registered",
        )
    })?;
    let requested = std::fs::canonicalize(requested).map_err(|error| {
        IpcError::new(
            ErrorCode::PermissionDenied,
            format!("canonicalize {}: {error}", requested.display()),
        )
    })?;
    let detected = std::fs::canonicalize(&detected.config_path).map_err(|error| {
        IpcError::new(
            ErrorCode::PermissionDenied,
            format!("canonicalize {}: {error}", detected.config_path),
        )
    })?;
    if rules::eqapo_path_allowed(&requested, &detected) {
        Ok(detected)
    } else {
        Err(IpcError::new(
            ErrorCode::PermissionDenied,
            "managed Equalizer APO path does not match HKLM ConfigPath",
        ))
    }
}

#[cfg(target_os = "windows")]
fn configure_watcher(
    core: &Core,
    sender: &mpsc::Sender<()>,
) -> Result<Option<secs_sys::host::ConfigWatcher>, IpcError> {
    core.managed
        .eqapo
        .as_ref()
        .map(|managed| {
            let sender = sender.clone();
            let config_root = validate_eqapo_path(Path::new(&managed.config_path))?;
            secs_sys::host::watch_config(&config_root.join("config.txt"), move || {
                let _ = sender.send(());
            })
        })
        .transpose()
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq)]
enum PortTakeover {
    Free,
    TakenOver,
    /// The old CamillaDSP ignored `Exit`; the service ended this PID.
    Terminated(u32),
    /// Why the port stayed busy, for the `port_busy` log line.
    Busy(&'static str, String),
}

#[cfg(target_os = "windows")]
fn takeover_port(port: u16) -> PortTakeover {
    // netstat, not a connect probe, says whether anything listens (see
    // `tcp_listener_pid`: a free loopback port times out a short connect).
    match secs_sys::host::tcp_listener_pid(port) {
        Ok(None) => {
            return match rules::takeover_decision(false, Some(rules::PortObservation::Empty)) {
                rules::TakeoverDecision::Launch => PortTakeover::Free,
                _ => PortTakeover::Busy("refused", String::new()),
            };
        }
        Ok(Some(_)) => {}
        Err(error) => return PortTakeover::Busy("netstat", error.detail),
    }

    let address = format!("127.0.0.1:{port}");
    let transport =
        match secs_apply::camilla::WsTransport::connect(&address, Duration::from_secs(3)) {
            Ok(transport) => transport,
            Err(secs_apply::ApplyError::Connect {
                connection_refused: true,
                ..
            }) => return PortTakeover::Free,
            Err(error) => {
                return match rules::takeover_decision(
                    false,
                    Some(rules::PortObservation::OtherProcess),
                ) {
                    rules::TakeoverDecision::PortBusy => {
                        PortTakeover::Busy("not_camilla", error.to_string())
                    }
                    _ => PortTakeover::Free,
                };
            }
        };
    if rules::takeover_decision(false, Some(rules::PortObservation::CamillaDsp))
        != rules::TakeoverDecision::RequestExit
    {
        return PortTakeover::Busy("policy", String::new());
    }
    let mut camilla = secs_apply::camilla::Camilla::new(transport);
    if let Err(error) = camilla.version().and_then(|_| camilla.exit()) {
        return PortTakeover::Busy("exit_request", error.to_string());
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match secs_sys::host::tcp_listener_pid(port) {
            Ok(None) => {
                return match rules::after_exit_decision(false) {
                    rules::TakeoverDecision::Launch => PortTakeover::TakenOver,
                    _ => PortTakeover::Busy("policy", String::new()),
                };
            }
            _ if Instant::now() >= deadline => {
                if rules::after_exit_decision(true) != rules::TakeoverDecision::PortBusy {
                    return PortTakeover::TakenOver;
                }
                // CamillaDSP 4.1.3 stops processing on Exit but keeps the
                // port when a capture thread blocks (a silent loopback).
                return match secs_sys::host::terminate_camilla_listener(port) {
                    Ok(secs_sys::host::ListenerOutcome::Terminated(pid)) => {
                        if port_closes(port, Duration::from_secs(5)) {
                            PortTakeover::Terminated(pid)
                        } else {
                            PortTakeover::Busy("still_open", format!("ended {pid}"))
                        }
                    }
                    // The listener went away between the last poll and now.
                    Ok(secs_sys::host::ListenerOutcome::NotFound) => PortTakeover::TakenOver,
                    Ok(secs_sys::host::ListenerOutcome::NotCamilla { pid, image }) => {
                        PortTakeover::Busy("other_program", format!("{pid} {image}"))
                    }
                    Err(error) => PortTakeover::Busy("terminate_failed", error.detail),
                };
            }
            _ => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

/// Whether netstat stops showing a listener on `port` within `within`.
#[cfg(target_os = "windows")]
fn port_closes(port: u16, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    loop {
        match secs_sys::host::tcp_listener_pid(port) {
            Ok(None) => return true,
            _ if Instant::now() >= deadline => return false,
            _ => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

#[cfg(target_os = "windows")]
fn spawn_host_work(name: &str, work: impl FnOnce() + Send + 'static) -> Result<(), IpcError> {
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(work)
        .map(|_| ())
        .map_err(|error| {
            IpcError::new(
                ErrorCode::ApplyFailed,
                format!("start managed-host worker {name}: {error}"),
            )
        })
}

#[cfg(target_os = "windows")]
fn takeover_work(pending: &transitions::PendingTakeover) -> Result<TakeoverWorkResult, IpcError> {
    let takeover = takeover_port(pending.target.port);
    if let PortTakeover::Busy(reason, detail) = takeover {
        return Ok(TakeoverWorkResult::Busy { reason, detail });
    }
    match secs_sys::host::launch_camilla(
        Path::new(&pending.target.exe_path),
        Path::new(&pending.target.config_path),
        pending.target.port,
        pending.session,
    ) {
        Ok(child) => {
            let pid = child.pid()?;
            Ok(TakeoverWorkResult::Started {
                child,
                pid,
                takeover,
            })
        }
        Err(error)
            if error
                .params
                .get("reason")
                .is_some_and(|value| value == "no_session") =>
        {
            Ok(TakeoverWorkResult::NoSession)
        }
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "windows")]
#[allow(clippy::too_many_arguments)]
fn finish_takeover_work(
    pending: transitions::PendingTakeover,
    result: Result<TakeoverWorkResult, IpcError>,
    core: &mut Core,
    child: &mut Option<secs_sys::host::ManagedChild>,
    transition_state: &mut TransitionState,
    watcher: &mut Option<secs_sys::host::ConfigWatcher>,
    config_sender: &mpsc::Sender<()>,
    work_sender: &mpsc::Sender<HostWorkResult>,
    paths: &Paths,
) -> Result<(), IpcError> {
    if transition_state.pending_takeover.as_ref() != Some(&pending) {
        if let Ok(TakeoverWorkResult::Started { child, .. }) = result {
            let _ = child.stop();
        }
        return Ok(());
    }
    transition_state.pending_takeover = None;

    match result {
        Ok(TakeoverWorkResult::Started {
            child: running,
            pid,
            takeover,
        }) => {
            match takeover {
                PortTakeover::TakenOver => record(
                    core,
                    paths,
                    rules::log_line(
                        "takeover",
                        [("port".to_owned(), pending.target.port.to_string())],
                    ),
                )?,
                PortTakeover::Terminated(terminated_pid) => record(
                    core,
                    paths,
                    rules::log_line(
                        "takeover_terminated",
                        [
                            ("port".to_owned(), pending.target.port.to_string()),
                            ("pid".to_owned(), terminated_pid.to_string()),
                        ],
                    ),
                )?,
                PortTakeover::Free => {}
                PortTakeover::Busy(..) => unreachable!("busy work does not launch"),
            }
            if let Some(previous) = child.replace(running) {
                let _ = previous.stop();
            }
            transition_state.running_child_session = Some(pending.session);
            drive_transition(
                transition_state,
                vec![transitions::Effect::SetCamillaState(
                    ManagedProcessState::Running,
                )],
                core,
                child,
                watcher,
                config_sender,
                work_sender,
                paths,
            )?;
            record(
                core,
                paths,
                rules::log_line(
                    "launch",
                    [
                        ("pid".to_owned(), pid.to_string()),
                        ("design_id".to_owned(), pending.target.design_id),
                    ],
                ),
            )
        }
        Ok(TakeoverWorkResult::Busy { reason, detail }) => {
            let (next, effects) = transitions::decide(
                transition_state,
                TransitionInput::PortBusy {
                    reason,
                    detail,
                    now: Instant::now(),
                },
            );
            *transition_state = next;
            drive_transition(
                transition_state,
                effects,
                core,
                child,
                watcher,
                config_sender,
                work_sender,
                paths,
            )?;
            Ok(())
        }
        Ok(TakeoverWorkResult::NoSession) => {
            transition_state.running_child_session = None;
            drive_transition(
                transition_state,
                vec![transitions::Effect::SetCamillaState(
                    ManagedProcessState::NoSession,
                )],
                core,
                child,
                watcher,
                config_sender,
                work_sender,
                paths,
            )?;
            Ok(())
        }
        Err(error) => {
            let detail = error.detail;
            record(
                core,
                paths,
                rules::log_line("launch_failed", [("detail".to_owned(), detail.clone())]),
            )?;
            let (next, effects) = transitions::decide(
                transition_state,
                TransitionInput::PortBusy {
                    reason: "launch_failed",
                    detail,
                    now: Instant::now(),
                },
            );
            *transition_state = next;
            drive_transition(
                transition_state,
                effects,
                core,
                child,
                watcher,
                config_sender,
                work_sender,
                paths,
            )?;
            Ok(())
        }
    }
}

#[cfg(target_os = "windows")]
#[allow(clippy::too_many_arguments)]
fn finish_host_work(
    work: HostWorkResult,
    core: &mut Core,
    child: &mut Option<secs_sys::host::ManagedChild>,
    transition_state: &mut TransitionState,
    watcher: &mut Option<secs_sys::host::ConfigWatcher>,
    config_sender: &mpsc::Sender<()>,
    work_sender: &mpsc::Sender<HostWorkResult>,
    paths: &Paths,
) -> Result<(), IpcError> {
    match work {
        HostWorkResult::Takeover { pending, result } => finish_takeover_work(
            pending,
            result,
            core,
            child,
            transition_state,
            watcher,
            config_sender,
            work_sender,
            paths,
        ),
        HostWorkResult::Repair { target, result } => {
            if transition_state.managed.eqapo != target {
                return Ok(());
            }
            let transition_result = match result {
                Ok(work) => {
                    core.eqapo_include = work.include_state;
                    if let Some(repaired) = work.repaired {
                        core.last_repair = Some(repaired.timestamp.clone());
                        record(
                            core,
                            paths,
                            rules::log_line(
                                "repair",
                                [
                                    ("path".to_owned(), repaired.path),
                                    ("design_id".to_owned(), repaired.design_id),
                                    ("timestamp".to_owned(), repaired.timestamp),
                                    ("helper_pid".to_owned(), repaired.helper_pid.to_string()),
                                ],
                            ),
                        )?;
                    }
                    Ok(())
                }
                Err(error) => {
                    core.eqapo_include = IncludeState::Damaged;
                    Err(error)
                }
            };
            if let Some(line) = transitions::finish_repair(transition_state, &transition_result) {
                record(core, paths, line)?;
            }
            Ok(())
        }
    }
}

#[cfg(target_os = "windows")]
fn repair_eqapo_work(
    managed: Option<secs_types::ipc::ManagedEqApo>,
    session: u32,
) -> Result<RepairWorkResult, IpcError> {
    let Some(managed) = managed else {
        return Ok(RepairWorkResult {
            include_state: IncludeState::Absent,
            repaired: None,
        });
    };
    let config_root = validate_eqapo_path(Path::new(&managed.config_path))?;
    let config = config_root.join("config.txt");
    let text = std::fs::read_to_string(&config).map_err(|error| {
        IpcError::new(
            ErrorCode::ApplyFailed,
            format!("read {}: {error}", config.display()),
        )
    })?;
    let state = include_state(&text);
    if rules::repair_text(&text).is_none() {
        return Ok(RepairWorkResult {
            include_state: state,
            repaired: None,
        });
    }

    let host = std::env::current_exe().map_err(|error| {
        IpcError::new(
            ErrorCode::ApplyFailed,
            format!("find host executable: {error}"),
        )
    })?;
    let helper = secs_sys::host::launch_eqapo_repair(&host, &config_root, session)?;
    let helper_pid = helper.pid()?;
    let exit = helper.wait(Duration::from_secs(30))?;
    let Some(exit) = exit else {
        helper.stop()?;
        return Err(IpcError::new(
            ErrorCode::ApplyFailed,
            "Equalizer APO repair helper timed out",
        ));
    };
    if exit != 0 {
        return Err(IpcError::new(
            ErrorCode::ApplyFailed,
            format!("Equalizer APO repair helper exited with {exit}"),
        ));
    }

    let repaired = std::fs::read_to_string(&config).map_err(|error| {
        IpcError::new(
            ErrorCode::ApplyFailed,
            format!("read repaired {}: {error}", config.display()),
        )
    })?;
    let timestamp = chrono::Utc::now().to_rfc3339();
    Ok(RepairWorkResult {
        include_state: include_state(&repaired),
        repaired: Some(RepairRecord {
            path: config.to_string_lossy().into_owned(),
            design_id: managed.design_id,
            timestamp,
            helper_pid,
        }),
    })
}

#[cfg(target_os = "windows")]
fn include_state(text: &str) -> IncludeState {
    match secs_apply::eqapo::include_state(text) {
        secs_apply::eqapo::IncludeState::Absent => IncludeState::Absent,
        secs_apply::eqapo::IncludeState::Present => IncludeState::Present,
        secs_apply::eqapo::IncludeState::Damaged => IncludeState::Damaged,
    }
}

#[cfg(target_os = "windows")]
fn repair_eqapo_as_user(config_folder: &Path) -> Result<(), IpcError> {
    let config_root = validate_eqapo_path(config_folder)?;
    let config = config_root.join("config.txt");
    if secs_sys::host::is_reparse_point(&config)? {
        return Err(IpcError::new(
            ErrorCode::PermissionDenied,
            format!("{} must not be a reparse point", config.display()),
        ));
    }
    let text = std::fs::read_to_string(&config).map_err(|error| {
        IpcError::new(
            ErrorCode::ApplyFailed,
            format!("read {}: {error}", config.display()),
        )
    })?;
    let Some(repaired) = rules::repair_text(&text) else {
        return Ok(());
    };
    secs_types::fs::write_atomic_bytes(&config, repaired.as_bytes()).map_err(|error| {
        IpcError::new(
            ErrorCode::ApplyFailed,
            format!("repair {}: {error}", config.display()),
        )
    })
}

#[cfg(target_os = "windows")]
fn record(
    core: &mut Core,
    paths: &Paths,
    line: secs_types::ipc::HostLogLine,
) -> Result<(), IpcError> {
    append_log_nonfatal(&paths.log, &line);
    core.record(line);
    Ok(())
}

#[cfg(target_os = "windows")]
fn println_json(value: &impl serde::Serialize) {
    match serde_json::to_string(value) {
        Ok(json) => println!("{json}"),
        Err(error) => eprintln!("secs2r-host: could not serialize status: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_parse() {
        for (name, expected) in [
            ("install", Action::Install),
            ("uninstall", Action::Uninstall),
            ("run", Action::Run),
            ("status", Action::Status),
        ] {
            assert_eq!(
                Cli::try_parse_from(["secs2r-host", name]).unwrap().action,
                expected
            );
        }
        assert_eq!(
            Cli::try_parse_from(["secs2r-host", "repair-eqapo", r"C:\apo"])
                .unwrap()
                .action,
            Action::RepairEqapo {
                config_folder: std::path::PathBuf::from(r"C:\apo")
            }
        );
        assert_eq!(
            Cli::try_parse_from(["secs2r-host", "finish-update"])
                .unwrap()
                .action,
            Action::FinishUpdate
        );
    }
}
