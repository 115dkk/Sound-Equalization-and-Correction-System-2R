#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! SECS 2 compatible `.secs` project files and `.irs` WAV storage.

mod calibration;
mod config;
mod dsp_profile;
mod error;
mod irs;
mod project;
pub mod pyjson;
mod target;

pub use calibration::{calibration_from_project, store_calibration};
pub use config::{Band, Speaker, SpeakerConfig, default_bands, ordered_speakers};
pub use dsp_profile::DspProfile;
pub use error::ProjectError;
pub use irs::{
    ir_directory, ir_file, load_ir, read_mono_float_wav, read_wav_float32, save_ir,
    try_write_wav_float32, write_wav_float32,
};
pub use project::{
    FOCUS_STEPS, PROJECT_VERSION, Project, migrate_calibration_keys, migrate_group_keys,
    rename_keys, snap_focus,
};
pub use pyjson::{PyJson, PyJsonError, dumps, loads};
pub use target::{TargetCurve, TargetJson};
