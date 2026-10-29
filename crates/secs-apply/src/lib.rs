#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Applying exported designs without operating-system-specific dependencies.

pub mod camilla;
pub mod eqapo;
pub mod supervise;

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("connect {addr}: {message}")]
    Connect { addr: String, message: String },
    #[error("websocket: {0}")]
    Transport(String),
    #[error("{command}: {message}")]
    Rejected { command: String, message: String },
    #[error("{command}: unexpected reply {reply}")]
    Reply { command: String, reply: String },
    #[error("{command}: not supported by this CamillaDSP")]
    Unsupported { command: String },
}
