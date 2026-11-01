use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use base64::Engine;
use secs_apply::supervise::{Decision, RestartPolicy, Supervisor};
use secs_types::ipc::{
    ErrorCode, HostLogLine, HostManaged, HostRequest, HostResponse, HostStatus, IncludeState,
    IpcError, ManagedProcess, ManagedProcessState,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const LOG_LIMIT: u64 = 256 * 1024;
pub const INSTALLED_HOST_DIRECTORY: &str = "SECS 2R Host";
pub const INSTALLED_HOST_EXECUTABLE: &str = "secs2r-host.exe";
pub const STAGED_HOST_EXECUTABLE: &str = ".secs2r-host.exe.tmp";
pub const UPDATE_MANIFEST: &str = "secs2r-host.json";
pub const UPDATE_SIGNATURE: &str = "secs2r-host.json.sig";
pub const UPDATE_STAGED_EXECUTABLE: &str = "secs2r-host.new.exe";
pub const UPDATE_OLD_EXECUTABLE: &str = "secs2r-host.old.exe";
pub const MANIFEST_LIMIT: u64 = 64 * 1024;
pub const HOST_EXECUTABLE_LIMIT: u64 = 64 * 1024 * 1024;

/// Base64 of the complete minisign public-key file used by release builds.
pub const UPDATER_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IENCRThGOTI4NTc2RUYyRUUKUldUdThtNVhLUG5veTJ0eHM0dzZ4V1ArNlNRWjRJbzJrVE96NGpESXZIY1VtVHg2SWp1Qk85QloK";

/// PR builds use a temporary signing key because they cannot read the
/// `updater-signing` environment. `ci.yml` supplies its matching public key
/// through this variable; `release.yml` does not set it and uses the checked-in key.
pub fn updater_pubkey() -> &'static str {
    match option_env!("SECS2R_UPDATER_PUBKEY") {
        Some(key) if !key.is_empty() => key,
        _ => UPDATER_PUBKEY,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostManifest {
    pub version: String,
    pub sha256: String,
    pub file: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateReject {
    BadSignature,
    UnsignedVersion,
    BadManifest(String),
    NotNewer { own: String, offered: String },
    HashMismatch,
    TooLarge,
    Io(String),
}

impl UpdateReject {
    pub fn log_reason(&self) -> &'static str {
        match self {
            Self::BadSignature => "bad_signature",
            Self::UnsignedVersion => "unsigned_version",
            Self::BadManifest(_) => "bad_manifest",
            Self::NotNewer { .. } => "not_newer",
            Self::HashMismatch => "hash_mismatch",
            Self::TooLarge => "too_large",
            Self::Io(_) => "io",
        }
    }

    pub fn detail(&self) -> String {
        match self {
            Self::BadSignature => "the host update manifest signature is invalid".into(),
            Self::UnsignedVersion => {
                "the signed version does not match the host update manifest".into()
            }
            Self::BadManifest(detail) => format!("invalid host update manifest: {detail}"),
            Self::NotNewer { own, offered } => {
                format!("host update {offered} is not newer than {own}")
            }
            Self::HashMismatch => "the staged host executable hash does not match".into(),
            Self::TooLarge => "the host update file exceeds its size limit".into(),
            Self::Io(detail) => format!("host update I/O failed: {detail}"),
        }
    }
}

pub fn verify_manifest(
    pubkey: &str,
    manifest_bytes: &[u8],
    signature_text: &str,
) -> Result<HostManifest, UpdateReject> {
    let decoded_key = base64::engine::general_purpose::STANDARD
        .decode(pubkey.trim())
        .map_err(|_| UpdateReject::BadSignature)?;
    let decoded_key = std::str::from_utf8(&decoded_key).map_err(|_| UpdateReject::BadSignature)?;
    let key_line = decoded_key
        .lines()
        .nth(1)
        .ok_or(UpdateReject::BadSignature)?;
    let public_key = minisign_verify::PublicKey::from_base64(key_line)
        .map_err(|_| UpdateReject::BadSignature)?;

    let decoded_signature = base64::engine::general_purpose::STANDARD
        .decode(signature_text.trim())
        .map_err(|_| UpdateReject::BadSignature)?;
    let decoded_signature =
        std::str::from_utf8(&decoded_signature).map_err(|_| UpdateReject::BadSignature)?;
    let signature = minisign_verify::Signature::decode(decoded_signature)
        .map_err(|_| UpdateReject::BadSignature)?;
    public_key
        .verify(manifest_bytes, &signature, true)
        .map_err(|_| UpdateReject::BadSignature)?;

    let manifest = serde_json::from_slice::<HostManifest>(manifest_bytes)
        .map_err(|error| UpdateReject::BadManifest(error.to_string()))?;
    let offered = semver::Version::parse(&manifest.version)
        .map_err(|error| UpdateReject::BadManifest(error.to_string()))?;
    if manifest.file != INSTALLED_HOST_EXECUTABLE {
        return Err(UpdateReject::BadManifest(
            "file must be secs2r-host.exe".into(),
        ));
    }
    if manifest.sha256.len() != 64
        || !manifest
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(UpdateReject::BadManifest(
            "sha256 must be 64 lowercase hexadecimal characters".into(),
        ));
    }

    let signed_version = signature
        .trusted_comment()
        .split('\t')
        .find_map(|field| field.strip_prefix("version:"))
        .and_then(|version| semver::Version::parse(version.trim_start_matches('v')).ok());
    if signed_version.as_ref() != Some(&offered) {
        return Err(UpdateReject::UnsignedVersion);
    }

    Ok(manifest)
}

pub fn accept_update(
    own: &str,
    manifest: &HostManifest,
    staged_sha256: &str,
) -> Result<(), UpdateReject> {
    let own_version = semver::Version::parse(own)
        .map_err(|error| UpdateReject::BadManifest(format!("running version: {error}")))?;
    let offered = semver::Version::parse(&manifest.version)
        .map_err(|error| UpdateReject::BadManifest(error.to_string()))?;
    if offered.cmp_precedence(&own_version).is_le() {
        return Err(UpdateReject::NotNewer {
            own: own.to_owned(),
            offered: manifest.version.clone(),
        });
    }
    if staged_sha256 != manifest.sha256 {
        return Err(UpdateReject::HashMismatch);
    }
    Ok(())
}

fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_type().is_symlink()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(target_os = "windows"))]
    {
        metadata.file_type().is_symlink()
    }
}

/// Checks the folder a caller names for `update_self`: a local folder on a
/// drive, never a UNC share, device path, link, or junction which the service
/// (SYSTEM) would otherwise open on the caller's behalf.
pub fn validate_update_source(from_dir: &Path) -> Result<(), UpdateReject> {
    let text = from_dir.to_string_lossy();
    let bytes = text.as_bytes();
    let drive_absolute = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    if !drive_absolute {
        return Err(UpdateReject::Io(format!(
            "the update folder must be a local absolute path: {text}"
        )));
    }

    for component in from_dir.ancestors().collect::<Vec<_>>().into_iter().rev() {
        match fs::symlink_metadata(component) {
            Ok(metadata) if metadata_is_link_or_reparse(&metadata) => {
                return Err(UpdateReject::Io(format!(
                    "the update folder must not contain a link or junction: {}",
                    component.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(UpdateReject::Io(format!(
                    "inspect update folder component {}: {error}",
                    component.display()
                )));
            }
        }
    }
    // This check and the later opens are not atomic. The active console user
    // can replace a component between them, but every installed executable is
    // still covered by the signed manifest and verified hash (audit #155 TD-135).
    Ok(())
}

/// Reads a regular file of at most `limit` bytes. The service reads these for
/// a caller, so a directory, a pipe or a device is refused before it is opened.
pub fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, UpdateReject> {
    let metadata = fs::metadata(path).map_err(|error| UpdateReject::Io(error.to_string()))?;
    if !metadata.is_file() {
        return Err(UpdateReject::Io(format!(
            "{} is not a regular file",
            path.display()
        )));
    }
    if metadata.len() > limit {
        return Err(UpdateReject::TooLarge);
    }
    let file = fs::File::open(path).map_err(|error| UpdateReject::Io(error.to_string()))?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| UpdateReject::Io(error.to_string()))?;
    if bytes.len() as u64 > limit {
        Err(UpdateReject::TooLarge)
    } else {
        Ok(bytes)
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push(char::from(DIGITS[usize::from(byte >> 4)]));
        hex.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    hex
}

/// Writes the staged executable. Called only with bytes whose hash already
/// matched the signed manifest: the protected folder inherits read access for
/// users, so nothing unverified may be written there even for a moment.
pub fn write_staged(destination: &Path, bytes: &[u8]) -> Result<(), UpdateReject> {
    match fs::remove_file(destination) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(UpdateReject::Io(error.to_string())),
    }
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(|error| UpdateReject::Io(error.to_string()))?;
        file.write_all(bytes)
            .map_err(|error| UpdateReject::Io(error.to_string()))?;
        file.sync_all()
            .map_err(|error| UpdateReject::Io(error.to_string()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostUpdatePaths {
    pub executable: PathBuf,
    pub staged: PathBuf,
    pub old: PathBuf,
}

pub fn host_update_paths(directory: &Path) -> HostUpdatePaths {
    HostUpdatePaths {
        executable: directory.join(INSTALLED_HOST_EXECUTABLE),
        staged: directory.join(UPDATE_STAGED_EXECUTABLE),
        old: directory.join(UPDATE_OLD_EXECUTABLE),
    }
}

pub trait UpdateFileOps {
    fn remove_file(&mut self, path: &Path) -> std::io::Result<()>;
    fn rename(&mut self, from: &Path, to: &Path) -> std::io::Result<()>;
    fn wait_before_retry(&mut self, duration: Duration);
}

pub struct RealUpdateFileOps;

impl UpdateFileOps for RealUpdateFileOps {
    fn remove_file(&mut self, path: &Path) -> std::io::Result<()> {
        fs::remove_file(path)
    }

    fn rename(&mut self, from: &Path, to: &Path) -> std::io::Result<()> {
        fs::rename(from, to)
    }

    fn wait_before_retry(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

fn rename_with_retry(
    files: &mut impl UpdateFileOps,
    from: &Path,
    to: &Path,
) -> std::io::Result<()> {
    const RETRIES: usize = 50;
    for attempt in 0..=RETRIES {
        match files.rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error) if attempt < RETRIES => {
                files.wait_before_retry(Duration::from_millis(100));
                let _ = error;
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("the retry loop always returns")
}

/// Replaces the stopped service executable and restores it if the second rename fails.
pub fn replace_staged_update(
    files: &mut impl UpdateFileOps,
    paths: &HostUpdatePaths,
) -> std::io::Result<()> {
    match files.remove_file(&paths.old) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    rename_with_retry(files, &paths.executable, &paths.old)?;
    if let Err(update_error) = rename_with_retry(files, &paths.staged, &paths.executable) {
        if let Err(restore_error) = rename_with_retry(files, &paths.old, &paths.executable) {
            return Err(std::io::Error::new(
                restore_error.kind(),
                format!("install new host: {update_error}; restore old host: {restore_error}"),
            ));
        }
        return Err(update_error);
    }
    Ok(())
}

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
            // Replacing the executable is the pipe handler's job (P133).
            HostRequest::UpdateSelf { .. } => HostResponse::Error(IpcError::new(
                ErrorCode::Unsupported,
                "the service cannot update itself yet",
            )),
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

    struct TestUpdateFiles {
        operations: Vec<String>,
        fail_staged_renames: usize,
    }

    impl TestUpdateFiles {
        fn operation(name: &str, first: &Path, second: Option<&Path>) -> String {
            match second {
                Some(second) => format!(
                    "{name}:{}:{}",
                    first.file_name().unwrap().to_string_lossy(),
                    second.file_name().unwrap().to_string_lossy()
                ),
                None => format!("{name}:{}", first.file_name().unwrap().to_string_lossy()),
            }
        }
    }

    impl UpdateFileOps for TestUpdateFiles {
        fn remove_file(&mut self, path: &Path) -> std::io::Result<()> {
            self.operations.push(Self::operation("remove", path, None));
            fs::remove_file(path)
        }

        fn rename(&mut self, from: &Path, to: &Path) -> std::io::Result<()> {
            self.operations
                .push(Self::operation("rename", from, Some(to)));
            if from.file_name() == Some(std::ffi::OsStr::new(UPDATE_STAGED_EXECUTABLE))
                && self.fail_staged_renames > 0
            {
                self.fail_staged_renames -= 1;
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "injected lock",
                ));
            }
            fs::rename(from, to)
        }

        fn wait_before_retry(&mut self, _duration: Duration) {}
    }

    #[test]
    fn updater_key_matches_the_app_configuration() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../secs2r/tauri.conf.json")).unwrap();
        assert_eq!(config["plugins"]["updater"]["pubkey"], UPDATER_PUBKEY);
        assert_eq!(
            updater_pubkey(),
            option_env!("SECS2R_UPDATER_PUBKEY")
                .filter(|key| !key.is_empty())
                .unwrap_or(UPDATER_PUBKEY)
        );
    }

    #[test]
    fn signed_manifest_requires_valid_bytes_shape_and_signed_version() {
        let public_key = include_str!("../tests/data/signed-host-manifest.pub").trim();
        let manifest_bytes = include_bytes!("../tests/data/signed-host-manifest.json");
        let signature = include_str!("../tests/data/signed-host-manifest.json.sig");
        let manifest = verify_manifest(public_key, manifest_bytes, signature).unwrap();
        assert_eq!(manifest.version, "9.8.7");
        assert_eq!(manifest.file, INSTALLED_HOST_EXECUTABLE);

        let wrong_version = include_str!("../tests/data/signed-host-manifest-wrong-version.sig");
        assert_eq!(
            verify_manifest(public_key, manifest_bytes, wrong_version),
            Err(UpdateReject::UnsignedVersion)
        );
        let mut tampered = manifest_bytes.to_vec();
        tampered[0] ^= 1;
        assert_eq!(
            verify_manifest(public_key, &tampered, signature),
            Err(UpdateReject::BadSignature)
        );
    }

    #[test]
    fn update_acceptance_uses_semver_and_the_copied_hash() {
        let manifest = HostManifest {
            version: "1.2.0".into(),
            sha256: "a".repeat(64),
            file: INSTALLED_HOST_EXECUTABLE.into(),
        };
        assert_eq!(accept_update("1.1.9", &manifest, &"a".repeat(64)), Ok(()));
        assert_eq!(
            accept_update("1.2.0", &manifest, &"a".repeat(64)),
            Err(UpdateReject::NotNewer {
                own: "1.2.0".into(),
                offered: "1.2.0".into(),
            })
        );
        assert_eq!(
            accept_update("1.1.9", &manifest, &"b".repeat(64)),
            Err(UpdateReject::HashMismatch)
        );
        let build_metadata = HostManifest {
            version: "1.2.0+build.2".into(),
            ..manifest
        };
        assert_eq!(
            accept_update("1.2.0+build.1", &build_metadata, &"a".repeat(64)),
            Err(UpdateReject::NotNewer {
                own: "1.2.0+build.1".into(),
                offered: "1.2.0+build.2".into(),
            })
        );
    }

    #[test]
    fn bounded_reads_take_regular_files_only_and_staging_writes_verified_bytes() {
        let root = root("bounded-read");
        let source = root.join("source.exe");
        let destination = root.join(UPDATE_STAGED_EXECUTABLE);
        fs::write(&source, b"host bytes").unwrap();
        let bytes = read_bounded(&source, 10).unwrap();
        assert_eq!(
            sha256_hex(&bytes),
            "43bcde663fb99877d335d8d1c06c25386bb8528a324218f1a9e8424f50d3cb03"
        );
        assert_eq!(read_bounded(&source, 9), Err(UpdateReject::TooLarge));
        assert!(matches!(
            read_bounded(&root, 1024),
            Err(UpdateReject::Io(_))
        ));
        assert!(matches!(
            read_bounded(&root.join("missing.exe"), 1024),
            Err(UpdateReject::Io(_))
        ));
        write_staged(&destination, &bytes).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"host bytes");
        write_staged(&destination, b"newer").unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"newer");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn update_source_must_be_a_local_drive_folder() {
        for accepted in [r"C:\Users\a\AppData\Local\SECS 2R", "D:/apps/SECS 2R"] {
            assert_eq!(
                validate_update_source(Path::new(accepted)),
                Ok(()),
                "{accepted}"
            );
        }
        for refused in [
            r"\\server\share\SECS 2R",
            r"\\.\pipe\secs2r",
            r"\\?\C:\SECS 2R",
            r"relative\SECS 2R",
            "C:",
            "",
        ] {
            assert!(
                matches!(
                    validate_update_source(Path::new(refused)),
                    Err(UpdateReject::Io(_))
                ),
                "{refused}"
            );
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn update_source_refuses_an_existing_system_junction() {
        use std::os::windows::fs::MetadataExt;

        let junction = Path::new(r"C:\Documents and Settings");
        let Ok(metadata) = fs::symlink_metadata(junction) else {
            eprintln!(
                "skipping system junction test: {} is absent",
                junction.display()
            );
            return;
        };
        if metadata.file_attributes() & 0x400 == 0 {
            eprintln!(
                "skipping system junction test: {} is not a reparse point",
                junction.display()
            );
            return;
        }
        assert!(matches!(
            validate_update_source(&junction.join("bundle")),
            Err(UpdateReject::Io(_))
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn update_source_refuses_an_existing_directory_symlink() {
        use std::os::windows::fs::symlink_dir;

        let root = root("update-source-link");
        let target = root.join("real");
        let link = root.join("linked");
        fs::create_dir(&target).unwrap();
        if let Err(error) = symlink_dir(&target, &link) {
            eprintln!("skipping directory symlink test: {error}");
            fs::remove_dir_all(root).unwrap();
            return;
        }
        let source = link.join("bundle");

        assert!(matches!(
            validate_update_source(&source),
            Err(UpdateReject::Io(_))
        ));

        fs::remove_dir(&link).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finish_update_replaces_in_order_and_retries_locked_rename() {
        let root = root("finish-success");
        let paths = host_update_paths(&root);
        fs::write(&paths.executable, b"old").unwrap();
        fs::write(&paths.staged, b"new").unwrap();
        fs::write(&paths.old, b"older").unwrap();
        let mut files = TestUpdateFiles {
            operations: Vec::new(),
            fail_staged_renames: 2,
        };

        replace_staged_update(&mut files, &paths).unwrap();

        assert_eq!(fs::read(&paths.executable).unwrap(), b"new");
        assert_eq!(fs::read(&paths.old).unwrap(), b"old");
        assert_eq!(
            &files.operations[..3],
            [
                "remove:secs2r-host.old.exe",
                "rename:secs2r-host.exe:secs2r-host.old.exe",
                "rename:secs2r-host.new.exe:secs2r-host.exe",
            ]
        );
        assert_eq!(
            files
                .operations
                .iter()
                .filter(
                    |operation| operation.as_str() == "rename:secs2r-host.new.exe:secs2r-host.exe"
                )
                .count(),
            3
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finish_update_restores_old_executable_when_new_rename_never_succeeds() {
        let root = root("finish-restore");
        let paths = host_update_paths(&root);
        fs::write(&paths.executable, b"old").unwrap();
        fs::write(&paths.staged, b"new").unwrap();
        let mut files = TestUpdateFiles {
            operations: Vec::new(),
            fail_staged_renames: 51,
        };

        assert!(replace_staged_update(&mut files, &paths).is_err());

        assert_eq!(fs::read(&paths.executable).unwrap(), b"old");
        assert_eq!(fs::read(&paths.staged).unwrap(), b"new");
        assert!(!paths.old.exists());
        assert_eq!(
            files.operations.last().map(String::as_str),
            Some("rename:secs2r-host.old.exe:secs2r-host.exe")
        );
        fs::remove_dir_all(root).unwrap();
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
