use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

use secs_types::compute::ComputePreference;
use secs_types::ipc::{
    AppIcon, AppSettings, ApplySettings, ErrorCode, IpcError, LocalApiSettings, Notice,
    UpdatePolicy,
};
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
}

/// Receives one application-settings transition after it has been saved.
pub trait SettingsEffect: Send + Sync + std::fmt::Debug {
    fn changed(&self, previous: &AppSettings, next: &AppSettings);
}

/// One owner for the settings file, in-memory revision, typed reads and effects.
pub struct AppSettingsStore {
    path: Option<PathBuf>,
    state: Mutex<State>,
    effects: Mutex<AppEffects>,
    effects_changed: Condvar,
    registered: Vec<Arc<dyn SettingsEffect>>,
}

impl std::fmt::Debug for AppSettingsStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AppSettingsStore")
            .field("path", &self.path)
            .field("state", &self.state)
            .field("registered", &self.registered.len())
            .finish_non_exhaustive()
    }
}

/// Result of opening the application settings.
#[derive(Debug)]
pub struct Opened {
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
        let (settings, read_only_reason, warning) = load(path);
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
                }),
                effects_changed: Condvar::new(),
                registered: Vec::new(),
            },
            notice: None,
            warning,
        }
    }

    pub fn register(&mut self, effect: Arc<dyn SettingsEffect>) {
        self.registered.push(effect);
    }

    pub fn get(&self) -> AppSettings {
        lock(&self.state).settings.clone()
    }

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
                atomic_write(path, &bytes)?;
            }
            state.revision = revision;
            state.settings = settings.clone();
            revision
        };

        // Keep effect delivery in saved-revision order without holding the settings lock.
        let mut effects = lock(&self.effects);
        while revision != effects.revision + 1 {
            effects = self
                .effects_changed
                .wait(effects)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        let previous = effects.settings.clone();
        for effect in &self.registered {
            effect.changed(&previous, &settings);
        }
        effects.revision = revision;
        effects.settings = settings.clone();
        self.effects_changed.notify_all();
        Ok(RevisedAppSettings { revision, settings })
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

    #[allow(dead_code)] // Used by the application effect adapter in ADR 0051 step 3.
    pub fn local_api(&self) -> LocalApiSettings {
        lock(&self.state).settings.local_api
    }

    #[allow(dead_code)] // Used by the application effect adapter in ADR 0051 step 3.
    pub fn icon(&self) -> AppIcon {
        lock(&self.state).settings.icon
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

fn load(path: Option<&Path>) -> (AppSettings, Option<&'static str>, Option<String>) {
    let Some(path) = path else {
        return (AppSettings::default(), None, None);
    };
    if !path.exists() {
        return (AppSettings::default(), None, None);
    }
    let bytes = match read_bounded(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return (
                AppSettings::default(),
                Some("settings_unreadable"),
                Some(format!(
                    "could not read settings file {}: {}",
                    path.display(),
                    error.detail
                )),
            );
        }
    };
    let version = serde_json::from_slice::<AppSettingsVersion>(&bytes).ok();
    let file = match serde_json::from_slice::<AppSettingsFile>(&bytes) {
        Ok(file) => file,
        Err(error) => {
            return (
                AppSettings::default(),
                version
                    .filter(|file| file.version > APP_SETTINGS_VERSION)
                    .map(|_| "newer_version"),
                Some(format!(
                    "could not parse settings file {}: {error}",
                    path.display()
                )),
            );
        }
    };
    if file.version != APP_SETTINGS_VERSION {
        return (
            AppSettings::default(),
            (file.version > APP_SETTINGS_VERSION).then_some("newer_version"),
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
    (settings, None, None)
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
        let mut store = AppSettingsStore::open(None).store;
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

    #[test]
    fn typed_readers_return_the_stored_fields_and_apply_settings_set_touches_nothing_else() {
        let store = AppSettingsStore::open(None).store;
        let mut settings = AppSettings {
            compute: ComputePreference::Gpu,
            updates: UpdatePolicy::Notify,
            icon: AppIcon::Civilian,
            ..AppSettings::default()
        };
        settings.apply.camilla.address = "127.0.0.1:2345".into();
        settings.local_api.enabled = true;
        store.set(settings.clone()).unwrap();
        assert_eq!(store.apply(), settings.apply);
        assert_eq!(store.compute(), settings.compute);
        assert_eq!(store.updates(), settings.updates);
        assert_eq!(store.local_api(), settings.local_api);
        assert_eq!(store.icon(), settings.icon);
    }
}
