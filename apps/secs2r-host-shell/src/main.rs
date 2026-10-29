#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Tray shell for ADR 0018's managed host service.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use secs_types::ipc::{
    ErrorCode, HostLogLine, HostRequest, HostResponse, HostStatus, IpcError, ServiceState,
};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime, State, WebviewUrl, WebviewWindowBuilder};

const HOST_EXECUTABLE: &str = if cfg!(target_os = "windows") {
    "secs2r-host.exe"
} else {
    "secs2r-host"
};
const SHELL_HIDDEN_ARGUMENT: &str = "--hidden";
const MAIN_APP_NAMES: [&str; 2] = ["SECS 2R.exe", "secs2r.exe"];

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
}

#[derive(Debug)]
struct HostShell<P> {
    platform: P,
    executable: PathBuf,
}

impl<P: HostPlatform> HostShell<P> {
    fn new(platform: P, executable: PathBuf) -> Self {
        Self {
            platform,
            executable,
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
        compose_status(service, response)
    }

    fn log(&self, lines: u32) -> Result<Vec<HostLogLine>, IpcError> {
        let lines = clamp_log_lines(lines);
        match self.platform.pipe_call(&HostRequest::Log { lines })? {
            HostResponse::Log(lines) => Ok(lines),
            HostResponse::Error(error) => Err(error),
            response => Err(unexpected_response("host log", &response)),
        }
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
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }))
        .setup(move |app| {
            let executable = std::env::current_exe()?;
            app.manage(Arc::new(HostShell::new(NativeHost, executable)));

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
    }

    impl FakeHost {
        fn new(service: ServiceState, responses: Vec<Result<HostResponse, IpcError>>) -> Self {
            Self {
                service,
                responses: Mutex::new(responses.into()),
                actions: Mutex::new(Vec::new()),
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
        let shell = HostShell::new(fake, PathBuf::from(r"C:\SECS 2R\secs2r-host-shell.exe"));
        assert!(shell.log(0).unwrap().is_empty());
        shell.install().unwrap();
        assert_eq!(
            shell.platform.actions(),
            [
                "Log { lines: 1 }",
                r"install:C:\SECS 2R\secs2r-host.exe",
                r"autostart:C:\SECS 2R\secs2r-host-shell.exe",
            ]
        );
    }
}
