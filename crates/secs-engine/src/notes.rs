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
    LowProtect {
        role: String,
        rolloff_hz: f64,
        hp_hz: Option<f64>,
        limited: bool,
        reduced: usize,
        cuts: usize,
    },
    RcLimit {
        hz: f64,
        auto: bool,
        schroeder_hz: Option<f64>,
        basis: secs_types::calibration::RcHiBasis,
        factor: u8,
        band_hz: Option<f64>,
        band_t60_s: Option<f64>,
        positions: Option<usize>,
    },
    RcBands {
        bands: Vec<(f64, f64)>,
    },
    /// ADR 0025, one per main when `lf_align` is on and the mode has FIR
    /// taps. `arrival_ms` is the median LF arrival left after correction
    /// (positive = later than HF); `limited` means the pull clamp cut the
    /// requested correction by more than 1 ms somewhere in the band.
    LfAlign {
        role: String,
        strength: f64,
        arrival_ms: Option<f64>,
        limited: bool,
    },
    /// ADR 0025, one per main when `mode_decay` is on. Each mode is
    /// `(hz, measured T60 s, cut depth dB)` with the depth positive; an empty
    /// list means no mode qualified.
    ModeDecay {
        role: String,
        target_t60_s: f64,
        modes: Vec<(f64, f64, f64)>,
    },
    /// ADR 0030, one per channel whose FIR realizes excess phase when
    /// `hf_phase` is on. `rotation_deg` is the wrapped rotation unwound above
    /// the band, `None` when less than an octave is left below 0.45·SR;
    /// `limited` means the `[−5 ms, lift]` clamp kept part of it.
    HfPhase {
        role: String,
        rotation_deg: Option<f64>,
        limited: bool,
    },
    /// ADR 0031, once when `input_hp_hz` is set: the common input high-pass,
    /// 8th-order Butterworth, at `hz`.
    InputHp {
        hz: f64,
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
    Mimo(MimoNote),
}

/// Semantic diagnostics in producer order; never translated presentation text.
#[derive(Debug, Clone, PartialEq)]
pub enum MimoNote {
    InsufficientPositions {
        measured: usize,
    },
    ExcludedSupports(Vec<String>),
    SupportRanges(Vec<(String, Option<(f64, f64)>)>),
    GuardReduced {
        role: String,
        gain_db: f64,
        cause: GuardCause,
    },
    GuardDisabled {
        role: String,
        cause: GuardCause,
    },
    SupportLimit {
        max_supports: usize,
    },
    FirBudget {
        support_count: usize,
        taps_total: usize,
    },
}

/// What the MIMO guard rejected last: the support level of that trial
/// (3 dB above `GuardReduced::gain_db`, or −12 dB before `GuardDisabled`)
/// and the guard values there that broke their 1 dB limits.
#[derive(Debug, Clone, PartialEq)]
pub struct GuardCause {
    pub rejected_db: f64,
    /// Late energy rise over every position, `(lo Hz, hi Hz, dB)`: a
    /// third-octave band or a ±1.5 Hz band around a late mode.
    pub band: Option<(f64, f64, f64)>,
    /// Modal tail energy rise over every position, `(mode Hz, dB)`.
    pub tail: Option<(f64, f64)>,
}

#[cfg(test)]
impl MimoNote {
    // Only the oracle adapter renders the legacy diagnostic strings.
    pub(crate) fn oracle_text(&self) -> String {
        match self {
            Self::InsufficientPositions { measured } => {
                format!("측정 위치 {measured}곳 — MIMO 는 3곳 이상일 때만")
            }
            Self::ExcludedSupports(roles) => format!("서포트 제외(사용자): {}", roles.join(", ")),
            Self::SupportRanges(ranges) => format!(
                "서포트 대역 {}",
                ranges
                    .iter()
                    .map(|(role, range)| match range {
                        Some((lo, hi)) => format!("{role} {lo:.0}–{hi:.0}"),
                        None => format!("{role} 제외"),
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::GuardReduced { role, gain_db, .. } => {
                format!("{role}: 대역 늦은 에너지 악화로 서포트 {gain_db:.0} dB")
            }
            Self::GuardDisabled { role, .. } => format!("{role}: MIMO 끔(악화)"),
            Self::SupportLimit { max_supports } => format!("입력당 서포트 ≤ {max_supports}"),
            Self::FirBudget {
                support_count,
                taps_total,
            } => {
                let digits = taps_total.to_string();
                let grouped = digits
                    .chars()
                    .enumerate()
                    .fold(String::new(), |mut out, (i, ch)| {
                        if i > 0 && (digits.len() - i).is_multiple_of(3) {
                            out.push(',');
                        }
                        out.push(ch);
                        out
                    });
                format!("FIR 서포트 {support_count}개 · 총 {grouped}탭(48 kHz)")
            }
        }
    }
}
