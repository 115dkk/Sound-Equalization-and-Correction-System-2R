//! Rust-only application updater (ADR 0020). No updater command is exposed to the webview.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use secs_service::{LocalTime, Service, UpdateAdapter};
use secs_types::ipc::{
    ErrorCode, IpcError, PendingUpdate, UpdateInstall, UpdateNotice, UpdatePhase, UpdatePolicy,
    UpdateStatus,
};
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

const RELEASES: &str =
    "https://github.com/115dkk/Sound-Equalization-and-Correction-System-2R/releases/tag/v";

struct Inner {
    status: UpdateStatus,
    update: Option<Update>,
    bytes: Option<Vec<u8>>,
    task: Option<tauri::async_runtime::JoinHandle<()>>,
    generation: u64,
    stopped: bool,
}

struct Shared {
    inner: Mutex<Inner>,
    wake: Condvar,
}

#[derive(Clone)]
pub(crate) struct TauriUpdates {
    app: AppHandle,
    path: PathBuf,
    portable: bool,
    shared: Arc<Shared>,
}

impl std::fmt::Debug for TauriUpdates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TauriUpdates")
            .field("path", &self.path)
            .field("portable", &self.portable)
            .field("status", &lock(&self.shared.inner).status)
            .finish_non_exhaustive()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn initial_phase(policy: UpdatePolicy) -> UpdatePhase {
    if policy == UpdatePolicy::Off {
        UpdatePhase::Off
    } else {
        UpdatePhase::Idle
    }
}

fn found_phase(version: &str, portable: bool) -> UpdatePhase {
    if portable {
        UpdatePhase::Portable {
            version: version.into(),
            url: format!("{RELEASES}{version}"),
        }
    } else {
        UpdatePhase::Available {
            version: version.into(),
        }
    }
}

fn endpoint_override(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

fn notice_from_pending(pending: PendingUpdate, current: &str) -> Option<UpdateNotice> {
    if pending.to != current {
        return None;
    }
    Some(UpdateNotice {
        skipped: pending
            .previous
            .as_ref()
            .is_some_and(|previous| previous != &pending.from),
        release_url: format!("{RELEASES}{}", pending.to),
        from: pending.from,
        to: pending.to,
        notes: pending.notes,
    })
}

fn require_download(phase: &UpdatePhase) -> Result<(), IpcError> {
    if matches!(phase, UpdatePhase::Available { .. }) {
        Ok(())
    } else {
        Err(invalid("No update is available to download"))
    }
}

fn require_restart(phase: &UpdatePhase) -> Result<(), IpcError> {
    if matches!(phase, UpdatePhase::Ready { .. }) {
        Ok(())
    } else {
        Err(invalid("No update is ready to install"))
    }
}

fn invalid(reason: &str) -> IpcError {
    IpcError::new(ErrorCode::InvalidRequest, reason)
}

fn install_mode(deb: bool) -> UpdateInstall {
    if deb {
        UpdateInstall::OnRequest
    } else {
        UpdateInstall::OnExit
    }
}

fn atomic_pending(path: &Path, pending: &PendingUpdate) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(pending)?;
    let parent = path.parent().expect("update.json has a config directory");
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!("update-{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn remove_pending(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

impl TauriUpdates {
    /// Scheduling starts only after the service has loaded the stored policy.
    pub(crate) fn new(app: AppHandle, config_dir: &Path) -> Self {
        let path = config_dir.join("update.json");
        let current = app.package_info().version.to_string();
        let notice = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .and_then(|pending| notice_from_pending(pending, &current));
        if notice.is_none() {
            let _ = remove_pending(&path);
        }
        let portable = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("portable").is_file()))
            .unwrap_or(false);
        Self {
            app,
            path,
            portable,
            shared: Arc::new(Shared {
                inner: Mutex::new(Inner {
                    status: UpdateStatus {
                        current,
                        policy: UpdatePolicy::Off,
                        phase: UpdatePhase::Off,
                        checked_at: None,
                        notice,
                    },
                    update: None,
                    bytes: None,
                    task: None,
                    generation: 0,
                    stopped: false,
                }),
                wake: Condvar::new(),
            }),
        }
    }

    pub(crate) fn start(&self, policy: UpdatePolicy) {
        {
            let mut inner = lock(&self.shared.inner);
            inner.status.policy = policy;
            inner.status.phase = initial_phase(policy);
        }
        let this = self.clone();
        // Tauri does not re-export Tokio's timer. Its blocking pool waits on a
        // condition variable instead, releasing the same state mutex and waking
        // immediately on exit. Network futures still run on the async executor.
        tauri::async_runtime::spawn_blocking(move || {
            let mut delay = Duration::from_secs(15);
            let mut inner = lock(&this.shared.inner);
            loop {
                let (next, _) = this
                    .shared
                    .wake
                    .wait_timeout_while(inner, delay, |inner| !inner.stopped)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                inner = next;
                if inner.stopped {
                    break;
                }
                if inner.status.policy != UpdatePolicy::Off
                    && !matches!(
                        inner.status.phase,
                        UpdatePhase::Checking
                            | UpdatePhase::Downloading { .. }
                            | UpdatePhase::Ready { .. }
                            | UpdatePhase::Installing { .. }
                    )
                {
                    this.check_locked(&mut inner);
                }
                delay = Duration::from_secs(6 * 60 * 60);
            }
        });
    }

    fn cancel_locked(&self, inner: &mut Inner) {
        inner.generation += 1;
        if let Some(task) = inner.task.take() {
            task.abort();
        }
    }

    fn check_locked(&self, inner: &mut Inner) {
        self.cancel_locked(inner);
        if inner.bytes.take().is_some() {
            let _ = remove_pending(&self.path);
        }
        inner.update = None;
        inner.status.phase = UpdatePhase::Checking;
        let generation = inner.generation;
        let this = self.clone();
        inner.task = Some(tauri::async_runtime::spawn(async move {
            let result = async {
                let mut builder = this
                    .app
                    .updater_builder()
                    .restart_after_install(false)
                    .timeout(Duration::from_secs(60));
                if let Some(endpoint) =
                    endpoint_override(std::env::var("SECS2R_UPDATE_ENDPOINT").ok().as_deref())
                {
                    let url = endpoint.parse().map_err(|_| "Invalid update address")?;
                    builder = builder
                        .endpoints(vec![url])
                        .map_err(|_| "Invalid update address")?;
                }
                builder
                    .build()
                    .map_err(|_| "Could not configure updates")?
                    .check()
                    .await
                    .map_err(|_| "Could not check for updates")
            }
            .await;
            let download = {
                let mut inner = lock(&this.shared.inner);
                if inner.generation != generation || inner.stopped {
                    return;
                }
                inner.status.checked_at =
                    Some(LocalTime::from_system(SystemTime::now(), 0).rfc3339_utc());
                match result {
                    Ok(Some(mut update)) => {
                        update.timeout = Some(Duration::from_secs(10 * 60));
                        inner.status.phase = found_phase(&update.version, this.portable);
                        let download = inner.status.policy == UpdatePolicy::Auto && !this.portable;
                        if download {
                            inner.status.phase = UpdatePhase::Downloading {
                                version: update.version.clone(),
                                received: 0,
                                total: None,
                            };
                        }
                        inner.update = Some(update);
                        download
                    }
                    Ok(None) => {
                        inner.status.phase = UpdatePhase::UpToDate;
                        false
                    }
                    Err(reason) => {
                        inner.status.phase = UpdatePhase::Failed {
                            reason: reason.into(),
                        };
                        false
                    }
                }
            };
            if download {
                this.download_work(generation).await;
            }
        }));
    }

    async fn download_work(&self, generation: u64) {
        let update = {
            let mut inner = lock(&self.shared.inner);
            if inner.generation != generation || inner.stopped {
                return;
            }
            let Some(update) = inner.update.clone() else {
                return;
            };
            inner.status.phase = UpdatePhase::Downloading {
                version: update.version.clone(),
                received: 0,
                total: None,
            };
            update
        };
        let result = update
            .download(
                |chunk, length| {
                    let mut inner = lock(&self.shared.inner);
                    if inner.generation == generation
                        && let UpdatePhase::Downloading {
                            received, total, ..
                        } = &mut inner.status.phase
                    {
                        *received = received.saturating_add(chunk as u64);
                        *total = length;
                    }
                },
                || {},
            )
            .await;
        let mut inner = lock(&self.shared.inner);
        if inner.generation != generation || inner.stopped {
            return;
        }
        match result {
            Ok(bytes) => {
                let pending = PendingUpdate {
                    from: inner.status.current.clone(),
                    to: update.version.clone(),
                    notes: update.body.clone().unwrap_or_default(),
                    previous: update
                        .raw_json
                        .get("previous")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                };
                if atomic_pending(&self.path, &pending).is_err() {
                    inner.status.phase = UpdatePhase::Failed {
                        reason: "Could not save update details".into(),
                    };
                    return;
                }
                inner.bytes = Some(bytes);
                inner.status.phase = UpdatePhase::Ready {
                    version: update.version,
                    install: install_mode(matches!(
                        tauri::utils::platform::bundle_type(),
                        Some(tauri::utils::config::BundleType::Deb)
                    )),
                };
            }
            Err(_) => {
                inner.status.phase = UpdatePhase::Failed {
                    reason: "Could not download or verify the update".into(),
                }
            }
        }
    }

    fn download_locked(&self, inner: &mut Inner) {
        self.cancel_locked(inner);
        let generation = inner.generation;
        if let Some(update) = &inner.update {
            inner.status.phase = UpdatePhase::Downloading {
                version: update.version.clone(),
                received: 0,
                total: None,
            };
        }
        let this = self.clone();
        inner.task = Some(tauri::async_runtime::spawn(async move {
            this.download_work(generation).await;
        }));
    }

    fn take_install(inner: &mut Inner) -> Option<(Update, Vec<u8>)> {
        let update = inner.update.take()?;
        let bytes = inner.bytes.take()?;
        inner.status.phase = UpdatePhase::Installing {
            version: update.version.clone(),
        };
        Some((update, bytes))
    }

    fn install(&self, update: Update, bytes: Vec<u8>, restart: bool) {
        match update.restart_after_install(restart).install(bytes) {
            Ok(()) if restart => self.app.restart(),
            Ok(()) => {}
            Err(_) => {
                lock(&self.shared.inner).status.phase = UpdatePhase::Failed {
                    reason: "Could not install the update".into(),
                };
            }
        }
    }

    /// Called only after Destroyed has shut jobs down and the save flow finished.
    pub(crate) fn on_exit(&self) {
        let pending = {
            let mut inner = lock(&self.shared.inner);
            inner.stopped = true;
            self.cancel_locked(&mut inner);
            self.shared.wake.notify_all();
            if matches!(
                inner.status.phase,
                UpdatePhase::Ready {
                    install: UpdateInstall::OnExit,
                    ..
                }
            ) {
                Self::take_install(&mut inner)
            } else {
                None
            }
        };
        if let Some((update, bytes)) = pending {
            self.install(update, bytes, false);
        }
    }
}

impl UpdateAdapter for TauriUpdates {
    fn status(&self) -> UpdateStatus {
        lock(&self.shared.inner).status.clone()
    }

    fn check(&self) -> Result<UpdateStatus, IpcError> {
        let mut inner = lock(&self.shared.inner);
        if inner.stopped || matches!(inner.status.phase, UpdatePhase::Installing { .. }) {
            return Err(invalid("The app is closing"));
        }
        self.check_locked(&mut inner);
        Ok(inner.status.clone())
    }

    fn download(&self) -> Result<UpdateStatus, IpcError> {
        let mut inner = lock(&self.shared.inner);
        require_download(&inner.status.phase)?;
        if inner.stopped {
            return Err(invalid("The app is closing"));
        }
        self.download_locked(&mut inner);
        Ok(inner.status.clone())
    }

    fn restart(&self) -> Result<(), IpcError> {
        let mut inner = lock(&self.shared.inner);
        require_restart(&inner.status.phase)?;
        if inner.stopped {
            return Err(invalid("The app is closing"));
        }
        let (update, bytes) = Self::take_install(&mut inner)
            .ok_or_else(|| invalid("No update is ready to install"))?;
        let this = self.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let _ = this
                .app
                .state::<Arc<Service>>()
                .shutdown(Duration::from_secs(3));
            this.install(update, bytes, true);
        });
        Ok(())
    }

    fn dismiss_notice(&self) -> UpdateStatus {
        let mut inner = lock(&self.shared.inner);
        // A newly downloaded update may already have replaced the previous notice
        // on disk. Dismissing the old notice must not delete that pending record.
        if inner.bytes.is_some()
            || matches!(inner.status.phase, UpdatePhase::Installing { .. })
            || remove_pending(&self.path).is_ok()
        {
            inner.status.notice = None;
        } else {
            inner.status.phase = UpdatePhase::Failed {
                reason: "Could not dismiss the update notice".into(),
            };
        }
        inner.status.clone()
    }

    fn set_policy(&self, policy: UpdatePolicy) {
        let mut inner = lock(&self.shared.inner);
        inner.status.policy = policy;
        if inner.stopped || matches!(inner.status.phase, UpdatePhase::Installing { .. }) {
            return;
        }
        if policy == UpdatePolicy::Off {
            self.cancel_locked(&mut inner);
            if inner.bytes.take().is_some() {
                let _ = remove_pending(&self.path);
            }
            inner.update = None;
            inner.status.phase = UpdatePhase::Off;
        } else if policy == UpdatePolicy::Notify
            && matches!(inner.status.phase, UpdatePhase::Downloading { .. })
        {
            self.cancel_locked(&mut inner);
            inner.status.phase = inner.update.as_ref().map_or(UpdatePhase::Idle, |update| {
                found_phase(&update.version, self.portable)
            });
        } else if policy == UpdatePolicy::Auto
            && matches!(inner.status.phase, UpdatePhase::Available { .. })
        {
            self.download_locked(&mut inner);
        } else if matches!(inner.status.phase, UpdatePhase::Off) {
            self.check_locked(&mut inner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updater_config_requires_signed_version() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config["plugins"]["updater"]["requireSignedVersion"], true);
        let updater: tauri_plugin_updater::Config =
            serde_json::from_value(config["plugins"]["updater"].clone()).unwrap();
        assert!(updater.require_signed_version);
        assert_eq!(updater.endpoints.len(), 2);
        assert_eq!(
            config["plugins"]["updater"]["windows"]["installMode"],
            "quiet"
        );
        assert_eq!(config["bundle"]["createUpdaterArtifacts"], true);
    }

    #[test]
    fn update_policy_and_portable_decisions() {
        assert_eq!(initial_phase(UpdatePolicy::Off), UpdatePhase::Off);
        for policy in [UpdatePolicy::Auto, UpdatePolicy::Notify] {
            assert_eq!(initial_phase(policy), UpdatePhase::Idle);
        }
        assert_eq!(
            found_phase("1.2.3", false),
            UpdatePhase::Available {
                version: "1.2.3".into()
            }
        );
        assert_eq!(
            found_phase("1.2.3", true),
            UpdatePhase::Portable {
                version: "1.2.3".into(),
                url: format!("{RELEASES}1.2.3")
            }
        );
        assert_eq!(install_mode(true), UpdateInstall::OnRequest);
        assert_eq!(install_mode(false), UpdateInstall::OnExit);
    }

    #[test]
    fn update_endpoint_override_preserves_nonempty_address() {
        assert_eq!(endpoint_override(None), None);
        assert_eq!(endpoint_override(Some("")), None);
        assert_eq!(
            endpoint_override(Some("http://127.0.0.1:1234/latest.json")),
            Some("http://127.0.0.1:1234/latest.json")
        );
    }

    #[test]
    fn update_notice_matches_running_version_and_tracks_skipped_releases() {
        let mut pending: PendingUpdate = serde_json::from_str(
            r#"{"from":"1.0.0","to":"1.2.0","notes":"Changes","previous":"1.1.0"}"#,
        )
        .unwrap();
        let notice = notice_from_pending(pending.clone(), "1.2.0").unwrap();
        assert!(notice.skipped);
        assert_eq!(notice.notes, "Changes");
        assert_eq!(notice.release_url, format!("{RELEASES}1.2.0"));
        assert!(notice_from_pending(pending.clone(), "1.0.0").is_none());
        pending.previous = Some(pending.from.clone());
        assert!(
            !notice_from_pending(pending.clone(), "1.2.0")
                .unwrap()
                .skipped
        );
        pending.previous = None;
        assert!(!notice_from_pending(pending, "1.2.0").unwrap().skipped);
    }

    #[test]
    fn update_actions_reject_wrong_phases() {
        let available = found_phase("1.0.0", false);
        let ready = UpdatePhase::Ready {
            version: "1.0.0".into(),
            install: UpdateInstall::OnExit,
        };
        assert!(require_download(&available).is_ok());
        assert!(require_restart(&ready).is_ok());
        for phase in [
            UpdatePhase::Off,
            UpdatePhase::Idle,
            UpdatePhase::Checking,
            UpdatePhase::UpToDate,
            UpdatePhase::Downloading {
                version: "1.0.0".into(),
                received: 40,
                total: Some(100),
            },
            UpdatePhase::Installing {
                version: "1.0.0".into(),
            },
            found_phase("1.0.0", true),
            UpdatePhase::Failed {
                reason: "Check failed".into(),
            },
        ] {
            assert_eq!(
                require_download(&phase).unwrap_err().code,
                ErrorCode::InvalidRequest
            );
            assert_eq!(
                require_restart(&phase).unwrap_err().code,
                ErrorCode::InvalidRequest
            );
        }
        assert!(require_download(&ready).is_err());
        assert!(require_restart(&available).is_err());
    }
}
