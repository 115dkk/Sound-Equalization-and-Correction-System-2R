#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use secs_service::{Deps, Host, NoEvents, Service};
use secs_types::ipc::*;

#[derive(Debug, Default)]
struct RecordingHost {
    calls: Mutex<Vec<String>>,
    replies: Mutex<VecDeque<Option<String>>>,
}

impl RecordingHost {
    fn with_replies(replies: impl IntoIterator<Item = Option<&'static str>>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            replies: Mutex::new(replies.into_iter().map(|v| v.map(str::to_owned)).collect()),
        }
    }

    fn answer(&self, call: String) -> Result<Option<String>, IpcError> {
        self.calls.lock().unwrap().push(call);
        Ok(self.replies.lock().unwrap().pop_front().unwrap_or(None))
    }
}

impl Host for RecordingHost {
    fn dialog_open_project(&self) -> Result<Option<String>, IpcError> {
        self.answer("open_project".into())
    }

    fn dialog_save_project(&self, suggested: &str) -> Result<Option<String>, IpcError> {
        self.answer(format!("save_project:{suggested}"))
    }

    fn dialog_pick_mdat(&self) -> Result<Option<String>, IpcError> {
        self.answer("pick_mdat".into())
    }

    fn dialog_pick_folder(&self, start: Option<&Path>) -> Result<Option<String>, IpcError> {
        self.answer(format!("pick_folder:{}", start.unwrap().display()))
    }

    fn dialog_save_file(
        &self,
        kind: SaveFileKind,
        suggested: &str,
    ) -> Result<Option<String>, IpcError> {
        self.answer(format!("save_file:{kind:?}:{suggested}"))
    }

    fn dialog_open_file(&self, kind: OpenFileKind) -> Result<Option<String>, IpcError> {
        self.answer(format!("open_file:{kind:?}"))
    }

    fn open_folder(&self, path: &Path) -> Result<(), IpcError> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("open_folder:{}", path.display()));
        Ok(())
    }

    fn home_dir(&self) -> Result<PathBuf, IpcError> {
        Ok(PathBuf::from("/home/test"))
    }
}

fn call(service: &Service, request: Request) -> Result<Response, IpcError> {
    match service.call(request, Arc::new(NoEvents)) {
        Envelope::Ok(response) => Ok(*response),
        Envelope::Err(error) => Err(error),
    }
}

#[test]
fn host_requests_forward_parameters_and_cancellation() {
    let host = Arc::new(RecordingHost::with_replies([
        Some("project.secs"),
        Some("saved.secs"),
        Some("room.mdat"),
        Some("exports"),
        Some("patch.csv"),
        None,
    ]));
    let service = Service::new(Deps {
        host: host.clone(),
        ..Deps::default()
    });
    assert_eq!(
        call(&service, Request::DialogOpenProject).unwrap(),
        Response::DialogOpenProject(Some("project.secs".into()))
    );
    call(
        &service,
        Request::DialogSaveProject(DialogSuggestedParams {
            suggested: "new.secs".into(),
        }),
    )
    .unwrap();
    call(&service, Request::DialogPickMdat).unwrap();
    call(
        &service,
        Request::DialogPickFolder(DialogPickFolderParams {
            start: Some("/start".into()),
        }),
    )
    .unwrap();
    call(
        &service,
        Request::DialogSaveFile(DialogSaveFileParams {
            kind: SaveFileKind::PatchSheet,
            suggested: "sheet.csv".into(),
        }),
    )
    .unwrap();
    assert_eq!(
        call(
            &service,
            Request::DialogOpenFile(DialogOpenFileParams {
                kind: OpenFileKind::Target,
            }),
        )
        .unwrap(),
        Response::DialogOpenFile(None)
    );
    call(
        &service,
        Request::OpenFolder(OpenFolderParams {
            path: "/exports".into(),
        }),
    )
    .unwrap();
    assert_eq!(
        *host.calls.lock().unwrap(),
        [
            "open_project",
            "save_project:new.secs",
            "pick_mdat",
            "pick_folder:/start",
            "save_file:PatchSheet:sheet.csv",
            "open_file:Target",
            "open_folder:/exports",
        ]
    );
}

#[test]
fn null_host_reports_not_implemented() {
    let error = call(&Service::new(Deps::default()), Request::DialogOpenProject).unwrap_err();
    assert_eq!(error.code, ErrorCode::NotImplemented);
    let error = call(
        &Service::new(Deps::default()),
        Request::OpenFolder(OpenFolderParams { path: ".".into() }),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::NotImplemented);
}
