use super::*;
use secs_audio::{AudioBackend, LevelInput, LevelOutput, LevelSignal};
use secs_project::{MeasurementPlan, MeasurementResult};

pub type BackendFactory =
    Arc<dyn Fn() -> Result<Box<dyn AudioBackend>, secs_audio::AudioError> + Send + Sync>;
pub struct NativeMeasurer {
    backend: Box<dyn AudioBackend>,
    pub settings: MeasurementSettings,
}
impl std::fmt::Debug for NativeMeasurer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeMeasurer")
            .field("backend", &self.backend.name())
            .field("settings", &self.settings)
            .finish()
    }
}
impl NativeMeasurer {
    pub fn new(
        factory: &BackendFactory,
        settings: MeasurementSettings,
    ) -> Result<Self, MeasureError> {
        Ok(Self {
            backend: factory()?,
            settings,
        })
    }
    pub fn from_backend(backend: Box<dyn AudioBackend>, settings: MeasurementSettings) -> Self {
        Self { backend, settings }
    }

    pub(crate) fn level_check_streaming(
        &mut self,
        plan: &MeasurementPlan,
        role: &str,
        seconds: f64,
        cancel: &CancelToken,
        on_level: &mut dyn FnMut(secs_audio::LevelReport),
    ) -> Result<secs_audio::LevelReport, MeasureError> {
        let (output, input) = self.level_io(plan, role)?;
        Ok(secs_audio::level::level_check_streaming(
            self.backend.as_ref(),
            &output,
            &input,
            plan.sweep_level_db,
            seconds,
            &cancel.audio,
            on_level,
        )?)
    }

    fn level_io(
        &self,
        plan: &MeasurementPlan,
        role: &str,
    ) -> Result<(LevelOutput, LevelInput), MeasureError> {
        if !plan.roles.iter().any(|candidate| candidate == role) {
            return Err(MeasureError::Input("unknown measurement role".into()));
        }
        let audio = to_audio_plan(plan, &self.backend.enumerate()?, &self.settings)?;
        let channel = audio
            .roles
            .iter()
            .find(|candidate| candidate.role == role)
            .ok_or_else(|| MeasureError::Input("unknown measurement role".into()))?
            .channel;
        Ok((
            LevelOutput {
                endpoint: audio.output,
                channel,
                sample_rate: audio.sample_rate,
                signal: if role.to_ascii_uppercase().starts_with("SW") {
                    LevelSignal::SubSine { frequency: 50.0 }
                } else {
                    LevelSignal::Pink
                },
                share: audio.share,
            },
            LevelInput {
                endpoint: audio.input,
                channel: audio.input_channel,
                calibration: self.settings.calibration.clone(),
                spl_reference: self.settings.spl_reference,
            },
        ))
    }
}
impl Measurer for NativeMeasurer {
    fn devices(&mut self) -> Result<Vec<Device>, MeasureError> {
        Ok(self
            .backend
            .enumerate()?
            .into_iter()
            .map(|e| Device {
                id: e.id,
                name: e.name,
                input_channels: usize::from(e.max_input_channels),
                output_channels: usize::from(e.max_output_channels),
            })
            .collect())
    }
    fn status(&mut self) -> Result<Status, MeasureError> {
        Ok(Status {
            available: !self.backend.enumerate()?.is_empty(),
            version: None,
            pro: None,
        })
    }
    fn measure_position(
        &mut self,
        plan: &MeasurementPlan,
        pos: i64,
        progress: Option<ProgressSink>,
        cancel: &CancelToken,
    ) -> Result<Vec<MeasurementResult>, MeasureError> {
        let position = positive(pos, "position")?;
        if cancel.is_cancelled() {
            emit(&progress, "", pos, "cancelled", 0.0);
            return Ok(vec![]);
        }
        let p = to_audio_plan(plan, &self.backend.enumerate()?, &self.settings)?;
        emit(&progress, "", pos, "sweep", 0.0);
        let got = secs_audio::measure_position(
            self.backend.as_ref(),
            &p,
            position,
            &cancel.audio,
            &mut |event| {
                let (phase, percent) = match event {
                    secs_audio::session::SessionEvent::OutputStarted { .. } => ("running", 10.0),
                    secs_audio::session::SessionEvent::OutputDrained(_) => ("process", 90.0),
                    _ => return,
                };
                emit(&progress, "", pos, phase, percent);
            },
        );
        let got = match got {
            Err(secs_audio::AudioError::Cancelled) => {
                emit(&progress, "", pos, "cancelled", 0.0);
                return Ok(vec![]);
            }
            other => other?,
        };
        Ok(got
            .roles
            .into_iter()
            .map(|r| {
                emit(&progress, &r.role, pos, "done", 100.0);
                MeasurementResult {
                    role: r.role,
                    pos,
                    ir: r.ir,
                    sample_rate: r.sample_rate,
                    t0_s: r.t0,
                    spl_db: None,
                    spl_offset_db: self.settings.spl_reference.map(|s| s.offset_db),
                    snr_db: Some(r.snr_db),
                    uuid: None,
                    source: None,
                    source_info: None,
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_commits_a_position_at_once() {
        let measurer = NativeMeasurer::from_backend(
            Box::new(secs_audio::NullBackend),
            MeasurementSettings::default(),
        );
        assert_eq!(measurer.commit_boundary(), CommitBoundary::Position);
    }
}
