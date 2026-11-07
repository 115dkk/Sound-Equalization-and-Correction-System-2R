use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

use secs_types::compute::ComputePreference;
use secs_types::ipc::{AppSettings, ApplySettings, ErrorCode, IpcError, Notice, UpdatePolicy};
use secs_types::rules::RECENT_PROJECTS_MAX;
use serde::{Deserialize, Serialize};

use crate::{RevisedAppSettings, UpdateAdapter, lock};

const FILE_LIMIT: u64 = 1024 * 1024;
const APP_SETTINGS_VERSION: u32 = 1;

#[derive(Debug)]
struct State {
    revision: u64,
    settings: AppSettings,
    read_only_reason: Option<&'static str>,
}

#[derive(Debug)]
struct AppEffects {
    revision: u64,
    settings: AppSettings,
    pending: BTreeMap<u64, AppSettings>,
    delivering: bool,
}

/// Receives one application-settings transition after it has been saved.
pub trait SettingsEffect: Send + Sync + std::fmt::Debug {
    fn changed(&self, previous: &AppSettings, next: &AppSettings);
}

/// One owner for the settings file, in-memory revision, typed reads and effects.
pub(crate) struct AppSettingsStore {
    path: Option<PathBuf>,
    state: Mutex<State>,
    effects: Mutex<AppEffects>,
    effects_changed: Condvar,
    registered: Mutex<Vec<Arc<dyn SettingsEffect>>>,
}

impl std::fmt::Debug for AppSettingsStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AppSettingsStore")
            .field("path", &self.path)
            .field("state", &self.state)
            .field("registered", &lock(&self.registered).len())
            .finish_non_exhaustive()
    }
}

/// Result of opening the application settings.
#[derive(Debug)]
pub(crate) struct Opened {
    pub store: AppSettingsStore,
    pub notice: Option<Notice>,
    pub(crate) warning: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AppSettingsFile {
    version: u32,
    settings: AppSettings,
}

#[derive(Debug, Deserialize)]
struct AppSettingsVersion {
    version: u32,
}

impl AppSettingsStore {
    pub fn open(path: Option<&Path>) -> Opened {
        let (settings, read_only_reason, notice, warning) = load(path);
        Opened {
            store: Self {
                path: path.map(Path::to_path_buf),
                state: Mutex::new(State {
                    revision: 1,
                    settings: settings.clone(),
                    read_only_reason,
                }),
                effects: Mutex::new(AppEffects {
                    revision: 1,
                    settings,
                    pending: BTreeMap::new(),
                    delivering: false,
                }),
                effects_changed: Condvar::new(),
                registered: Mutex::new(Vec::new()),
            },
            notice,
            warning,
        }
    }

    pub fn register(&self, effect: Arc<dyn SettingsEffect>) {
        lock(&self.registered).push(effect);
    }

    pub fn get(&self) -> AppSettings {
        lock(&self.state).settings.clone()
    }

    #[cfg(test)]
    pub fn revised(&self) -> RevisedAppSettings {
        let state = lock(&self.state);
        RevisedAppSettings {
            revision: state.revision,
            settings: state.settings.clone(),
        }
    }

    pub fn set(&self, mut settings: AppSettings) -> Result<RevisedAppSettings, IpcError> {
        settings.recent_projects.truncate(RECENT_PROJECTS_MAX);
        if let Some((width, height)) = settings.last_window_size
            && (width == 0 || height == 0)
        {
            return Err(invalid(
                "last_window_size",
                "window dimensions must be positive",
            ));
        }
        let bytes = serde_json::to_vec_pretty(&AppSettingsFile {
            version: APP_SETTINGS_VERSION,
            settings: settings.clone(),
        })
        .map_err(|error| invalid("settings", error.to_string()))?;
        if bytes.len() as u64 > FILE_LIMIT {
            return Err(invalid("settings", "settings exceed 1 MiB"));
        }

        let revision = {
            let mut state = lock(&self.state);
            if let Some(reason) = state.read_only_reason {
                let reason = if reason == "newer_version" {
                    "app_settings_newer"
                } else {
                    reason
                };
                return Err(IpcError::new(
                    ErrorCode::Unsupported,
                    "existing settings cannot be overwritten",
                )
                .with_param("reason", reason));
            }
            let revision = state.revision.checked_add(1).ok_or_else(|| {
                IpcError::new(ErrorCode::Internal, "app settings revision exhausted")
            })?;
            if let Some(path) = &self.path {
                if read_bounded(path)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<AppSettingsVersion>(&bytes).ok())
                    .is_some_and(|file| file.version > APP_SETTINGS_VERSION)
                {
                    state.read_only_reason = Some("newer_version");
                    return Err(IpcError::new(
                        ErrorCode::Unsupported,
                        "existing settings cannot be overwritten",
                    )
                    .with_param("reason", "app_settings_newer"));
                }
                atomic_write(path, &bytes)?;
            }
            state.revision = revision;
            state.settings = settings.clone();
            revision
        };

        // One drainer delivers saved revisions in order. A re-entrant set only
        // queues its revision, so no store lock is held while an effect runs.
        let mut effects = lock(&self.effects);
        effects.pending.insert(revision, settings.clone());
        self.effects_changed.notify_all();
        if effects.delivering {
            return Ok(RevisedAppSettings { revision, settings });
        }
        effects.delivering = true;
        loop {
            let next_revision = effects.revision + 1;
            while !effects.pending.contains_key(&next_revision) {
                if effects.pending.is_empty() {
                    effects.delivering = false;
                    self.effects_changed.notify_all();
                    return Ok(RevisedAppSettings { revision, settings });
                }
                effects = self
                    .effects_changed
                    .wait(effects)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            let next = effects
                .pending
                .remove(&next_revision)
                .expect("pending settings revision disappeared");
            let previous = effects.settings.clone();
            let registered = lock(&self.registered).clone();
            drop(effects);

            for effect in &registered {
                effect.changed(&previous, &next);
            }

            effects = lock(&self.effects);
            effects.revision = next_revision;
            effects.settings = next;
            self.effects_changed.notify_all();
        }
    }

    pub fn apply(&self) -> ApplySettings {
        lock(&self.state).settings.apply.clone()
    }

    pub fn compute(&self) -> ComputePreference {
        lock(&self.state).settings.compute
    }

    pub fn updates(&self) -> UpdatePolicy {
        lock(&self.state).settings.updates
    }
}

#[derive(Debug)]
pub(crate) struct UpdatePolicyEffect(pub Arc<dyn UpdateAdapter>);

impl SettingsEffect for UpdatePolicyEffect {
    fn changed(&self, previous: &AppSettings, next: &AppSettings) {
        if previous.updates != next.updates {
            self.0.set_policy(next.updates);
        }
    }
}

fn load(
    path: Option<&Path>,
) -> (
    AppSettings,
    Option<&'static str>,
    Option<Notice>,
    Option<String>,
) {
    let Some(path) = path else {
        return (AppSettings::default(), None, None, None);
    };
    if !path.exists() {
        return (AppSettings::default(), None, None, None);
    }
    let unreadable = |error: String, read_only_reason| {
        let notice = Notice {
            code: "app_settings_unreadable".into(),
            params: [
                ("path".into(), path.to_string_lossy().into_owned()),
                ("error".into(), error.clone()),
            ]
            .into(),
        };
        (
            AppSettings::default(),
            read_only_reason,
            Some(notice),
            Some(format!(
                "could not read settings file {}: {error}",
                path.display()
            )),
        )
    };
    let bytes = match read_bounded(path) {
        Ok(bytes) => bytes,
        Err(error) => return unreadable(error.detail, Some("settings_unreadable")),
    };
    let version = serde_json::from_slice::<AppSettingsVersion>(&bytes).ok();
    let file = match serde_json::from_slice::<AppSettingsFile>(&bytes) {
        Ok(file) => file,
        Err(error) => {
            if let Some(version) = version.filter(|file| file.version > APP_SETTINGS_VERSION) {
                return newer(path, version.version);
            }
            return unreadable(error.to_string(), None);
        }
    };
    if file.version > APP_SETTINGS_VERSION {
        return newer(path, file.version);
    }
    if file.version < APP_SETTINGS_VERSION {
        return (
            AppSettings::default(),
            None,
            None,
            Some(format!(
                "unsupported settings version {} in {} (supported version {})",
                file.version,
                path.display(),
                APP_SETTINGS_VERSION
            )),
        );
    }
    let mut settings = file.settings;
    settings.recent_projects.truncate(RECENT_PROJECTS_MAX);
    (settings, None, None, None)
}

fn newer(
    path: &Path,
    version: u32,
) -> (
    AppSettings,
    Option<&'static str>,
    Option<Notice>,
    Option<String>,
) {
    (
        AppSettings::default(),
        Some("newer_version"),
        Some(Notice {
            code: "app_settings_newer".into(),
            params: [("version".into(), version.to_string())].into(),
        }),
        Some(format!(
            "unsupported settings version {version} in {} (supported version {APP_SETTINGS_VERSION})",
            path.display()
        )),
    )
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, IpcError> {
    let file = fs::File::open(path).map_err(|error| project_io(path, error))?;
    let length = file
        .metadata()
        .map_err(|error| project_io(path, error))?
        .len();
    if length > FILE_LIMIT {
        return Err(project_io(path, "file exceeds 1 MiB"));
    }
    let mut bytes = Vec::with_capacity(length as usize);
    file.take(FILE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| project_io(path, error))?;
    if bytes.len() as u64 > FILE_LIMIT {
        return Err(project_io(path, "file exceeds 1 MiB"));
    }
    Ok(bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), IpcError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| project_io(path, error))?;
    secs_types::fs::write_atomic_bytes(path, bytes).map_err(|error| project_io(path, error))
}

fn invalid(field: &str, detail: impl Into<String>) -> IpcError {
    IpcError::new(ErrorCode::InvalidRequest, detail).with_param("field", field)
}

fn project_io(path: &Path, error: impl ToString) -> IpcError {
    IpcError::new(ErrorCode::ProjectIo, error.to_string())
        .with_param("path", path.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn root(name: &str) -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "secs-app-settings-{}-{name}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn open_reports_a_damaged_or_oversized_file_as_app_settings_unreadable() {
        let root = root("unreadable");
        for (name, bytes) in [
            ("damaged", b"{not json".to_vec()),
            ("oversized", vec![b'x'; FILE_LIMIT as usize + 1]),
        ] {
            let path = root.join(format!("{name}.json"));
            std::fs::write(&path, bytes).unwrap();
            let opened = AppSettingsStore::open(Some(&path));
            assert_eq!(opened.store.get(), AppSettings::default());
            let notice = opened.notice.expect("settings notice is absent");
            assert_eq!(notice.code, "app_settings_unreadable");
            assert_eq!(
                notice.params.get("path").map(String::as_str),
                Some(path.to_string_lossy().as_ref())
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn open_keeps_a_newer_file_read_only_and_set_refuses_it() {
        let root = root("newer");
        let path = root.join("settings.json");
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version": APP_SETTINGS_VERSION + 1,
            "settings": AppSettings::default(),
        }))
        .unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let opened = AppSettingsStore::open(Some(&path));
        let notice = opened.notice.expect("newer settings notice is absent");
        assert_eq!(notice.code, "app_settings_newer");
        assert_eq!(notice.params.get("version").map(String::as_str), Some("2"));
        let error = opened.store.set(AppSettings::default()).unwrap_err();
        assert_eq!(error.code, ErrorCode::Unsupported);
        assert_eq!(
            error.params.get("reason").map(String::as_str),
            Some("app_settings_newer")
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn open_without_a_file_starts_from_defaults_without_a_notice() {
        let root = root("missing");
        let opened = AppSettingsStore::open(Some(&root.join("missing.json")));
        assert_eq!(opened.store.get(), AppSettings::default());
        assert!(opened.notice.is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn set_increments_the_revision_and_writes_atomically_and_a_rejected_value_changes_nothing() {
        let root = root("set");
        let path = root.join("settings.json");
        let store = AppSettingsStore::open(Some(&path)).store;
        let mut settings = AppSettings {
            language: "en".into(),
            ..AppSettings::default()
        };
        assert_eq!(store.set(settings.clone()).unwrap().revision, 2);
        assert_eq!(store.revised().settings, settings);
        let bytes = std::fs::read(&path).unwrap();
        settings.last_window_size = Some((0, 720));
        assert_eq!(
            store.set(settings).unwrap_err().code,
            ErrorCode::InvalidRequest
        );
        assert_eq!(store.revised().revision, 2);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(!path.with_extension("json.tmp").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[derive(Debug)]
    struct RecordingEffect {
        calls: Arc<Mutex<Vec<(String, String)>>>,
        entered: Option<Arc<(Mutex<bool>, Condvar)>>,
        release: Option<Arc<(Mutex<bool>, Condvar)>>,
        first: AtomicBool,
    }

    impl SettingsEffect for RecordingEffect {
        fn changed(&self, previous: &AppSettings, next: &AppSettings) {
            self.calls
                .lock()
                .unwrap()
                .push((previous.language.clone(), next.language.clone()));
            if self.first.swap(false, Ordering::SeqCst) {
                if let Some(entered) = &self.entered {
                    *entered.0.lock().unwrap() = true;
                    entered.1.notify_all();
                }
                if let Some(release) = &self.release {
                    let mut ready = release.0.lock().unwrap();
                    while !*ready {
                        ready = release.1.wait(ready).unwrap();
                    }
                }
            }
        }
    }

    #[test]
    fn effects_run_once_per_revision_in_order_even_when_two_threads_interleave() {
        let store = AppSettingsStore::open(None).store;
        let calls_a = Arc::new(Mutex::new(Vec::new()));
        let calls_b = Arc::new(Mutex::new(Vec::new()));
        let entered = Arc::new((Mutex::new(false), Condvar::new()));
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        store.register(Arc::new(RecordingEffect {
            calls: calls_a.clone(),
            entered: Some(entered.clone()),
            release: Some(release.clone()),
            first: AtomicBool::new(true),
        }));
        store.register(Arc::new(RecordingEffect {
            calls: calls_b.clone(),
            entered: None,
            release: None,
            first: AtomicBool::new(false),
        }));
        let store = Arc::new(store);
        let first_store = store.clone();
        let first = std::thread::spawn(move || {
            first_store
                .set(AppSettings {
                    language: "en".into(),
                    ..AppSettings::default()
                })
                .unwrap();
        });
        let mut did_enter = entered.0.lock().unwrap();
        while !*did_enter {
            did_enter = entered.1.wait(did_enter).unwrap();
        }
        drop(did_enter);
        let second_store = store.clone();
        let second = std::thread::spawn(move || {
            second_store
                .set(AppSettings {
                    language: "fr".into(),
                    ..AppSettings::default()
                })
                .unwrap();
        });
        *release.0.lock().unwrap() = true;
        release.1.notify_all();
        first.join().unwrap();
        second.join().unwrap();
        let expected = vec![("ko".into(), "en".into()), ("en".into(), "fr".into())];
        assert_eq!(*calls_a.lock().unwrap(), expected);
        assert_eq!(*calls_b.lock().unwrap(), expected);
    }

    #[derive(Debug)]
    struct ReentrantEffect {
        store: std::sync::Weak<AppSettingsStore>,
        calls: Arc<Mutex<Vec<(String, String)>>>,
        first: AtomicBool,
    }

    impl SettingsEffect for ReentrantEffect {
        fn changed(&self, previous: &AppSettings, next: &AppSettings) {
            self.calls
                .lock()
                .unwrap()
                .push((previous.language.clone(), next.language.clone()));
            if self.first.swap(false, Ordering::SeqCst) {
                self.store
                    .upgrade()
                    .expect("settings store was dropped")
                    .set(AppSettings {
                        language: "fr".into(),
                        ..next.clone()
                    })
                    .unwrap();
            }
        }
    }

    #[test]
    fn an_effect_that_sets_again_is_delivered_after_the_current_revision_without_deadlock() {
        let store = Arc::new(AppSettingsStore::open(None).store);
        let reentrant_calls = Arc::new(Mutex::new(Vec::new()));
        let recording_calls = Arc::new(Mutex::new(Vec::new()));
        store.register(Arc::new(ReentrantEffect {
            store: Arc::downgrade(&store),
            calls: reentrant_calls.clone(),
            first: AtomicBool::new(true),
        }));
        store.register(Arc::new(RecordingEffect {
            calls: recording_calls.clone(),
            entered: None,
            release: None,
            first: AtomicBool::new(false),
        }));

        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        let worker_store = store.clone();
        std::thread::spawn(move || {
            let result = worker_store.set(AppSettings {
                language: "en".into(),
                ..AppSettings::default()
            });
            finished_tx.send(result).ok();
        });
        let result = finished_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("re-entrant settings set timed out");
        assert_eq!(result.unwrap().revision, 2);

        let expected = vec![("ko".into(), "en".into()), ("en".into(), "fr".into())];
        assert_eq!(*reentrant_calls.lock().unwrap(), expected);
        assert_eq!(*recording_calls.lock().unwrap(), expected);
        assert_eq!(store.revised().revision, 3);
        assert_eq!(store.get().language, "fr");
    }

    #[derive(Debug)]
    struct RecordingUpdates(Arc<Mutex<Vec<UpdatePolicy>>>);

    impl UpdateAdapter for RecordingUpdates {
        fn status(&self) -> secs_types::ipc::UpdateStatus {
            secs_types::ipc::UpdateStatus {
                current: String::new(),
                policy: self
                    .0
                    .lock()
                    .unwrap()
                    .last()
                    .copied()
                    .unwrap_or(UpdatePolicy::Off),
                phase: secs_types::ipc::UpdatePhase::Off,
                checked_at: None,
                notice: None,
            }
        }

        fn check(&self) -> Result<secs_types::ipc::UpdateStatus, IpcError> {
            unimplemented!()
        }

        fn download(&self) -> Result<secs_types::ipc::UpdateStatus, IpcError> {
            unimplemented!()
        }

        fn restart(&self) -> Result<(), IpcError> {
            unimplemented!()
        }

        fn dismiss_notice(&self) -> secs_types::ipc::UpdateStatus {
            self.status()
        }

        fn set_policy(&self, policy: UpdatePolicy) {
            self.0.lock().unwrap().push(policy);
        }
    }

    #[test]
    fn update_policy_effect_receives_the_last_policy_in_revision_order() {
        let store = AppSettingsStore::open(None).store;
        let entered = Arc::new((Mutex::new(false), Condvar::new()));
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        store.register(Arc::new(RecordingEffect {
            calls: Arc::new(Mutex::new(Vec::new())),
            entered: Some(entered.clone()),
            release: Some(release.clone()),
            first: AtomicBool::new(true),
        }));
        let policies = Arc::new(Mutex::new(Vec::new()));
        store.register(Arc::new(UpdatePolicyEffect(Arc::new(RecordingUpdates(
            policies.clone(),
        )))));
        let store = Arc::new(store);

        let first_store = store.clone();
        let first = std::thread::spawn(move || {
            first_store
                .set(AppSettings {
                    updates: UpdatePolicy::Notify,
                    ..AppSettings::default()
                })
                .unwrap();
        });
        let mut did_enter = entered.0.lock().unwrap();
        while !*did_enter {
            did_enter = entered.1.wait(did_enter).unwrap();
        }
        drop(did_enter);
        let second_store = store.clone();
        let second = std::thread::spawn(move || {
            second_store
                .set(AppSettings {
                    updates: UpdatePolicy::Off,
                    ..AppSettings::default()
                })
                .unwrap();
        });
        *release.0.lock().unwrap() = true;
        release.1.notify_all();
        first.join().unwrap();
        second.join().unwrap();

        assert_eq!(
            *policies.lock().unwrap(),
            [UpdatePolicy::Notify, UpdatePolicy::Off]
        );
        assert_eq!(store.updates(), UpdatePolicy::Off);
    }

    #[test]
    fn typed_readers_return_the_stored_fields_and_apply_settings_set_touches_nothing_else() {
        let store = AppSettingsStore::open(None).store;
        let mut settings = AppSettings {
            compute: ComputePreference::Gpu,
            updates: UpdatePolicy::Notify,
            icon: secs_types::ipc::AppIcon::Civilian,
            ..AppSettings::default()
        };
        settings.apply.camilla.address = "127.0.0.1:2345".into();
        settings.local_api.enabled = true;
        store.set(settings.clone()).unwrap();
        assert_eq!(store.apply(), settings.apply);
        assert_eq!(store.compute(), settings.compute);
        assert_eq!(store.updates(), settings.updates);
        assert_eq!(store.get(), settings);
    }
}
