// Ported from Impulcifer-pip313 impulcifer-audio-io/src/policy.rs, commit cac2217.
// MIT; Copyright (c) 2018- Jaakko Pasanen, 2024- 115dkk, 2025- LionLion123, SDC.
// See ../THIRD_PARTY_LICENSES.txt and docs/THIRD_PARTY.md.
//! Exclusive first; only format rejection permits shared auto-convert fallback.

use crate::audio::{
    AudioBackend, AudioError, Endpoint, InputSession, OutputSession, ShareMode, SharePreference,
    StreamSpec,
};

pub fn open_output_with_policy(
    backend: &dyn AudioBackend,
    endpoint: &Endpoint,
    spec: StreamSpec,
) -> Result<(Box<dyn OutputSession>, ShareMode), AudioError> {
    match backend.open_output(endpoint, spec, ShareMode::Exclusive) {
        Ok(session) => Ok((session, ShareMode::Exclusive)),
        Err(AudioError::UnsupportedFormat(_)) => backend
            .open_output(endpoint, spec, ShareMode::SharedAutoConvert)
            .map(|session| (session, ShareMode::SharedAutoConvert)),
        Err(error) => Err(error),
    }
}

pub fn open_input_with_policy(
    backend: &dyn AudioBackend,
    endpoint: &Endpoint,
    spec: StreamSpec,
) -> Result<(Box<dyn InputSession>, ShareMode), AudioError> {
    match backend.open_input(endpoint, spec, ShareMode::Exclusive) {
        Ok(session) => Ok((session, ShareMode::Exclusive)),
        Err(AudioError::UnsupportedFormat(_)) => backend
            .open_input(endpoint, spec, ShareMode::SharedAutoConvert)
            .map(|session| (session, ShareMode::SharedAutoConvert)),
        Err(error) => Err(error),
    }
}

pub fn open_output_with_preference(
    backend: &dyn AudioBackend,
    endpoint: &Endpoint,
    spec: StreamSpec,
    preference: SharePreference,
) -> Result<(Box<dyn OutputSession>, ShareMode), AudioError> {
    let Some(mode) = preference.fixed_mode() else {
        return open_output_with_policy(backend, endpoint, spec);
    };
    backend
        .open_output(endpoint, spec, mode)
        .map(|session| (session, mode))
}

pub fn open_input_with_preference(
    backend: &dyn AudioBackend,
    endpoint: &Endpoint,
    spec: StreamSpec,
    preference: SharePreference,
) -> Result<(Box<dyn InputSession>, ShareMode), AudioError> {
    let Some(mode) = preference.fixed_mode() else {
        return open_input_with_policy(backend, endpoint, spec);
    };
    backend
        .open_input(endpoint, spec, mode)
        .map(|session| (session, mode))
}
