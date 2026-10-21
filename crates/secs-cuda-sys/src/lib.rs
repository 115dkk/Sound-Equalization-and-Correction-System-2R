#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Owned, checked access to optional CUDA 13 runtime libraries (ADR 0008).
//! CPU-only machines can build and load the application without this runtime.

#[cfg(any(target_os = "windows", target_os = "linux"))]
mod native;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    UnsupportedPlatform,
    RuntimeUnavailable,
    NoDevice,
    DeviceFailure,
    InsufficientMemory,
    InvalidInput,
    Cancelled,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{detail}")]
pub struct Error {
    pub kind: ErrorKind,
    pub detail: String,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: u32,
    pub name: String,
    pub compute_major: u32,
    pub compute_minor: u32,
    pub multiprocessors: u32,
    pub memory_bytes: u64,
    pub integrated: bool,
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
pub use native::{Runtime, SpectralSystem};

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
mod unavailable {
    use super::{Device, Error, ErrorKind};

    #[derive(Debug, Clone)]
    pub struct Runtime;
    #[derive(Debug)]
    pub struct SpectralSystem;
    fn unsupported() -> Error {
        Error::new(
            ErrorKind::UnsupportedPlatform,
            "CUDA is supported on Windows and Linux",
        )
    }
    impl Runtime {
        pub fn load() -> Result<Self, Error> {
            Err(unsupported())
        }
        pub fn devices(&self) -> Result<Vec<Device>, Error> {
            Err(unsupported())
        }
        pub fn factor(
            &self,
            _: u32,
            _: &[f64],
            _: &[f64],
            _: &dyn Fn() -> bool,
        ) -> Result<SpectralSystem, Error> {
            Err(unsupported())
        }
    }
    impl SpectralSystem {
        pub fn solve(&mut self, _: f64, _: &dyn Fn() -> bool) -> Result<Vec<f64>, Error> {
            Err(unsupported())
        }
        pub fn solve_rhs(
            &mut self,
            _: f64,
            _: &[f64],
            _: &dyn Fn() -> bool,
        ) -> Result<Vec<f64>, Error> {
            Err(unsupported())
        }
    }
}
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub use unavailable::{Runtime, SpectralSystem};
