//! Typed engine diagnostics; presentation owns their translated text.

use crate::input::Peq;

#[derive(Debug, Clone, PartialEq)]
pub enum Note {
    /// ADR 0033, one per main whose direct sound is opposite to the majority
    /// of the mains it could be correlated with. `compared` is the number of
    /// decided pairs, `opposite` how many of them were negative, `confidence`
    /// the median |r| over the decided pairs.
    PolarityReversed {
        role: String,
        confidence: f64,
        compared: usize,
        opposite: usize,
    },
    PolaritySplit {
        groups: (Vec<String>, Vec<String>),
        confidence: f64,
    },
    /// ADR 0035, one per main pair whose corrected reference response cancels
    /// by at least 6 dB in one or more inspected one-third-octave bands.
    PairPhase {
        a: String,
        b: String,
        bands: Vec<crate::pair_phase::Band>,
    },
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
    /// ADR 0034, one per main when `dip_limit` is on. Each entry is
    /// `(centre Hz, depth dB, allowed fill dB)`, deepest first.
    DipLimit {
        role: String,
        dips: Vec<(f64, f64, f64)>,
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
    /// Input whose joint support fit returned its seeds after both solves failed.
    IirSeedFallback {
        support: String,
        status: i32,
    },
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
    /// Measured position whose band change broke the limit when the seat
    /// guard is on (ADR 0036); the tail guard has its own.
    pub band_position: Option<u32>,
    /// Measured position whose modal tail broke the limit when the seat guard is on.
    pub tail_position: Option<u32>,
    /// Late energy rise over every position, `(lo Hz, hi Hz, dB)`: a
    /// third-octave band or a ±1.5 Hz band around a late mode.
    pub band: Option<(f64, f64, f64)>,
    /// Modal tail energy rise over every position, `(mode Hz, dB)`.
    pub tail: Option<(f64, f64)>,
}

/// One wire record of a note: the stable code and scalar string parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteRecord {
    pub code: &'static str,
    pub params: Vec<(&'static str, String)>,
}

/// Every stable record code produced by [`Note::record`] and [`MimoNote::record`].
pub const RECORD_CODES: &[&str] = &[
    "polarity_reversed",
    "polarity_split",
    "pair_phase",
    "lift_capped",
    "sub_filters",
    "sub_filter_peq",
    "low_protect",
    "rc_limit",
    "rc_bands",
    "lf_align",
    "mode_decay",
    "mode_decay_none",
    "hf_phase",
    "hf_phase_none",
    "dip_limit",
    "input_hp",
    "latency",
    "phase_strength",
    "phase_band",
    "long_mode_boost",
    "gd_cuts",
    "ba_cost",
    "ba_bus_pass",
    "ba_manual",
    "layout_fallback",
    "layout",
    "two_way",
    "bus_gain_anchored",
    "mimo_iir_seed_fallback",
    "mimo_insufficient_positions",
    "mimo_support_excluded",
    "mimo_support_range",
    "mimo_guard_reduced",
    "mimo_guard_disabled",
    "mimo_support_limit",
    "mimo_fir_budget",
];

fn record(code: &'static str, params: Vec<(&'static str, String)>) -> NoteRecord {
    NoteRecord { code, params }
}

impl Note {
    /// Expands the note into records in producer order. Every value is scalar.
    pub fn record(&self) -> Vec<NoteRecord> {
        match self {
            Self::PolarityReversed {
                role,
                confidence,
                compared,
                opposite,
            } => vec![record(
                "polarity_reversed",
                vec![
                    ("role", role.clone()),
                    ("confidence", confidence.to_string()),
                    ("compared", compared.to_string()),
                    ("opposite", opposite.to_string()),
                ],
            )],
            Self::PolaritySplit { groups, confidence } => vec![record(
                "polarity_split",
                vec![
                    ("a", groups.0.join(",")),
                    ("b", groups.1.join(",")),
                    ("confidence", confidence.to_string()),
                ],
            )],
            Self::PairPhase { a, b, bands } => vec![record(
                "pair_phase",
                vec![
                    ("a", a.clone()),
                    ("b", b.clone()),
                    (
                        "bands",
                        bands
                            .iter()
                            .map(|(lo, hi, phase, loss)| {
                                format!("{lo:.0}:{hi:.0}:{phase:.1}:{loss:.1}")
                            })
                            .collect::<Vec<_>>()
                            .join(","),
                    ),
                ],
            )],
            Self::LiftCapped {
                from_ms,
                taps,
                lift_ms,
            } => vec![record(
                "lift_capped",
                vec![
                    ("from_ms", from_ms.to_string()),
                    ("taps", taps.to_string()),
                    ("lift_ms", lift_ms.to_string()),
                ],
            )],
            Self::SubFilters(filters) => filters
                .iter()
                .flat_map(|(role, filter)| {
                    let mut records = vec![record(
                        "sub_filters",
                        vec![
                            ("role", role.clone()),
                            ("hp_hz", filter.hp_hz.to_string()),
                            ("hp_type", filter.hp_type.name().into()),
                            ("lp_hz", filter.lp_hz.to_string()),
                            ("lp_type", filter.lp_type.name().into()),
                            ("peq_count", filter.peq.len().to_string()),
                        ],
                    )];
                    records.extend(filter.peq.iter().enumerate().map(
                        |(index, (hz, q, gain_db))| {
                            record(
                                "sub_filter_peq",
                                vec![
                                    ("role", role.clone()),
                                    ("index", index.to_string()),
                                    ("hz", hz.to_string()),
                                    ("q", q.to_string()),
                                    ("gain_db", gain_db.to_string()),
                                ],
                            )
                        },
                    ));
                    records
                })
                .collect(),
            Self::LowProtect {
                role,
                rolloff_hz,
                hp_hz,
                limited,
                reduced,
                cuts,
            } => {
                let mut params = vec![
                    ("role", role.clone()),
                    ("rolloff_hz", rolloff_hz.to_string()),
                    ("reduced", reduced.to_string()),
                    ("cuts", cuts.to_string()),
                    ("limited", limited.to_string()),
                ];
                if let Some(hz) = hp_hz {
                    params.push(("hp_hz", hz.to_string()));
                }
                vec![record("low_protect", params)]
            }
            Self::RcLimit {
                hz,
                auto,
                schroeder_hz,
                basis,
                factor,
                band_hz,
                band_t60_s,
                positions,
            } => {
                let mut params = vec![("hz", hz.to_string()), ("auto", auto.to_string())];
                if let Some(value) = schroeder_hz {
                    params.push(("schroeder_hz", value.to_string()));
                }
                params.push(("basis", basis.as_str().into()));
                params.push(("factor", factor.to_string()));
                if let Some(value) = band_hz {
                    params.push(("band_hz", value.to_string()));
                }
                if let Some(value) = band_t60_s {
                    params.push(("band_t60_s", value.to_string()));
                }
                if let Some(value) = positions {
                    params.push(("positions", value.to_string()));
                }
                vec![record("rc_limit", params)]
            }
            Self::RcBands { bands } => vec![record(
                "rc_bands",
                vec![(
                    "bands",
                    bands
                        .iter()
                        .map(|(lo_hz, hi_hz)| format!("{lo_hz}-{hi_hz}"))
                        .collect::<Vec<_>>()
                        .join(","),
                )],
            )],
            Self::LfAlign {
                role,
                strength,
                arrival_ms,
                limited,
            } => {
                let mut params = vec![
                    ("role", role.clone()),
                    ("strength", strength.to_string()),
                    ("limited", limited.to_string()),
                ];
                if let Some(value) = arrival_ms {
                    params.push(("arrival_ms", value.to_string()));
                }
                vec![record("lf_align", params)]
            }
            Self::ModeDecay {
                role,
                target_t60_s,
                modes,
            } if modes.is_empty() => vec![record(
                "mode_decay_none",
                vec![
                    ("role", role.clone()),
                    ("target_s", target_t60_s.to_string()),
                ],
            )],
            Self::ModeDecay {
                role,
                target_t60_s,
                modes,
            } => modes
                .iter()
                .map(|(hz, t60_s, depth_db)| {
                    record(
                        "mode_decay",
                        vec![
                            ("role", role.clone()),
                            ("hz", hz.to_string()),
                            ("t60_s", t60_s.to_string()),
                            ("target_s", target_t60_s.to_string()),
                            ("depth_db", depth_db.to_string()),
                        ],
                    )
                })
                .collect(),
            Self::HfPhase {
                role,
                rotation_deg,
                limited,
            } => {
                let mut params = vec![("role", role.clone())];
                let code = if let Some(value) = rotation_deg {
                    params.push(("rotation_deg", value.to_string()));
                    params.push(("limited", limited.to_string()));
                    "hf_phase"
                } else {
                    "hf_phase_none"
                };
                vec![record(code, params)]
            }
            Self::DipLimit { role, dips } => vec![record(
                "dip_limit",
                vec![
                    ("role", role.clone()),
                    ("count", dips.len().to_string()),
                    (
                        "dips",
                        dips.iter()
                            .map(|(hz, depth, fill)| format!("{hz:.2}:{depth:.2}:{fill:.2}"))
                            .collect::<Vec<_>>()
                            .join(","),
                    ),
                ],
            )],
            Self::InputHp { hz } => vec![record(
                "input_hp",
                vec![("hz", hz.to_string()), ("order", "8".into())],
            )],
            Self::Latency {
                latency_ms,
                lift_ms,
                main_extra_ms,
            } => vec![record(
                "latency",
                vec![
                    ("latency_ms", latency_ms.to_string()),
                    ("lift_ms", lift_ms.to_string()),
                    ("main_extra_ms", main_extra_ms.to_string()),
                ],
            )],
            Self::PhaseStrength(values) => values
                .iter()
                .map(|(role, strength)| {
                    record(
                        "phase_strength",
                        vec![("role", role.clone()), ("strength", strength.to_string())],
                    )
                })
                .collect(),
            Self::PhaseBand(values) => values
                .iter()
                .map(|(role, hz)| {
                    let mut params = vec![
                        ("role", role.clone()),
                        ("enabled", hz.is_some().to_string()),
                    ];
                    if let Some(hz) = hz {
                        params.push(("hz", hz.to_string()));
                    }
                    record("phase_band", params)
                })
                .collect(),
            Self::LongModeBoost { cap_db, modes } => modes
                .iter()
                .map(|(hz, t60_s)| {
                    record(
                        "long_mode_boost",
                        vec![
                            ("cap_db", cap_db.to_string()),
                            ("hz", hz.to_string()),
                            ("t60_s", t60_s.to_string()),
                        ],
                    )
                })
                .collect(),
            Self::GdCuts { role, cuts } => cuts
                .iter()
                .enumerate()
                .map(|(index, (hz, q, gain_db))| {
                    record(
                        "gd_cuts",
                        vec![
                            ("role", role.clone()),
                            ("index", index.to_string()),
                            ("hz", hz.to_string()),
                            ("q", q.to_string()),
                            ("gain_db", gain_db.to_string()),
                        ],
                    )
                })
                .collect(),
            Self::BaCost(value) => vec![record("ba_cost", vec![("value", value.to_string())])],
            Self::BaBusPass {
                first,
                second,
                kept_first,
            } => vec![record(
                "ba_bus_pass",
                vec![
                    ("first", first.to_string()),
                    ("second", second.to_string()),
                    ("kept_first", kept_first.to_string()),
                ],
            )],
            Self::BaManual => vec![record("ba_manual", Vec::new())],
            Self::LayoutFallback(value) => {
                vec![record("layout_fallback", vec![("value", value.clone())])]
            }
            Self::Layout(value) => vec![record("layout", vec![("value", value.clone())])],
            Self::TwoWay {
                roles,
                gain_db,
                xo_hz,
            } => roles
                .iter()
                .map(|role| {
                    record(
                        "two_way",
                        vec![
                            ("role", role.clone()),
                            ("gain_db", gain_db.to_string()),
                            ("xo_hz", xo_hz.to_string()),
                        ],
                    )
                })
                .collect(),
            Self::BusGainAnchored(value) => vec![record(
                "bus_gain_anchored",
                vec![("value", value.to_string())],
            )],
            Self::Mimo(note) => note.record(),
        }
    }
}

impl MimoNote {
    /// Expands one MIMO note into records in producer order.
    pub fn record(&self) -> Vec<NoteRecord> {
        match self {
            Self::IirSeedFallback { support, status } => vec![record(
                "mimo_iir_seed_fallback",
                vec![("input", support.clone()), ("status", status.to_string())],
            )],
            Self::InsufficientPositions { measured } => vec![record(
                "mimo_insufficient_positions",
                vec![("measured", measured.to_string()), ("required", "3".into())],
            )],
            Self::ExcludedSupports(roles) => roles
                .iter()
                .map(|role| record("mimo_support_excluded", vec![("role", role.clone())]))
                .collect(),
            Self::SupportRanges(ranges) => ranges
                .iter()
                .map(|(role, range)| {
                    let mut params = vec![
                        ("role", role.clone()),
                        ("enabled", range.is_some().to_string()),
                    ];
                    if let Some((lo, hi)) = range {
                        params.extend([("lo_hz", lo.to_string()), ("hi_hz", hi.to_string())]);
                    }
                    record("mimo_support_range", params)
                })
                .collect(),
            Self::GuardReduced {
                role,
                gain_db,
                cause,
            } => {
                let mut params = vec![("role", role.clone()), ("gain_db", gain_db.to_string())];
                params.extend(guard_cause_params(cause));
                vec![record("mimo_guard_reduced", params)]
            }
            Self::GuardDisabled { role, cause } => {
                let mut params = vec![("role", role.clone())];
                params.extend(guard_cause_params(cause));
                vec![record("mimo_guard_disabled", params)]
            }
            Self::SupportLimit { max_supports } => vec![record(
                "mimo_support_limit",
                vec![("max_supports", max_supports.to_string())],
            )],
            Self::FirBudget {
                support_count,
                taps_total,
            } => vec![record(
                "mimo_fir_budget",
                vec![
                    ("support_count", support_count.to_string()),
                    ("taps_total", taps_total.to_string()),
                    ("sample_rate_hz", "48000".into()),
                ],
            )],
        }
    }
}

fn guard_cause_params(cause: &GuardCause) -> Vec<(&'static str, String)> {
    let mut params = vec![("rejected_db", cause.rejected_db.to_string())];
    if let Some(position) = cause.band_position {
        params.push(("band_position", position.to_string()));
    }
    if let Some(position) = cause.tail_position {
        params.push(("tail_position", position.to_string()));
    }
    if let Some((lo, hi, db)) = cause.band {
        params.extend([
            ("band_lo_hz", lo.to_string()),
            ("band_hi_hz", hi.to_string()),
            ("band_db", db.to_string()),
        ]);
    }
    if let Some((hz, db)) = cause.tail {
        params.extend([("mode_hz", hz.to_string()), ("tail_db", db.to_string())]);
    }
    params
}

#[cfg(test)]
impl MimoNote {
    // Only the oracle adapter renders the legacy diagnostic strings.
    pub(crate) fn oracle_text(&self) -> String {
        match self {
            Self::IirSeedFallback { support, status } => {
                format!("IIR seed fallback {support}: {status}")
            }
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
                format!(
                    "FIR 서포트 {support_count}개 · 총 {}탭(48 kHz)",
                    secs_graph::emit::grouped(*taps_total)
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::SubFilter;
    use secs_types::{calibration::RcHiBasis, chain::BandType};

    fn record(code: &'static str, params: &[(&'static str, &str)]) -> NoteRecord {
        NoteRecord {
            code,
            params: params
                .iter()
                .map(|&(key, value)| (key, value.to_owned()))
                .collect(),
        }
    }

    #[test]
    fn record_of_polarity_reversed_matches_the_service_literal() {
        assert_eq!(
            Note::PolarityReversed {
                role: "SR".into(),
                confidence: 0.94,
                compared: 3,
                opposite: 3,
            }
            .record(),
            vec![record(
                "polarity_reversed",
                &[
                    ("role", "SR"),
                    ("confidence", "0.94"),
                    ("compared", "3"),
                    ("opposite", "3"),
                ],
            )],
        );
        assert_eq!(
            Note::PolaritySplit {
                groups: (vec!["L".into(), "R".into()], vec!["C".into(), "Ls".into()],),
                confidence: 0.91,
            }
            .record(),
            vec![record(
                "polarity_split",
                &[("a", "L,R"), ("b", "C,Ls"), ("confidence", "0.91")],
            )],
        );
    }

    #[test]
    fn record_of_pair_phase_matches_the_service_literal() {
        assert_eq!(
            Note::PairPhase {
                a: "L".into(),
                b: "Ls".into(),
                bands: vec![(89.1, 112.2, 167.25, -20.04), (112.2, 141.4, 179.96, -30.0),],
            }
            .record(),
            vec![record(
                "pair_phase",
                &[
                    ("a", "L"),
                    ("b", "Ls"),
                    ("bands", "89:112:167.2:-20.0,112:141:180.0:-30.0"),
                ],
            )],
        );
    }

    #[test]
    fn record_of_rc_limit_and_rc_bands_match_the_service_literals() {
        assert_eq!(
            Note::RcLimit {
                hz: 400.0,
                auto: true,
                schroeder_hz: Some(154.0),
                basis: RcHiBasis::Lenient,
                factor: 2,
                band_hz: Some(200.0),
                band_t60_s: Some(0.8),
                positions: None,
            }
            .record(),
            vec![record(
                "rc_limit",
                &[
                    ("hz", "400"),
                    ("auto", "true"),
                    ("schroeder_hz", "154"),
                    ("basis", "lenient"),
                    ("factor", "2"),
                    ("band_hz", "200"),
                    ("band_t60_s", "0.8"),
                ],
            )],
        );
        assert_eq!(
            Note::RcLimit {
                hz: 287.0,
                auto: true,
                schroeder_hz: None,
                basis: RcHiBasis::Agreement,
                factor: 1,
                band_hz: None,
                band_t60_s: None,
                positions: Some(9),
            }
            .record(),
            vec![record(
                "rc_limit",
                &[
                    ("hz", "287"),
                    ("auto", "true"),
                    ("basis", "agreement"),
                    ("factor", "1"),
                    ("positions", "9"),
                ],
            )],
        );
        assert_eq!(
            Note::RcBands {
                bands: vec![(20.0, 300.0), (15_000.5, 20_000.0)],
            }
            .record(),
            vec![record("rc_bands", &[("bands", "20-300,15000.5-20000")],)],
        );
    }

    #[test]
    fn record_of_sub_filters_mode_decay_and_mimo_guard_match_the_service_literals() {
        let notes = [
            Note::SubFilters(vec![(
                "SW1".into(),
                SubFilter {
                    hp_hz: 10.0,
                    hp_type: BandType::Lr12,
                    lp_hz: 80.0,
                    lp_type: BandType::Lr24,
                    peq: vec![(40.0, 1.0, -2.0)],
                },
            )]),
            Note::ModeDecay {
                role: "L".into(),
                target_t60_s: 0.25,
                modes: vec![(63.0, 0.8, -6.5), (125.0, 0.5, -3.0)],
            },
            Note::ModeDecay {
                role: "R".into(),
                target_t60_s: 0.3,
                modes: Vec::new(),
            },
            Note::Mimo(MimoNote::IirSeedFallback {
                support: "L".into(),
                status: -1,
            }),
            Note::Mimo(MimoNote::GuardReduced {
                role: "L".into(),
                gain_db: -6.25,
                cause: GuardCause {
                    rejected_db: -3.25,
                    band_position: Some(3),
                    tail_position: Some(5),
                    band: Some((40.0, 50.5, 1.5)),
                    tail: None,
                },
            }),
            Note::Mimo(MimoNote::GuardDisabled {
                role: "R".into(),
                cause: GuardCause {
                    rejected_db: -12.0,
                    band_position: None,
                    tail_position: None,
                    band: Some((46.5, 49.5, 2.0)),
                    tail: Some((48.0, 1.25)),
                },
            }),
        ];
        assert_eq!(
            notes.iter().flat_map(Note::record).collect::<Vec<_>>(),
            vec![
                record(
                    "sub_filters",
                    &[
                        ("role", "SW1"),
                        ("hp_hz", "10"),
                        ("hp_type", "LR12"),
                        ("lp_hz", "80"),
                        ("lp_type", "LR24"),
                        ("peq_count", "1"),
                    ],
                ),
                record(
                    "sub_filter_peq",
                    &[
                        ("role", "SW1"),
                        ("index", "0"),
                        ("hz", "40"),
                        ("q", "1"),
                        ("gain_db", "-2"),
                    ],
                ),
                record(
                    "mode_decay",
                    &[
                        ("role", "L"),
                        ("hz", "63"),
                        ("t60_s", "0.8"),
                        ("target_s", "0.25"),
                        ("depth_db", "-6.5"),
                    ],
                ),
                record(
                    "mode_decay",
                    &[
                        ("role", "L"),
                        ("hz", "125"),
                        ("t60_s", "0.5"),
                        ("target_s", "0.25"),
                        ("depth_db", "-3"),
                    ],
                ),
                record("mode_decay_none", &[("role", "R"), ("target_s", "0.3")],),
                record(
                    "mimo_iir_seed_fallback",
                    &[("input", "L"), ("status", "-1")],
                ),
                record(
                    "mimo_guard_reduced",
                    &[
                        ("role", "L"),
                        ("gain_db", "-6.25"),
                        ("rejected_db", "-3.25"),
                        ("band_position", "3"),
                        ("tail_position", "5"),
                        ("band_lo_hz", "40"),
                        ("band_hi_hz", "50.5"),
                        ("band_db", "1.5"),
                    ],
                ),
                record(
                    "mimo_guard_disabled",
                    &[
                        ("role", "R"),
                        ("rejected_db", "-12"),
                        ("band_lo_hz", "46.5"),
                        ("band_hi_hz", "49.5"),
                        ("band_db", "2"),
                        ("mode_hz", "48"),
                        ("tail_db", "1.25"),
                    ],
                ),
            ],
        );
    }

    #[test]
    fn record_of_flat_notes_preserves_scalar_values_and_order() {
        let notes = [
            Note::LowProtect {
                role: "SW1".into(),
                rolloff_hz: 28.0,
                hp_hz: Some(20.0),
                limited: true,
                reduced: 2,
                cuts: 1,
            },
            Note::LowProtect {
                role: "L".into(),
                rolloff_hz: 30.0,
                hp_hz: None,
                limited: false,
                reduced: 2,
                cuts: 1,
            },
            Note::DipLimit {
                role: "R".into(),
                dips: vec![(94.123, 7.456, 4.728), (136.0, 9.2, 5.6)],
            },
            Note::DipLimit {
                role: "L".into(),
                dips: Vec::new(),
            },
            Note::LfAlign {
                role: "L".into(),
                strength: 0.75,
                arrival_ms: Some(2.5),
                limited: true,
            },
            Note::LfAlign {
                role: "R".into(),
                strength: 0.0,
                arrival_ms: None,
                limited: false,
            },
            Note::HfPhase {
                role: "L".into(),
                rotation_deg: Some(-114.5),
                limited: true,
            },
            Note::HfPhase {
                role: "R".into(),
                rotation_deg: None,
                limited: false,
            },
            Note::InputHp { hz: 27.0 },
            Note::PhaseStrength(vec![("L".into(), 0.5), ("R".into(), 1.0)]),
            Note::PhaseBand(vec![("L".into(), Some(100.0)), ("R".into(), None)]),
            Note::LongModeBoost {
                cap_db: 3.0,
                modes: vec![(40.0, 2.0)],
            },
            Note::GdCuts {
                role: "L".into(),
                cuts: vec![(50.0, 2.0, -3.0)],
            },
            Note::TwoWay {
                roles: vec!["SW1".into(), "SW2".into()],
                gain_db: -1.0,
                xo_hz: 50.0,
            },
            Note::Mimo(MimoNote::InsufficientPositions { measured: 2 }),
            Note::Mimo(MimoNote::ExcludedSupports(vec!["SW2".into()])),
            Note::Mimo(MimoNote::SupportRanges(vec![
                ("L".into(), Some((50.25, 150.0))),
                ("R".into(), None),
            ])),
            Note::Mimo(MimoNote::SupportLimit { max_supports: 2 }),
            Note::Mimo(MimoNote::FirBudget {
                support_count: 2,
                taps_total: 16_000,
            }),
        ];
        assert_eq!(
            notes.iter().flat_map(Note::record).collect::<Vec<_>>(),
            vec![
                record(
                    "low_protect",
                    &[
                        ("role", "SW1"),
                        ("rolloff_hz", "28"),
                        ("reduced", "2"),
                        ("cuts", "1"),
                        ("limited", "true"),
                        ("hp_hz", "20"),
                    ],
                ),
                record(
                    "low_protect",
                    &[
                        ("role", "L"),
                        ("rolloff_hz", "30"),
                        ("reduced", "2"),
                        ("cuts", "1"),
                        ("limited", "false"),
                    ],
                ),
                record(
                    "dip_limit",
                    &[
                        ("role", "R"),
                        ("count", "2"),
                        ("dips", "94.12:7.46:4.73,136.00:9.20:5.60"),
                    ],
                ),
                record("dip_limit", &[("role", "L"), ("count", "0"), ("dips", "")],),
                record(
                    "lf_align",
                    &[
                        ("role", "L"),
                        ("strength", "0.75"),
                        ("limited", "true"),
                        ("arrival_ms", "2.5"),
                    ],
                ),
                record(
                    "lf_align",
                    &[("role", "R"), ("strength", "0"), ("limited", "false"),],
                ),
                record(
                    "hf_phase",
                    &[
                        ("role", "L"),
                        ("rotation_deg", "-114.5"),
                        ("limited", "true"),
                    ],
                ),
                record("hf_phase_none", &[("role", "R")]),
                record("input_hp", &[("hz", "27"), ("order", "8")]),
                record("phase_strength", &[("role", "L"), ("strength", "0.5")]),
                record("phase_strength", &[("role", "R"), ("strength", "1")]),
                record(
                    "phase_band",
                    &[("role", "L"), ("enabled", "true"), ("hz", "100")],
                ),
                record("phase_band", &[("role", "R"), ("enabled", "false")],),
                record(
                    "long_mode_boost",
                    &[("cap_db", "3"), ("hz", "40"), ("t60_s", "2")],
                ),
                record(
                    "gd_cuts",
                    &[
                        ("role", "L"),
                        ("index", "0"),
                        ("hz", "50"),
                        ("q", "2"),
                        ("gain_db", "-3"),
                    ],
                ),
                record(
                    "two_way",
                    &[("role", "SW1"), ("gain_db", "-1"), ("xo_hz", "50")],
                ),
                record(
                    "two_way",
                    &[("role", "SW2"), ("gain_db", "-1"), ("xo_hz", "50")],
                ),
                record(
                    "mimo_insufficient_positions",
                    &[("measured", "2"), ("required", "3")],
                ),
                record("mimo_support_excluded", &[("role", "SW2")]),
                record(
                    "mimo_support_range",
                    &[
                        ("role", "L"),
                        ("enabled", "true"),
                        ("lo_hz", "50.25"),
                        ("hi_hz", "150"),
                    ],
                ),
                record("mimo_support_range", &[("role", "R"), ("enabled", "false")],),
                record("mimo_support_limit", &[("max_supports", "2")]),
                record(
                    "mimo_fir_budget",
                    &[
                        ("support_count", "2"),
                        ("taps_total", "16000"),
                        ("sample_rate_hz", "48000"),
                    ],
                ),
            ],
        );
    }

    #[test]
    fn every_note_variant_records_at_least_one_code() {
        let sub_filter = SubFilter {
            hp_hz: 10.0,
            hp_type: BandType::Lr12,
            lp_hz: 80.0,
            lp_type: BandType::Lr24,
            peq: vec![(40.0, 1.0, -2.0)],
        };
        let cause = GuardCause {
            rejected_db: -3.0,
            band_position: None,
            tail_position: None,
            band: None,
            tail: None,
        };
        let notes = vec![
            Note::PolarityReversed {
                role: "L".into(),
                confidence: 0.9,
                compared: 2,
                opposite: 2,
            },
            Note::PolaritySplit {
                groups: (vec!["L".into()], vec!["R".into()]),
                confidence: 0.8,
            },
            Note::PairPhase {
                a: "L".into(),
                b: "R".into(),
                bands: vec![(40.0, 50.0, 180.0, -6.0)],
            },
            Note::LiftCapped {
                from_ms: 2.0,
                taps: 3,
                lift_ms: 1.0,
            },
            Note::SubFilters(vec![("SW1".into(), sub_filter)]),
            Note::LowProtect {
                role: "L".into(),
                rolloff_hz: 30.0,
                hp_hz: None,
                limited: false,
                reduced: 0,
                cuts: 0,
            },
            Note::RcLimit {
                hz: 300.0,
                auto: true,
                schroeder_hz: None,
                basis: RcHiBasis::Strict,
                factor: 1,
                band_hz: None,
                band_t60_s: None,
                positions: None,
            },
            Note::RcBands {
                bands: vec![(20.0, 300.0)],
            },
            Note::LfAlign {
                role: "L".into(),
                strength: 0.75,
                arrival_ms: None,
                limited: false,
            },
            Note::ModeDecay {
                role: "L".into(),
                target_t60_s: 0.25,
                modes: vec![(63.0, 0.8, -6.5)],
            },
            Note::HfPhase {
                role: "L".into(),
                rotation_deg: Some(-90.0),
                limited: false,
            },
            Note::DipLimit {
                role: "L".into(),
                dips: vec![(90.0, 7.0, 4.5)],
            },
            Note::InputHp { hz: 20.0 },
            Note::Latency {
                latency_ms: 10.0,
                lift_ms: 2.0,
                main_extra_ms: 1.0,
            },
            Note::PhaseStrength(vec![("L".into(), 0.5)]),
            Note::PhaseBand(vec![("L".into(), Some(100.0))]),
            Note::LongModeBoost {
                cap_db: 3.0,
                modes: vec![(40.0, 2.0)],
            },
            Note::GdCuts {
                role: "L".into(),
                cuts: vec![(50.0, 2.0, -3.0)],
            },
            Note::BaCost(1.5),
            Note::BaBusPass {
                first: 1.0,
                second: 2.0,
                kept_first: true,
            },
            Note::BaManual,
            Note::LayoutFallback("fallback".into()),
            Note::Layout("dba".into()),
            Note::TwoWay {
                roles: vec!["SW1".into()],
                gain_db: -1.0,
                xo_hz: 50.0,
            },
            Note::BusGainAnchored(-2.0),
            Note::Mimo(MimoNote::IirSeedFallback {
                support: "L".into(),
                status: -1,
            }),
            Note::Mimo(MimoNote::InsufficientPositions { measured: 2 }),
            Note::Mimo(MimoNote::ExcludedSupports(vec!["L".into()])),
            Note::Mimo(MimoNote::SupportRanges(vec![(
                "L".into(),
                Some((50.0, 150.0)),
            )])),
            Note::Mimo(MimoNote::GuardReduced {
                role: "L".into(),
                gain_db: -6.0,
                cause: cause.clone(),
            }),
            Note::Mimo(MimoNote::GuardDisabled {
                role: "R".into(),
                cause,
            }),
            Note::Mimo(MimoNote::SupportLimit { max_supports: 2 }),
            Note::Mimo(MimoNote::FirBudget {
                support_count: 2,
                taps_total: 16_000,
            }),
        ];
        for note in notes {
            let records = note.record();
            assert!(!records.is_empty(), "{note:?}");
            assert!(
                records
                    .iter()
                    .all(|record| RECORD_CODES.contains(&record.code))
            );
        }
    }
}
