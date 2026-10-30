#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! `secs2r-host`: the Windows service for ADR 0018's managed apply level.

#[cfg(any(target_os = "windows", test))]
// The service runs only on Windows; elsewhere main is a stub and only the
// rules' unit tests use this module.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
mod rules;

#[cfg(target_os = "windows")]
use std::path::{Path, PathBuf};
use std::process::ExitCode;
#[cfg(target_os = "windows")]
use std::sync::mpsc;
#[cfg(target_os = "windows")]
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
#[cfg(target_os = "windows")]
use rules::{Core, ProcessAction, ProcessEvent, ProcessRules};
#[cfg(target_os = "windows")]
use secs_types::ipc::{
    ErrorCode, HostManaged, HostRequest, HostResponse, IncludeState, IpcError, ManagedProcessState,
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
            rules::remove_host_copy(&paths).map_err(|error| {
                IpcError::new(
                    ErrorCode::ApplyFailed,
                    format!("remove {}: {error}", paths.executable.display()),
                )
            })
        }
        Action::Run => {
            let paths = Paths::from_env();
            if let Some(root) = &paths.program_data_root {
                prepare_state_directory(root)?;
            }
            secs_sys::host::run_service(move |events| {
                if let Err(error) = service_loop(&paths, &events) {
                    eprintln!("secs2r-host service: {}", error.detail);
                    let line =
                        rules::log_line("service_error", [("detail".to_owned(), error.detail)]);
                    let _ = rules::append_log(&paths.log, &line, rules::LOG_LIMIT);
                }
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
    }
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
fn mark_other_user(core: &mut Core, paths: &Paths, session: u32) -> Result<(), IpcError> {
    if rules::mark_other_user(&mut core.camilla) {
        record(
            core,
            paths,
            rules::log_line("other_user", [("session".to_owned(), session.to_string())]),
        )
    } else {
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn service_loop(
    paths: &Paths,
    service_events: &mpsc::Receiver<secs_sys::host::ServiceEvent>,
) -> Result<(), IpcError> {
    let managed = match rules::load_state(&paths.state) {
        Ok(managed) => managed,
        Err(error) => {
            let line = rules::log_line(
                "state_corrupt",
                [("detail".to_owned(), error.detail.clone())],
            );
            rules::append_log(&paths.log, &line, rules::LOG_LIMIT)?;
            HostManaged::default()
        }
    };
    let mut core = Core::new(env!("CARGO_PKG_VERSION"), managed);
    for line in rules::load_logs(&paths.log, 1000) {
        core.record(line);
    }
    let (config_sender, config_events) = mpsc::channel();
    let mut watcher = configure_watcher(&core, &config_sender)?;
    let mut child = None;
    let active_session = managing_session(&core)?;
    if active_session.is_none()
        && (core.managed.camilla.is_some() || core.managed.eqapo.is_some())
        && let Some(session) = secs_sys::host::active_console_session()
    {
        mark_other_user(&mut core, paths, session)?;
    }
    let mut process_rules = ProcessRules::new(active_session.is_some());
    let mut restart_at = None;
    if core.managed.camilla.is_some()
        && let Some(session) = active_session
    {
        launch(
            &mut core,
            &mut child,
            &mut process_rules,
            &mut restart_at,
            paths,
            session,
        )?;
    }
    if let Some(session) = active_session {
        repair_eqapo(&mut core, paths, session)?;
    }

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
                    record(
                        &mut core,
                        paths,
                        rules::log_line(
                            "session_logon",
                            [("session".to_owned(), session.to_string())],
                        ),
                    )?;
                    if managing_session(&core)? == Some(session) {
                        if process_rules.apply(ProcessEvent::Logon, Instant::now())
                            == ProcessAction::Launch
                        {
                            launch(
                                &mut core,
                                &mut child,
                                &mut process_rules,
                                &mut restart_at,
                                paths,
                                session,
                            )?;
                        }
                        repair_eqapo(&mut core, paths, session)?;
                    } else {
                        if process_rules.apply(ProcessEvent::Logoff, Instant::now())
                            == ProcessAction::Stop
                            && let Some(running) = child.take()
                        {
                            running.stop()?;
                        }
                        mark_other_user(&mut core, paths, session)?;
                    }
                }
                secs_sys::host::ServiceEvent::SessionLogoff(session) => {
                    record(
                        &mut core,
                        paths,
                        rules::log_line(
                            "session_logoff",
                            [("session".to_owned(), session.to_string())],
                        ),
                    )?;
                    if process_rules.apply(ProcessEvent::Logoff, Instant::now())
                        == ProcessAction::Stop
                        && let Some(running) = child.take()
                    {
                        running.stop()?;
                    }
                    core.camilla.state = ManagedProcessState::NoSession;
                    core.camilla.pid = None;
                    restart_at = None;
                }
            }
        }

        while config_events.try_recv().is_ok() {
            if let Some(session) = managing_session(&core)? {
                repair_eqapo(&mut core, paths, session)?;
            }
        }

        if let Some(running) = child.as_ref()
            && let Some(exit_code) = running.wait(Duration::ZERO)?
        {
            let pid = core.camilla.pid.take();
            child = None;
            record(
                &mut core,
                paths,
                rules::log_line(
                    "exit",
                    [
                        (
                            "pid".to_owned(),
                            pid.map_or_else(String::new, |pid| pid.to_string()),
                        ),
                        ("code".to_owned(), exit_code.to_string()),
                    ],
                ),
            )?;
            match process_rules.apply(ProcessEvent::Exit, Instant::now()) {
                ProcessAction::RestartAfter(delay) => {
                    let deadline = Instant::now() + delay;
                    restart_at = Some(deadline);
                    core.camilla.restarts_in_window += 1;
                    core.camilla.state = ManagedProcessState::BackingOff {
                        until: (chrono::Utc::now()
                            + chrono::Duration::from_std(delay).unwrap_or_default())
                        .to_rfc3339(),
                    };
                    record(
                        &mut core,
                        paths,
                        rules::log_line(
                            "restart",
                            [("after_ms".to_owned(), delay.as_millis().to_string())],
                        ),
                    )?;
                }
                ProcessAction::GiveUp => {
                    let reason = "five exits within five minutes";
                    core.camilla.state = ManagedProcessState::GivenUp {
                        reason: reason.into(),
                    };
                    record(&mut core, paths, rules::given_up_line(reason))?;
                }
                _ => {}
            }
        }

        if restart_at.is_some_and(|deadline| Instant::now() >= deadline) {
            restart_at = None;
            if let Some(session) = managing_session(&core)? {
                launch(
                    &mut core,
                    &mut child,
                    &mut process_rules,
                    &mut restart_at,
                    paths,
                    session,
                )?;
            }
        }

        let served = secs_sys::host::pipe_server(
            &paths.pipe,
            Duration::from_millis(100),
            |identity, request| {
                handle_request(
                    identity,
                    request,
                    &mut core,
                    &mut child,
                    &mut process_rules,
                    &mut watcher,
                    &mut restart_at,
                    &config_sender,
                    paths,
                )
                .unwrap_or_else(HostResponse::Error)
            },
        );
        // One client's failure (it left before the answer, a broken frame)
        // must not stop the service; the next loop serves the next client.
        if let Err(error) = served {
            let line = rules::log_line("pipe_error", [("detail".to_owned(), error.detail)]);
            let _ = rules::append_log(&paths.log, &line, rules::LOG_LIMIT);
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = &watcher;
    }
}

#[cfg(target_os = "windows")]
#[allow(clippy::too_many_arguments)]
fn handle_request(
    identity: secs_sys::host::PipeClientIdentity,
    request: HostRequest,
    core: &mut Core,
    child: &mut Option<secs_sys::host::ManagedChild>,
    process_rules: &mut ProcessRules,
    watcher: &mut Option<secs_sys::host::ConfigWatcher>,
    restart_at: &mut Option<Instant>,
    config_sender: &mpsc::Sender<()>,
    paths: &Paths,
) -> Result<HostResponse, IpcError> {
    match request {
        HostRequest::SetManaged(mut update) => {
            authorize_mutation(identity)?;
            if let Some(eqapo) = &update.eqapo {
                validate_eqapo_path(Path::new(&eqapo.config_path))?;
            }
            update.owner_session_id = Some(identity.session_id);
            update.owner_user_sid =
                Some(secs_sys::host::session_user_identity(identity.session_id)?);
            let previous_camilla = core.managed.camilla.clone();
            let response = core.handle(HostRequest::SetManaged(update));
            rules::save_state(&paths.state, &core.managed)?;
            *watcher = configure_watcher(core, config_sender)?;
            if let Some(session) = managing_session(core)? {
                repair_eqapo(core, paths, session)?;
            }
            if previous_camilla != core.managed.camilla {
                *restart_at = None;
                if let Some(running) = child.take() {
                    running.stop()?;
                }
            }
            if child.is_none()
                && core.managed.camilla.is_some()
                && let Some(session) = managing_session(core)?
            {
                *restart_at = None;
                launch(core, child, process_rules, restart_at, paths, session)?;
            }
            Ok(response)
        }
        HostRequest::StopManaging => {
            authorize_mutation(identity)?;
            let response = core.handle(HostRequest::StopManaging);
            rules::save_state(&paths.state, &core.managed)?;
            *watcher = None;
            *restart_at = None;
            if let Some(running) = child.take() {
                running.stop()?;
            }
            Ok(response)
        }
        other => Ok(core.handle(other)),
    }
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
fn managing_session(core: &Core) -> Result<Option<u32>, IpcError> {
    let Some(active) = secs_sys::host::active_console_session() else {
        return Ok(None);
    };
    let sid = secs_sys::host::session_user_identity(active)?;
    Ok(rules::managed_user_allowed(core.managed.owner_user_sid.as_deref(), &sid).then_some(active))
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
    use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};

    let socket = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    match TcpStream::connect_timeout(&socket.into(), Duration::from_secs(1)) {
        Ok(stream) => drop(stream),
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
            return match rules::takeover_decision(false, Some(rules::PortObservation::Empty)) {
                rules::TakeoverDecision::Launch => PortTakeover::Free,
                _ => PortTakeover::Busy("refused", String::new()),
            };
        }
        Err(error) => {
            return match rules::takeover_decision(false, Some(rules::PortObservation::OtherProcess))
            {
                rules::TakeoverDecision::PortBusy => PortTakeover::Busy("probe", error.to_string()),
                _ => PortTakeover::Free,
            };
        }
    }

    let address = format!("127.0.0.1:{port}");
    let transport =
        match secs_apply::camilla::WsTransport::connect(&address, Duration::from_secs(1)) {
            Ok(transport) => transport,
            Err(secs_apply::ApplyError::Connect {
                connection_refused: true,
                ..
            }) => return PortTakeover::Free,
            Err(error) => return PortTakeover::Busy("not_camilla", error.to_string()),
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
        match TcpStream::connect_timeout(&socket.into(), Duration::from_millis(100)) {
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
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
                        if port_closes(socket, Duration::from_secs(5)) {
                            PortTakeover::Terminated(pid)
                        } else {
                            PortTakeover::Busy("still_open", format!("ended {pid}"))
                        }
                    }
                    // The old process closed its listener but is still ending:
                    // connects then hang instead of being refused. Nothing
                    // listens, so the new CamillaDSP can bind the port.
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

/// Whether connecting to `socket` is refused within `within`.
#[cfg(target_os = "windows")]
fn port_closes(socket: std::net::SocketAddrV4, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    loop {
        match std::net::TcpStream::connect_timeout(&socket.into(), Duration::from_millis(100)) {
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => return true,
            _ if Instant::now() >= deadline => return false,
            _ => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

#[cfg(target_os = "windows")]
fn launch(
    core: &mut Core,
    child: &mut Option<secs_sys::host::ManagedChild>,
    process_rules: &mut ProcessRules,
    restart_at: &mut Option<Instant>,
    paths: &Paths,
    session: u32,
) -> Result<(), IpcError> {
    if rules::takeover_decision(child.is_some(), None) == rules::TakeoverDecision::KeepOwnChild {
        return Ok(());
    }
    let Some(managed) = core.managed.camilla.clone() else {
        core.camilla.state = ManagedProcessState::Stopped;
        core.camilla.pid = None;
        return Ok(());
    };
    match takeover_port(managed.port) {
        PortTakeover::Free => {}
        PortTakeover::TakenOver => {
            record(
                core,
                paths,
                rules::log_line("takeover", [("port".to_owned(), managed.port.to_string())]),
            )?;
        }
        PortTakeover::Terminated(pid) => {
            record(
                core,
                paths,
                rules::log_line(
                    "takeover_terminated",
                    [
                        ("port".to_owned(), managed.port.to_string()),
                        ("pid".to_owned(), pid.to_string()),
                    ],
                ),
            )?;
        }
        PortTakeover::Busy(reason, detail) => {
            record(
                core,
                paths,
                rules::log_line(
                    "port_busy",
                    [
                        ("port".to_owned(), managed.port.to_string()),
                        ("reason".to_owned(), reason.to_owned()),
                        ("detail".to_owned(), detail),
                    ],
                ),
            )?;
            match process_rules.apply(ProcessEvent::Exit, Instant::now()) {
                ProcessAction::RestartAfter(delay) => {
                    *restart_at = Some(Instant::now() + delay);
                    core.camilla.restarts_in_window += 1;
                    core.camilla.state = ManagedProcessState::BackingOff {
                        until: (chrono::Utc::now()
                            + chrono::Duration::from_std(delay).unwrap_or_default())
                        .to_rfc3339(),
                    };
                    record(
                        core,
                        paths,
                        rules::log_line(
                            "restart",
                            [
                                ("after_ms".to_owned(), delay.as_millis().to_string()),
                                ("reason".to_owned(), "port_busy".to_owned()),
                            ],
                        ),
                    )?;
                }
                ProcessAction::GiveUp => {
                    *restart_at = None;
                    core.camilla.state = ManagedProcessState::GivenUp {
                        reason: "port_busy".into(),
                    };
                    record(core, paths, rules::given_up_line("port_busy"))?;
                }
                _ => {}
            }
            core.camilla.pid = None;
            return Ok(());
        }
    }

    match secs_sys::host::launch_camilla(
        Path::new(&managed.exe_path),
        Path::new(&managed.config_path),
        managed.port,
        session,
    ) {
        Ok(running) => {
            let pid = running.pid()?;
            process_rules.launched();
            core.camilla.state = ManagedProcessState::Running;
            core.camilla.pid = Some(pid);
            record(
                core,
                paths,
                rules::log_line(
                    "launch",
                    [
                        ("pid".to_owned(), pid.to_string()),
                        ("design_id".to_owned(), managed.design_id),
                    ],
                ),
            )?;
            *child = Some(running);
            Ok(())
        }
        Err(error)
            if error
                .params
                .get("reason")
                .is_some_and(|value| value == "no_session") =>
        {
            core.camilla.state = ManagedProcessState::NoSession;
            core.camilla.pid = None;
            Ok(())
        }
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "windows")]
fn repair_eqapo(core: &mut Core, paths: &Paths, session: u32) -> Result<(), IpcError> {
    let Some(managed) = core.managed.eqapo.clone() else {
        core.eqapo_include = IncludeState::Absent;
        return Ok(());
    };
    let config_root = validate_eqapo_path(Path::new(&managed.config_path))?;
    let config = config_root.join("config.txt");
    let text = std::fs::read_to_string(&config).map_err(|error| {
        IpcError::new(
            ErrorCode::ApplyFailed,
            format!("read {}: {error}", config.display()),
        )
    })?;
    core.eqapo_include = include_state(&text);
    if rules::repair_text(&text).is_none() {
        return Ok(());
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
    core.eqapo_include = include_state(&repaired);
    let timestamp = chrono::Utc::now().to_rfc3339();
    core.last_repair = Some(timestamp.clone());
    record(
        core,
        paths,
        rules::log_line(
            "repair",
            [
                ("path".to_owned(), config.to_string_lossy().into_owned()),
                ("design_id".to_owned(), managed.design_id),
                ("timestamp".to_owned(), timestamp),
                ("helper_pid".to_owned(), helper_pid.to_string()),
            ],
        ),
    )
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
    rules::append_log(&paths.log, &line, rules::LOG_LIMIT)?;
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
    }
}
