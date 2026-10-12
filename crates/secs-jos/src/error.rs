// Port of src/secs/rew_mdat/errors.py (GPL-3.0-only).
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum JosError {
    Format {
        message: String,
        offset: Option<u64>,
    },
    Unsupported {
        message: String,
        offset: Option<u64>,
    },
    InvalidHandle {
        message: String,
        offset: Option<u64>,
    },
    LimitExceeded {
        message: String,
        offset: Option<u64>,
    },
}
impl JosError {
    pub fn message(&self) -> &str {
        match self {
            Self::Format { message, .. }
            | Self::Unsupported { message, .. }
            | Self::InvalidHandle { message, .. }
            | Self::LimitExceeded { message, .. } => message,
        }
    }
    pub fn offset(&self) -> Option<u64> {
        match self {
            Self::Format { offset, .. }
            | Self::Unsupported { offset, .. }
            | Self::InvalidHandle { offset, .. }
            | Self::LimitExceeded { offset, .. } => *offset,
        }
    }
    pub(crate) fn at(mut self, at: usize) -> Self {
        let offset = match &mut self {
            Self::Format { offset, .. }
            | Self::Unsupported { offset, .. }
            | Self::InvalidHandle { offset, .. }
            | Self::LimitExceeded { offset, .. } => offset,
        };
        if offset.is_none() {
            *offset = Some(at as u64);
        }
        self
    }
    pub(crate) fn format(message: impl Into<String>) -> Self {
        Self::Format {
            message: message.into(),
            offset: None,
        }
    }
    pub(crate) fn limit(message: impl Into<String>, at: usize) -> Self {
        Self::LimitExceeded {
            message: message.into(),
            offset: Some(at as u64),
        }
    }
}
impl fmt::Display for JosError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())?;
        if let Some(offset) = self.offset() {
            write!(f, " at offset 0x{offset:x}")?;
        }
        Ok(())
    }
}
