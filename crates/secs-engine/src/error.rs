//! Engine errors.

/// Why a design cannot be computed. Messages are for logs; the service maps
/// variants to user-facing error codes.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// Resampling was requested for a rate outside the converter's bounds.
    #[error("invalid input: position {position} {role}: cannot resample sample rate {actual_hz}")]
    UnsupportedResamplingRate {
        position: u32,
        role: String,
        actual_hz: u32,
    },
    /// An input failure without a structured measurement diagnostic.
    #[error("invalid input: {0}")]
    Input(String),
    /// A reservation exceeded the computation's memory budget.
    #[error("request of {requested} bytes exceeds memory budget of {total} bytes")]
    Memory { requested: u64, total: u64 },
    /// The CPU solver failed after any accelerator fallback.
    #[error("solver failed: {0}")]
    Solver(String),
    /// A channel's measurement does not use the engine sample rate.
    #[error(
        "invalid input: position {position} {role}: sample rate {actual_hz} (expected {expected_hz})"
    )]
    MeasurementSampleRate {
        position: u32,
        role: String,
        actual_hz: u32,
        expected_hz: u32,
    },
    /// An IR is empty, or its samples/time origin contain a nonfinite value.
    #[error("invalid input: position {position} {role}: empty or nonfinite IR")]
    InvalidMeasurement { position: u32, role: String },
    /// A position contains two measurements assigned to the same role.
    #[error("invalid input: position {position}: duplicate role {role}")]
    DuplicateMeasurement { position: u32, role: String },
    /// The configured reference position has no measurements.
    #[error("invalid input: missing main position")]
    MissingReferencePosition { position: u32 },
    /// The reference position has no measurement of a configured main speaker.
    #[error("invalid input: no measured main")]
    MissingMainMeasurement { position: u32 },
    /// A configured sub has no measurement at the reference position.
    #[error("invalid input: position {position}: missing sub measurement {role}")]
    MissingSubMeasurement { position: u32, role: String },
    /// A DSP primitive rejected its arguments.
    #[error(transparent)]
    Dsp(#[from] secs_dsp::DspError),
    /// The computation was cancelled.
    #[error("cancelled")]
    Cancelled,
}

impl From<secs_compute::Cancelled> for EngineError {
    fn from(_: secs_compute::Cancelled) -> Self {
        Self::Cancelled
    }
}
