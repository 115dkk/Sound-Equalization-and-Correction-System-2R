//! Rust-only application updater (ADR 0020). No updater command is exposed to the webview.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use secs_service::update_machine::{
    FoundRelease, UpdateCommand, UpdateEvent, UpdateFailure, UpdateMachine, notice_from_pending,
};
use secs_service::{LogLevel, Service, UpdateAdapter};
use secs_types::ipc::{
    IpcError, PendingUpdate, UpdateInstall, UpdatePhase, UpdatePolicy, UpdateStatus,
};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_updater::{Update, UpdaterExt};

struct Inner {
    machine: UpdateMachine,
    update: Option<Update>,
    bytes: Option<Vec<u8>>,
    task: Option<tauri::async_runtime::JoinHandle<()>>,
    closed: bool,
}

struct Shared {
    inner: Mutex<Inner>,
    wake: Condvar,
}

/// Receives update-machine diagnostics with their service log level.
pub(crate) type UpdateLog = Arc<dyn Fn(LogLevel, &str) + Send + Sync>;

pub(crate) struct TauriUpdates<R: Runtime = tauri::Wry> {
    app: AppHandle<R>,
    path: PathBuf,
    endpoint: Option<String>,
    log: UpdateLog,
    shared: Arc<Shared>,
}

impl<R: Runtime> Clone for TauriUpdates<R> {
    fn clone(&self) -> Self {
        Self {
            app: self.app.clone(),
            path: self.path.clone(),
            endpoint: self.endpoint.clone(),
            log: self.log.clone(),
            shared: self.shared.clone(),
        }
    }
}

impl<R: Runtime> std::fmt::Debug for TauriUpdates<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TauriUpdates")
            .field("path", &self.path)
            .field("status", lock(&self.shared.inner).machine.status())
            .finish_non_exhaustive()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn endpoint_override(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
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
    secs_types::fs::write_atomic_bytes(path, &bytes)
}

fn remove_pending(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn found_release(update: &Update) -> FoundRelease {
    FoundRelease {
        version: update.version.clone(),
        notes: update.body.clone().unwrap_or_default(),
        previous: update
            .raw_json
            .get("previous")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
    }
}

fn phase_version(phase: &UpdatePhase) -> Option<&str> {
    match phase {
        UpdatePhase::Available { version }
        | UpdatePhase::Downloading { version, .. }
        | UpdatePhase::Ready { version, .. }
        | UpdatePhase::Installing { version }
        | UpdatePhase::Portable { version, .. } => Some(version),
        _ => None,
    }
}

impl<R: Runtime> TauriUpdates<R> {
    /// Scheduling starts only after the service has loaded the stored policy.
    pub(crate) fn new(app: AppHandle<R>, config_dir: &Path, log: UpdateLog) -> Self {
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
        let install = install_mode(matches!(
            tauri::utils::platform::bundle_type(),
            Some(tauri::utils::config::BundleType::Deb)
        ));
        let mut machine = UpdateMachine::new(&current, portable, install);
        machine
            .step(UpdateEvent::Start {
                policy: UpdatePolicy::Off,
                notice,
            })
            .expect("Start cannot fail");
        Self {
            app,
            path,
            endpoint: endpoint_override(std::env::var("SECS2R_UPDATE_ENDPOINT").ok().as_deref())
                .map(str::to_owned),
            log,
            shared: Arc::new(Shared {
                inner: Mutex::new(Inner {
                    machine,
                    update: None,
                    bytes: None,
                    task: None,
                    closed: false,
                }),
                wake: Condvar::new(),
            }),
        }
    }

    #[cfg(test)]
    fn with_endpoint(mut self, endpoint: &str) -> Self {
        self.endpoint = Some(endpoint.to_owned());
        self
    }

    fn step(&self, event: UpdateEvent) -> Result<Vec<UpdateCommand>, IpcError> {
        lock(&self.shared.inner).machine.step(event)
    }

    fn dispatch(&self, event: UpdateEvent) -> Result<UpdateStatus, IpcError> {
        let commands = self.step(event)?;
        self.execute(commands, None);
        Ok(self.status())
    }

    fn execute(&self, commands: Vec<UpdateCommand>, generation: Option<u64>) {
        for command in commands {
            match command {
                UpdateCommand::Check { generation } => self.start_check(generation),
                UpdateCommand::Download { generation, .. } => self.start_download(generation),
                UpdateCommand::Cancel => self.cancel_task(),
                UpdateCommand::SavePending(pending) => {
                    if let Err(error) = atomic_pending(&self.path, &pending) {
                        lock(&self.shared.inner).bytes = None;
                        if let Some(generation) = generation {
                            let commands = self
                                .step(UpdateEvent::DownloadResult {
                                    generation,
                                    result: Err(UpdateFailure {
                                        reason: "Could not save update details",
                                        cause: format!("saving the update details failed: {error}"),
                                    }),
                                })
                                .unwrap_or_default();
                            self.execute(commands, Some(generation));
                        } else {
                            (self.log)(
                                LogLevel::Error,
                                &format!("saving the update details failed: {error}"),
                            );
                        }
                    }
                }
                UpdateCommand::RemovePending => {
                    lock(&self.shared.inner).bytes = None;
                    if let Err(error) = remove_pending(&self.path) {
                        (self.log)(
                            LogLevel::Error,
                            &format!("removing the update details failed: {error}"),
                        );
                    }
                }
                UpdateCommand::Install { restart } => self.start_install(restart),
                UpdateCommand::Log { level, line } => (self.log)(level, &line),
            }
        }
    }

    fn cancel_task(&self) {
        if let Some(task) = lock(&self.shared.inner).task.take() {
            task.abort();
        }
    }

    fn start_check(&self, generation: u64) {
        let had_bytes = {
            let mut inner = lock(&self.shared.inner);
            if let Some(task) = inner.task.take() {
                task.abort();
            }
            inner.update = None;
            inner.bytes.take().is_some()
        };
        if had_bytes {
            let _ = remove_pending(&self.path);
        }
        let this = self.clone();
        let task = tauri::async_runtime::spawn(async move {
            let result = async {
                let mut builder = this
                    .app
                    .updater_builder()
                    .restart_after_install(false)
                    .timeout(Duration::from_secs(60));
                if let Some(endpoint) = this.endpoint.as_deref() {
                    let url = endpoint.parse().map_err(|error| UpdateFailure {
                        reason: "Invalid update address",
                        cause: format!("update address failed: {endpoint:?}: {error}"),
                    })?;
                    builder = builder
                        .endpoints(vec![url])
                        .map_err(|error| UpdateFailure {
                            reason: "Invalid update address",
                            cause: format!("update address failed: {error}"),
                        })?;
                }
                builder
                    .build()
                    .map_err(|error| UpdateFailure {
                        reason: "Could not configure updates",
                        cause: format!("update configuration failed: {error}"),
                    })?
                    .check()
                    .await
                    .map_err(|error| UpdateFailure {
                        reason: "Could not check for updates",
                        cause: format!("update check failed: {error}"),
                    })
            }
            .await;
            this.finish_check(generation, result);
        });
        lock(&self.shared.inner).task = Some(task);
    }

    fn finish_check(&self, generation: u64, result: Result<Option<Update>, UpdateFailure>) {
        let commands = {
            let mut inner = lock(&self.shared.inner);
            let was_checking = matches!(inner.machine.status().phase, UpdatePhase::Checking);
            match result {
                Ok(Some(mut update)) => {
                    update.timeout = Some(Duration::from_secs(10 * 60));
                    let release = found_release(&update);
                    let commands = inner
                        .machine
                        .step(UpdateEvent::CheckResult {
                            generation,
                            result: Ok(Some(release)),
                        })
                        .unwrap_or_default();
                    if was_checking
                        && phase_version(&inner.machine.status().phase)
                            == Some(update.version.as_str())
                    {
                        inner.update = Some(update);
                        inner.bytes = None;
                    }
                    commands
                }
                Ok(None) => {
                    let commands = inner
                        .machine
                        .step(UpdateEvent::CheckResult {
                            generation,
                            result: Ok(None),
                        })
                        .unwrap_or_default();
                    if was_checking && matches!(inner.machine.status().phase, UpdatePhase::UpToDate)
                    {
                        inner.update = None;
                        inner.bytes = None;
                    }
                    commands
                }
                Err(failure) => {
                    let commands = inner
                        .machine
                        .step(UpdateEvent::CheckResult {
                            generation,
                            result: Err(failure),
                        })
                        .unwrap_or_default();
                    if was_checking
                        && matches!(inner.machine.status().phase, UpdatePhase::Failed { .. })
                    {
                        inner.update = None;
                        inner.bytes = None;
                    }
                    commands
                }
            }
        };
        self.execute(commands, None);
    }

    fn start_download(&self, generation: u64) {
        let update = lock(&self.shared.inner).update.clone();
        let Some(update) = update else {
            let commands = self
                .step(UpdateEvent::DownloadResult {
                    generation,
                    result: Err(UpdateFailure {
                        reason: "Could not download or verify the update",
                        cause: "update download failed: no checked release is available".into(),
                    }),
                })
                .unwrap_or_default();
            self.execute(commands, Some(generation));
            return;
        };
        let this = self.clone();
        let task = tauri::async_runtime::spawn(async move {
            let progress = this.clone();
            let mut received = 0_u64;
            let result = update
                .download(
                    move |chunk, total| {
                        received = received.saturating_add(chunk as u64);
                        let commands = progress
                            .step(UpdateEvent::DownloadProgress {
                                generation,
                                received,
                                total,
                            })
                            .unwrap_or_default();
                        progress.execute(commands, Some(generation));
                    },
                    || {},
                )
                .await
                .map_err(|error| UpdateFailure {
                    reason: "Could not download or verify the update",
                    cause: format!("update download failed: {error}"),
                });
            this.finish_download(generation, result);
        });
        lock(&self.shared.inner).task = Some(task);
    }

    fn finish_download(&self, generation: u64, result: Result<Vec<u8>, UpdateFailure>) {
        let commands = {
            let mut inner = lock(&self.shared.inner);
            let was_downloading = matches!(
                inner.machine.status().phase,
                UpdatePhase::Downloading { .. }
            );
            match result {
                Ok(bytes) => {
                    let commands = inner
                        .machine
                        .step(UpdateEvent::DownloadResult {
                            generation,
                            result: Ok(()),
                        })
                        .unwrap_or_default();
                    if was_downloading
                        && matches!(inner.machine.status().phase, UpdatePhase::Ready { .. })
                    {
                        inner.bytes = Some(bytes);
                    }
                    commands
                }
                Err(failure) => inner
                    .machine
                    .step(UpdateEvent::DownloadResult {
                        generation,
                        result: Err(failure),
                    })
                    .unwrap_or_default(),
            }
        };
        self.execute(commands, Some(generation));
    }

    fn take_install(&self) -> Option<(Update, Vec<u8>)> {
        let mut inner = lock(&self.shared.inner);
        Some((inner.update.take()?, inner.bytes.take()?))
    }

    fn install(&self, update: Update, bytes: Vec<u8>, restart: bool) {
        match update.restart_after_install(restart).install(bytes) {
            Ok(()) if restart => self.app.restart(),
            Ok(()) => {}
            Err(error) => (self.log)(LogLevel::Error, &format!("update install failed: {error}")),
        }
    }

    fn start_install(&self, restart: bool) {
        let Some((update, bytes)) = self.take_install() else {
            (self.log)(
                LogLevel::Error,
                "update install failed: no downloaded release is available",
            );
            return;
        };
        if restart {
            let this = self.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let _ = this
                    .app
                    .state::<Arc<Service>>()
                    .shutdown(Duration::from_secs(3));
                this.install(update, bytes, true);
            });
        } else {
            self.install(update, bytes, false);
        }
    }

    pub(crate) fn start(&self, policy: UpdatePolicy) {
        {
            let mut inner = lock(&self.shared.inner);
            inner.closed = false;
            let notice = inner.machine.status().notice.clone();
            inner
                .machine
                .step(UpdateEvent::Start { policy, notice })
                .expect("Start cannot fail");
        }
        let this = self.clone();
        // Tauri does not re-export Tokio's timer. Its blocking pool waits on a
        // condition variable and wakes immediately when the app exits.
        tauri::async_runtime::spawn_blocking(move || {
            let mut delay = Duration::from_secs(15);
            loop {
                let inner = lock(&this.shared.inner);
                let (inner, _) = this
                    .shared
                    .wake
                    .wait_timeout_while(inner, delay, |inner| !inner.closed)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if inner.closed {
                    break;
                }
                drop(inner);
                let _ = this.dispatch(UpdateEvent::Tick);
                delay = Duration::from_secs(6 * 60 * 60);
            }
        });
    }

    /// Called only after Destroyed has shut jobs down and the save flow finished.
    pub(crate) fn on_exit(&self) {
        {
            let mut inner = lock(&self.shared.inner);
            inner.closed = true;
            self.shared.wake.notify_all();
        }
        let _ = self.dispatch(UpdateEvent::Exit);
    }
}

impl<R: Runtime> UpdateAdapter for TauriUpdates<R> {
    fn status(&self) -> UpdateStatus {
        lock(&self.shared.inner).machine.status().clone()
    }

    fn check(&self) -> Result<UpdateStatus, IpcError> {
        self.dispatch(UpdateEvent::Check)
    }

    fn download(&self) -> Result<UpdateStatus, IpcError> {
        self.dispatch(UpdateEvent::Download)
    }

    fn restart(&self) -> Result<(), IpcError> {
        self.dispatch(UpdateEvent::Restart).map(|_| ())
    }

    fn dismiss_notice(&self) -> UpdateStatus {
        self.dispatch(UpdateEvent::DismissNotice)
            .unwrap_or_else(|_| self.status())
    }

    fn set_policy(&self, policy: UpdatePolicy) {
        let _ = self.dispatch(UpdateEvent::SetPolicy(policy));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// The real updater, a mock app and a closed port: the connection error
    /// reaches the log with its cause while the screen's reason stays fixed
    /// (audit #186 TD-164).
    #[test]
    fn check_failure_logs_its_cause_td164() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        // The mock context carries no plugin section; give the updater the
        // app's own configuration (public key, endpoints).
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context
            .config_mut()
            .plugins
            .0
            .insert("updater".into(), config["plugins"]["updater"].clone());
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .expect("mock app");
        let lines: Arc<Mutex<Vec<String>>> = Arc::default();
        let sink = lines.clone();
        let dir = std::env::temp_dir().join(format!(
            "secs2r-update-td164-{}-{}",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let updates = TauriUpdates::new(
            app.handle().clone(),
            &dir,
            Arc::new(move |_level, text| lock(&sink).push(text.to_owned())),
        )
        .with_endpoint("https://127.0.0.1:1/latest.json");

        let status = updates.check().unwrap();
        assert_eq!(status.phase, UpdatePhase::Checking);
        let deadline = Instant::now() + Duration::from_secs(60);
        let phase = loop {
            let phase = updates.status().phase;
            if !matches!(phase, UpdatePhase::Checking) {
                break phase;
            }
            assert!(Instant::now() < deadline, "the check never settled");
            std::thread::sleep(Duration::from_millis(50));
        };
        assert_eq!(
            phase,
            UpdatePhase::Failed {
                reason: "Could not check for updates".into()
            }
        );
        let lines = lock(&lines);
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("update check failed: ") && line.contains("127.0.0.1")),
            "the connection error must reach the log with its cause: {lines:?}"
        );
        drop(lines);
        let _ = std::fs::remove_dir_all(&dir);
    }

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
    fn update_endpoint_override_preserves_nonempty_address() {
        assert_eq!(endpoint_override(None), None);
        assert_eq!(endpoint_override(Some("")), None);
        assert_eq!(
            endpoint_override(Some("http://127.0.0.1:1234/latest.json")),
            Some("http://127.0.0.1:1234/latest.json")
        );
    }

    #[derive(Clone, Copy)]
    enum RecordedPhase {
        Off,
        Idle,
        Checking,
        UpToDate,
        Available,
        Downloading,
        Ready,
        Installing,
        Portable,
        Failed,
    }

    impl RecordedPhase {
        const ALL: [Self; 10] = [
            Self::Off,
            Self::Idle,
            Self::Checking,
            Self::UpToDate,
            Self::Available,
            Self::Downloading,
            Self::Ready,
            Self::Installing,
            Self::Portable,
            Self::Failed,
        ];
    }

    fn machine_check(machine: &mut secs_service::update_machine::UpdateMachine) -> u64 {
        let commands = machine
            .step(secs_service::update_machine::UpdateEvent::Tick)
            .unwrap();
        match commands.as_slice() {
            [secs_service::update_machine::UpdateCommand::Check { generation }] => *generation,
            other => panic!("expected check command, got {other:?}"),
        }
    }

    fn recorded_machine(phase: RecordedPhase) -> secs_service::update_machine::UpdateMachine {
        use secs_service::update_machine::{
            FoundRelease, UpdateEvent, UpdateFailure, UpdateMachine,
        };

        let portable = matches!(phase, RecordedPhase::Portable);
        let policy = match phase {
            RecordedPhase::Off => UpdatePolicy::Off,
            RecordedPhase::Idle
            | RecordedPhase::Checking
            | RecordedPhase::UpToDate
            | RecordedPhase::Available => UpdatePolicy::Notify,
            _ => UpdatePolicy::Auto,
        };
        let mut machine = UpdateMachine::new("1.0.0", portable, UpdateInstall::OnExit);
        machine
            .step(UpdateEvent::Start {
                policy,
                notice: None,
            })
            .unwrap();
        if matches!(phase, RecordedPhase::Off | RecordedPhase::Idle) {
            return machine;
        }
        let generation = machine_check(&mut machine);
        if matches!(phase, RecordedPhase::Checking) {
            return machine;
        }
        let result = match phase {
            RecordedPhase::UpToDate => Ok(None),
            RecordedPhase::Failed => Err(UpdateFailure {
                reason: "Could not check for updates",
                cause: "recorded failure".into(),
            }),
            _ => Ok(Some(FoundRelease {
                version: "1.2.3".into(),
                notes: "Changes".into(),
                previous: Some("1.1.0".into()),
            })),
        };
        let commands = machine
            .step(UpdateEvent::CheckResult { generation, result })
            .unwrap();
        if matches!(phase, RecordedPhase::Downloading) {
            return machine;
        }
        if matches!(
            phase,
            RecordedPhase::Available
                | RecordedPhase::Portable
                | RecordedPhase::UpToDate
                | RecordedPhase::Failed
        ) {
            return machine;
        }
        let generation = match commands.as_slice() {
            [secs_service::update_machine::UpdateCommand::Download { generation, .. }] => {
                *generation
            }
            other => panic!("expected download command, got {other:?}"),
        };
        machine
            .step(UpdateEvent::DownloadResult {
                generation,
                result: Ok(()),
            })
            .unwrap();
        if matches!(phase, RecordedPhase::Installing) {
            machine.step(UpdateEvent::Restart).unwrap();
        }
        machine
    }

    fn phase_kind(machine: &secs_service::update_machine::UpdateMachine) -> serde_json::Value {
        serde_json::to_value(&machine.status().phase).unwrap()["kind"].clone()
    }

    fn transition_entry(
        method: &str,
        input: RecordedPhase,
        policy: UpdatePolicy,
        event: secs_service::update_machine::UpdateEvent,
    ) -> serde_json::Value {
        let mut machine = recorded_machine(input);
        let input_kind = phase_kind(&machine);
        let result = machine.step(event);
        let params = serde_json::json!({
            "method": method,
            "policy": policy,
            "phase": input_kind,
        });
        let data = match result {
            Ok(_) => serde_json::json!({
                "policy": machine.status().policy,
                "phase": phase_kind(&machine),
            }),
            Err(error) => serde_json::json!({
                "error": { "code": error.code, "detail": error.detail }
            }),
        };
        serde_json::json!({ "params": params, "data": data })
    }

    fn update_transition_records() -> Vec<serde_json::Value> {
        use secs_service::update_machine::UpdateEvent;

        let mut entries = Vec::new();
        for phase in RecordedPhase::ALL {
            for policy in [UpdatePolicy::Auto, UpdatePolicy::Notify, UpdatePolicy::Off] {
                entries.push(transition_entry(
                    "app_settings_set",
                    phase,
                    policy,
                    UpdateEvent::SetPolicy(policy),
                ));
            }
            let policy = recorded_machine(phase).status().policy;
            for (method, event) in [
                ("update_check", UpdateEvent::Check),
                ("update_download", UpdateEvent::Download),
                ("update_restart", UpdateEvent::Restart),
                ("update_notice_dismiss", UpdateEvent::DismissNotice),
            ] {
                entries.push(transition_entry(method, phase, policy, event));
            }
        }
        entries
    }

    #[test]
    fn update_transitions_match_the_recorded_table() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("ui/src/ipc/recorded/update_transitions.json");
        let actual = serde_json::Value::Array(update_transition_records());
        if std::env::var("SECS_RECORD").as_deref() == Ok("1") {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, serde_json::to_string_pretty(&actual).unwrap() + "\n").unwrap();
            return;
        }
        let expected = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
        assert_eq!(
            expected.as_ref(),
            Some(&actual),
            "{} differs from the updater state machine; run SECS_RECORD=1 cargo test -p secs2r update_transitions_match_the_recorded_table",
            path.display()
        );
    }
}
