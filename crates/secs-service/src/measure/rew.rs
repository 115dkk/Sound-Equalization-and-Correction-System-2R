use super::*;
use crate::{Deps, lock};
use secs_project::{MeasurementPlan, MeasurementResult};
use std::{
    fs,
    io::Read,
    sync::{Arc as SyncArc, Mutex},
    time::Duration,
};

const MAX_CALIBRATION_BYTES: u64 = 4 * 1024 * 1024;

pub(crate) struct Transport(Box<dyn secs_rew::Transport>);
impl secs_rew::Transport for Transport {
    fn request(
        &self,
        request: &secs_rew::Request,
    ) -> Result<secs_rew::Response, secs_rew::RewError> {
        self.0.request(request)
    }
}

/// The service's one link to REW: it builds clients on the configured
/// transport and keeps what REW said about the Pro license.
#[derive(Clone)]
pub(crate) struct RewLink {
    transport: SyncArc<dyn Fn() -> Box<dyn secs_rew::Transport> + Send + Sync>,
    license: SyncArc<Mutex<Option<bool>>>,
}

impl std::fmt::Debug for RewLink {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RewLink")
            .field("transport", &"factory")
            .field("license", &self.license())
            .finish()
    }
}

impl RewLink {
    pub(crate) fn new(deps: &Deps) -> Self {
        Self {
            transport: deps.rew_transport.clone(),
            license: SyncArc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn client(&self) -> secs_rew::RewClient<Transport> {
        secs_rew::RewClient::new(Transport((self.transport)()))
    }

    /// Records what an error says about the license (ProRequired → not Pro).
    pub(crate) fn observe(&self, error: &secs_rew::RewError) {
        if matches!(error, secs_rew::RewError::ProRequired { .. }) {
            *lock(&self.license) = Some(false);
        }
    }

    pub(crate) fn license(&self) -> Option<bool> {
        *lock(&self.license)
    }

    pub(crate) fn confirm_license(&self) {
        *lock(&self.license) = Some(true);
    }

    pub(crate) fn forget_license(&self) {
        *lock(&self.license) = None;
    }
}

/// Accepts `RewClient<T>` in production and `FakeRew<S>` in device-free tests.
pub struct RewMeasurer<R> {
    pub client: R,
    unix_ms: SyncArc<dyn Fn() -> i64 + Send + Sync>,
    link: Option<RewLink>,
    notices: Vec<secs_types::ipc::Notice>,
}
impl<R: std::fmt::Debug> std::fmt::Debug for RewMeasurer<R> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RewMeasurer")
            .field("client", &self.client)
            .finish_non_exhaustive()
    }
}
impl<R> RewMeasurer<R> {
    pub fn new(client: R, unix_ms: SyncArc<dyn Fn() -> i64 + Send + Sync>) -> Self {
        Self {
            client,
            unix_ms,
            link: None,
            notices: Vec::new(),
        }
    }

    pub(crate) fn linked(
        client: R,
        unix_ms: SyncArc<dyn Fn() -> i64 + Send + Sync>,
        link: RewLink,
    ) -> Self {
        Self {
            client,
            unix_ms,
            link: Some(link),
            notices: Vec::new(),
        }
    }

    fn observe<T>(&self, result: Result<T, MeasureError>) -> Result<T, MeasureError> {
        if let Err(MeasureError::Rew(error)) = &result
            && let Some(link) = &self.link
        {
            link.observe(error);
        }
        result
    }
}

impl<R: secs_rew::MeasurementEnvironment> RewMeasurer<R> {
    fn input_calibration(&mut self) -> Option<secs_mdat::CalFile> {
        let path = self
            .client
            .input_calibration_file()
            .ok()
            .flatten()
            .filter(|path| !path.is_empty())?;
        let text = match read_calibration(&path) {
            Ok(text) => Some(text),
            Err((code, detail)) => {
                self.notices.push(secs_types::ipc::Notice {
                    code: code.into(),
                    params: [("path".into(), path.clone()), ("detail".into(), detail)].into(),
                });
                None
            }
        };
        Some(secs_mdat::CalFile { path, text })
    }
}

fn read_calibration(path: &str) -> Result<Vec<u8>, (&'static str, String)> {
    let mut file = fs::File::open(path)
        .map_err(|error| ("measurement_calibration_unreadable", error.to_string()))?;
    let length = file
        .metadata()
        .map_err(|error| ("measurement_calibration_unreadable", error.to_string()))?
        .len();
    if length > MAX_CALIBRATION_BYTES {
        return Err((
            "measurement_calibration_too_large",
            format!("calibration file is {length} bytes"),
        ));
    }
    let mut bytes = Vec::with_capacity(length as usize);
    file.by_ref()
        .take(MAX_CALIBRATION_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| ("measurement_calibration_unreadable", error.to_string()))?;
    if bytes.len() as u64 > MAX_CALIBRATION_BYTES {
        return Err((
            "measurement_calibration_too_large",
            "calibration file exceeds 4 MiB".into(),
        ));
    }
    Ok(bytes)
}

trait RoleRecorder {
    fn configure_position(
        &mut self,
        plan: &secs_rew::MeasurementPlan,
    ) -> Result<(), secs_rew::RewError>;

    fn record_role(
        &mut self,
        plan: &secs_rew::MeasurementPlan,
        role: &str,
        pos: u32,
        progress: Option<secs_rew::ProgressCallback>,
        cancel: &secs_rew::CancelToken,
    ) -> Result<Option<secs_rew::MeasurementResult>, secs_rew::RewError>;
}

impl RoleRecorder for secs_rew::RewClient<Transport> {
    fn configure_position(
        &mut self,
        plan: &secs_rew::MeasurementPlan,
    ) -> Result<(), secs_rew::RewError> {
        self.configure(plan)
    }

    fn record_role(
        &mut self,
        plan: &secs_rew::MeasurementPlan,
        role: &str,
        pos: u32,
        progress: Option<secs_rew::ProgressCallback>,
        cancel: &secs_rew::CancelToken,
    ) -> Result<Option<secs_rew::MeasurementResult>, secs_rew::RewError> {
        if cancel.is_cancelled() {
            if let Some(callback) = &progress {
                callback(secs_rew::progress(
                    role,
                    pos,
                    "cancelled",
                    0.0,
                    "측정을 취소했습니다",
                ));
            }
            return Ok(None);
        }
        let own_progress = if let Some(callback) = progress.clone() {
            let role = role.to_owned();
            self.subscribe_progress(SyncArc::new(move |body| {
                let (percent, message) = secs_rew::parse_progress(&body);
                if let Some(percent) = percent {
                    callback(secs_rew::progress(
                        &role,
                        pos,
                        "running",
                        percent.clamp(0.0, 100.0) * 0.9,
                        &message,
                    ));
                }
            }))
            .is_ok()
        } else {
            false
        };
        let result = (|| {
            if cancel.is_cancelled() {
                if let Some(callback) = &progress {
                    callback(secs_rew::progress(
                        role,
                        pos,
                        "cancelled",
                        0.0,
                        "측정을 취소했습니다",
                    ));
                }
                return Ok(None);
            }
            let name = plan.name_for(role, pos);
            if let Some(callback) = &progress {
                callback(secs_rew::progress(
                    role,
                    pos,
                    "sweep",
                    0.0,
                    &format!("{role} 스윕 (위치 {pos})"),
                ));
            }
            self.set_output_channel(plan.output_for(role))?;
            self.set_naming(&name)?;
            let (base, band) = secs_rew::split_role(role);
            let suffix = band.map(|band| format!("/{band}")).unwrap_or_default();
            self.set_notes(&format!(
                "SECS {base}{suffix} P{pos} ({} timing)",
                plan.timing_reference
            ))?;
            let before = self.measurement_ids()?;
            if let Err(error) = self.start_sweep() {
                if matches!(error, secs_rew::RewError::ProRequired { .. }) || !cancel.is_cancelled()
                {
                    return Err(error);
                }
                return Ok(None);
            }
            let id = if cancel.is_cancelled() {
                None
            } else {
                self.wait_for_new_measurement(
                    &before,
                    Duration::from_secs_f64(Self::sweep_wait_s(plan)),
                    cancel,
                )?
            };
            if cancel.is_cancelled() {
                self.abort();
                if let Some(callback) = &progress {
                    callback(secs_rew::progress(
                        role,
                        pos,
                        "cancelled",
                        0.0,
                        "측정을 취소했습니다",
                    ));
                }
                return Ok(None);
            }
            let id = id.ok_or_else(|| secs_rew::RewError::Schema {
                path: "/measurements".into(),
                message: format!("no new measurement: {name}"),
            })?;
            if let Some(callback) = &progress {
                callback(secs_rew::progress(
                    role,
                    pos,
                    "process",
                    90.0,
                    &format!("{role} 임펄스 응답 수신"),
                ));
            }
            let ir = self.impulse_response(&id)?;
            let result = secs_rew::MeasurementResult {
                role: role.to_owned(),
                pos,
                uuid: Some(id.clone()),
                ir: ir.ir,
                sample_rate: ir.sample_rate,
                name,
                spl_db: self.spl(1),
                t0_s: ir.t0_s,
                snr_db: self.snr_db(&id),
                spl_offset_db: self.spl_offset_db(&id),
            };
            if let Some(callback) = &progress {
                callback(secs_rew::progress(
                    role,
                    pos,
                    "done",
                    100.0,
                    &format!("{role} 완료"),
                ));
            }
            Ok(Some(result))
        })();
        if own_progress {
            self.unsubscribe_progress();
        }
        result
    }
}

impl<S: secs_rew::IrSource> RoleRecorder for secs_rew::FakeRew<S> {
    fn configure_position(
        &mut self,
        _plan: &secs_rew::MeasurementPlan,
    ) -> Result<(), secs_rew::RewError> {
        Ok(())
    }

    fn record_role(
        &mut self,
        plan: &secs_rew::MeasurementPlan,
        role: &str,
        pos: u32,
        progress: Option<secs_rew::ProgressCallback>,
        cancel: &secs_rew::CancelToken,
    ) -> Result<Option<secs_rew::MeasurementResult>, secs_rew::RewError> {
        let mut single = plan.clone();
        single.roles = vec![role.to_owned()];
        Ok(self
            .run_position(&single, pos, progress, cancel)?
            .into_iter()
            .next())
    }
}

impl<R> Measurer for RewMeasurer<R>
where
    R: secs_rew::RewStatus
        + secs_rew::DeviceLister
        + secs_rew::LevelMonitor
        + secs_rew::MeasurementEnvironment
        + secs_rew::Measurer
        + RoleRecorder
        + Send,
{
    fn commit_boundary(&self) -> CommitBoundary {
        CommitBoundary::Role
    }

    fn devices(&mut self) -> Result<Vec<Device>, MeasureError> {
        let mut out = self
            .client
            .output_devices()
            .into_iter()
            .map(|d| Device {
                id: d.id,
                name: d.name,
                input_channels: 0,
                output_channels: d.channels,
            })
            .collect::<Vec<_>>();
        out.extend(self.client.input_devices().into_iter().map(|d| Device {
            id: d.id,
            name: d.name,
            input_channels: d.channels,
            output_channels: 0,
        }));
        Ok(out)
    }
    fn status(&mut self) -> Result<Status, MeasureError> {
        Ok(Status {
            available: self.client.is_running(),
            version: self.client.version(),
            pro: None,
        })
    }
    fn take_notices(&mut self) -> Vec<secs_types::ipc::Notice> {
        std::mem::take(&mut self.notices)
    }
    fn measure_recording(
        &mut self,
        plan: &MeasurementPlan,
        pos: i64,
        progress: Option<ProgressSink>,
        cancel: &CancelToken,
        record: &mut dyn FnMut(MeasurementResult) -> Result<(), MeasureError>,
    ) -> Result<(), MeasureError> {
        let measured = (|| {
            let current_audio = self.client.current_audio();
            let output_devices = self.client.output_devices();
            let input_devices = self.client.input_devices();
            let fallback = |stored: &str, devices: &[secs_rew::DeviceInfo]| {
                if devices
                    .iter()
                    .any(|device| device.id == stored || device.name == stored)
                {
                    stored.to_owned()
                } else {
                    String::new()
                }
            };
            let output_device = current_audio
                .output_device
                .unwrap_or_else(|| fallback(&plan.output_device, &output_devices));
            let input_device = current_audio
                .input_device
                .unwrap_or_else(|| fallback(&plan.input_device, &input_devices));
            let progress = progress.map(|sink| {
                Arc::new(move |progress: secs_rew::Progress| {
                    sink(Progress {
                        role: progress.role,
                        pos: i64::from(progress.pos),
                        phase: progress.phase,
                        percent: progress.percent,
                    });
                }) as secs_rew::ProgressCallback
            });
            let rew_plan = to_rew_plan(plan)?;
            let position = positive(pos, "position")?;
            self.client.configure_position(&rew_plan)?;
            let mut calibration_loaded = false;
            let mut calibration = None;
            for role in &plan.roles {
                let Some(result) = self.client.record_role(
                    &rew_plan,
                    role,
                    position,
                    progress.clone(),
                    &cancel.rew,
                )?
                else {
                    break;
                };
                if !calibration_loaded {
                    calibration = self.input_calibration();
                    calibration_loaded = true;
                }
                let output_channel = format!("out {}", plan.output_for(&result.role));
                let source_info = secs_mdat::from_rew_measurement(
                    calibration.as_ref(),
                    &output_device,
                    &input_device,
                    &output_channel,
                    (self.unix_ms)(),
                )
                .ok()
                .flatten();
                record(MeasurementResult {
                    role: result.role,
                    pos: i64::from(result.pos),
                    ir: result.ir,
                    sample_rate: result.sample_rate,
                    t0_s: result.t0_s,
                    spl_db: result.spl_db,
                    spl_offset_db: result.spl_offset_db,
                    snr_db: result.snr_db,
                    uuid: result.uuid,
                    source: None,
                    source_info,
                })?;
            }
            Ok(())
        })();
        self.observe(measured)
    }
    fn measure_position(
        &mut self,
        plan: &MeasurementPlan,
        pos: i64,
        progress: Option<ProgressSink>,
        cancel: &CancelToken,
    ) -> Result<Vec<MeasurementResult>, MeasureError> {
        let result = (|| {
            let current_audio = self.client.current_audio();
            let output_devices = self.client.output_devices();
            let input_devices = self.client.input_devices();
            let fallback = |stored: &str, devices: &[secs_rew::DeviceInfo]| {
                if devices
                    .iter()
                    .any(|device| device.id == stored || device.name == stored)
                {
                    stored.to_owned()
                } else {
                    String::new()
                }
            };
            let output_device = current_audio
                .output_device
                .unwrap_or_else(|| fallback(&plan.output_device, &output_devices));
            let input_device = current_audio
                .input_device
                .unwrap_or_else(|| fallback(&plan.input_device, &input_devices));
            let progress = progress.map(|sink| {
                Arc::new(move |p: secs_rew::Progress| {
                    sink(Progress {
                        role: p.role,
                        pos: i64::from(p.pos),
                        phase: p.phase,
                        percent: p.percent,
                    })
                }) as secs_rew::ProgressCallback
            });
            let results = self.client.run_position(
                &to_rew_plan(plan)?,
                positive(pos, "position")?,
                progress,
                &cancel.rew,
            )?;
            let cal = self.input_calibration();
            Ok(results
                .into_iter()
                .map(|r| {
                    let output_channel = format!("out {}", plan.output_for(&r.role));
                    let source_info = secs_mdat::from_rew_measurement(
                        cal.as_ref(),
                        &output_device,
                        &input_device,
                        &output_channel,
                        (self.unix_ms)(),
                    )
                    .ok()
                    .flatten();
                    MeasurementResult {
                        role: r.role,
                        pos: i64::from(r.pos),
                        ir: r.ir,
                        sample_rate: r.sample_rate,
                        t0_s: r.t0_s,
                        spl_db: r.spl_db,
                        spl_offset_db: r.spl_offset_db,
                        snr_db: r.snr_db,
                        uuid: r.uuid,
                        source: None,
                        source_info,
                    }
                })
                .collect())
        })();
        self.observe(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rew_commits_each_role_as_it_arrives() {
        let measurer = RewMeasurer::new(
            secs_rew::FakeRew::new(super::super::sample::SampleSource),
            SyncArc::new(|| 0),
        );
        assert_eq!(measurer.commit_boundary(), CommitBoundary::Role);
    }

    #[test]
    fn link_observes_only_pro_required() {
        let link = RewLink::new(&Deps::default());
        link.observe(&secs_rew::RewError::Connection {
            path: "/version".into(),
            message: "offline".into(),
        });
        assert_eq!(link.license(), None);

        link.observe(&secs_rew::RewError::ProRequired {
            status: 403,
            path: "/measure/command".into(),
        });
        assert_eq!(link.license(), Some(false));

        link.observe(&secs_rew::RewError::Schema {
            path: "/version".into(),
            message: "bad response".into(),
        });
        assert_eq!(link.license(), Some(false));
    }
}
