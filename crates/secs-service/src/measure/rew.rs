use super::*;
use crate::{Deps, lock};
use secs_project::{MeasurementPlan, MeasurementResult};
use std::{
    fs,
    sync::{Arc as SyncArc, Mutex},
};

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
impl<R> Measurer for RewMeasurer<R>
where
    R: secs_rew::RewStatus
        + secs_rew::DeviceLister
        + secs_rew::LevelMonitor
        + secs_rew::MeasurementEnvironment
        + secs_rew::Measurer
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
    fn measure_recording(
        &mut self,
        plan: &MeasurementPlan,
        pos: i64,
        cancel: &CancelToken,
        record: &mut dyn FnMut(MeasurementResult) -> Result<(), MeasureError>,
    ) -> Result<(), MeasureError> {
        for role in &plan.roles {
            if cancel.is_cancelled() {
                break;
            }
            let mut single = plan.clone();
            single.roles = vec![role.clone()];
            for result in self.measure_position(&single, pos, None, cancel)? {
                record(result)?;
            }
        }
        Ok(())
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
            let cal = self
                .client
                .input_calibration_file()
                .ok()
                .flatten()
                .filter(|path| !path.is_empty())
                .map(|path| secs_mdat::CalFile {
                    text: fs::read(&path).ok(),
                    path,
                });
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
