#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! REW `.mdat` schema, deterministic template builder and IR extraction.
//!
//! Official port of SECS 2 `src/secs/rew_mdat/{models,builder,__init__}.py`,
//! `rew/adapters.py` and the IR portion of `src/secs/mdat_loader.py`, GPL-3.0-only.
//! REW class names, cache formulas, the x0.01 IR scale and timing-note parsing
//! are kept here, without a role-assignment or operating-system dependency.

mod adapter;
mod builder;
mod ir;
mod models;
pub mod source_info;

pub use builder::{MeasurementInput, build};
pub use ir::{LoadedMdatProject, MdatIRProvider, MeasurementInfo, extract_ir, load_mdat_project};
pub use models::*;
pub use source_info::{Boxed, CalFile, CalInfo, SourceDate, SourceInfo};

pub const TEMPLATE: &[u8] = include_bytes!("../../../src/secs/rew_mdat/template_1.mdat");
pub const REW_MDAT_IR_SCALE: f64 = 0.01;

#[derive(Debug, thiserror::Error)]
pub enum MdatError {
    #[error(transparent)]
    Stream(#[from] secs_jos::JosError),
    #[error("{0}")]
    Schema(String),
    #[error("{0}")]
    Input(String),
}

/// Parse a bounded Java stream, then adapt REW fields without executing Java.
/// Adapted f64 arrays have per-array limits and a cumulative max_file_bytes
/// allocation budget, including repeated references to the same raw array.
pub fn load(bytes: &[u8], limits: &secs_jos::Limits) -> Result<MdatFile, MdatError> {
    adapter::adapt(secs_jos::parse(bytes, limits)?, bytes.len(), limits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_invalid_streams_are_rejected_without_filesystem() {
        for bytes in [&[][..], &[0xac], &[0xac, 0xed, 0, 5]] {
            assert!(load(bytes, &secs_jos::Limits::default()).is_err());
        }
    }

    #[test]
    fn invalid_build_inputs_do_not_consume_uuid() {
        let mut uuid = || panic!("invalid input must not consume UUID");
        assert!(build(&[], &[], None, 0, &mut uuid).is_err());
        let item = MeasurementInput {
            title: "invalid".into(),
            notes: String::new(),
            sample_rate_hz: 48000,
            samples: vec![],
            start_time_s: 0.0,
            spl_offset_db: None,
            source_info: None,
            computed: false,
        };
        assert!(build(&[item], &[], None, 0, &mut uuid).is_err());
    }
}
