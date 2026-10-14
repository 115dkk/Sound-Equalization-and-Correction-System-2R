//! Measurement adapters. This is a Rust service seam, not the IPC schema.
pub(crate) mod json;
mod native;
mod rew;
pub mod sample;
mod settings;

pub use native::{BackendFactory, NativeMeasurer};
pub use rew::RewMeasurer;
pub use sample::MockMeasurer;
pub use settings::{
    MeasurementSettings, apply_level, plan_for, position_count, store_devices, to_audio_plan,
    to_rew_plan,
};

use secs_project::{MeasurementPlan, MeasurementResult};
use std::{fmt, sync::Arc};

#[derive(Debug)]
pub enum MeasureError {
    Input(String),
    Audio(secs_audio::AudioError),
    Rew(secs_rew::RewError),
    Project(secs_project::ProjectError),
}
impl fmt::Display for MeasureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(s) => f.write_str(s),
            Self::Audio(e) => e.fmt(f),
            Self::Rew(e) => e.fmt(f),
            Self::Project(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for MeasureError {}
impl From<secs_audio::AudioError> for MeasureError {
    fn from(e: secs_audio::AudioError) -> Self {
        Self::Audio(e)
    }
}
impl From<secs_rew::RewError> for MeasureError {
    fn from(e: secs_rew::RewError) -> Self {
        Self::Rew(e)
    }
}
impl From<secs_project::ProjectError> for MeasureError {
    fn from(e: secs_project::ProjectError) -> Self {
        Self::Project(e)
    }
}

/// One cancellation handle fans out to both libraries; no polling bridge thread.
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    audio: secs_audio::CancelToken,
    rew: secs_rew::CancelToken,
}
impl CancelToken {
    pub fn cancel(&self) {
        self.audio.cancel();
        self.rew.cancel();
    }
    pub fn is_cancelled(&self) -> bool {
        self.audio.is_cancelled()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub input_channels: usize,
    pub output_channels: usize,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub available: bool,
    pub version: Option<String>,
    pub pro: Option<bool>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub role: String,
    pub pos: i64,
    pub phase: String,
    pub percent: f64,
}
pub type ProgressSink = Arc<dyn Fn(Progress) + Send + Sync>;

/// A position's results use the canonical `.secs` record regardless of source.
/// Native measures one duplex sequence; cancellation yields no partial sequence.
/// REW returns completed roles on cancellation, as its upstream session does.
pub trait Measurer: Send {
    fn devices(&mut self) -> Result<Vec<Device>, MeasureError>;
    fn status(&mut self) -> Result<Status, MeasureError>;
    fn level_check(
        &mut self,
        plan: &MeasurementPlan,
        role: &str,
        seconds: f64,
        cancel: &CancelToken,
    ) -> Result<secs_audio::LevelReport, MeasureError>;
    fn measure_position(
        &mut self,
        plan: &MeasurementPlan,
        pos: i64,
        progress: Option<ProgressSink>,
        cancel: &CancelToken,
    ) -> Result<Vec<MeasurementResult>, MeasureError>;
}

pub(crate) fn positive(value: i64, name: &str) -> Result<u32, MeasureError> {
    u32::try_from(value)
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| MeasureError::Input(format!("{name} must be a positive u32")))
}
pub(crate) fn emit(sink: &Option<ProgressSink>, role: &str, pos: i64, phase: &str, percent: f64) {
    if let Some(sink) = sink {
        sink(Progress {
            role: role.into(),
            pos,
            phase: phase.into(),
            percent,
        });
    }
}
