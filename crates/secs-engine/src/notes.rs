//! Typed engine diagnostics; presentation owns their translated text.

use crate::input::Peq;

#[derive(Debug, Clone, PartialEq)]
pub enum Note {
    LongModeBoost { cap_db: f64, modes: Vec<(f64, f64)> },
    GdCuts { role: String, cuts: Vec<Peq> },
}
