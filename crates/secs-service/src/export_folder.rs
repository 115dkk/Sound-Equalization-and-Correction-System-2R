use std::path::{Path, PathBuf};

use secs_project::PyJson;
use secs_types::ipc::{ExportFolderResult, ExportFolderSetParams, IpcError};

use crate::measure::json::JsonExt;
use crate::{LocalTime, Service, lock};

impl Service {
    pub(crate) fn export_folder_get(&self) -> Result<ExportFolderResult, IpcError> {
        let session = lock(&self.session);
        let project_path = match session.require() {
            Ok(open) => {
                if let Some(folder) = stored_folder(open.store.project()) {
                    return Ok(ExportFolderResult {
                        folder: folder.to_owned(),
                        stored: true,
                    });
                }
                Some(open.path.as_path())
            }
            Err(crate::ServiceError::ProjectNotOpen) => None,
            Err(error) => return Err(error.into()),
        };
        let folder = default_export_folder(
            project_path,
            &self.deps.host.home_dir()?,
            (self.deps.clock)(),
        );
        Ok(ExportFolderResult {
            folder: folder.to_string_lossy().into_owned(),
            stored: false,
        })
    }

    pub(crate) fn export_folder_set(
        &self,
        params: ExportFolderSetParams,
    ) -> Result<ExportFolderResult, IpcError> {
        let mut session = lock(&self.session);
        let open = session.require_mut()?;
        set_stored_folder(open.store.project_mut(), &params.folder);
        open.dirty = true;
        Ok(ExportFolderResult {
            folder: params.folder,
            stored: true,
        })
    }

    pub(crate) fn resolved_export_base(&self) -> Result<PathBuf, IpcError> {
        Ok(PathBuf::from(self.export_folder_get()?.folder))
    }
}

pub(crate) fn default_export_folder(
    project_path: Option<&Path>,
    home: &Path,
    now: LocalTime,
) -> PathBuf {
    let date = now.date();
    match project_path {
        Some(project) => project
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("SECS_export")
            .join(format!(
                "{date} {}",
                project
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("project")
            )),
        None => home.join("Documents").join("SECS_export").join(date),
    }
}

/// Derives the timestamped mdat destination below an export base.
pub fn mdat_folder(base: &Path, mimo: bool, mode: &str, now: LocalTime) -> PathBuf {
    timestamped_folder(base, "mdat", mimo, mode, now)
}

/// Derives the timestamped CamillaDSP destination below an export base.
pub fn camilladsp_folder(base: &Path, mimo: bool, mode: &str, now: LocalTime) -> PathBuf {
    timestamped_folder(base, "camilla", mimo, mode, now).join("camilladsp")
}

/// Derives the timestamped filter-text destination below an export base.
pub fn text_folder(base: &Path, mimo: bool, mode: &str, now: LocalTime) -> PathBuf {
    timestamped_folder(base, "text", mimo, mode, now)
}

/// Derives the timestamped Equalizer APO destination below an export base.
pub fn eqapo_folder(base: &Path, mimo: bool, mode: &str, now: LocalTime) -> PathBuf {
    timestamped_folder(base, "eqapo", mimo, mode, now)
}

fn timestamped_folder(base: &Path, kind: &str, mimo: bool, mode: &str, now: LocalTime) -> PathBuf {
    let what = if mimo { "RC_BA_MIMO" } else { "RC_BA" };
    base.join(format!("SECS_{kind}_{what}_{mode}_{}", now.stamp()))
}

fn stored_folder(project: &secs_project::Project) -> Option<&str> {
    project
        .extra
        .iter()
        .find(|(key, _)| key == "export")
        .and_then(|(_, value)| value.get("folder"))
        .and_then(|value| match value {
            PyJson::Str(value) if !value.is_empty() => Some(value.as_str()),
            _ => None,
        })
}

fn set_stored_folder(project: &mut secs_project::Project, folder: &str) {
    let export = if let Some((_, value)) = project.extra.iter_mut().find(|(key, _)| key == "export")
    {
        value
    } else {
        project
            .extra
            .push(("export".into(), PyJson::Object(Vec::new())));
        &mut project.extra.last_mut().expect("just inserted").1
    };
    if !matches!(export, PyJson::Object(_)) {
        *export = PyJson::Object(Vec::new());
    }
    let PyJson::Object(fields) = export else {
        unreachable!()
    };
    if let Some((_, value)) = fields.iter_mut().find(|(key, _)| key == "folder") {
        *value = PyJson::Str(folder.to_owned());
    } else {
        fields.push(("folder".into(), PyJson::Str(folder.to_owned())));
    }
}
