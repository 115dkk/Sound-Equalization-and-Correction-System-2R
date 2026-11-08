//! Typed engine diagnostics shared with API consumers and the screen.

use crate::{calibration::RcHiBasis, chain::BandType};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

mod finite {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &f64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if value.is_finite() {
            serializer.serialize_f64(*value)
        } else {
            Err(<S::Error as serde::ser::Error>::custom(
                "note numbers must be finite",
            ))
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<f64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        if value.is_finite() {
            Ok(value)
        } else {
            Err(<D::Error as serde::de::Error>::custom(
                "note numbers must be finite",
            ))
        }
    }
}

mod finite_option {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(value) if value.is_finite() => serializer.serialize_some(value),
            Some(_) => Err(<S::Error as serde::ser::Error>::custom(
                "note numbers must be finite",
            )),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<f64>::deserialize(deserializer)?;
        match value {
            Some(value) if !value.is_finite() => Err(<D::Error as serde::de::Error>::custom(
                "note numbers must be finite",
            )),
            value => Ok(value),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PolarityGroups {
    pub a: Vec<String>,
    pub b: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PairBand {
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub lo_hz: f64,
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub hi_hz: f64,
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub phase_deg: f64,
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub loss_db: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Dip {
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub hz: f64,
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub depth_db: f64,
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub fill_db: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, rename = "NoteMode")]
pub struct Mode {
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub hz: f64,
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub t60_s: f64,
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub cut_db: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LongMode {
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub hz: f64,
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub t60_s: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Band {
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub lo_hz: f64,
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub hi_hz: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Peq {
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub hz: f64,
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub q: f64,
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub gain_db: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RoleNote<T> {
    pub role: String,
    pub value: T,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RoleValue {
    pub role: String,
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RoleBand {
    pub role: String,
    #[serde(with = "finite_option")]
    #[ts(type = "number | null")]
    pub hz: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubFilterNote {
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub hp_hz: f64,
    pub hp_type: BandType,
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub lp_hz: f64,
    pub lp_type: BandType,
    pub peq: Vec<Peq>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GuardBand {
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub lo_hz: f64,
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub hi_hz: f64,
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub db: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GuardTail {
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub hz: f64,
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub db: f64,
}

/// What the MIMO guard rejected last and the values that broke their limits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GuardCause {
    #[serde(with = "finite")]
    #[ts(type = "number")]
    pub rejected_db: f64,
    pub band_position: Option<u32>,
    pub tail_position: Option<u32>,
    pub band: Option<GuardBand>,
    pub tail: Option<GuardTail>,
}

/// Semantic MIMO diagnostics in producer order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "code", rename_all = "snake_case")]
#[ts(export)]
pub enum MimoNote {
    IirSeedFallback {
        support: String,
        status: i32,
    },
    InsufficientPositions {
        measured: usize,
    },
    ExcludedSupports {
        roles: Vec<String>,
    },
    SupportRanges {
        ranges: Vec<RoleNote<Option<Band>>>,
    },
    GuardReduced {
        role: String,
        #[serde(with = "finite")]
        #[ts(type = "number")]
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

/// Semantic engine diagnostics in producer order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "code", rename_all = "snake_case")]
#[ts(export)]
pub enum Note {
    PolarityReversed {
        role: String,
        #[serde(with = "finite")]
        #[ts(type = "number")]
        confidence: f64,
        compared: usize,
        opposite: usize,
    },
    PolaritySplit {
        groups: PolarityGroups,
        #[serde(with = "finite")]
        #[ts(type = "number")]
        confidence: f64,
    },
    PairPhase {
        a: String,
        b: String,
        bands: Vec<PairBand>,
    },
    LiftCapped {
        #[serde(with = "finite")]
        #[ts(type = "number")]
        from_ms: f64,
        taps: usize,
        #[serde(with = "finite")]
        #[ts(type = "number")]
        lift_ms: f64,
    },
    SubFilters {
        filters: Vec<RoleNote<SubFilterNote>>,
    },
    LowProtect {
        role: String,
        #[serde(with = "finite")]
        #[ts(type = "number")]
        rolloff_hz: f64,
        #[serde(with = "finite_option")]
        #[ts(type = "number | null")]
        hp_hz: Option<f64>,
        limited: bool,
        reduced: usize,
        cuts: usize,
    },
    RcLimit {
        #[serde(with = "finite")]
        #[ts(type = "number")]
        hz: f64,
        auto: bool,
        #[serde(with = "finite_option")]
        #[ts(type = "number | null")]
        schroeder_hz: Option<f64>,
        basis: RcHiBasis,
        factor: u8,
        #[serde(with = "finite_option")]
        #[ts(type = "number | null")]
        band_hz: Option<f64>,
        #[serde(with = "finite_option")]
        #[ts(type = "number | null")]
        band_t60_s: Option<f64>,
        positions: Option<usize>,
    },
    RcBands {
        bands: Vec<Band>,
    },
    LfAlign {
        role: String,
        #[serde(with = "finite")]
        #[ts(type = "number")]
        strength: f64,
        #[serde(with = "finite_option")]
        #[ts(type = "number | null")]
        arrival_ms: Option<f64>,
        limited: bool,
    },
    ModeDecay {
        role: String,
        #[serde(with = "finite")]
        #[ts(type = "number")]
        target_t60_s: f64,
        modes: Vec<Mode>,
    },
    HfPhase {
        role: String,
        #[serde(with = "finite_option")]
        #[ts(type = "number | null")]
        rotation_deg: Option<f64>,
        limited: bool,
    },
    DipLimit {
        role: String,
        dips: Vec<Dip>,
    },
    InputHp {
        #[serde(with = "finite")]
        #[ts(type = "number")]
        hz: f64,
    },
    Latency {
        #[serde(with = "finite")]
        #[ts(type = "number")]
        latency_ms: f64,
        #[serde(with = "finite")]
        #[ts(type = "number")]
        lift_ms: f64,
        #[serde(with = "finite")]
        #[ts(type = "number")]
        main_extra_ms: f64,
    },
    PhaseStrength {
        values: Vec<RoleValue>,
    },
    PhaseBand {
        values: Vec<RoleBand>,
    },
    LongModeBoost {
        #[serde(with = "finite")]
        #[ts(type = "number")]
        cap_db: f64,
        modes: Vec<LongMode>,
    },
    GdCuts {
        role: String,
        cuts: Vec<Peq>,
    },
    BaCost {
        #[serde(with = "finite")]
        #[ts(type = "number")]
        value: f64,
    },
    BaBusPass {
        #[serde(with = "finite")]
        #[ts(type = "number")]
        first: f64,
        #[serde(with = "finite")]
        #[ts(type = "number")]
        second: f64,
        kept_first: bool,
    },
    BaManual,
    LayoutFallback {
        value: String,
    },
    Layout {
        value: String,
    },
    TwoWay {
        roles: Vec<String>,
        #[serde(with = "finite")]
        #[ts(type = "number")]
        gain_db: f64,
        #[serde(with = "finite")]
        #[ts(type = "number")]
        xo_hz: f64,
    },
    BusGainAnchored {
        #[serde(with = "finite")]
        #[ts(type = "number")]
        value: f64,
    },
    Mimo {
        note: MimoNote,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T>(json: &str)
    where
        T: serde::de::DeserializeOwned + Serialize,
    {
        let value: T = serde_json::from_str(json).unwrap();
        assert_eq!(serde_json::to_string(&value).unwrap(), json);
    }

    #[test]
    fn every_note_variant_round_trips_json_with_its_snake_case_code() {
        let notes = [
            r#"{"code":"polarity_reversed","role":"L","confidence":0.9,"compared":2,"opposite":2}"#,
            r#"{"code":"polarity_split","groups":{"a":["L"],"b":["R"]},"confidence":0.8}"#,
            r#"{"code":"pair_phase","a":"L","b":"R","bands":[{"lo_hz":40.0,"hi_hz":50.0,"phase_deg":180.0,"loss_db":-6.0}]}"#,
            r#"{"code":"lift_capped","from_ms":2.0,"taps":2048,"lift_ms":1.0}"#,
            r#"{"code":"sub_filters","filters":[{"role":"SW1","value":{"hp_hz":10.0,"hp_type":"LR12","lp_hz":80.0,"lp_type":"LR24","peq":[{"hz":40.0,"q":1.0,"gain_db":-2.0}]}}]}"#,
            r#"{"code":"low_protect","role":"L","rolloff_hz":30.0,"hp_hz":null,"limited":false,"reduced":0,"cuts":0}"#,
            r#"{"code":"rc_limit","hz":300.0,"auto":true,"schroeder_hz":null,"basis":"strict","factor":1,"band_hz":null,"band_t60_s":null,"positions":null}"#,
            r#"{"code":"rc_bands","bands":[{"lo_hz":20.0,"hi_hz":300.0}]}"#,
            r#"{"code":"lf_align","role":"L","strength":0.75,"arrival_ms":null,"limited":false}"#,
            r#"{"code":"mode_decay","role":"L","target_t60_s":0.25,"modes":[{"hz":63.0,"t60_s":0.8,"cut_db":-6.5}]}"#,
            r#"{"code":"hf_phase","role":"L","rotation_deg":-90.0,"limited":false}"#,
            r#"{"code":"dip_limit","role":"L","dips":[{"hz":80.0,"depth_db":6.1,"fill_db":3.0}]}"#,
            r#"{"code":"input_hp","hz":20.0}"#,
            r#"{"code":"latency","latency_ms":10.0,"lift_ms":2.0,"main_extra_ms":1.0}"#,
            r#"{"code":"phase_strength","values":[{"role":"L","value":0.5}]}"#,
            r#"{"code":"phase_band","values":[{"role":"L","hz":100.0}]}"#,
            r#"{"code":"long_mode_boost","cap_db":3.0,"modes":[{"hz":40.0,"t60_s":2.0}]}"#,
            r#"{"code":"gd_cuts","role":"L","cuts":[{"hz":50.0,"q":2.0,"gain_db":-3.0}]}"#,
            r#"{"code":"ba_cost","value":1.5}"#,
            r#"{"code":"ba_bus_pass","first":1.0,"second":2.0,"kept_first":true}"#,
            r#"{"code":"ba_manual"}"#,
            r#"{"code":"layout_fallback","value":"fallback"}"#,
            r#"{"code":"layout","value":"dba"}"#,
            r#"{"code":"two_way","roles":["SW1"],"gain_db":-1.0,"xo_hz":50.0}"#,
            r#"{"code":"bus_gain_anchored","value":-2.0}"#,
            r#"{"code":"mimo","note":{"code":"support_limit","max_supports":2}}"#,
        ];
        for json in notes {
            round_trip::<Note>(json);
        }

        let cause = r#"{"rejected_db":-3.0,"band_position":3,"tail_position":null,"band":{"lo_hz":40.0,"hi_hz":50.0,"db":1.5},"tail":{"hz":48.0,"db":1.25}}"#;
        let mimo = [
            r#"{"code":"iir_seed_fallback","support":"L","status":-1}"#.to_owned(),
            r#"{"code":"insufficient_positions","measured":2}"#.to_owned(),
            r#"{"code":"excluded_supports","roles":["SW2"]}"#.to_owned(),
            r#"{"code":"support_ranges","ranges":[{"role":"L","value":{"lo_hz":50.0,"hi_hz":150.0}}]}"#.to_owned(),
            format!(r#"{{"code":"guard_reduced","role":"L","gain_db":-6.0,"cause":{cause}}}"#),
            format!(r#"{{"code":"guard_disabled","role":"R","cause":{cause}}}"#),
            r#"{"code":"support_limit","max_supports":2}"#.to_owned(),
            r#"{"code":"fir_budget","support_count":2,"taps_total":16000}"#.to_owned(),
        ];
        for json in mimo {
            round_trip::<MimoNote>(&json);
        }
    }

    #[test]
    fn note_json_rejects_non_finite_numbers() {
        let note = Note::PolarityReversed {
            role: "L".into(),
            confidence: f64::NAN,
            compared: 2,
            opposite: 2,
        };
        assert!(serde_json::to_string(&note).is_err());
    }
}
