#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! SECS 2 compatible `.secs` project files and `.irs` WAV storage.

mod calibration;
mod config;
mod dsp_profile;
mod error;
mod irs;
mod plan;
mod project;
pub mod pyjson;
mod record;
mod signature;
mod target;

pub use calibration::{calibration_from_project, store_calibration};
pub use config::{Band, Speaker, SpeakerConfig, default_bands, ordered_speakers};
pub use dsp_profile::DspProfile;
pub use error::ProjectError;
pub use irs::{
    MAX_IR_BYTES, ir_directory, ir_file, load_ir, read_mono_float_wav, read_wav_float32, save_ir,
    try_write_wav_float32, write_wav_float32,
};
pub use plan::{
    MeasurementPlan, PlanValidationError, SWEEP_LENGTHS, TimingReference, default_ref_role,
    measurement_roles, plan_from_project, position_seconds, split_role, start_delay_seconds,
    sweep_length_samples, sweep_seconds, timing_settings,
};
pub use project::{
    FOCUS_STEPS, PROJECT_VERSION, Project, migrate_calibration_keys, migrate_group_keys,
    rename_keys, snap_focus,
};
pub use pyjson::{PyJson, PyJsonError, dumps, dumps_compact, loads};
pub use record::{
    IR_CACHE_LIMIT_BYTES, IrCache, LoadedIr, MAX_PROJECT_BYTES, MeasurementRecord,
    MeasurementResult, MeasurementStore, MigrationWarning, load_project_irs, load_project_sources,
    load_source, save_source, source_path,
};
pub use signature::{
    FloatKey, MeasurementSignatureEntry, estimate_seconds, estimate_subs_seconds,
    estimate_xo_seconds, estimate_xo_seconds_on, measured_positions, measurement_signature,
    project_spl_offset, target_key,
};
pub use target::{TargetCurve, TargetJson};
