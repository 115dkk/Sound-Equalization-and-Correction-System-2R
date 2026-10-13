//! Engine errors.

/// Why a design cannot be computed. Messages are for logs; the service maps
/// variants to user-facing error codes.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// The inputs cannot be designed from (no mains measured, wrong sample
    /// rate, a missing reference position …).
    #[error("invalid input: {0}")]
    Input(String),
    /// A DSP primitive rejected its arguments.
    #[error(transparent)]
    Dsp(#[from] secs_dsp::DspError),
    /// The computation was cancelled.
    #[error("cancelled")]
    Cancelled,
}
