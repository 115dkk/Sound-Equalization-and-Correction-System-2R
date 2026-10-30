use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::Instant;

use secs_apply::supervise::{Decision, RestartPolicy, Supervisor};
use secs_types::ipc::{
    ErrorCode, HostLogLine, HostManaged, HostRequest, HostResponse, HostStatus, IncludeState,
    IpcError, ManagedProcess, ManagedProcessState,
};

pub const LOG_LIMIT: u64 = 256 * 1024;
pub const INSTALLED_HOST_DIRECTORY: &str = "SECS 2R Host";
pub const INSTALLED_HOST_EXECUTABLE: &str = "secs2r-host.exe";
pub const STAGED_HOST_EXECUTABLE: &str = ".secs2r-host.exe.tmp";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostInstallPaths {
    pub directory: std::path::PathBuf,
    pub executable: std::path::PathBuf,
    pub staged: std::path::PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallAction {
    Register,
    ReplaceAndUpdate,
}

pub fn host_install_paths(program_files: &Path) -> HostInstallPaths {
    let directory = program_files.join(INSTALLED_HOST_DIRECTORY);
    HostInstallPaths {
        executable: directory.join(INSTALLED_HOST_EXECUTABLE),
        staged: directory.join(STAGED_HOST_EXECUTABLE),
        directory,
    }
}

pub fn install_action(service_exists: bool) -> InstallAction {
    if service_exists {
        InstallAction::ReplaceAndUpdate
    } else {
        InstallAction::Register
    }
}

pub fn replace_host_executable(source: &Path, paths: &HostInstallPaths) -> std::io::Result<()> {
    if source == paths.executable {
        return Ok(());
    }
    match fs::remove_file(&paths.staged) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut input = fs::File::open(source)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&paths.staged)?;
    std::io::copy(&mut input, &mut output)?;
    output.sync_all()?;
    drop(output);
    match fs::remove_file(&paths.executable) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::rename(&paths.staged, &paths.executable)
}

pub fn remove_host_copy(paths: &HostInstallPaths) -> std::io::Result<()> {
    for path in [&paths.staged, &paths.executable] {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    match fs::remove_dir(&paths.directory) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

pub fn eqapo_path_allowed(requested: &Path, detected: &Path) -> bool {
    fn normalized(path: &Path) -> String {
        path.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_ascii_lowercase()
    }
    normalized(requested) == normalized(detected)
}

pub fn client_session_allowed(active_session: Option<u32>, client_session: u32) -> bool {
    active_session == Some(client_session)
}

/// Whether the console user is the one who asked for management. Session IDs
/// change at every logon, so only the SID decides.
pub fn managed_user_allowed(owner_user_sid: Option<&str>, active_user_sid: &str) -> bool {
    owner_user_sid == Some(active_user_sid)
}

pub fn state_directory_allowed(
    is_directory: bool,
    is_reparse_point: bool,
    owner_is_system_or_administrators: bool,
) -> bool {
    is_directory && !is_reparse_point && owner_is_system_or_administrators
}

pub fn service_directory_allowed(
    is_directory: bool,
    is_reparse_point: bool,
    owner_is_system_administrators_or_trusted_installer: bool,
) -> bool {
    is_directory && !is_reparse_point && owner_is_system_administrators_or_trusted_installer
}

#[derive(Debug)]
pub struct Core {
    version: String,
    pub managed: HostManaged,
    pub camilla: ManagedProcess,
    pub eqapo_include: IncludeState,
    pub last_repair: Option<String>,
    logs: Vec<HostLogLine>,
}

impl Core {
    pub fn new(version: impl Into<String>, managed: HostManaged) -> Self {
        Self {
            version: version.into(),
            managed,
            camilla: ManagedProcess::default(),
            eqapo_include: IncludeState::Absent,
            last_repair: None,
            logs: Vec::new(),
        }
    }

    pub fn status(&self) -> HostStatus {
        HostStatus {
            version: self.version.clone(),
            managed: self.managed.clone(),
            camilla: self.camilla.clone(),
            eqapo_include: self.eqapo_include,
            last_repair: self.last_repair.clone(),
        }
    }

    pub fn handle(&mut self, request: HostRequest) -> HostResponse {
        match request {
            HostRequest::Status => HostResponse::Status(Box::new(self.status())),
            HostRequest::SetManaged(update) => {
                if update.camilla.is_some() {
                    self.managed.camilla = update.camilla;
                }
                if update.eqapo.is_some() {
                    self.managed.eqapo = update.eqapo;
                }
                if update.owner_session_id.is_some() {
                    self.managed.owner_session_id = update.owner_session_id;
                }
                if update.owner_user_sid.is_some() {
                    self.managed.owner_user_sid = update.owner_user_sid;
                }
                HostResponse::Ok
            }
            HostRequest::StopManaging => {
                self.managed = HostManaged::default();
                self.camilla = ManagedProcess::default();
                self.eqapo_include = IncludeState::Absent;
                HostResponse::Ok
            }
            HostRequest::Log { lines } => {
                let keep = usize::try_from(lines).unwrap_or(usize::MAX);
                let start = self.logs.len().saturating_sub(keep);
                HostResponse::Log(self.logs[start..].to_vec())
            }
        }
    }

    pub fn record(&mut self, line: HostLogLine) {
        self.logs.push(line);
    }
}

pub fn load_state(path: &Path) -> Result<HostManaged, IpcError> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
            IpcError::new(
                ErrorCode::ApplyFailed,
                format!("read {}: {error}", path.display()),
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(HostManaged::default()),
        Err(error) => Err(IpcError::new(
            ErrorCode::ApplyFailed,
            format!("read {}: {error}", path.display()),
        )),
    }
}

pub fn save_state(path: &Path, managed: &HostManaged) -> Result<(), IpcError> {
    let bytes = serde_json::to_vec_pretty(managed)
        .map_err(|error| IpcError::new(ErrorCode::Internal, error.to_string()))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            IpcError::new(
                ErrorCode::ApplyFailed,
                format!("create {}: {error}", parent.display()),
            )
        })?;
    }
    secs_types::fs::write_atomic_bytes(path, &bytes).map_err(|error| {
        IpcError::new(
            ErrorCode::ApplyFailed,
            format!("write {}: {error}", path.display()),
        )
    })
}

/// Returns repaired text only when the marker pair is absent or damaged.
pub fn repair_text(config: &str) -> Option<String> {
    secs_apply::eqapo::with_include(config)
}

pub fn load_logs(path: &Path, maximum: usize) -> Vec<HostLogLine> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut lines = text
        .lines()
        .filter_map(|line| serde_json::from_str::<HostLogLine>(line).ok())
        .collect::<Vec<_>>();
    if lines.len() > maximum {
        lines.drain(..lines.len() - maximum);
    }
    lines
}

pub fn append_log(path: &Path, line: &HostLogLine, limit: u64) -> Result<(), IpcError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            IpcError::new(
                ErrorCode::ApplyFailed,
                format!("create {}: {error}", parent.display()),
            )
        })?;
    }
    let encoded = serde_json::to_vec(line)
        .map_err(|error| IpcError::new(ErrorCode::Internal, error.to_string()))?;
    let incoming = u64::try_from(encoded.len().saturating_add(1)).unwrap_or(u64::MAX);
    let existing = fs::metadata(path).map_or(0, |metadata| metadata.len());
    if existing.saturating_add(incoming) > limit && existing != 0 {
        let rotated = path.with_extension("log.1");
        match fs::remove_file(&rotated) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(IpcError::new(
                    ErrorCode::ApplyFailed,
                    format!("remove {}: {error}", rotated.display()),
                ));
            }
        }
        fs::rename(path, &rotated).map_err(|error| {
            IpcError::new(
                ErrorCode::ApplyFailed,
                format!("rotate {}: {error}", path.display()),
            )
        })?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| {
            IpcError::new(
                ErrorCode::ApplyFailed,
                format!("open {}: {error}", path.display()),
            )
        })?;
    file.write_all(&encoded)
        .and_then(|()| file.write_all(b"\n"))
        .map_err(|error| {
            IpcError::new(
                ErrorCode::ApplyFailed,
                format!("append {}: {error}", path.display()),
            )
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortObservation {
    Empty,
    CamillaDsp,
    OtherProcess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TakeoverDecision {
    KeepOwnChild,
    Launch,
    RequestExit,
    PortBusy,
}

pub fn takeover_decision(
    own_child: bool,
    observation: Option<PortObservation>,
) -> TakeoverDecision {
    if own_child {
        return TakeoverDecision::KeepOwnChild;
    }
    match observation {
        Some(PortObservation::Empty) => TakeoverDecision::Launch,
        Some(PortObservation::CamillaDsp) => TakeoverDecision::RequestExit,
        Some(PortObservation::OtherProcess) | None => TakeoverDecision::PortBusy,
    }
}

pub fn after_exit_decision(still_answering: bool) -> TakeoverDecision {
    if still_answering {
        TakeoverDecision::PortBusy
    } else {
        TakeoverDecision::Launch
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessEvent {
    Exit,
    Logon,
    Logoff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessAction {
    Launch,
    Stop,
    RestartAfter(std::time::Duration),
    GiveUp,
    None,
}

#[derive(Debug)]
pub struct ProcessRules {
    supervisor: Supervisor,
    has_session: bool,
    running: bool,
}

impl ProcessRules {
    pub fn new(has_session: bool) -> Self {
        Self {
            supervisor: Supervisor::new(RestartPolicy::default()),
            has_session,
            running: false,
        }
    }

    pub fn launched(&mut self) {
        self.running = true;
    }

    pub fn apply(&mut self, event: ProcessEvent, now: Instant) -> ProcessAction {
        match event {
            ProcessEvent::Logoff => {
                self.has_session = false;
                if std::mem::replace(&mut self.running, false) {
                    ProcessAction::Stop
                } else {
                    ProcessAction::None
                }
            }
            ProcessEvent::Logon => {
                self.has_session = true;
                self.supervisor.on_stable();
                if self.running {
                    ProcessAction::None
                } else {
                    ProcessAction::Launch
                }
            }
            ProcessEvent::Exit => {
                self.running = false;
                if !self.has_session {
                    return ProcessAction::None;
                }
                match self.supervisor.on_exit(now) {
                    Decision::RestartAfter(delay) => ProcessAction::RestartAfter(delay),
                    Decision::GiveUp => ProcessAction::GiveUp,
                }
            }
        }
    }
}

pub fn mark_other_user(process: &mut ManagedProcess) -> bool {
    process.pid = None;
    if process.state == ManagedProcessState::OtherUser {
        false
    } else {
        process.state = ManagedProcessState::OtherUser;
        true
    }
}

pub fn given_up_line(reason: &str) -> HostLogLine {
    log_line("given_up", [("reason".to_owned(), reason.to_owned())])
}

pub fn log_line(event: &str, fields: impl IntoIterator<Item = (String, String)>) -> HostLogLine {
    HostLogLine {
        timestamp: chrono::Utc::now().to_rfc3339(),
        event: event.to_owned(),
        fields: fields.into_iter().collect::<BTreeMap<_, _>>(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    use secs_types::ipc::{ManagedCamilla, ManagedEqApo};

    use super::*;

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn root(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "secs2r-host-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn managed() -> HostManaged {
        HostManaged {
            camilla: Some(ManagedCamilla {
                exe_path: "camilladsp.exe".into(),
                config_path: "room.yml".into(),
                port: 1234,
                design_id: "camilla-design".into(),
            }),
            eqapo: Some(ManagedEqApo {
                config_path: "apo".into(),
                design_id: "apo-design".into(),
            }),
            owner_session_id: Some(7),
            owner_user_sid: Some("sid-a".into()),
        }
    }

    #[test]
    fn state_file_round_trip_missing_and_corrupt_handling() {
        let root = root("state");
        let path = root.join("host.json");
        assert_eq!(load_state(&path).unwrap(), HostManaged::default());
        save_state(&path, &managed()).unwrap();
        assert_eq!(load_state(&path).unwrap(), managed());
        fs::write(&path, b"{not json").unwrap();
        assert_eq!(load_state(&path).unwrap_err().code, ErrorCode::ApplyFailed);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn request_handling_merges_targets_and_stop_clears_them() {
        let mut core = Core::new("1", HostManaged::default());
        let value = managed();
        assert_eq!(
            core.handle(HostRequest::SetManaged(HostManaged {
                camilla: value.camilla.clone(),
                ..HostManaged::default()
            })),
            HostResponse::Ok
        );
        core.handle(HostRequest::SetManaged(HostManaged {
            eqapo: value.eqapo.clone(),
            owner_session_id: value.owner_session_id,
            owner_user_sid: value.owner_user_sid.clone(),
            ..HostManaged::default()
        }));
        assert_eq!(core.status().managed, value);
        core.handle(HostRequest::StopManaging);
        assert_eq!(core.status().managed, HostManaged::default());
    }

    #[test]
    fn takeover_exits_foreign_camilla_backs_off_busy_ports_and_skips_own_child() {
        assert_eq!(
            takeover_decision(false, Some(PortObservation::CamillaDsp)),
            TakeoverDecision::RequestExit
        );
        assert_eq!(after_exit_decision(false), TakeoverDecision::Launch);
        assert_eq!(after_exit_decision(true), TakeoverDecision::PortBusy);
        assert_eq!(
            takeover_decision(false, Some(PortObservation::OtherProcess)),
            TakeoverDecision::PortBusy
        );
        assert_eq!(
            takeover_decision(true, None),
            TakeoverDecision::KeepOwnChild
        );

        let now = Instant::now();
        let mut process = ProcessRules::new(true);
        assert_eq!(
            process.apply(ProcessEvent::Exit, now),
            ProcessAction::RestartAfter(Duration::from_secs(1))
        );
    }

    #[test]
    fn exit_logon_and_logoff_choose_supervision_actions() {
        let start = Instant::now();
        let mut rules = ProcessRules::new(true);
        assert_eq!(
            rules.apply(ProcessEvent::Logon, start),
            ProcessAction::Launch
        );
        rules.launched();
        assert_eq!(
            rules.apply(ProcessEvent::Exit, start),
            ProcessAction::RestartAfter(Duration::from_secs(1))
        );
        rules.launched();
        assert_eq!(
            rules.apply(ProcessEvent::Logoff, start),
            ProcessAction::Stop
        );
        assert_eq!(rules.apply(ProcessEvent::Exit, start), ProcessAction::None);
        assert_eq!(
            rules.apply(ProcessEvent::Logon, start),
            ProcessAction::Launch
        );
    }

    #[test]
    fn file_change_repairs_only_absent_or_damaged_pairs() {
        let absent = "Preamp: -3 dB\n";
        let repaired = repair_text(absent).unwrap();
        assert_eq!(
            secs_apply::eqapo::include_state(&repaired),
            secs_apply::eqapo::IncludeState::Present
        );
        assert!(repair_text(&repaired).is_none());
        let damaged = format!("{}\nwrong\n", secs_apply::eqapo::MARKER);
        assert!(repair_text(&damaged).is_some());
    }

    #[test]
    fn install_paths_stage_then_replace_and_reinstall_updates() {
        let root = root("install-copy");
        let paths = host_install_paths(&root);
        assert_eq!(paths.directory, root.join("SECS 2R Host"));
        assert_eq!(paths.executable, paths.directory.join("secs2r-host.exe"));
        assert_eq!(paths.staged, paths.directory.join(".secs2r-host.exe.tmp"));
        assert_eq!(install_action(false), InstallAction::Register);
        assert_eq!(install_action(true), InstallAction::ReplaceAndUpdate);

        fs::create_dir(&paths.directory).unwrap();
        let source = root.join("source.exe");
        fs::write(&source, b"first").unwrap();
        replace_host_executable(&source, &paths).unwrap();
        assert_eq!(fs::read(&paths.executable).unwrap(), b"first");
        assert!(!paths.staged.exists());

        fs::write(&source, b"second").unwrap();
        replace_host_executable(&source, &paths).unwrap();
        assert_eq!(fs::read(&paths.executable).unwrap(), b"second");
        assert!(!paths.staged.exists());
        remove_host_copy(&paths).unwrap();
        assert!(!paths.directory.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn privileged_boundaries_accept_only_the_detected_path_owner_and_user() {
        assert!(eqapo_path_allowed(
            Path::new(r"C:\Program Files\EqualizerAPO\config\"),
            Path::new(r"c:/program files/equalizerapo/config")
        ));
        assert!(!eqapo_path_allowed(
            Path::new(r"C:\Windows\System32"),
            Path::new(r"C:\Program Files\EqualizerAPO\config")
        ));

        assert!(client_session_allowed(Some(7), 7));
        assert!(!client_session_allowed(Some(8), 7));
        assert!(!client_session_allowed(None, 7));
        assert!(managed_user_allowed(Some("sid-a"), "sid-a"));
        assert!(!managed_user_allowed(Some("sid-a"), "sid-b"));
        assert!(!managed_user_allowed(None, "sid-a"));

        assert!(state_directory_allowed(true, false, true));
        assert!(!state_directory_allowed(true, true, true));
        assert!(!state_directory_allowed(false, false, true));
        assert!(!state_directory_allowed(true, false, false));
        assert!(service_directory_allowed(true, false, true));
        assert!(!service_directory_allowed(true, true, true));
        assert!(!service_directory_allowed(false, false, true));
        assert!(!service_directory_allowed(true, false, false));
    }

    #[test]
    fn other_user_changes_state_once_and_given_up_log_names_the_reason() {
        let mut process = ManagedProcess {
            state: ManagedProcessState::Running,
            pid: Some(42),
            restarts_in_window: 0,
        };
        assert!(mark_other_user(&mut process));
        assert_eq!(process.state, ManagedProcessState::OtherUser);
        assert_eq!(process.pid, None);
        assert!(!mark_other_user(&mut process));

        let line = given_up_line("port_busy");
        assert_eq!(line.event, "given_up");
        assert_eq!(
            line.fields.get("reason").map(String::as_str),
            Some("port_busy")
        );
    }

    #[test]
    fn log_rotates_once_at_the_size_cap() {
        let root = root("log");
        let path = root.join("host.log");
        fs::write(&path, vec![b'x'; 64]).unwrap();
        append_log(&path, &log_line("launch", []), 64).unwrap();
        assert!(root.join("host.log.1").is_file());
        assert!(path.is_file());
        append_log(&path, &log_line("exit", []), 64).unwrap();
        assert!(root.join("host.log.1").is_file());
        fs::remove_dir_all(root).unwrap();
    }
}
