use std::sync::Arc;

use secs_types::ipc::{Envelope, IpcError, Request, Response};

use crate::{EventSink, Service, lock};

pub(crate) fn dispatch(
    service: &Service,
    request: Request,
    events: Arc<dyn EventSink>,
) -> Envelope {
    let response = match request {
        Request::Bootstrap => Ok(Response::Bootstrap(service.bootstrap())),
        Request::ProjectNew(params) => {
            let response = lock(&service.session)
                .create(
                    params.path.into(),
                    &params.preset,
                    params.sub_count,
                    params.sample,
                )
                .map(Response::ProjectNew)
                .map_err(IpcError::from);
            if response.is_ok() {
                *lock(&service.results_state) = Default::default();
            }
            response
        }
        Request::ProjectOpen(params) => {
            let response = lock(&service.session)
                .open(params.path.into())
                .map(Response::ProjectOpen)
                .map_err(IpcError::from);
            if response.is_ok() {
                *lock(&service.results_state) = Default::default();
            }
            response
        }
        Request::ProjectSave(params) => lock(&service.session)
            .save(params.path.map(Into::into), &service.deps.uuid)
            .map(Response::ProjectSave)
            .map_err(IpcError::from),
        Request::ProjectInfo => lock(&service.session)
            .info()
            .map(Response::ProjectInfo)
            .map_err(IpcError::from),
        Request::SampleLoad(params) => service
            .start_sample_load(params, events)
            .map(Response::SampleLoad),
        Request::JobCancel(params) => service.jobs.cancel(params.job).map(Response::JobCancel),
        Request::Jobs => Ok(Response::Jobs(service.jobs.snapshots())),
        Request::Calibrate(params) => service
            .start_calibration(params, events)
            .map(Response::Calibrate),
        Request::RecommendXo => service.start_xo(events).map(Response::RecommendXo),
        Request::Mimo(params) => service.start_mimo(params, events).map(Response::Mimo),
        Request::Results => service.results().map(Response::Results),
        Request::Export(params) => service.start_export(params, events).map(Response::Export),
        Request::MeasureMethods => Ok(Response::MeasureMethods(service.measure_methods())),
        Request::MeasureConfigure(params) => service
            .measure_configure(params)
            .map(Response::MeasureConfigure),
        Request::LevelCheck(params) => service
            .start_level(params, events)
            .map(Response::LevelCheck),
        Request::LevelApply(params) => service.level_apply(params).map(Response::LevelApply),
        Request::MeasurePosition(params) => service
            .start_measure(params, events)
            .map(Response::MeasurePosition),
        Request::PositionClear(params) => {
            service.position_clear(params).map(Response::PositionClear)
        }
        Request::Undo(params) => service.undo(params).map(Response::Undo),
        Request::RewLaunch => service.rew_launch().map(Response::RewLaunch),
        Request::MdatScan(params) => service.mdat_scan(params).map(Response::MdatScan),
        Request::MdatImport(params) => service.start_mdat(params, events).map(Response::MdatImport),
        Request::DialogOpenProject => service
            .dialog_open_project()
            .map(Response::DialogOpenProject),
        Request::DialogSaveProject(params) => service
            .dialog_save_project(params)
            .map(Response::DialogSaveProject),
        Request::DialogPickMdat => service.dialog_pick_mdat().map(Response::DialogPickMdat),
        Request::DialogPickFolder(params) => service
            .dialog_pick_folder(params)
            .map(Response::DialogPickFolder),
        Request::DialogSaveFile(params) => service
            .dialog_save_file(params)
            .map(Response::DialogSaveFile),
        Request::DialogOpenFile(params) => service
            .dialog_open_file(params)
            .map(Response::DialogOpenFile),
        Request::OpenFolder(params) => service.open_folder(params).map(Response::OpenFolder),
        Request::LevelMonitorStart(params) => service
            .start_level_monitor(params, events)
            .map(Response::LevelMonitorStart),
        Request::LevelMonitorStop => service.stop_level_monitor().map(Response::LevelMonitorStop),
        Request::LevelPlay(params) => service.level_play(params).map(Response::LevelPlay),
        Request::StatusWatchStart => service
            .start_status_watch(events)
            .map(Response::StatusWatchStart),
        Request::StatusWatchStop => service.stop_status_watch().map(Response::StatusWatchStop),
        Request::ExportFolderGet => service.export_folder_get().map(Response::ExportFolderGet),
        Request::ExportFolderSet(params) => service
            .export_folder_set(params)
            .map(Response::ExportFolderSet),
    };
    match response {
        Ok(response) => Envelope::Ok(Box::new(response)),
        Err(error) => Envelope::Err(error),
    }
}
