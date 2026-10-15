use std::path::Path;

use secs_types::ipc::{
    DialogOpenFileParams, DialogPickFolderParams, DialogSaveFileParams, DialogSuggestedParams,
    IpcError, OpenFolderParams,
};

use crate::Service;

impl Service {
    pub(crate) fn dialog_open_project(&self) -> Result<Option<String>, IpcError> {
        self.deps.host.dialog_open_project()
    }

    pub(crate) fn dialog_save_project(
        &self,
        params: DialogSuggestedParams,
    ) -> Result<Option<String>, IpcError> {
        self.deps.host.dialog_save_project(&params.suggested)
    }

    pub(crate) fn dialog_pick_mdat(&self) -> Result<Option<String>, IpcError> {
        self.deps.host.dialog_pick_mdat()
    }

    pub(crate) fn dialog_pick_folder(
        &self,
        params: DialogPickFolderParams,
    ) -> Result<Option<String>, IpcError> {
        self.deps
            .host
            .dialog_pick_folder(params.start.as_deref().map(Path::new))
    }

    pub(crate) fn dialog_save_file(
        &self,
        params: DialogSaveFileParams,
    ) -> Result<Option<String>, IpcError> {
        self.deps
            .host
            .dialog_save_file(params.kind, &params.suggested)
    }

    pub(crate) fn dialog_open_file(
        &self,
        params: DialogOpenFileParams,
    ) -> Result<Option<String>, IpcError> {
        self.deps.host.dialog_open_file(params.kind)
    }

    pub(crate) fn open_folder(&self, params: OpenFolderParams) -> Result<(), IpcError> {
        self.deps.host.open_folder(Path::new(&params.path))
    }
}
