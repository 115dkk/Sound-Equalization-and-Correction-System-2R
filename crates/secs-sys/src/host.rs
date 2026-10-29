//! Windows managed-host boundary: service control, named pipes, process launch,
//! elevation, and file notification. Non-Windows functions return typed stubs.

use std::path::Path;
use std::time::Duration;

use secs_types::ipc::{ErrorCode, HostRequest, HostResponse, IpcError, ServiceState};

pub const SERVICE_NAME: &str = "SECS2RHost";
pub const SERVICE_DISPLAY_NAME: &str = "SECS 2R Host";
pub const DEFAULT_PIPE_NAME: &str = r"\\.\pipe\secs2r-host";
pub const PIPE_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GRGW;;;IU)";
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
pub const PIPE_TIMEOUT: Duration = Duration::from_secs(5);

#[cfg(any(target_os = "windows", test))]
fn ipc_error(detail: impl Into<String>) -> IpcError {
    IpcError::new(ErrorCode::ApplyFailed, detail)
}

#[cfg(not(target_os = "windows"))]
fn unsupported(operation: &str) -> IpcError {
    IpcError::new(
        ErrorCode::Unsupported,
        format!("{operation} is available only on Windows"),
    )
}

#[cfg(any(target_os = "windows", test))]
fn frame<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, IpcError> {
    let payload = serde_json::to_vec(value).map_err(|error| ipc_error(error.to_string()))?;
    if payload.len() > MAX_MESSAGE_BYTES {
        return Err(IpcError::new(
            ErrorCode::InvalidRequest,
            "managed-host message exceeds 1 MiB",
        ));
    }
    let length = u32::try_from(payload.len()).map_err(|_| {
        IpcError::new(
            ErrorCode::InvalidRequest,
            "managed-host message is too large",
        )
    })?;
    let mut framed = Vec::with_capacity(4 + payload.len());
    framed.extend_from_slice(&length.to_le_bytes());
    framed.extend_from_slice(&payload);
    Ok(framed)
}

#[cfg(any(target_os = "windows", test))]
fn parse_frame<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, IpcError> {
    if bytes.len() < 4 {
        return Err(IpcError::new(
            ErrorCode::InvalidRequest,
            "managed-host frame has no complete length prefix",
        ));
    }
    let length = u32::from_le_bytes(bytes[..4].try_into().expect("four bytes")) as usize;
    if length > MAX_MESSAGE_BYTES {
        return Err(IpcError::new(
            ErrorCode::InvalidRequest,
            "managed-host message exceeds 1 MiB",
        ));
    }
    if bytes.len() != 4 + length {
        return Err(IpcError::new(
            ErrorCode::InvalidRequest,
            "managed-host frame length does not match its payload",
        ));
    }
    serde_json::from_slice(&bytes[4..]).map_err(|error| {
        IpcError::new(
            ErrorCode::InvalidRequest,
            format!("managed-host message is not valid JSON: {error}"),
        )
    })
}

#[cfg(any(target_os = "windows", test))]
fn read_frame(
    mut read: impl FnMut(&mut [u8]) -> Result<usize, IpcError>,
) -> Result<Vec<u8>, IpcError> {
    let mut header = [0_u8; 4];
    read_exact(&mut read, &mut header)?;
    let length = u32::from_le_bytes(header) as usize;
    if length > MAX_MESSAGE_BYTES {
        return Err(IpcError::new(
            ErrorCode::InvalidRequest,
            "managed-host message exceeds 1 MiB",
        ));
    }
    let mut framed = Vec::with_capacity(4 + length);
    framed.extend_from_slice(&header);
    framed.resize(4 + length, 0);
    read_exact(&mut read, &mut framed[4..])?;
    Ok(framed)
}

#[cfg(any(target_os = "windows", test))]
fn read_exact(
    read: &mut impl FnMut(&mut [u8]) -> Result<usize, IpcError>,
    mut destination: &mut [u8],
) -> Result<(), IpcError> {
    while !destination.is_empty() {
        let count = read(destination)?;
        if count == 0 {
            return Err(ipc_error(
                "managed-host pipe closed before the frame was complete",
            ));
        }
        let (_, remaining) = destination.split_at_mut(count.min(destination.len()));
        destination = remaining;
    }
    Ok(())
}

/// Identity Windows reports for one named-pipe client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipeClientIdentity {
    pub process_id: u32,
    pub session_id: u32,
}

/// Serves one pipe connection. `Ok(false)` means no client arrived before
/// `accept_timeout`; callers use a short timeout to poll their stop signal.
#[cfg(target_os = "windows")]
pub fn pipe_server(
    pipe_name: &str,
    accept_timeout: Duration,
    handler: impl FnOnce(PipeClientIdentity, HostRequest) -> HostResponse,
) -> Result<bool, IpcError> {
    use win_custody::{PipeDirection, PipeError, PipeServer, PipeWait, SecurityDescriptor};

    let descriptor = SecurityDescriptor::from_sddl(PIPE_SDDL)
        .map_err(|error| ipc_error(format!("pipe security descriptor: {error}")))?;
    let server = PipeServer::create(pipe_name, PipeDirection::Duplex, &descriptor)
        .map_err(|error| ipc_error(format!("create managed-host pipe: {error}")))?;
    match server.connect(&PipeWait::within(accept_timeout)) {
        Ok(()) => {}
        Err(PipeError::TimedOut) => return Ok(false),
        Err(error) => return Err(ipc_error(format!("accept managed-host pipe: {error}"))),
    }
    let process_id = server
        .client_process_id()
        .map_err(|error| ipc_error(format!("identify managed-host client: {error}")))?;
    let session_id = win_custody::process_session_id(process_id)
        .map_err(|error| ipc_error(format!("identify managed-host client session: {error}")))?;
    let identity = PipeClientIdentity {
        process_id,
        session_id,
    };
    let wait = PipeWait::within(PIPE_TIMEOUT);
    let request_bytes = read_frame(|buffer| {
        server
            .read(buffer, &wait)
            .map_err(|error| ipc_error(format!("read managed-host request: {error}")))
    })?;
    let request = parse_frame::<HostRequest>(&request_bytes)?;
    let response = frame(&handler(identity, request))?;
    server
        .write_all(&response, &wait)
        .map_err(|error| ipc_error(format!("write managed-host response: {error}")))?;
    Ok(true)
}

#[cfg(not(target_os = "windows"))]
pub fn pipe_server(
    _pipe_name: &str,
    _accept_timeout: Duration,
    _handler: impl FnOnce(PipeClientIdentity, HostRequest) -> HostResponse,
) -> Result<bool, IpcError> {
    Err(unsupported("managed-host pipe server"))
}

/// Makes one request and reads one response from the default host pipe.
pub fn pipe_call(request: &HostRequest) -> Result<HostResponse, IpcError> {
    pipe_call_at(DEFAULT_PIPE_NAME, request)
}

/// Makes one request against an explicitly injected pipe name.
#[cfg(target_os = "windows")]
pub fn pipe_call_at(pipe_name: &str, request: &HostRequest) -> Result<HostResponse, IpcError> {
    use std::time::Instant;
    use win_custody::{PipeAccess, PipeClient, PipeWait};

    let deadline = Instant::now() + PIPE_TIMEOUT;
    let client = PipeClient::open(
        pipe_name,
        PipeAccess::ReadWrite,
        deadline,
        Duration::from_millis(10),
    )
    .map_err(|error| ipc_error(format!("open managed-host pipe: {error}")))?;
    client
        .server_process_id()
        .map_err(|error| ipc_error(format!("identify managed-host server: {error}")))?;
    let wait = PipeWait::until(deadline);
    client
        .write_all(&frame(request)?, &wait)
        .map_err(|error| ipc_error(format!("write managed-host request: {error}")))?;
    let response = read_frame(|buffer| {
        client
            .read(buffer, &wait)
            .map_err(|error| ipc_error(format!("read managed-host response: {error}")))
    })?;
    parse_frame(&response)
}

#[cfg(not(target_os = "windows"))]
pub fn pipe_call_at(_pipe_name: &str, _request: &HostRequest) -> Result<HostResponse, IpcError> {
    Err(unsupported("managed-host pipe client"))
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
pub struct ManagedChild {
    process: win_custody::Process,
    job: win_custody::Job,
}

#[cfg(target_os = "windows")]
impl ManagedChild {
    pub fn pid(&self) -> Result<u32, IpcError> {
        self.process
            .pid()
            .map_err(|error| ipc_error(format!("read CamillaDSP process ID: {error}")))
    }

    /// Waits for process exit and returns its exit code, or `None` on timeout.
    pub fn wait(&self, timeout: Duration) -> Result<Option<u32>, IpcError> {
        use win_custody::WaitOutcome;
        match self
            .process
            .wait(Some(timeout))
            .map_err(|error| ipc_error(format!("wait for CamillaDSP: {error}")))?
        {
            WaitOutcome::TimedOut => Ok(None),
            WaitOutcome::Signaled | WaitOutcome::Abandoned => self
                .process
                .exit_code()
                .map_err(|error| ipc_error(format!("read CamillaDSP exit code: {error}"))),
        }
    }

    pub fn stop(self) -> Result<(), IpcError> {
        self.job
            .terminate(0)
            .map_err(|error| ipc_error(format!("stop CamillaDSP job: {error}")))
    }
}

#[cfg(not(target_os = "windows"))]
#[derive(Debug)]
pub struct ManagedChild;

/// Launches CamillaDSP in the active console user's session and assigns it to a
/// kill-on-close job before its first instruction runs.
#[cfg(target_os = "windows")]
pub fn launch_camilla(
    executable: &Path,
    config: &Path,
    port: u16,
    session: u32,
) -> Result<ManagedChild, IpcError> {
    use win_custody::{Job, JobLimits, Launch, PrimaryToken};

    let token = PrimaryToken::for_session_user(session)
        .map_err(|error| ipc_error(format!("open active session token: {error}")))?;
    let job = Job::create(&JobLimits::new().kill_on_close(true))
        .map_err(|error| ipc_error(format!("create CamillaDSP job: {error}")))?;
    let process = Launch::new(executable)
        .args([
            "-p".into(),
            port.to_string().into(),
            "-a".into(),
            "127.0.0.1".into(),
            config.as_os_str().to_owned(),
        ])
        .as_user(&token)
        .desktop(r"winsta0\default")
        .spawn_in_job(&job)
        .map_err(|error| ipc_error(format!("launch CamillaDSP: {error}")))?;
    Ok(ManagedChild { process, job })
}

#[cfg(not(target_os = "windows"))]
pub fn launch_camilla(
    _executable: &Path,
    _config: &Path,
    _port: u16,
    _session: u32,
) -> Result<ManagedChild, IpcError> {
    Err(unsupported("managed CamillaDSP launch"))
}

/// Starts the Equalizer APO repair helper as the managing user.
#[cfg(target_os = "windows")]
pub fn launch_eqapo_repair(
    host_executable: &Path,
    config_folder: &Path,
    session: u32,
) -> Result<ManagedChild, IpcError> {
    use win_custody::{Job, JobLimits, Launch, PrimaryToken};

    let token = PrimaryToken::for_session_user(session)
        .map_err(|error| ipc_error(format!("open repair session token: {error}")))?;
    let job = Job::create(&JobLimits::new().kill_on_close(true))
        .map_err(|error| ipc_error(format!("create repair job: {error}")))?;
    let process = Launch::new(host_executable)
        .args(["repair-eqapo".into(), config_folder.as_os_str().to_owned()])
        .as_user(&token)
        .desktop(r"winsta0\default")
        .spawn_in_job(&job)
        .map_err(|error| ipc_error(format!("launch Equalizer APO repair: {error}")))?;
    Ok(ManagedChild { process, job })
}

#[cfg(not(target_os = "windows"))]
pub fn launch_eqapo_repair(
    _host_executable: &Path,
    _config_folder: &Path,
    _session: u32,
) -> Result<ManagedChild, IpcError> {
    Err(unsupported("Equalizer APO repair launch"))
}

#[must_use]
pub fn active_console_session() -> Option<u32> {
    #[cfg(target_os = "windows")]
    {
        win_custody::active_console_session_id()
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// Canonical `S-1-...` SID of the user represented by the session token.
#[cfg(target_os = "windows")]
pub fn session_user_identity(session: u32) -> Result<String, IpcError> {
    let token = win_custody::PrimaryToken::for_session_user(session)
        .map_err(|error| ipc_error(format!("open session {session} user token: {error}")))?;
    token
        .user_sid_string()
        .map_err(|error| ipc_error(format!("read session {session} user SID: {error}")))
}

#[cfg(not(target_os = "windows"))]
pub fn session_user_identity(_session: u32) -> Result<String, IpcError> {
    Err(unsupported("session user identity"))
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
pub struct ConfigWatcher {
    _watcher: notify::RecommendedWatcher,
}

#[cfg(not(target_os = "windows"))]
#[derive(Debug)]
pub struct ConfigWatcher;

/// Watches one configuration file and calls `on_change` after relevant file
/// events. The returned guard must stay alive while watching.
#[cfg(target_os = "windows")]
pub fn watch_config(
    path: &Path,
    mut on_change: impl FnMut() + Send + 'static,
) -> Result<ConfigWatcher, IpcError> {
    use notify::{RecursiveMode, Watcher};

    let watched = path.to_path_buf();
    let parent = path
        .parent()
        .ok_or_else(|| IpcError::new(ErrorCode::InvalidRequest, "config path has no parent"))?
        .to_path_buf();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event
            .as_ref()
            .is_ok_and(|event| event.paths.iter().any(|changed| changed == &watched))
        {
            on_change();
        }
    })
    .map_err(|error| ipc_error(format!("create config watcher: {error}")))?;
    watcher
        .watch(&parent, RecursiveMode::NonRecursive)
        .map_err(|error| ipc_error(format!("watch {}: {error}", parent.display())))?;
    Ok(ConfigWatcher { _watcher: watcher })
}

#[cfg(not(target_os = "windows"))]
pub fn watch_config(
    _path: &Path,
    _on_change: impl FnMut() + Send + 'static,
) -> Result<ConfigWatcher, IpcError> {
    Err(unsupported("managed config watcher"))
}

/// Facts used by the host's pure ProgramData ownership decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateDirectoryFacts {
    pub is_directory: bool,
    pub is_reparse_point: bool,
    pub owner_is_system_or_administrators: bool,
}

/// Facts used to accept an existing protected service executable directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceDirectoryFacts {
    pub is_directory: bool,
    pub is_reparse_point: bool,
    pub owner_is_system_administrators_or_trusted_installer: bool,
}

/// Whether a file or directory carries the Windows reparse-point attribute.
#[cfg(target_os = "windows")]
pub fn is_reparse_point(path: &Path) -> Result<bool, IpcError> {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    std::fs::symlink_metadata(path)
        .map(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .map_err(|error| ipc_error(format!("inspect {}: {error}", path.display())))
}

#[cfg(not(target_os = "windows"))]
pub fn is_reparse_point(_path: &Path) -> Result<bool, IpcError> {
    Err(unsupported("reparse-point inspection"))
}

/// Reads the kind and owner of an existing state directory without following a
/// directory reparse point as an acceptable host root.
#[cfg(target_os = "windows")]
pub fn inspect_state_directory(path: &Path) -> Result<StateDirectoryFacts, IpcError> {
    use std::fs::OpenOptions;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use win_custody::{ObjectSecurityDescriptor, OwnedSid};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    };

    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| ipc_error(format!("inspect {}: {error}", path.display())))?;
    let is_reparse_point = metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    let owner_is_system_or_administrators = if metadata.is_dir() && !is_reparse_point {
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
            .map_err(|error| ipc_error(format!("open {}: {error}", path.display())))?;
        let descriptor = ObjectSecurityDescriptor::query_kernel_object(&directory)
            .map_err(|error| ipc_error(format!("query {} security: {error}", path.display())))?;
        let owner = descriptor
            .owner()
            .map_err(|error| ipc_error(format!("read {} owner: {error}", path.display())))?;
        let system = OwnedSid::from_string("SY")
            .map_err(|error| ipc_error(format!("construct SYSTEM SID: {error}")))?;
        let administrators = OwnedSid::from_string("BA")
            .map_err(|error| ipc_error(format!("construct Administrators SID: {error}")))?;
        owner.matches(&system) || owner.matches(&administrators)
    } else {
        false
    };
    Ok(StateDirectoryFacts {
        is_directory: metadata.is_dir(),
        is_reparse_point,
        owner_is_system_or_administrators,
    })
}

#[cfg(not(target_os = "windows"))]
pub fn inspect_state_directory(_path: &Path) -> Result<StateDirectoryFacts, IpcError> {
    Err(unsupported("state directory inspection"))
}

/// Reads the kind and owner of an existing Program Files service directory.
#[cfg(target_os = "windows")]
pub fn inspect_service_directory(path: &Path) -> Result<ServiceDirectoryFacts, IpcError> {
    use std::fs::OpenOptions;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use win_custody::{ObjectSecurityDescriptor, OwnedSid};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    };

    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| ipc_error(format!("inspect {}: {error}", path.display())))?;
    let is_reparse_point = metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    let owner_is_allowed = if metadata.is_dir() && !is_reparse_point {
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
            .map_err(|error| ipc_error(format!("open {}: {error}", path.display())))?;
        let descriptor = ObjectSecurityDescriptor::query_kernel_object(&directory)
            .map_err(|error| ipc_error(format!("query {} security: {error}", path.display())))?;
        let owner = descriptor
            .owner()
            .map_err(|error| ipc_error(format!("read {} owner: {error}", path.display())))?;
        [
            "SY",
            "BA",
            "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464",
        ]
        .into_iter()
        .map(OwnedSid::from_string)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ipc_error(format!("construct privileged owner SID: {error}")))?
        .iter()
        .any(|allowed| owner.matches(allowed))
    } else {
        false
    };
    Ok(ServiceDirectoryFacts {
        is_directory: metadata.is_dir(),
        is_reparse_point,
        owner_is_system_administrators_or_trusted_installer: owner_is_allowed,
    })
}

#[cfg(not(target_os = "windows"))]
pub fn inspect_service_directory(_path: &Path) -> Result<ServiceDirectoryFacts, IpcError> {
    Err(unsupported("service directory inspection"))
}

/// Applies the protected host-state DACL to a newly created or accepted root.
#[cfg(target_os = "windows")]
pub fn secure_state_directory(path: &Path) -> Result<(), IpcError> {
    let descriptor = win_custody::SecurityDescriptor::from_sddl(
        "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FR;;;BU)",
    )
    .map_err(|error| ipc_error(format!("state directory security descriptor: {error}")))?;
    descriptor
        .apply_to_tree(path, true)
        .map_err(|error| ipc_error(format!("secure {}: {error}", path.display())))
}

#[cfg(not(target_os = "windows"))]
pub fn secure_state_directory(_path: &Path) -> Result<(), IpcError> {
    Err(unsupported("state directory security"))
}

/// Queries the registered Windows service without changing it.
#[cfg(target_os = "windows")]
pub fn service_state() -> ServiceState {
    use windows_service::service::{ServiceAccess, ServiceState as WindowsState};
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    let Ok(manager) = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
    else {
        return ServiceState::Unreachable;
    };
    let service = match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
        Ok(service) => service,
        Err(windows_service::Error::Winapi(error)) if error.raw_os_error() == Some(1060) => {
            return ServiceState::NotInstalled;
        }
        Err(_) => return ServiceState::Unreachable,
    };
    match service.query_status().map(|status| status.current_state) {
        Ok(WindowsState::Running) => ServiceState::Running,
        Ok(_) => ServiceState::Stopped,
        Err(_) => ServiceState::Unreachable,
    }
}

#[cfg(not(target_os = "windows"))]
pub fn service_state() -> ServiceState {
    ServiceState::NotInstalled
}

/// Starts the host executable's privileged installer through the GUI UAC flow.
#[cfg(target_os = "windows")]
pub fn install_service_elevated(executable: &Path) -> Result<(), IpcError> {
    elevated(executable, "install")
}

#[cfg(not(target_os = "windows"))]
pub fn install_service_elevated(_executable: &Path) -> Result<(), IpcError> {
    Err(unsupported("service installation"))
}

/// Starts the host executable's privileged uninstaller through the GUI UAC flow.
#[cfg(target_os = "windows")]
pub fn uninstall_service_elevated(executable: &Path) -> Result<(), IpcError> {
    elevated(executable, "uninstall")
}

#[cfg(not(target_os = "windows"))]
pub fn uninstall_service_elevated(_executable: &Path) -> Result<(), IpcError> {
    Err(unsupported("service removal"))
}

#[cfg(target_os = "windows")]
fn elevated(executable: &Path, argument: &str) -> Result<(), IpcError> {
    let status = runas::Command::new(executable)
        .arg(argument)
        .gui(true)
        .status()
        .map_err(|error| ipc_error(format!("elevated {argument}: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(ipc_error(format!(
            "elevated {argument} exited with {status}"
        )))
    }
}

#[cfg(target_os = "windows")]
const SHELL_AUTOSTART_VALUE: &str = "SECS2RHostShell";
#[cfg(target_os = "windows")]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Builds the per-user Run value used to start the host shell without its window.
#[must_use]
pub fn shell_autostart_value(executable: &Path) -> String {
    format!(r#""{}" --hidden"#, executable.display())
}

/// Enables or disables the current user's host-shell logon start.
#[cfg(target_os = "windows")]
pub fn set_shell_autostart(executable: Option<&Path>) -> Result<(), IpcError> {
    const ERROR_FILE_NOT_FOUND: u32 = 0x8007_0002;

    match executable {
        Some(executable) => {
            let key = windows_registry::CURRENT_USER
                .create(RUN_KEY)
                .map_err(|error| ipc_error(format!("open host-shell Run key: {error}")))?;
            key.set_string(SHELL_AUTOSTART_VALUE, shell_autostart_value(executable))
                .map_err(|error| ipc_error(format!("enable host-shell logon start: {error}")))
        }
        None => {
            let key = match windows_registry::CURRENT_USER.open(RUN_KEY) {
                Ok(key) => key,
                Err(error) if error.code().0 as u32 == ERROR_FILE_NOT_FOUND => return Ok(()),
                Err(error) => {
                    return Err(ipc_error(format!("open host-shell Run key: {error}")));
                }
            };
            match key.remove_value(SHELL_AUTOSTART_VALUE) {
                Ok(()) => Ok(()),
                Err(error) if error.code().0 as u32 == ERROR_FILE_NOT_FOUND => Ok(()),
                Err(error) => Err(ipc_error(format!(
                    "disable host-shell logon start: {error}"
                ))),
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_shell_autostart(_executable: Option<&Path>) -> Result<(), IpcError> {
    Err(unsupported("host-shell logon start"))
}

/// Starts a GUI companion without a shell and returns after process creation.
#[cfg(target_os = "windows")]
pub fn spawn_detached(executable: &Path, args: &[&str]) -> Result<(), IpcError> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    Command::new(executable)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|error| ipc_error(format!("start {}: {error}", executable.display())))
}

#[cfg(not(target_os = "windows"))]
pub fn spawn_detached(_executable: &Path, _args: &[&str]) -> Result<(), IpcError> {
    Err(unsupported("detached process launch"))
}

/// Stops the existing host service before its protected executable is replaced.
/// Returns whether the service was registered.
#[cfg(target_os = "windows")]
pub fn stop_service_for_update() -> Result<bool, IpcError> {
    use windows_service::service::{ServiceAccess, ServiceState as WindowsState};
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|error| ipc_error(format!("open service manager: {error}")))?;
    let service = match manager.open_service(
        SERVICE_NAME,
        ServiceAccess::QUERY_STATUS | ServiceAccess::STOP,
    ) {
        Ok(service) => service,
        Err(windows_service::Error::Winapi(error)) if error.raw_os_error() == Some(1060) => {
            return Ok(false);
        }
        Err(error) => return Err(ipc_error(format!("open {SERVICE_NAME}: {error}"))),
    };
    if service
        .query_status()
        .map_err(|error| ipc_error(format!("query {SERVICE_NAME}: {error}")))?
        .current_state
        != WindowsState::Stopped
    {
        service
            .stop()
            .map_err(|error| ipc_error(format!("stop {SERVICE_NAME}: {error}")))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            if service
                .query_status()
                .map_err(|error| ipc_error(format!("query {SERVICE_NAME}: {error}")))?
                .current_state
                == WindowsState::Stopped
            {
                break;
            }
            if std::time::Instant::now() >= deadline {
                return Err(ipc_error(format!(
                    "stop {SERVICE_NAME}: timed out after 30 seconds"
                )));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Ok(true)
}

#[cfg(not(target_os = "windows"))]
pub fn stop_service_for_update() -> Result<bool, IpcError> {
    Err(unsupported("service update"))
}

/// Registers or updates an automatic LocalSystem service. The caller must already be elevated.
#[cfg(target_os = "windows")]
pub fn register_service(executable: &Path, service_exists: bool) -> Result<(), IpcError> {
    use std::ffi::{OsStr, OsString};
    use windows_service::service::{
        ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceType,
    };
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )
    .map_err(|error| ipc_error(format!("open service manager: {error}")))?;
    let info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(SERVICE_DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: executable.to_path_buf(),
        launch_arguments: vec![OsString::from("run")],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };
    let service = if service_exists {
        let service = manager
            .open_service(
                SERVICE_NAME,
                ServiceAccess::CHANGE_CONFIG | ServiceAccess::START,
            )
            .map_err(|error| ipc_error(format!("open {SERVICE_NAME}: {error}")))?;
        service
            .change_config(&info)
            .map_err(|error| ipc_error(format!("update {SERVICE_NAME}: {error}")))?;
        service
    } else {
        manager
            .create_service(&info, ServiceAccess::CHANGE_CONFIG | ServiceAccess::START)
            .map_err(|error| ipc_error(format!("register {SERVICE_NAME}: {error}")))?
    };
    service
        .set_description("Keeps managed CamillaDSP and Equalizer APO configurations active")
        .map_err(|error| ipc_error(format!("describe {SERVICE_NAME}: {error}")))?;
    service
        .start::<&OsStr>(&[])
        .map_err(|error| ipc_error(format!("start {SERVICE_NAME}: {error}")))
}

#[cfg(not(target_os = "windows"))]
pub fn register_service(_executable: &Path, _service_exists: bool) -> Result<(), IpcError> {
    Err(unsupported("service registration"))
}

/// Stops and deletes the service. The caller must already be elevated.
#[cfg(target_os = "windows")]
pub fn unregister_service() -> Result<(), IpcError> {
    use windows_service::service::ServiceAccess;
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|error| ipc_error(format!("open service manager: {error}")))?;
    let service = match manager.open_service(
        SERVICE_NAME,
        ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE,
    ) {
        Ok(service) => service,
        Err(windows_service::Error::Winapi(error)) if error.raw_os_error() == Some(1060) => {
            return Ok(());
        }
        Err(error) => return Err(ipc_error(format!("open {SERVICE_NAME}: {error}"))),
    };
    if service
        .query_status()
        .map_err(|error| ipc_error(format!("query {SERVICE_NAME}: {error}")))?
        .current_state
        != windows_service::service::ServiceState::Stopped
    {
        service
            .stop()
            .map_err(|error| ipc_error(format!("stop {SERVICE_NAME}: {error}")))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            if service
                .query_status()
                .map_err(|error| ipc_error(format!("query {SERVICE_NAME}: {error}")))?
                .current_state
                == windows_service::service::ServiceState::Stopped
            {
                break;
            }
            if std::time::Instant::now() >= deadline {
                return Err(ipc_error(format!(
                    "stop {SERVICE_NAME}: timed out after 30 seconds"
                )));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    service
        .delete()
        .map_err(|error| ipc_error(format!("delete {SERVICE_NAME}: {error}")))
}

#[cfg(not(target_os = "windows"))]
pub fn unregister_service() -> Result<(), IpcError> {
    Err(unsupported("service removal"))
}

/// Events delivered by the Windows Service Control Manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceEvent {
    Stop,
    SessionLogon(u32),
    SessionLogoff(u32),
}

#[cfg(target_os = "windows")]
mod service_dispatch {
    use std::ffi::OsString;
    use std::sync::{Mutex, OnceLock, mpsc};
    use std::time::Duration;

    use windows_service::define_windows_service;
    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType, SessionChangeReason,
    };
    use windows_service::service_control_handler::{
        self, ServiceControlHandlerResult, ServiceStatusHandle,
    };

    use super::{SERVICE_NAME, ServiceEvent};

    type Worker = Box<dyn FnOnce(mpsc::Receiver<ServiceEvent>) + Send + 'static>;
    static WORKER: OnceLock<Mutex<Option<Worker>>> = OnceLock::new();

    define_windows_service!(ffi_service_main, service_main);

    pub fn run(worker: Worker) -> windows_service::Result<()> {
        let slot = WORKER.get_or_init(|| Mutex::new(None));
        *slot.lock().unwrap_or_else(|error| error.into_inner()) = Some(worker);
        windows_service::service_dispatcher::start(SERVICE_NAME, ffi_service_main)
    }

    fn service_main(_arguments: Vec<OsString>) {
        let _ = run_inner();
    }

    fn status(handle: &ServiceStatusHandle, state: ServiceState) -> windows_service::Result<()> {
        handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: if state == ServiceState::Running {
                ServiceControlAccept::STOP | ServiceControlAccept::SESSION_CHANGE
            } else {
                ServiceControlAccept::empty()
            },
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::ZERO,
            process_id: None,
        })
    }

    fn run_inner() -> windows_service::Result<()> {
        let (sender, receiver) = mpsc::channel();
        let event_sender = sender.clone();
        let handler = move |control| match control {
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            ServiceControl::Stop => {
                let _ = event_sender.send(ServiceEvent::Stop);
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::SessionChange(change) => {
                let event = match change.reason {
                    SessionChangeReason::SessionLogon => {
                        Some(ServiceEvent::SessionLogon(change.notification.session_id))
                    }
                    SessionChangeReason::SessionLogoff => {
                        Some(ServiceEvent::SessionLogoff(change.notification.session_id))
                    }
                    _ => None,
                };
                if let Some(event) = event {
                    let _ = event_sender.send(event);
                }
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        };
        let status_handle = service_control_handler::register(SERVICE_NAME, handler)?;
        status(&status_handle, ServiceState::Running)?;
        if let Some(worker) = WORKER.get().and_then(|worker| worker.lock().ok()?.take()) {
            worker(receiver);
        }
        status(&status_handle, ServiceState::Stopped)
    }
}

/// Runs a closure as the Windows service body. The closure returns only after a
/// stop event and receives logon/logoff notifications through the channel.
#[cfg(target_os = "windows")]
pub fn run_service(
    worker: impl FnOnce(std::sync::mpsc::Receiver<ServiceEvent>) + Send + 'static,
) -> Result<(), IpcError> {
    service_dispatch::run(Box::new(worker))
        .map_err(|error| ipc_error(format!("run {SERVICE_NAME}: {error}")))
}

#[cfg(not(target_os = "windows"))]
pub fn run_service(
    _worker: impl FnOnce(std::sync::mpsc::Receiver<ServiceEvent>) + Send + 'static,
) -> Result<(), IpcError> {
    Err(unsupported("Windows service dispatcher"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_autostart_value_quotes_the_executable_and_hides_the_window() {
        assert_eq!(
            shell_autostart_value(Path::new(r"C:\Program Files\SECS 2R\secs2r-host-shell.exe")),
            r#""C:\Program Files\SECS 2R\secs2r-host-shell.exe" --hidden"#
        );
    }

    #[test]
    fn framing_round_trips_and_rejects_bad_lengths() {
        let request = HostRequest::Log { lines: 25 };
        let encoded = frame(&request).unwrap();
        assert_eq!(parse_frame::<HostRequest>(&encoded).unwrap(), request);

        let mut too_large = ((MAX_MESSAGE_BYTES + 1) as u32).to_le_bytes().to_vec();
        too_large.extend_from_slice(b"{}");
        assert_eq!(
            parse_frame::<HostRequest>(&too_large).unwrap_err().code,
            ErrorCode::InvalidRequest
        );
        assert_eq!(
            parse_frame::<HostRequest>(&[2, 0, 0, 0, b'{'])
                .unwrap_err()
                .code,
            ErrorCode::InvalidRequest
        );
    }

    #[test]
    fn fake_transport_handles_partial_reads_and_deadlines() {
        let encoded = frame(&HostRequest::Status).unwrap();
        let mut offset = 0;
        let received = read_frame(|destination| {
            if offset == encoded.len() {
                return Ok(0);
            }
            destination[0] = encoded[offset];
            offset += 1;
            Ok(1)
        })
        .unwrap();
        assert_eq!(
            parse_frame::<HostRequest>(&received).unwrap(),
            HostRequest::Status
        );

        let error = read_frame(|_| Err(ipc_error("fake transport deadline"))).unwrap_err();
        assert!(error.detail.contains("deadline"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn real_pipe_round_trip_uses_a_unique_name() {
        let name = format!(
            r"\\.\pipe\secs2r-host-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let server_name = name.clone();
        let server = std::thread::spawn(move || {
            pipe_server(&server_name, PIPE_TIMEOUT, |_, _| HostResponse::Ok)
        });
        assert_eq!(
            pipe_call_at(&name, &HostRequest::Status).unwrap(),
            HostResponse::Ok
        );
        assert!(server.join().unwrap().unwrap());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn pipe_deadline_is_reported_without_a_server() {
        let name = format!(
            r"\\.\pipe\secs2r-host-missing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let started = std::time::Instant::now();
        assert!(pipe_call_at(&name, &HostRequest::Status).is_err());
        assert!(started.elapsed() < Duration::from_secs(7));
    }
}
