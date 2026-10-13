//! Structured engine diagnostics; presentation belongs to the caller.

#[derive(Debug, Clone, PartialEq)]
pub enum Note {
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
