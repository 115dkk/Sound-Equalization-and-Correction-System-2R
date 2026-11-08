//! Typed engine diagnostics; presentation owns their translated text.

pub(crate) use secs_types::notes::{
    Band, GuardBand, GuardTail, Peq, RoleBand, RoleNote, RoleValue, SubFilterNote,
};
#[cfg(test)]
use secs_types::notes::{Dip, LongMode, Mode, PairBand, PolarityGroups};
pub use secs_types::notes::{GuardCause, MimoNote, Note};

/// One wire record of a note: the stable code and scalar string parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteRecord {
    pub code: &'static str,
    pub params: Vec<(&'static str, String)>,
}

/// Every stable record code that [`NoteRecords::record`] produces for [`Note`] and [`MimoNote`].
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(not(test), allow(dead_code))]
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

/// Stable scalar-record projection for typed engine notes.
pub trait NoteRecords {
    /// Expands the note into records in producer order. Every value is scalar.
    fn record(&self) -> Vec<NoteRecord>;
}

impl NoteRecords for Note {
    fn record(&self) -> Vec<NoteRecord> {
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
                    ("a", groups.a.join(",")),
                    ("b", groups.b.join(",")),
                    ("confidence", confidence.to_string()),
                ],
            )],
            Self::PairPhase { a, b, bands } => bands
                .iter()
                .enumerate()
                .map(|(index, band)| {
                    record(
                        "pair_phase",
                        vec![
                            ("a", a.clone()),
                            ("b", b.clone()),
                            ("index", index.to_string()),
                            ("lo_hz", format!("{:.0}", band.lo_hz)),
                            ("hi_hz", format!("{:.0}", band.hi_hz)),
                            ("phase_deg", format!("{:.1}", band.phase_deg)),
                            ("loss_db", format!("{:.1}", band.loss_db)),
                        ],
                    )
                })
                .collect(),
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
            Self::SubFilters { filters } => filters
                .iter()
                .flat_map(|entry| {
                    let role = &entry.role;
                    let filter = &entry.value;
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
                    records.extend(filter.peq.iter().enumerate().map(|(index, peq)| {
                        record(
                            "sub_filter_peq",
                            vec![
                                ("role", role.clone()),
                                ("index", index.to_string()),
                                ("hz", peq.hz.to_string()),
                                ("q", peq.q.to_string()),
                                ("gain_db", peq.gain_db.to_string()),
                            ],
                        )
                    }));
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
            Self::RcBands { bands } => bands
                .iter()
                .enumerate()
                .map(|(index, band)| {
                    record(
                        "rc_bands",
                        vec![
                            ("index", index.to_string()),
                            ("lo_hz", band.lo_hz.to_string()),
                            ("hi_hz", band.hi_hz.to_string()),
                        ],
                    )
                })
                .collect(),
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
                .map(|mode| {
                    record(
                        "mode_decay",
                        vec![
                            ("role", role.clone()),
                            ("hz", mode.hz.to_string()),
                            ("t60_s", mode.t60_s.to_string()),
                            ("target_s", target_t60_s.to_string()),
                            ("depth_db", mode.cut_db.to_string()),
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
            Self::DipLimit { role, dips } => dips
                .iter()
                .enumerate()
                .map(|(index, dip)| {
                    record(
                        "dip_limit",
                        vec![
                            ("role", role.clone()),
                            ("index", index.to_string()),
                            ("hz", format!("{:.2}", dip.hz)),
                            ("depth_db", format!("{:.2}", dip.depth_db)),
                            ("fill_db", format!("{:.2}", dip.fill_db)),
                        ],
                    )
                })
                .collect(),
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
            Self::PhaseStrength { values } => values
                .iter()
                .map(|entry| {
                    record(
                        "phase_strength",
                        vec![
                            ("role", entry.role.clone()),
                            ("strength", entry.value.to_string()),
                        ],
                    )
                })
                .collect(),
            Self::PhaseBand { values } => values
                .iter()
                .map(|entry| {
                    let mut params = vec![
                        ("role", entry.role.clone()),
                        ("enabled", entry.hz.is_some().to_string()),
                    ];
                    if let Some(hz) = entry.hz {
                        params.push(("hz", hz.to_string()));
                    }
                    record("phase_band", params)
                })
                .collect(),
            Self::LongModeBoost { cap_db, modes } => modes
                .iter()
                .map(|mode| {
                    record(
                        "long_mode_boost",
                        vec![
                            ("cap_db", cap_db.to_string()),
                            ("hz", mode.hz.to_string()),
                            ("t60_s", mode.t60_s.to_string()),
                        ],
                    )
                })
                .collect(),
            Self::GdCuts { role, cuts } => cuts
                .iter()
                .enumerate()
                .map(|(index, peq)| {
                    record(
                        "gd_cuts",
                        vec![
                            ("role", role.clone()),
                            ("index", index.to_string()),
                            ("hz", peq.hz.to_string()),
                            ("q", peq.q.to_string()),
                            ("gain_db", peq.gain_db.to_string()),
                        ],
                    )
                })
                .collect(),
            Self::BaCost { value } => {
                vec![record("ba_cost", vec![("value", value.to_string())])]
            }
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
            Self::LayoutFallback { value } => {
                vec![record("layout_fallback", vec![("value", value.clone())])]
            }
            Self::Layout { value } => vec![record("layout", vec![("value", value.clone())])],
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
            Self::BusGainAnchored { value } => vec![record(
                "bus_gain_anchored",
                vec![("value", value.to_string())],
            )],
            Self::Mimo { note } => note.record(),
        }
    }
}

impl NoteRecords for MimoNote {
    fn record(&self) -> Vec<NoteRecord> {
        match self {
            Self::IirSeedFallback { support, status } => vec![record(
                "mimo_iir_seed_fallback",
                vec![("input", support.clone()), ("status", status.to_string())],
            )],
            Self::InsufficientPositions { measured } => vec![record(
                "mimo_insufficient_positions",
                vec![("measured", measured.to_string()), ("required", "3".into())],
            )],
            Self::ExcludedSupports { roles } => roles
                .iter()
                .map(|role| record("mimo_support_excluded", vec![("role", role.clone())]))
                .collect(),
            Self::SupportRanges { ranges } => ranges
                .iter()
                .map(|entry| {
                    let mut params = vec![
                        ("role", entry.role.clone()),
                        ("enabled", entry.value.is_some().to_string()),
                    ];
                    if let Some(range) = &entry.value {
                        params.extend([
                            ("lo_hz", range.lo_hz.to_string()),
                            ("hi_hz", range.hi_hz.to_string()),
                        ]);
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
    if let Some(band) = &cause.band {
        params.extend([
            ("band_lo_hz", band.lo_hz.to_string()),
            ("band_hi_hz", band.hi_hz.to_string()),
            ("band_db", band.db.to_string()),
        ]);
    }
    if let Some(tail) = &cause.tail {
        params.extend([
            ("mode_hz", tail.hz.to_string()),
            ("tail_db", tail.db.to_string()),
        ]);
    }
    params
}

#[cfg(test)]
pub(crate) trait MimoOracleText {
    fn oracle_text(&self) -> String;
}

#[cfg(test)]
impl MimoOracleText for MimoNote {
    // Only the oracle adapter renders the legacy diagnostic strings.
    fn oracle_text(&self) -> String {
        match self {
            Self::IirSeedFallback { support, status } => {
                format!("IIR seed fallback {support}: {status}")
            }
            Self::InsufficientPositions { measured } => {
                format!("측정 위치 {measured}곳 — MIMO 는 3곳 이상일 때만")
            }
            Self::ExcludedSupports { roles } => {
                format!("서포트 제외(사용자): {}", roles.join(", "))
            }
            Self::SupportRanges { ranges } => format!(
                "서포트 대역 {}",
                ranges
                    .iter()
                    .map(|entry| match &entry.value {
                        Some(range) =>
                            format!("{} {:.0}–{:.0}", entry.role, range.lo_hz, range.hi_hz),
                        None => format!("{} 제외", entry.role),
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

    fn pair_bands(values: &[(f64, f64, f64, f64)]) -> Vec<PairBand> {
        values
            .iter()
            .map(|&(lo_hz, hi_hz, phase_deg, loss_db)| PairBand {
                lo_hz,
                hi_hz,
                phase_deg,
                loss_db,
            })
            .collect()
    }

    fn bands(values: &[(f64, f64)]) -> Vec<Band> {
        values
            .iter()
            .map(|&(lo_hz, hi_hz)| Band { lo_hz, hi_hz })
            .collect()
    }

    fn dips(values: &[(f64, f64, f64)]) -> Vec<Dip> {
        values
            .iter()
            .map(|&(hz, depth_db, fill_db)| Dip {
                hz,
                depth_db,
                fill_db,
            })
            .collect()
    }

    fn modes(values: &[(f64, f64, f64)]) -> Vec<Mode> {
        values
            .iter()
            .map(|&(hz, t60_s, cut_db)| Mode { hz, t60_s, cut_db })
            .collect()
    }

    fn long_modes(values: &[(f64, f64)]) -> Vec<LongMode> {
        values
            .iter()
            .map(|&(hz, t60_s)| LongMode { hz, t60_s })
            .collect()
    }

    fn peqs(values: &[(f64, f64, f64)]) -> Vec<Peq> {
        values
            .iter()
            .map(|&(hz, q, gain_db)| Peq { hz, q, gain_db })
            .collect()
    }

    fn role_values(values: &[(&str, f64)]) -> Vec<RoleValue> {
        values
            .iter()
            .map(|&(role, value)| RoleValue {
                role: role.into(),
                value,
            })
            .collect()
    }

    fn role_bands(values: &[(&str, Option<f64>)]) -> Vec<RoleBand> {
        values
            .iter()
            .map(|&(role, hz)| RoleBand {
                role: role.into(),
                hz,
            })
            .collect()
    }

    fn sub_filter(role: &str, filter: SubFilter) -> RoleNote<SubFilterNote> {
        RoleNote {
            role: role.into(),
            value: SubFilterNote {
                hp_hz: filter.hp_hz,
                hp_type: filter.hp_type,
                lp_hz: filter.lp_hz,
                lp_type: filter.lp_type,
                peq: peqs(&filter.peq),
            },
        }
    }

    fn mimo(note: MimoNote) -> Note {
        Note::Mimo { note }
    }

    fn range(role: &str, value: Option<(f64, f64)>) -> RoleNote<Option<Band>> {
        RoleNote {
            role: role.into(),
            value: value.map(|(lo_hz, hi_hz)| Band { lo_hz, hi_hz }),
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
                groups: PolarityGroups {
                    a: vec!["L".into(), "R".into()],
                    b: vec!["C".into(), "Ls".into()],
                },
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
    fn record_of_rc_limit_matches_the_service_literals() {
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
    }

    #[test]
    fn record_of_sub_filters_mode_decay_and_mimo_guard_match_the_service_literals() {
        let notes = [
            Note::SubFilters {
                filters: vec![sub_filter(
                    "SW1",
                    SubFilter {
                        hp_hz: 10.0,
                        hp_type: BandType::Lr12,
                        lp_hz: 80.0,
                        lp_type: BandType::Lr24,
                        peq: vec![(40.0, 1.0, -2.0)],
                    },
                )],
            },
            Note::ModeDecay {
                role: "L".into(),
                target_t60_s: 0.25,
                modes: modes(&[(63.0, 0.8, -6.5), (125.0, 0.5, -3.0)]),
            },
            Note::ModeDecay {
                role: "R".into(),
                target_t60_s: 0.3,
                modes: Vec::new(),
            },
            mimo(MimoNote::IirSeedFallback {
                support: "L".into(),
                status: -1,
            }),
            mimo(MimoNote::GuardReduced {
                role: "L".into(),
                gain_db: -6.25,
                cause: GuardCause {
                    rejected_db: -3.25,
                    band_position: Some(3),
                    tail_position: Some(5),
                    band: Some(GuardBand {
                        lo_hz: 40.0,
                        hi_hz: 50.5,
                        db: 1.5,
                    }),
                    tail: None,
                },
            }),
            mimo(MimoNote::GuardDisabled {
                role: "R".into(),
                cause: GuardCause {
                    rejected_db: -12.0,
                    band_position: None,
                    tail_position: None,
                    band: Some(GuardBand {
                        lo_hz: 46.5,
                        hi_hz: 49.5,
                        db: 2.0,
                    }),
                    tail: Some(GuardTail { hz: 48.0, db: 1.25 }),
                },
            }),
        ];
        assert_eq!(
            notes
                .iter()
                .flat_map(NoteRecords::record)
                .collect::<Vec<_>>(),
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
            Note::PhaseStrength {
                values: role_values(&[("L", 0.5), ("R", 1.0)]),
            },
            Note::PhaseBand {
                values: role_bands(&[("L", Some(100.0)), ("R", None)]),
            },
            Note::LongModeBoost {
                cap_db: 3.0,
                modes: long_modes(&[(40.0, 2.0)]),
            },
            Note::GdCuts {
                role: "L".into(),
                cuts: peqs(&[(50.0, 2.0, -3.0)]),
            },
            Note::TwoWay {
                roles: vec!["SW1".into(), "SW2".into()],
                gain_db: -1.0,
                xo_hz: 50.0,
            },
            mimo(MimoNote::InsufficientPositions { measured: 2 }),
            mimo(MimoNote::ExcludedSupports {
                roles: vec!["SW2".into()],
            }),
            mimo(MimoNote::SupportRanges {
                ranges: vec![range("L", Some((50.25, 150.0))), range("R", None)],
            }),
            mimo(MimoNote::SupportLimit { max_supports: 2 }),
            mimo(MimoNote::FirBudget {
                support_count: 2,
                taps_total: 16_000,
            }),
        ];
        assert_eq!(
            notes
                .iter()
                .flat_map(NoteRecords::record)
                .collect::<Vec<_>>(),
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
        let filter = SubFilter {
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
                groups: PolarityGroups {
                    a: vec!["L".into()],
                    b: vec!["R".into()],
                },
                confidence: 0.8,
            },
            Note::PairPhase {
                a: "L".into(),
                b: "R".into(),
                bands: pair_bands(&[(40.0, 50.0, 180.0, -6.0)]),
            },
            Note::LiftCapped {
                from_ms: 2.0,
                taps: 3,
                lift_ms: 1.0,
            },
            Note::SubFilters {
                filters: vec![sub_filter("SW1", filter)],
            },
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
                bands: bands(&[(20.0, 300.0)]),
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
                modes: modes(&[(63.0, 0.8, -6.5)]),
            },
            Note::HfPhase {
                role: "L".into(),
                rotation_deg: Some(-90.0),
                limited: false,
            },
            Note::DipLimit {
                role: "L".into(),
                dips: dips(&[(90.0, 7.0, 4.5)]),
            },
            Note::InputHp { hz: 20.0 },
            Note::Latency {
                latency_ms: 10.0,
                lift_ms: 2.0,
                main_extra_ms: 1.0,
            },
            Note::PhaseStrength {
                values: role_values(&[("L", 0.5)]),
            },
            Note::PhaseBand {
                values: role_bands(&[("L", Some(100.0))]),
            },
            Note::LongModeBoost {
                cap_db: 3.0,
                modes: long_modes(&[(40.0, 2.0)]),
            },
            Note::GdCuts {
                role: "L".into(),
                cuts: peqs(&[(50.0, 2.0, -3.0)]),
            },
            Note::BaCost { value: 1.5 },
            Note::BaBusPass {
                first: 1.0,
                second: 2.0,
                kept_first: true,
            },
            Note::BaManual,
            Note::LayoutFallback {
                value: "fallback".into(),
            },
            Note::Layout {
                value: "dba".into(),
            },
            Note::TwoWay {
                roles: vec!["SW1".into()],
                gain_db: -1.0,
                xo_hz: 50.0,
            },
            Note::BusGainAnchored { value: -2.0 },
            mimo(MimoNote::IirSeedFallback {
                support: "L".into(),
                status: -1,
            }),
            mimo(MimoNote::InsufficientPositions { measured: 2 }),
            mimo(MimoNote::ExcludedSupports {
                roles: vec!["L".into()],
            }),
            mimo(MimoNote::SupportRanges {
                ranges: vec![range("L", Some((50.0, 150.0)))],
            }),
            mimo(MimoNote::GuardReduced {
                role: "L".into(),
                gain_db: -6.0,
                cause: cause.clone(),
            }),
            mimo(MimoNote::GuardDisabled {
                role: "R".into(),
                cause,
            }),
            mimo(MimoNote::SupportLimit { max_supports: 2 }),
            mimo(MimoNote::FirBudget {
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

    #[test]
    fn pair_phase_records_one_record_per_band_with_index() {
        let note = Note::PairPhase {
            a: "L".into(),
            b: "Ls".into(),
            bands: pair_bands(&[(89.1, 112.2, 167.25, -20.04), (112.2, 141.4, 179.96, -30.0)]),
        };
        assert_eq!(
            note.record(),
            vec![
                record(
                    "pair_phase",
                    &[
                        ("a", "L"),
                        ("b", "Ls"),
                        ("index", "0"),
                        ("lo_hz", "89"),
                        ("hi_hz", "112"),
                        ("phase_deg", "167.2"),
                        ("loss_db", "-20.0"),
                    ],
                ),
                record(
                    "pair_phase",
                    &[
                        ("a", "L"),
                        ("b", "Ls"),
                        ("index", "1"),
                        ("lo_hz", "112"),
                        ("hi_hz", "141"),
                        ("phase_deg", "180.0"),
                        ("loss_db", "-30.0"),
                    ],
                ),
            ],
        );
    }

    #[test]
    fn dip_limit_records_one_record_per_dip() {
        let note = Note::DipLimit {
            role: "R".into(),
            dips: dips(&[(94.123, 7.456, 4.728), (136.0, 9.2, 5.6)]),
        };
        assert_eq!(
            note.record(),
            vec![
                record(
                    "dip_limit",
                    &[
                        ("role", "R"),
                        ("index", "0"),
                        ("hz", "94.12"),
                        ("depth_db", "7.46"),
                        ("fill_db", "4.73"),
                    ],
                ),
                record(
                    "dip_limit",
                    &[
                        ("role", "R"),
                        ("index", "1"),
                        ("hz", "136.00"),
                        ("depth_db", "9.20"),
                        ("fill_db", "5.60"),
                    ],
                ),
            ],
        );
        assert!(
            Note::DipLimit {
                role: "L".into(),
                dips: Vec::new(),
            }
            .record()
            .is_empty()
        );
    }

    #[test]
    fn rc_bands_records_one_record_per_band() {
        let note = Note::RcBands {
            bands: bands(&[(20.0, 300.0), (15_000.5, 20_000.0)]),
        };
        assert_eq!(
            note.record(),
            vec![
                record(
                    "rc_bands",
                    &[("index", "0"), ("lo_hz", "20"), ("hi_hz", "300")],
                ),
                record(
                    "rc_bands",
                    &[("index", "1"), ("lo_hz", "15000.5"), ("hi_hz", "20000"),],
                ),
            ],
        );
    }
}
