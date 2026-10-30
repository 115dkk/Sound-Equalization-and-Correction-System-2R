#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Tray shell for ADR 0018's managed host service.

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use secs_types::ipc::{
    ErrorCode, HostLogLine, HostRequest, HostResponse, HostStatus, IpcError, PendingUpdate,
    ServiceState, UpdatePolicy,
};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_updater::UpdaterExt;

const HOST_EXECUTABLE: &str = if cfg!(target_os = "windows") {
    "secs2r-host.exe"
} else {
    "secs2r-host"
};
const SHELL_HIDDEN_ARGUMENT: &str = "--hidden";
const MAIN_APP_NAMES: [&str; 2] = ["SECS 2R.exe", "secs2r.exe"];
const PORTABLE_MARKER: &str = "portable";
const SETTINGS_FILE: &str = "settings.json";
const PENDING_UPDATE_FILE: &str = "update.json";
const APP_CONFIG_DIRECTORY: &str = "io.github.115dkk.secs2r";
const FIRST_UPDATE_CHECK: Duration = Duration::from_secs(60);
const UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone, Serialize, PartialEq)]
struct HostStatusReply {
    service: ServiceState,
    host: Option<HostStatus>,
}

trait HostPlatform: Send + Sync + 'static {
    fn service_state(&self) -> ServiceState;
    fn pipe_call(&self, request: &HostRequest) -> Result<HostResponse, IpcError>;
    fn install_service(&self, executable: &Path) -> Result<(), IpcError>;
    fn uninstall_service(&self, executable: &Path) -> Result<(), IpcError>;
    fn set_shell_autostart(&self, executable: Option<&Path>) -> Result<(), IpcError>;
    fn spawn_detached(&self, executable: &Path, args: &[&str]) -> Result<(), IpcError>;
    fn main_app_running(&self) -> bool;
}

#[derive(Debug)]
struct NativeHost;

impl HostPlatform for NativeHost {
    fn service_state(&self) -> ServiceState {
        secs_sys::host::service_state()
    }

    fn pipe_call(&self, request: &HostRequest) -> Result<HostResponse, IpcError> {
        secs_sys::host::pipe_call(request)
    }

    fn install_service(&self, executable: &Path) -> Result<(), IpcError> {
        secs_sys::host::install_service_elevated(executable)
    }

    fn uninstall_service(&self, executable: &Path) -> Result<(), IpcError> {
        secs_sys::host::uninstall_service_elevated(executable)
    }

    fn set_shell_autostart(&self, executable: Option<&Path>) -> Result<(), IpcError> {
        secs_sys::host::set_shell_autostart(executable)
    }

    fn spawn_detached(&self, executable: &Path, args: &[&str]) -> Result<(), IpcError> {
        secs_sys::host::spawn_detached(executable, args)
    }

    fn main_app_running(&self) -> bool {
        secs_sys::host::process_running_in_current_session(&MAIN_APP_NAMES).unwrap_or(true)
    }
}

#[derive(Debug)]
struct HostShell<P> {
    platform: P,
    executable: PathBuf,
    package_info: tauri::PackageInfo,
    offered_host_versions: Mutex<BTreeSet<String>>,
    local_logs: Mutex<Vec<HostLogLine>>,
}

impl<P: HostPlatform> HostShell<P> {
    fn new(platform: P, executable: PathBuf) -> Self {
        Self {
            platform,
            executable,
            package_info: tauri::PackageInfo {
                name: "SECS 2R Host".into(),
                version: env!("CARGO_PKG_VERSION")
                    .parse()
                    .expect("Cargo package version must be semver"),
                authors: env!("CARGO_PKG_AUTHORS"),
                description: env!("CARGO_PKG_DESCRIPTION"),
                crate_name: env!("CARGO_PKG_NAME"),
            },
            offered_host_versions: Mutex::new(BTreeSet::new()),
            local_logs: Mutex::new(Vec::new()),
        }
    }

    fn install_directory(&self) -> Result<&Path, IpcError> {
        self.executable.parent().ok_or_else(|| {
            IpcError::new(
                ErrorCode::ApplyFailed,
                "host-shell executable has no install directory",
            )
        })
    }

    fn status(&self) -> Result<HostStatusReply, IpcError> {
        let service = self.platform.service_state();
        let response = (service == ServiceState::Running)
            .then(|| self.platform.pipe_call(&HostRequest::Status));
        let reply = compose_status(service, response)?;
        if let Some(host) = &reply.host {
            self.offer_host_update(host);
        }
        Ok(reply)
    }

    fn offer_host_update(&self, host: &HostStatus) {
        if !version_is_older(&host.version, &self.package_info) {
            return;
        }
        let offered = self.package_info.version.to_string();
        let should_offer = self
            .offered_host_versions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(offered.to_owned());
        if !should_offer {
            return;
        }
        let response = self.install_directory().and_then(|directory| {
            self.platform.pipe_call(&HostRequest::UpdateSelf {
                from_dir: directory.to_string_lossy().into_owned(),
            })
        });
        let (result, detail) = match response {
            Ok(HostResponse::Ok) => ("ok", String::new()),
            Ok(HostResponse::Error(error)) | Err(error) => ("error", error.detail),
            Ok(other) => (
                "error",
                unexpected_response("host self-update", &other).detail,
            ),
        };
        let line = HostLogLine {
            timestamp: rfc3339_now(),
            event: "update_self_answer".into(),
            fields: [
                ("version".into(), offered.to_owned()),
                ("result".into(), result.into()),
                ("detail".into(), detail),
            ]
            .into_iter()
            .filter(|(_, value)| !value.is_empty())
            .collect(),
        };
        self.local_logs
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(line);
    }

    fn log(&self, lines: u32) -> Result<Vec<HostLogLine>, IpcError> {
        let lines = clamp_log_lines(lines);
        let mut host_lines = match self.platform.pipe_call(&HostRequest::Log { lines })? {
            HostResponse::Log(lines) => lines,
            HostResponse::Error(error) => return Err(error),
            response => return Err(unexpected_response("host log", &response)),
        };
        host_lines.extend(
            self.local_logs
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .iter()
                .cloned(),
        );
        let keep = lines as usize;
        if host_lines.len() > keep {
            host_lines.drain(..host_lines.len() - keep);
        }
        Ok(host_lines)
    }

    fn install(&self) -> Result<(), IpcError> {
        let host = self.install_directory()?.join(HOST_EXECUTABLE);
        self.platform.install_service(&host)?;
        self.platform.set_shell_autostart(Some(&self.executable))
    }

    fn uninstall(&self) -> Result<(), IpcError> {
        let host = self.install_directory()?.join(HOST_EXECUTABLE);
        self.platform.uninstall_service(&host)?;
        self.platform.set_shell_autostart(None)
    }

    fn stop_managing(&self) -> Result<(), IpcError> {
        match self.platform.pipe_call(&HostRequest::StopManaging)? {
            HostResponse::Ok => Ok(()),
            HostResponse::Error(error) => Err(error),
            response => Err(unexpected_response("stop managing", &response)),
        }
    }

    fn open_app(&self) -> Result<(), IpcError> {
        let directory = self.install_directory()?;
        let names = std::fs::read_dir(directory)
            .map_err(|error| {
                IpcError::new(
                    ErrorCode::ApplyFailed,
                    format!("read {}: {error}", directory.display()),
                )
            })?
            .filter_map(|entry| entry.ok().map(|entry| entry.file_name()))
            .collect::<Vec<_>>();
        let name = pick_main_app_name(&names).ok_or_else(|| {
            IpcError::new(
                ErrorCode::ApplyFailed,
                format!("SECS 2R executable is missing from {}", directory.display()),
            )
        })?;
        self.platform.spawn_detached(&directory.join(name), &[])
    }
}

fn compose_status(
    service: ServiceState,
    response: Option<Result<HostResponse, IpcError>>,
) -> Result<HostStatusReply, IpcError> {
    if service != ServiceState::Running {
        return Ok(HostStatusReply {
            service,
            host: None,
        });
    }
    match response {
        Some(Ok(HostResponse::Status(host))) => Ok(HostStatusReply {
            service,
            host: Some(*host),
        }),
        Some(Err(_)) => Ok(HostStatusReply {
            service: ServiceState::Unreachable,
            host: None,
        }),
        Some(Ok(HostResponse::Error(error))) => Err(error),
        Some(Ok(response)) => Err(unexpected_response("host status", &response)),
        None => Err(IpcError::new(
            ErrorCode::Internal,
            "running host status has no pipe response",
        )),
    }
}

fn unexpected_response(operation: &str, response: &HostResponse) -> IpcError {
    let response = serde_json::to_string(response).unwrap_or_else(|_| format!("{response:?}"));
    IpcError::new(
        ErrorCode::ApplyFailed,
        format!("unexpected {operation} response: {response}"),
    )
}

#[must_use]
fn clamp_log_lines(lines: u32) -> u32 {
    lines.clamp(1, 200)
}

#[must_use]
fn pick_main_app_name(names: &[OsString]) -> Option<&'static str> {
    MAIN_APP_NAMES
        .into_iter()
        .find(|candidate| names.iter().any(|name| name == OsStr::new(candidate)))
}

#[must_use]
fn started_hidden(args: &[OsString]) -> bool {
    args.iter()
        .any(|argument| argument == OsStr::new(SHELL_HIDDEN_ARGUMENT))
}

fn parse_like<T: std::str::FromStr>(value: &str, _example: &T) -> Option<T> {
    value.parse().ok()
}

fn version_is_older(installed: &str, offered: &tauri::PackageInfo) -> bool {
    parse_like(installed, &offered.version)
        .is_some_and(|installed| installed.cmp_precedence(&offered.version).is_lt())
}

fn rfc3339_now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = i64::try_from(seconds / 86_400).unwrap_or(i64::MAX);
    let seconds_of_day = seconds % 86_400;
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let hour = seconds_of_day / 3_600;
    let minute = seconds_of_day % 3_600 / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellUpdateSkip {
    Portable,
    MainAppRunning,
    Policy,
}

fn shell_update_skip(
    portable: bool,
    main_app_running: bool,
    policy: UpdatePolicy,
) -> Option<ShellUpdateSkip> {
    if portable {
        Some(ShellUpdateSkip::Portable)
    } else if main_app_running {
        Some(ShellUpdateSkip::MainAppRunning)
    } else if policy != UpdatePolicy::Auto {
        Some(ShellUpdateSkip::Policy)
    } else {
        None
    }
}

fn read_update_policy(path: &Path) -> UpdatePolicy {
    let Ok(bytes) = std::fs::read(path) else {
        return UpdatePolicy::Auto;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return UpdatePolicy::Auto;
    };
    match value["settings"]["updates"].as_str() {
        Some("notify") => UpdatePolicy::Notify,
        Some("off") => UpdatePolicy::Off,
        _ => UpdatePolicy::Auto,
    }
}

fn pending_update(
    from: &str,
    to: &str,
    notes: Option<&str>,
    raw_json: &serde_json::Value,
) -> PendingUpdate {
    PendingUpdate {
        from: from.to_owned(),
        to: to.to_owned(),
        notes: notes.unwrap_or_default().to_owned(),
        previous: raw_json["previous"].as_str().map(str::to_owned),
    }
}

fn endpoint_override() -> Option<String> {
    std::env::var("SECS2R_UPDATE_ENDPOINT")
        .ok()
        .filter(|endpoint| !endpoint.trim().is_empty())
}

async fn check_and_install_app_update(app: &AppHandle) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let install_directory = executable
        .parent()
        .ok_or_else(|| "host-shell executable has no install directory".to_owned())?;
    let app_config = app
        .path()
        .config_dir()
        .map_err(|error| error.to_string())?
        .join(APP_CONFIG_DIRECTORY);
    let policy = read_update_policy(&app_config.join(SETTINGS_FILE));
    let shell = app.state::<Arc<HostShell<NativeHost>>>();
    if shell_update_skip(
        install_directory.join(PORTABLE_MARKER).is_file(),
        shell.platform.main_app_running(),
        policy,
    )
    .is_some()
    {
        return Ok(());
    }

    let mut builder = app.updater_builder().restart_after_install(false);
    if let Some(endpoint) = endpoint_override() {
        let endpoint = endpoint
            .parse()
            .map_err(|error| format!("update endpoint: {error}"))?;
        builder = builder
            .endpoints(vec![endpoint])
            .map_err(|error| error.to_string())?;
    }
    let updater = builder.build().map_err(|error| error.to_string())?;
    let Some(update) = updater.check().await.map_err(|error| error.to_string())? else {
        return Ok(());
    };
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|error| error.to_string())?;
    if shell.platform.main_app_running() {
        return Ok(());
    }

    let pending = pending_update(
        env!("CARGO_PKG_VERSION"),
        &update.version,
        update.body.as_deref(),
        &update.raw_json,
    );
    let encoded = serde_json::to_vec_pretty(&pending).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&app_config).map_err(|error| error.to_string())?;
    secs_types::fs::write_atomic_bytes(&app_config.join(PENDING_UPDATE_FILE), &encoded)
        .map_err(|error| error.to_string())?;
    update
        .restart_after_install(false)
        .install(&bytes)
        .map_err(|error| error.to_string())
}

fn start_update_scheduler(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_UPDATE_CHECK);
        loop {
            if let Err(error) = tauri::async_runtime::block_on(check_and_install_app_update(&app)) {
                eprintln!("SECS 2R Host update check failed: {error}");
            }
            std::thread::sleep(UPDATE_CHECK_INTERVAL);
        }
    });
}

async fn run_blocking<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, IpcError> + Send + 'static,
) -> Result<T, IpcError> {
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|error| IpcError::new(ErrorCode::Internal, error.to_string()))?
}

#[tauri::command]
async fn host_status(
    shell: State<'_, Arc<HostShell<NativeHost>>>,
) -> Result<HostStatusReply, IpcError> {
    let shell = Arc::clone(shell.inner());
    run_blocking(move || shell.status()).await
}

#[tauri::command]
async fn host_log(
    lines: u32,
    shell: State<'_, Arc<HostShell<NativeHost>>>,
) -> Result<Vec<HostLogLine>, IpcError> {
    let shell = Arc::clone(shell.inner());
    run_blocking(move || shell.log(lines)).await
}

#[tauri::command]
async fn host_install(shell: State<'_, Arc<HostShell<NativeHost>>>) -> Result<(), IpcError> {
    let shell = Arc::clone(shell.inner());
    run_blocking(move || shell.install()).await
}

#[tauri::command]
async fn host_uninstall(shell: State<'_, Arc<HostShell<NativeHost>>>) -> Result<(), IpcError> {
    let shell = Arc::clone(shell.inner());
    run_blocking(move || shell.uninstall()).await
}

#[tauri::command]
async fn host_stop_managing(shell: State<'_, Arc<HostShell<NativeHost>>>) -> Result<(), IpcError> {
    let shell = Arc::clone(shell.inner());
    run_blocking(move || shell.stop_managing()).await
}

#[tauri::command]
async fn open_app(shell: State<'_, Arc<HostShell<NativeHost>>>) -> Result<(), IpcError> {
    let shell = Arc::clone(shell.inner());
    run_blocking(move || shell.open_app()).await
}

struct TrayMenu {
    open: MenuItem<tauri::Wry>,
    open_app: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
}

#[tauri::command]
async fn tray_labels(
    open: String,
    open_app: String,
    quit: String,
    menu: State<'_, TrayMenu>,
) -> Result<(), IpcError> {
    let open_item = menu.open.clone();
    let open_app_item = menu.open_app.clone();
    let quit_item = menu.quit.clone();
    run_blocking(move || {
        open_item
            .set_text(open)
            .and_then(|()| open_app_item.set_text(open_app))
            .and_then(|()| quit_item.set_text(quit))
            .map_err(|error| {
                IpcError::new(
                    ErrorCode::Internal,
                    format!("set host-shell tray labels: {error}"),
                )
            })
    })
    .await
}

fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn main() {
    let hidden = started_hidden(&std::env::args_os().skip(1).collect::<Vec<_>>());
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }))
        .setup(move |app| {
            let executable = std::env::current_exe()?;
            let shell = Arc::new(HostShell::new(NativeHost, executable));
            let _ = shell.status();
            app.manage(shell);
            start_update_scheduler(app.handle().clone());

            let window =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("host.html".into()))
                    .title("SECS 2R Host")
                    .inner_size(420.0, 560.0)
                    .min_inner_size(420.0, 560.0)
                    .center()
                    .visible(false)
                    .build()?;

            let open = MenuItem::with_id(app, "open", "Open", true, None::<&str>)?;
            let open_app = MenuItem::with_id(app, "open_app", "Open SECS 2R", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &open_app, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("SECS 2R Host")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => show_main(app),
                    "open_app" => {
                        let shell = app.state::<Arc<HostShell<NativeHost>>>();
                        let _ = shell.open_app();
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            app.manage(TrayMenu {
                open,
                open_app,
                quit,
            });

            if !hidden {
                window.show()?;
                window.set_focus()?;
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main"
                && let tauri::WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            host_status,
            host_log,
            host_install,
            host_uninstall,
            host_stop_managing,
            open_app,
            tray_labels
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| {
            eprintln!("SECS 2R Host could not start: {error}");
            std::process::exit(1);
        });
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use secs_types::ipc::{HostManaged, IncludeState, ManagedProcess};

    use super::*;

    #[derive(Debug)]
    struct FakeHost {
        service: ServiceState,
        responses: Mutex<VecDeque<Result<HostResponse, IpcError>>>,
        actions: Mutex<Vec<String>>,
        main_running: bool,
    }

    impl FakeHost {
        fn new(service: ServiceState, responses: Vec<Result<HostResponse, IpcError>>) -> Self {
            Self {
                service,
                responses: Mutex::new(responses.into()),
                actions: Mutex::new(Vec::new()),
                main_running: false,
            }
        }

        fn actions(&self) -> Vec<String> {
            self.actions.lock().unwrap().clone()
        }
    }

    impl HostPlatform for FakeHost {
        fn service_state(&self) -> ServiceState {
            self.service
        }

        fn pipe_call(&self, request: &HostRequest) -> Result<HostResponse, IpcError> {
            self.actions.lock().unwrap().push(format!("{request:?}"));
            self.responses.lock().unwrap().pop_front().unwrap()
        }

        fn install_service(&self, executable: &Path) -> Result<(), IpcError> {
            self.actions
                .lock()
                .unwrap()
                .push(format!("install:{}", executable.display()));
            Ok(())
        }

        fn uninstall_service(&self, executable: &Path) -> Result<(), IpcError> {
            self.actions
                .lock()
                .unwrap()
                .push(format!("uninstall:{}", executable.display()));
            Ok(())
        }

        fn set_shell_autostart(&self, executable: Option<&Path>) -> Result<(), IpcError> {
            self.actions.lock().unwrap().push(match executable {
                Some(path) => format!("autostart:{}", path.display()),
                None => "autostart:off".into(),
            });
            Ok(())
        }

        fn spawn_detached(&self, executable: &Path, args: &[&str]) -> Result<(), IpcError> {
            self.actions.lock().unwrap().push(format!(
                "spawn:{}:{}",
                executable.display(),
                args.join(" ")
            ));
            Ok(())
        }

        fn main_app_running(&self) -> bool {
            self.main_running
        }
    }

    fn host_status_value() -> HostStatus {
        HostStatus {
            version: "1".into(),
            managed: HostManaged::default(),
            camilla: ManagedProcess::default(),
            eqapo_include: IncludeState::Absent,
            last_repair: None,
        }
    }

    #[test]
    fn shell_update_skip_rules_require_installed_idle_auto_mode() {
        assert_eq!(
            shell_update_skip(true, false, UpdatePolicy::Auto),
            Some(ShellUpdateSkip::Portable)
        );
        assert_eq!(
            shell_update_skip(false, true, UpdatePolicy::Auto),
            Some(ShellUpdateSkip::MainAppRunning)
        );
        assert_eq!(
            shell_update_skip(false, false, UpdatePolicy::Notify),
            Some(ShellUpdateSkip::Policy)
        );
        assert_eq!(
            shell_update_skip(false, false, UpdatePolicy::Off),
            Some(ShellUpdateSkip::Policy)
        );
        assert_eq!(shell_update_skip(false, false, UpdatePolicy::Auto), None);
    }

    #[test]
    fn policy_reader_defaults_missing_and_unreadable_and_reads_each_policy() {
        let root = std::env::temp_dir().join(format!(
            "secs2r-host-shell-policy-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(SETTINGS_FILE);
        assert_eq!(read_update_policy(&path), UpdatePolicy::Auto);
        std::fs::create_dir(&path).unwrap();
        assert_eq!(read_update_policy(&path), UpdatePolicy::Auto);
        std::fs::remove_dir(&path).unwrap();
        for (name, expected) in [
            ("auto", UpdatePolicy::Auto),
            ("notify", UpdatePolicy::Notify),
            ("off", UpdatePolicy::Off),
        ] {
            std::fs::write(
                &path,
                format!(r#"{{"version":1,"settings":{{"updates":"{name}"}}}}"#),
            )
            .unwrap();
            assert_eq!(read_update_policy(&path), expected);
        }
        std::fs::write(&path, b"not json").unwrap();
        assert_eq!(read_update_policy(&path), UpdatePolicy::Auto);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn semantic_versions_compare_release_and_prerelease_order() {
        let offered = |version: &str| tauri::PackageInfo {
            name: "test".into(),
            version: version.parse().unwrap(),
            authors: "",
            description: "",
            crate_name: "test",
        };
        assert!(version_is_older("0.35.0", &offered("0.35.1")));
        assert!(version_is_older("1.0.0-alpha.1", &offered("1.0.0-alpha.2")));
        assert!(version_is_older("1.0.0-rc.1", &offered("1.0.0")));
        assert!(!version_is_older("1.0.0", &offered("1.0.0-rc.1")));
        assert!(!version_is_older("1.0", &offered("1.0.1")));
        assert!(!version_is_older("1.0.0", &offered("1.0.0+build.2")));
    }

    #[test]
    fn service_update_is_offered_once_for_the_shell_version() {
        let mut old = host_status_value();
        old.version = "0.35.0".into();
        let fake = FakeHost::new(
            ServiceState::Running,
            vec![
                Ok(HostResponse::Status(Box::new(old.clone()))),
                Ok(HostResponse::Ok),
                Ok(HostResponse::Status(Box::new(old))),
            ],
        );
        let shell = HostShell::new(fake, PathBuf::from("SECS 2R").join("secs2r-host-shell.exe"));

        shell.status().unwrap();
        shell.status().unwrap();

        let actions = shell.platform.actions();
        assert_eq!(
            actions
                .iter()
                .filter(|action| action.starts_with("UpdateSelf"))
                .count(),
            1
        );
        let logs = shell.local_logs.lock().unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].event, "update_self_answer");
        assert_eq!(logs[0].fields.get("result").map(String::as_str), Some("ok"));
    }

    #[test]
    fn pending_update_keeps_notes_and_previous_release() {
        let pending = pending_update(
            "1.0.0",
            "1.2.0",
            Some("notes"),
            &serde_json::json!({ "previous": "1.1.0" }),
        );
        assert_eq!(pending.from, "1.0.0");
        assert_eq!(pending.to, "1.2.0");
        assert_eq!(pending.notes, "notes");
        assert_eq!(pending.previous.as_deref(), Some("1.1.0"));
    }

    #[test]
    fn status_composition_marks_pipe_failures_unreachable() {
        let status = host_status_value();
        assert_eq!(
            compose_status(
                ServiceState::Running,
                Some(Ok(HostResponse::Status(Box::new(status.clone())))),
            )
            .unwrap(),
            HostStatusReply {
                service: ServiceState::Running,
                host: Some(status),
            }
        );
        assert_eq!(
            compose_status(
                ServiceState::Running,
                Some(Err(IpcError::new(ErrorCode::ApplyFailed, "pipe"))),
            )
            .unwrap(),
            HostStatusReply {
                service: ServiceState::Unreachable,
                host: None,
            }
        );
        assert_eq!(
            compose_status(ServiceState::Stopped, None).unwrap(),
            HostStatusReply {
                service: ServiceState::Stopped,
                host: None,
            }
        );
    }

    #[test]
    fn log_line_clamp_stays_between_one_and_two_hundred() {
        assert_eq!(clamp_log_lines(0), 1);
        assert_eq!(clamp_log_lines(50), 50);
        assert_eq!(clamp_log_lines(500), 200);
    }

    #[test]
    fn main_app_selection_prefers_portable_name() {
        let names = [OsString::from("secs2r.exe"), OsString::from("SECS 2R.exe")];
        assert_eq!(pick_main_app_name(&names), Some("SECS 2R.exe"));
        assert_eq!(
            pick_main_app_name(&[OsString::from("secs2r.exe")]),
            Some("secs2r.exe")
        );
        assert_eq!(pick_main_app_name(&[OsString::from("other.exe")]), None);
    }

    #[test]
    fn hidden_argument_is_order_independent() {
        assert!(started_hidden(&[
            OsString::from("anything"),
            OsString::from("--hidden")
        ]));
        assert!(!started_hidden(&[OsString::from("--visible")]));
    }

    #[test]
    fn fake_host_receives_clamped_log_and_install_sequence() {
        let fake = FakeHost::new(
            ServiceState::Running,
            vec![Ok(HostResponse::Log(Vec::new()))],
        );
        // Built from parts: a backslash is no separator off Windows, and the
        // service has no .exe suffix there.
        let folder = PathBuf::from("SECS 2R");
        let shell_path = folder.join("secs2r-host-shell.exe");
        let shell = HostShell::new(fake, shell_path.clone());
        assert!(shell.log(0).unwrap().is_empty());
        shell.install().unwrap();
        assert_eq!(
            shell.platform.actions(),
            [
                "Log { lines: 1 }".to_owned(),
                format!("install:{}", folder.join(HOST_EXECUTABLE).display()),
                format!("autostart:{}", shell_path.display()),
            ]
        );
    }
}
