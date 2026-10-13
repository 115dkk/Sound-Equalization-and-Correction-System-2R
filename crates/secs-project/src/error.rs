use crate::PyJsonError;
use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("invalid project JSON: {0}")]
    Json(#[from] PyJsonError),
    #[error("invalid project: {0}")]
    Invalid(String),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid WAV: {0}")]
    Wav(String),
}
