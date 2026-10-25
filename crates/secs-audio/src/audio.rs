// Ported from Impulcifer-pip313 impulcifer-types/src/audio.rs, commit cac2217.
// MIT; Copyright (c) 2018- Jaakko Pasanen, 2024- 115dkk, 2025- LionLion123, SDC.
// See ../THIRD_PARTY_LICENSES.txt and docs/THIRD_PARTY.md.
//! Audio backend contract. In SECS 2R the adapters live in `secs-sys`
//! (WASAPI on Windows, cpal on macOS and Linux), and `secs-audio` adds the
//! simulated room and the null backend for tests. Sessions are deliberately
//! not `Send`: they are created, used and dropped on one dedicated OS thread.
//! Threads exchange endpoint ids, specs, owned buffers and reports only.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Input,
    Output,
}

/// Windows share mode. The cpal backend ignores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareMode {
    Exclusive,
    SharedAutoConvert,
}

/// The user's device-access request for a measurement session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharePreference {
    /// Exclusive first, shared auto-convert when the exclusive format is refused.
    #[default]
    Auto,
    Exclusive,
    Shared,
}

impl SharePreference {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Exclusive => "exclusive",
            Self::Shared => "shared",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "auto" => Some(Self::Auto),
            "exclusive" => Some(Self::Exclusive),
            "shared" => Some(Self::Shared),
            _ => None,
        }
    }

    pub fn fixed_mode(self) -> Option<ShareMode> {
        match self {
            Self::Auto => None,
            Self::Exclusive => Some(ShareMode::Exclusive),
            Self::Shared => Some(ShareMode::SharedAutoConvert),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Endpoint {
    /// Stable backend identifier (WASAPI endpoint id string, cpal device name on other platforms).
    pub id: String,
    pub name: String,
    pub host_api: String,
    pub max_input_channels: u16,
    pub max_output_channels: u16,
    pub default_samplerate: f64,
    pub is_default_input: bool,
    pub is_default_output: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamSpec {
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProbeResult {
    pub supported: bool,
    pub mode: ShareMode,
    /// The format the backend would actually open (may differ from the request in shared mode).
    pub native_sample_rate: u32,
    pub native_channels: u16,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlaybackReport {
    pub frames_submitted: u64,
    pub frames_drained: u64,
    pub mode: ShareMode,
    pub underruns: u32,
    pub cancelled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaptureRead {
    pub frames: usize,
    pub discontinuity: bool,
    pub silent: bool,
}

/// Why a finished recording was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingIssue {
    Gap,
    Underrun,
    NonFinite,
}

impl std::fmt::Display for RecordingIssue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Gap => "gap",
            Self::Underrun => "underrun",
            Self::NonFinite => "nonfinite",
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("device not found: {0}")]
    DeviceNotFound(String),
    #[error("format not supported: {0}")]
    UnsupportedFormat(String),
    #[error("backend error: {0}")]
    Backend(String),
    #[error("recording rejected ({issue}): {detail}")]
    Recording {
        issue: RecordingIssue,
        detail: String,
    },
    #[error("cancelled")]
    Cancelled,
}

/// Cooperative cancellation flag shared between the coordinator and workers.
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

pub trait AudioBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn selectable_share_modes(&self) -> &'static [ShareMode] {
        &[]
    }
    fn enumerate(&self) -> Result<Vec<Endpoint>, AudioError>;
    fn probe(
        &self,
        endpoint: &Endpoint,
        direction: Direction,
        spec: StreamSpec,
        mode: ShareMode,
    ) -> Result<ProbeResult, AudioError>;
    fn open_output(
        &self,
        endpoint: &Endpoint,
        spec: StreamSpec,
        mode: ShareMode,
    ) -> Result<Box<dyn OutputSession>, AudioError>;
    fn open_input(
        &self,
        endpoint: &Endpoint,
        spec: StreamSpec,
        mode: ShareMode,
    ) -> Result<Box<dyn InputSession>, AudioError>;
}

/// Plays one interleaved f32 buffer to completion (including the backend's drain) and returns.
pub trait OutputSession {
    fn play_to_completion(
        &mut self,
        interleaved: &[f32],
        cancel: &CancelToken,
    ) -> Result<PlaybackReport, AudioError>;
}

/// Blocking capture into caller-owned interleaved f32 storage.
pub trait InputSession {
    fn start(&mut self) -> Result<(), AudioError>;
    fn read_into(
        &mut self,
        dst: &mut [f32],
        cancel: &CancelToken,
    ) -> Result<CaptureRead, AudioError>;
    fn stop(&mut self) -> Result<(), AudioError>;
}
