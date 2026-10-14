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
        Request::ConfigGet => service.config_get().map(Response::ConfigGet),
        Request::ConfigApplyPreset(params) => service
            .config_apply_preset(params)
            .map(Response::ConfigApplyPreset),
        Request::ConfigSetSubCount(params) => service
            .config_set_sub_count(params)
            .map(Response::ConfigSetSubCount),
        Request::ConfigSetSpeaker(params) => service
            .config_set_speaker(params)
            .map(Response::ConfigSetSpeaker),
        Request::ConfigAddSpeaker => service.config_add_speaker().map(Response::ConfigAddSpeaker),
        Request::ConfigRemoveSpeaker(params) => service
            .config_remove_speaker(params)
            .map(Response::ConfigRemoveSpeaker),
        Request::ConfigRenumber => service.config_renumber().map(Response::ConfigRenumber),
        Request::ConfigValidate => service.config_validate().map(Response::ConfigValidate),
        Request::PatchSheetCsv(params) => {
            service.patch_sheet_csv(params).map(Response::PatchSheetCsv)
        }
        Request::DspGet => service.dsp_get().map(Response::DspGet),
        Request::DspSet(params) => service.dsp_set(params).map(Response::DspSet),
        Request::LayoutGet => service.layout_get().map(Response::LayoutGet),
        Request::LayoutSet(params) => service.layout_set(params).map(Response::LayoutSet),
        Request::PositionEstimate => service.position_estimate().map(Response::PositionEstimate),
        Request::TargetGet => service.target_get().map(Response::TargetGet),
        Request::TargetSetPoints(params) => service
            .target_set_points(params)
            .map(Response::TargetSetPoints),
        Request::TargetApplyPreset(params) => service
            .target_apply_preset(params)
            .map(Response::TargetApplyPreset),
        Request::TargetSave(params) => service.target_save(params).map(Response::TargetSave),
        Request::TargetLoad(params) => service.target_load(params).map(Response::TargetLoad),
        Request::CalibrationSettingsGet => service
            .calibration_settings_get()
            .map(Response::CalibrationSettingsGet),
        Request::CalibrationSettingsSet(params) => service
            .calibration_settings_set(params)
            .map(Response::CalibrationSettingsSet),
        Request::ModeSpec(params) => Ok(Response::ModeSpec(service.mode_spec(params))),
        Request::AppSettingsGet => Ok(Response::AppSettingsGet(service.app_settings_get())),
        Request::AppSettingsSet(params) => service
            .app_settings_set(params)
            .map(Response::AppSettingsSet),
    };
    match response {
        Ok(response) => Envelope::Ok(Box::new(response)),
        Err(error) => Envelope::Err(error),
    }
}
