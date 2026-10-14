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
                .create(params.path.into(), &params.preset, params.sub_count)
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
            .save(params.path.map(Into::into))
            .map(Response::ProjectSave)
            .map_err(IpcError::from),
        Request::ProjectInfo => lock(&service.session)
            .info()
            .map(Response::ProjectInfo)
            .map_err(IpcError::from),
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
    };
    match response {
        Ok(response) => Envelope::Ok(Box::new(response)),
        Err(error) => Envelope::Err(error),
    }
}
