//! Typed engine diagnostics; presentation owns their translated text.

use crate::input::Peq;

#[derive(Debug, Clone, PartialEq)]
pub enum Note {
    // RC (P09)
    LongModeBoost {
        cap_db: f64,
        modes: Vec<(f64, f64)>,
    },
    GdCuts {
        role: String,
        cuts: Vec<Peq>,
    },
    // BA (P10)
    BaCost(f64),
    BaBusPass {
        first: f64,
        second: f64,
        kept_first: bool,
    },
    BaManual,
    LayoutFallback(String),
    Layout(String),
    TwoWay {
        roles: Vec<String>,
        gain_db: f64,
        xo_hz: f64,
    },
    BusGainAnchored(f64),
}
