// Ported from Impulcifer-pip313 impulcifer-audio-io/src/null_backend.rs, commit cac2217.
// MIT; Copyright (c) 2018- Jaakko Pasanen, 2024- 115dkk, 2025- LionLion123, SDC.
// See ../THIRD_PARTY_LICENSES.txt and docs/THIRD_PARTY.md.
//! Backend used by builds that deliberately omit native audio support.

use crate::audio::{
    AudioBackend, AudioError, Direction, Endpoint, InputSession, OutputSession, ProbeResult,
    ShareMode, StreamSpec,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct NullBackend;

fn unavailable<T>() -> Result<T, AudioError> {
    Err(AudioError::Backend(
        "this build has no audio backend".into(),
    ))
}

impl AudioBackend for NullBackend {
    fn name(&self) -> &'static str {
        "none"
    }

    fn enumerate(&self) -> Result<Vec<Endpoint>, AudioError> {
        Ok(Vec::new())
    }

    fn probe(
        &self,
        _: &Endpoint,
        _: Direction,
        _: StreamSpec,
        _: ShareMode,
    ) -> Result<ProbeResult, AudioError> {
        unavailable()
    }

    fn open_output(
        &self,
        _: &Endpoint,
        _: StreamSpec,
        _: ShareMode,
    ) -> Result<Box<dyn OutputSession>, AudioError> {
        unavailable()
    }

    fn open_input(
        &self,
        _: &Endpoint,
        _: StreamSpec,
        _: ShareMode,
    ) -> Result<Box<dyn InputSession>, AudioError> {
        unavailable()
    }
}
