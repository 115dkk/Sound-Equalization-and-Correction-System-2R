#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Native measurement without REW (docs/adr/0005).
//!
//! Platform-independent: sweep design, the play-and-record session contract
//! (open input, confirm ready, play to completion and drain, stop, join),
//! deconvolution to impulse responses, timing reference, level check and
//! microphone calibration files. Audio devices come in through the
//! `AudioBackend` seam: WASAPI and cpal adapters live in `secs-sys` and are
//! injected by the app; tests use a simulated room that convolves the sweep
//! with known impulse responses.
