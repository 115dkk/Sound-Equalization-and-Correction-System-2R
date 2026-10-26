#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Native measurement without REW (docs/adr/0005).
//!
//! Platform-independent: sweep design, the play-and-record session contract
//! (open input, confirm ready, play to completion and drain, stop, join),
//! deconvolution to impulse responses, timing reference, level check and
//! microphone calibration files. Audio devices come in through the
//! `AudioBackend` seam: WASAPI and cpal adapters live in `secs-sys` and are
//! injected by the app; tests use a simulated room that convolves the sweep
//! with known impulse responses.

pub mod audio;
pub mod cached_backend;
pub mod calibration;
pub mod estimator;
pub mod level;
pub mod measurement;
pub mod null_backend;
pub mod policy;
pub mod session;
pub mod sim_room;

pub use audio::*;
pub use cached_backend::CachedBackend;
pub use calibration::{MicCalibration, SplReference};
pub use level::{LevelInput, LevelOutput, LevelReport, LevelSignal, level_check};
pub use measurement::{
    MeasuredRole, MeasurementPlan, MeasurementWindow, PositionMeasurement, RoleOutput,
    measure_position,
};
pub use null_backend::NullBackend;
pub use sim_room::SimRoom;

fn invalid(message: &str) -> AudioError {
    AudioError::UnsupportedFormat(message.into())
}
fn check_cancel(cancel: &CancelToken) -> Result<(), AudioError> {
    if cancel.is_cancelled() {
        Err(AudioError::Cancelled)
    } else {
        Ok(())
    }
}
