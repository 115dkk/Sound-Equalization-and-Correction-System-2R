use super::*;
use secs_audio::{AudioBackend, LevelInput, LevelOutput, LevelSignal};
use secs_project::{MeasurementPlan, MeasurementResult};

pub type BackendFactory =
    Arc<dyn Fn() -> Result<Box<dyn AudioBackend>, secs_audio::AudioError> + Send + Sync>;
pub struct NativeMeasurer {
    backend: Box<dyn AudioBackend>,
    pub settings: MeasurementSettings,
    unix_ms: Arc<dyn Fn() -> i64 + Send + Sync>,
}

fn system_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
        })
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
        Ok(Self::from_backend(factory()?, settings))
    }
    pub fn from_backend(backend: Box<dyn AudioBackend>, settings: MeasurementSettings) -> Self {
        Self {
            backend,
            settings,
            unix_ms: Arc::new(system_unix_ms),
        }
    }

    /// Replaces the wall clock used in native sweep source sidecars.
    pub fn with_clock(mut self, clock: Arc<dyn Fn() -> i64 + Send + Sync>) -> Self {
        self.unix_ms = clock;
        self
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

    pub(crate) fn input_meter(
        &mut self,
        plan: &MeasurementPlan,
        cancel: &CancelToken,
        stop_when: &mut dyn FnMut() -> bool,
        on_level: &mut dyn FnMut(secs_audio::LevelReport),
    ) -> Result<(), MeasureError> {
        let audio = self.audio_plan(plan)?;
        let input = LevelInput {
            endpoint: audio.input,
            channel: audio.input_channel,
            calibration: self.settings.calibration.clone(),
            spl_reference: self.settings.spl_reference,
        };
        Ok(secs_audio::input_meter(
            self.backend.as_ref(),
            &input,
            audio.sample_rate,
            audio.share,
            &cancel.audio,
            stop_when,
            on_level,
        )?)
    }

    fn audio_plan(
        &self,
        plan: &MeasurementPlan,
    ) -> Result<secs_audio::MeasurementPlan, MeasureError> {
        to_audio_plan(plan, &self.backend.enumerate()?, &self.settings)
    }

    fn level_io(
        &self,
        plan: &MeasurementPlan,
        role: &str,
    ) -> Result<(LevelOutput, LevelInput), MeasureError> {
        if !plan.roles.iter().any(|candidate| candidate == role) {
            return Err(MeasureError::Input("unknown measurement role".into()));
        }
        let audio = self.audio_plan(plan)?;
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
        let source_info = self
            .settings
            .calibration_file
            .as_ref()
            .map(|file| {
                secs_mdat::from_rew_measurement(Some(file), "", "", "", (self.unix_ms)())
                    .map_err(|error| MeasureError::Input(error.to_string()))
            })
            .transpose()?
            .flatten();
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
                    source_info: source_info.clone(),
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_sweeps_carry_the_project_mic_calibration() {
        let mut project = secs_project::Project::from_preset("2.0").unwrap();
        let calibration = "Sens Factor = -1.337 dB\r\n20 -1.25 0.5\r\n1000 0.25\r\n";
        crate::measure::store_devices(
            &mut project,
            vec![
                (
                    "mic_calibration".into(),
                    secs_project::PyJson::Str(calibration.into()),
                ),
                (
                    "mic_calibration_path".into(),
                    secs_project::PyJson::Str("C:/microphones/umik-1.cal".into()),
                ),
            ],
        );
        let settings = MeasurementSettings::from_project(&project).unwrap();
        let mut plan = crate::measure::plan_for(&project, &settings);
        plan.sweep_length = "64k".into();
        let (room, _) = crate::measure::sample::room(&plan, 1).unwrap();
        let endpoint = room.endpoint();
        plan.output_device = endpoint.id.clone();
        plan.input_device = endpoint.id;
        let mut measurer = NativeMeasurer::from_backend(Box::new(room), settings)
            .with_clock(Arc::new(|| 1_704_164_645_123));
        let results = measurer
            .measure_position(&plan, 1, None, &CancelToken::default())
            .unwrap();
        assert_eq!(results.len(), 2);

        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "secs-native-calibration-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut store = secs_project::MeasurementStore::new(root.join("room.secs"), project);
        for result in results {
            store.record(result, Some(&plan.roles)).unwrap();
        }
        for role in &plan.roles {
            let source = store.source_of(role, 1).unwrap();
            assert_eq!(
                source.date,
                Some(secs_mdat::SourceDate::Int(1_704_164_645_123))
            );
            let meter = source
                .cal
                .iter()
                .find(|(name, _)| name == "meterCal")
                .map(|(_, value)| value)
                .unwrap();
            assert_eq!(meter.source_name, "umik-1.cal");
            assert_eq!(meter.freq, Some(vec![20.0, 1000.0]));
            assert_eq!(meter.gain, Some(vec![-1.25, 0.25]));
            assert_eq!(meter.phase, Some(vec![0.5, 0.0]));
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_commits_a_position_at_once() {
        let measurer = NativeMeasurer::from_backend(
            Box::new(secs_audio::NullBackend),
            MeasurementSettings::default(),
        );
        assert_eq!(measurer.commit_boundary(), CommitBoundary::Position);
    }
}
