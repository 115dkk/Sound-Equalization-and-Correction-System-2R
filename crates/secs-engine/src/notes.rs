//! Typed engine diagnostics; presentation owns their translated text.

use crate::input::Peq;

#[derive(Debug, Clone, PartialEq)]
pub enum Note {
    LiftCapped {
        from_ms: f64,
        taps: usize,
        lift_ms: f64,
    },
    SubFilters(Vec<(String, crate::setup::SubFilter)>),
    RcLimit {
        hz: f64,
        auto: bool,
        schroeder_hz: Option<f64>,
    },
    Latency {
        latency_ms: f64,
        lift_ms: f64,
        main_extra_ms: f64,
    },
    PhaseStrength(Vec<(String, f64)>),
    PhaseBand(Vec<(String, Option<f64>)>),
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
