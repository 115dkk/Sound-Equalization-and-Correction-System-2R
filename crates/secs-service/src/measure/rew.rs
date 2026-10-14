use super::*;
use secs_project::{MeasurementPlan, MeasurementResult};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

/// Accepts `RewClient<T>` in production and `FakeRew<S>` in device-free tests.
#[derive(Debug)]
pub struct RewMeasurer<R> {
    pub client: R,
}
impl<R> RewMeasurer<R> {
    pub fn new(client: R) -> Self {
        Self { client }
    }
}
impl<R> Measurer for RewMeasurer<R>
where
    R: secs_rew::RewStatus
        + secs_rew::DeviceLister
        + secs_rew::LevelMonitor
        + secs_rew::Measurer
        + Send,
{
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
            pro: Some(self.client.is_pro()),
        })
    }
    fn level_check(
        &mut self,
        plan: &MeasurementPlan,
        role: &str,
        seconds: f64,
        cancel: &CancelToken,
    ) -> Result<secs_audio::LevelReport, MeasureError> {
        if !seconds.is_finite()
            || !(0.05..=30.0).contains(&seconds)
            || !plan.roles.iter().any(|r| r == role)
        {
            return Err(MeasureError::Input("invalid level check".into()));
        }
        if cancel.is_cancelled() {
            return Err(secs_rew::RewError::Cancelled.into());
        }
        let (tx, rx) = mpsc::sync_channel(32);
        let result = (|| {
            self.client.play_pink(
                role,
                positive(plan.output_for(role), "output")?,
                plan.sweep_level_db,
            )?;
            self.client.start(Arc::new(move |level| {
                let _ = tx.try_send(level);
            }))?;
            let start = Instant::now();
            let mut last = None;
            while start.elapsed().as_secs_f64() < seconds {
                if cancel.is_cancelled() {
                    return Err(MeasureError::Rew(secs_rew::RewError::Cancelled));
                }
                if let Ok(level) = rx.recv_timeout(Duration::from_millis(10)) {
                    last = Some(level);
                }
            }
            let level =
                last.ok_or_else(|| MeasureError::Input("REW returned no level reading".into()))?;
            Ok(secs_audio::LevelReport {
                input_dbfs: level.input_dbfs,
                peak_dbfs: level.input_dbfs,
                clip: level.clip,
                spl_db: level.spl_db,
            })
        })();
        self.client.stop_generator();
        self.client.stop();
        result
    }
    fn measure_position(
        &mut self,
        plan: &MeasurementPlan,
        pos: i64,
        progress: Option<ProgressSink>,
        cancel: &CancelToken,
    ) -> Result<Vec<MeasurementResult>, MeasureError> {
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
        Ok(self
            .client
            .run_position(
                &to_rew_plan(plan)?,
                positive(pos, "position")?,
                progress,
                &cancel.rew,
            )?
            .into_iter()
            .map(|r| MeasurementResult {
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
            })
            .collect())
    }
}
