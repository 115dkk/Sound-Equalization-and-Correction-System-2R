use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use base64::Engine;
use secs_types::ipc::{
    ErrorCode, HostLogLine, HostManaged, HostRequest, HostResponse, HostStatus, IncludeState,
    IpcError, ManagedProcess,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const MEMORY_LOG_LIMIT: usize = 1_000;
const LOG_FIELD_VALUE_LIMIT: usize = 4 * 1024;
const LOG_TRUNCATION_SUFFIX: &str = " … [truncated]";
// Leave 64 KiB for the pipe frame and future response fields below its 1 MiB cap.
const LOG_RESPONSE_LIMIT: usize = 960 * 1024;
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

fn truncate_log_field_value(value: &mut String) {
    if value.len() <= LOG_FIELD_VALUE_LIMIT {
        return;
    }
    let mut keep = LOG_FIELD_VALUE_LIMIT - LOG_TRUNCATION_SUFFIX.len();
    while !value.is_char_boundary(keep) {
        keep -= 1;
    }
    value.truncate(keep);
    value.push_str(LOG_TRUNCATION_SUFFIX);
}

fn bounded_log_response(mut lines: Vec<HostLogLine>) -> HostResponse {
    let encoded_len = |start: usize| {
        serde_json::to_vec(&HostResponse::Log(lines[start..].to_vec()))
            .map_or(usize::MAX, |encoded| encoded.len())
    };
    if encoded_len(0) <= LOG_RESPONSE_LIMIT {
        return HostResponse::Log(lines);
    }

    let mut low = 0;
    let mut high = lines.len();
    while low < high {
        let middle = low + (high - low) / 2;
        if encoded_len(middle) <= LOG_RESPONSE_LIMIT {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    lines.drain(..low);
    HostResponse::Log(lines)
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
            HostRequest::SetManaged(_) | HostRequest::StopManaging => {
                HostResponse::Error(IpcError::new(
                    ErrorCode::Unsupported,
                    "managed mutations require transition context",
                ))
            }
            HostRequest::Log { lines } => {
                let keep = usize::try_from(lines).unwrap_or(usize::MAX);
                let start = self.logs.len().saturating_sub(keep);
                bounded_log_response(self.logs[start..].to_vec())
            }
            // Replacing the executable is the pipe handler's job (P133).
            HostRequest::UpdateSelf { .. } => HostResponse::Error(IpcError::new(
                ErrorCode::Unsupported,
                "the service cannot update itself yet",
            )),
        }
    }

    pub fn record(&mut self, mut line: HostLogLine) {
        for value in line.fields.values_mut() {
            truncate_log_field_value(value);
        }
        self.logs.push(line);
        if self.logs.len() > MEMORY_LOG_LIMIT {
            self.logs.drain(..self.logs.len() - MEMORY_LOG_LIMIT);
        }
    }
}

/// Returns repaired text only when the marker pair is absent or damaged.
pub fn repair_text(config: &str) -> Option<String> {
    secs_apply::eqapo::with_include(config)
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

pub fn log_line(event: &str, fields: impl IntoIterator<Item = (String, String)>) -> HostLogLine {
    HostLogLine {
        timestamp: chrono::Utc::now().to_rfc3339(),
        event: event.to_owned(),
        fields: fields.into_iter().collect::<BTreeMap<_, _>>(),
    }
}

#[cfg(test)]
mod tests {

    use secs_types::ipc::{ManagedCamilla, ManagedEqApo};

    use super::*;

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
    fn core_rejects_mutating_requests_without_transition_context() {
        let managed = managed();
        let mut core = Core::new("1", managed.clone());

        for request in [
            HostRequest::SetManaged(HostManaged::default()),
            HostRequest::StopManaging,
        ] {
            let HostResponse::Error(error) = core.handle(request) else {
                panic!("Core accepted a mutating request without transition context")
            };
            assert_eq!(error.code, ErrorCode::Unsupported);
        }
        assert_eq!(core.status().managed, managed);
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
    fn core_keeps_one_thousand_logs_and_bounds_the_serialized_response() {
        let mut core = Core::new("1", HostManaged::default());
        for index in 0..1_005 {
            core.record(log_line(&index.to_string(), []));
        }
        let HostResponse::Log(lines) = core.handle(HostRequest::Log { lines: 2_000 }) else {
            panic!("expected log response");
        };
        assert_eq!(lines.len(), 1_000);
        assert_eq!(lines.first().map(|line| line.event.as_str()), Some("5"));
        assert_eq!(lines.last().map(|line| line.event.as_str()), Some("1004"));

        let large = "x".repeat(2_000);
        for index in 0..1_000 {
            core.record(log_line(
                &index.to_string(),
                [("detail".to_owned(), large.clone())],
            ));
        }
        let response = core.handle(HostRequest::Log { lines: 2_000 });
        let encoded = serde_json::to_vec(&response).unwrap();
        assert!(encoded.len() <= 960 * 1024, "{} bytes", encoded.len());
        let HostResponse::Log(lines) = response else {
            panic!("expected log response");
        };
        assert_eq!(lines.last().map(|line| line.event.as_str()), Some("999"));
    }

    #[test]
    fn oversized_latest_log_is_truncated_instead_of_omitted() {
        let mut core = Core::new("1", HostManaged::default());
        core.record(log_line(
            "latest",
            [("detail".to_owned(), "🙂".repeat(524_289))],
        ));

        let response = core.handle(HostRequest::Log { lines: 1 });
        let encoded = serde_json::to_vec(&response).unwrap();
        assert!(encoded.len() <= 960 * 1024, "{} bytes", encoded.len());
        let HostResponse::Log(lines) = response else {
            panic!("expected log response");
        };
        assert_eq!(lines.len(), 1);
        let detail = &lines[0].fields["detail"];
        assert!(detail.len() <= 4 * 1024, "{} bytes", detail.len());
        assert!(detail.ends_with("[truncated]"));
    }
}
