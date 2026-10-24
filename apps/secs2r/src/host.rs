use std::path::{Path, PathBuf};

use secs_service::{Host, RewLaunchError};
use secs_types::ipc::{ErrorCode, IpcError, OpenFileKind, SaveFileKind};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_dialog::{DialogExt, FileDialogBuilder, FilePath};
use tauri_plugin_opener::OpenerExt;

#[derive(Clone)]
pub(crate) struct TauriHost {
    app: AppHandle,
}

impl std::fmt::Debug for TauriHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TauriHost").finish_non_exhaustive()
    }
}

impl TauriHost {
    pub(crate) fn new(app: AppHandle) -> Self {
        Self { app }
    }

    fn selected(path: Option<FilePath>) -> Result<Option<String>, IpcError> {
        path.map(|path| {
            path.into_path()
                .map(|path| path.to_string_lossy().into_owned())
                .map_err(host_error)
        })
        .transpose()
    }

    fn project_dialog(&self) -> FileDialogBuilder<Wry> {
        self.app
            .dialog()
            .file()
            .add_filter("SECS project", &["secs"])
    }

    fn file_dialog(&self, kind: OpenFileKind) -> FileDialogBuilder<Wry> {
        match kind {
            OpenFileKind::Target => self
                .app
                .dialog()
                .file()
                .add_filter("Target curve", &["json"]),
        }
    }

    fn save_dialog(&self, kind: SaveFileKind) -> FileDialogBuilder<Wry> {
        match kind {
            SaveFileKind::PatchSheet => {
                self.app.dialog().file().add_filter("Patch sheet", &["csv"])
            }
            SaveFileKind::Target => self
                .app
                .dialog()
                .file()
                .add_filter("Target curve", &["json"]),
        }
    }
}

impl Host for TauriHost {
    fn dialog_open_project(&self) -> Result<Option<String>, IpcError> {
        Self::selected(self.project_dialog().blocking_pick_file())
    }

    fn dialog_save_project(&self, suggested: &str) -> Result<Option<String>, IpcError> {
        Self::selected(
            self.project_dialog()
                .set_file_name(suggested)
                .blocking_save_file(),
        )
    }

    fn dialog_pick_mdat(&self) -> Result<Option<String>, IpcError> {
        Self::selected(
            self.app
                .dialog()
                .file()
                .add_filter("Measurements", &["mdat", "liveproject"])
                .add_filter("REW measurement", &["mdat"])
                .add_filter("Dirac Live project", &["liveproject"])
                .blocking_pick_file(),
        )
    }

    fn dialog_pick_folder(&self, start: Option<&Path>) -> Result<Option<String>, IpcError> {
        let mut dialog = self.app.dialog().file();
        if let Some(start) = start {
            dialog = dialog.set_directory(start);
        }
        Self::selected(dialog.blocking_pick_folder())
    }

    fn dialog_save_file(
        &self,
        kind: SaveFileKind,
        suggested: &str,
    ) -> Result<Option<String>, IpcError> {
        Self::selected(
            self.save_dialog(kind)
                .set_file_name(suggested)
                .blocking_save_file(),
        )
    }

    fn dialog_open_file(&self, kind: OpenFileKind) -> Result<Option<String>, IpcError> {
        Self::selected(self.file_dialog(kind).blocking_pick_file())
    }

    fn open_gpu_help(&self) -> Result<(), IpcError> {
        let url = gpu_help_url(std::env::consts::OS).ok_or_else(|| {
            IpcError::new(
                ErrorCode::NotImplemented,
                "CUDA is not supported on this platform",
            )
        })?;
        self.app
            .opener()
            .open_url(url, None::<&str>)
            .map_err(host_error)
    }

    fn open_folder(&self, path: &Path) -> Result<(), IpcError> {
        self.app
            .opener()
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_err(host_error)
    }

    fn home_dir(&self) -> Result<PathBuf, IpcError> {
        self.app.path().home_dir().map_err(host_error)
    }

    fn launch_rew(&self) -> Result<(), RewLaunchError> {
        secs_sys::rew::launch().map_err(|error| match error {
            secs_sys::rew::RewLaunchError::ExeNotFound => {
                RewLaunchError::ExeNotFound(error.to_string())
            }
            secs_sys::rew::RewLaunchError::CannotLaunch(_) => {
                RewLaunchError::CannotLaunch(error.to_string())
            }
            secs_sys::rew::RewLaunchError::LaunchFailed(_) => {
                RewLaunchError::LaunchFailed(error.to_string())
            }
        })
    }
}

fn host_error(error: impl std::fmt::Display) -> IpcError {
    IpcError::new(ErrorCode::Internal, error.to_string())
}

// No URL is accepted from IPC. Only the operating system chooses a fixed guide.
fn gpu_help_url(os: &str) -> Option<&'static str> {
    match os {
        "windows" => Some(
            "https://docs.nvidia.com/cuda/cuda-installation-guide-microsoft-windows/index.html",
        ),
        "linux" => Some("https://docs.nvidia.com/cuda/cuda-installation-guide-linux/index.html"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_help_only_opens_the_two_fixed_official_platform_guides() {
        assert_eq!(
            gpu_help_url("windows"),
            Some(
                "https://docs.nvidia.com/cuda/cuda-installation-guide-microsoft-windows/index.html"
            )
        );
        assert_eq!(
            gpu_help_url("linux"),
            Some("https://docs.nvidia.com/cuda/cuda-installation-guide-linux/index.html")
        );
        assert_eq!(gpu_help_url("macos"), None);
        assert_eq!(gpu_help_url("https://example.com"), None);
    }
}
