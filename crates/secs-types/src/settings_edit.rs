//! Wire-compatible edits from the calibration screen. Rules live in secs-service.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    calibration::{Mode, RcBand, RcHiBasis, SampleRatePolicy},
    chain::BandType,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum SubModeChoice {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "fixed")]
    Fixed,
    #[serde(rename = "bundle:A")]
    BundleA,
    #[serde(rename = "bundle:B")]
    BundleB,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum SubLayout {
    Values,
    Dba,
    Multi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum SubBand {
    Hp,
    Lp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum BandTypeChoice {
    Band(BandType),
    Off(BandOff),
}

/// The "off" choice of a sub's HP or LP filter type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum BandOff {
    Off,
}

/// Partial<Omit<SubEntry, "mode" | "bundle">>; absent fields are unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct SubPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gain_db: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delay_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub polarity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub allpass: Option<Vec<(f64, f64)>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mimo_support: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hp_hz: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lp_hz: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub peq: Option<Vec<(f64, f64, f64)>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hp_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lp_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dba_row: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum SettingEdit {
    Mode {
        mode: Mode,
    },
    SampleRatePolicy {
        policy: SampleRatePolicy,
    },
    UltraMs {
        ms: f64,
    },
    LowProtect {
        on: bool,
    },
    LfAlign {
        on: bool,
    },
    ModeDecay {
        on: bool,
    },
    HfPhase {
        on: bool,
    },
    /// The common input high-pass in Hz, `None` to turn it off (ADR 0031).
    InputHp {
        hz: Option<f64>,
    },
    RcLo {
        hz: f64,
    },
    RcHi {
        hz: Option<f64>,
    },
    RcHiBasis {
        basis: RcHiBasis,
    },
    RcHiTransition {
        factor: u8,
    },
    /// Replaces the whole list of extra correction bands (ADR 0024); an
    /// empty list removes them. Rejected, not clamped, when a band breaks
    /// `CALIBRATION_LIMITS.rc_band` or the list is longer than `rc_bands_max`.
    RcBands {
        bands: Vec<RcBand>,
    },
    RoomVolume {
        m3: f64,
    },
    RoomDims {
        dims: [f64; 3],
    },
    BoostMax {
        db: f64,
    },
    Xo {
        group: String,
        hz: f64,
    },
    XoRecInclude {
        on: bool,
    },
    XoPeq {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        on: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        lo: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        hi: Option<f64>,
    },
    Infra {
        role: Option<String>,
        hz: f64,
        #[serde(rename = "type")]
        typ: BandType,
        subs: Vec<String>,
    },
    InfraBoost {
        db: f64,
    },
    SubMode {
        role: String,
        mode: SubModeChoice,
    },
    SubLayout {
        bundle: String,
        layout: SubLayout,
    },
    RearTrim {
        bundle: String,
        ms: f64,
    },
    Sub {
        role: String,
        patch: SubPatch,
    },
    SubBandType {
        role: String,
        band: SubBand,
        #[serde(rename = "type")]
        typ: BandTypeChoice,
    },
    MainDelay {
        ms: f64,
    },
    MimoOn {
        on: bool,
    },
    Fsiso {
        hz: f64,
    },
    MimoLow {
        hz: Option<f64>,
    },
    MimoMax {
        count: f64,
    },
    MimoFir {
        on: bool,
    },
    /// Experimental closed-form derivatives (D43, D44); an absent part keeps its value.
    AnalyticDerivatives {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        peq: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        mimo_iir: Option<bool>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CalibrationSettingsEditParams {
    pub edit: SettingEdit,
}
