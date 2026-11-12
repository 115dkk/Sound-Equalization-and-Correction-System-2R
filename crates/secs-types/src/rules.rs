//! Service rule values the screen shows and the mock service reuses (ADR 0014).
//!
//! The service judges with these named constants instead of literals. This
//! crate's tests also write their values into `ipc/bindings/rules.ts` next to
//! the generated types, so the screen and the mock import them rather than
//! copying numbers by hand; CI's bindings diff (ADR 0003) covers that file.

use serde::Serialize;
use ts_rs::TS;

use crate::role::{HEIGHT_GROUP, TOP_GROUP};

/// Inclusive `(low, high)` range of each value `calibration_settings_edit`
/// clamps (upstream calibration.py and step_calibration.py constants).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct CalibrationLimits {
    /// RC low edge (Hz) an edit accepts. SECS 2R rejects below 20 Hz (ADR
    /// 0024); a project file is read with `RC_LO_READ` instead.
    pub rc_lo: (f64, f64),
    /// RC high edge (Hz) when it is not automatic.
    pub rc_hi: (f64, f64),
    /// Either edge of an extra correction band (Hz, ADR 0024).
    pub rc_band: (f64, f64),
    /// Number of extra correction bands (ADR 0024).
    pub rc_bands_max: (f64, f64),
    /// Room volume (m³); a value at or below the low end clears it.
    pub volume: (f64, f64),
    /// One room dimension (m) as entered.
    pub dim: (f64, f64),
    /// One room dimension (m) that yields a room volume.
    pub dim_valid: (f64, f64),
    /// Maximum boost (dB).
    pub boost: (f64, f64),
    /// Common input high-pass (Hz) when it is on (ADR 0031).
    pub input_hp: (f64, f64),
    /// Latency budget of the ultra mode (ms).
    pub ultra_ms: (f64, f64),
    /// Crossover of one group (Hz).
    pub xo: (f64, f64),
    /// XO PEQ band edges (Hz).
    pub xo_peq: (f64, f64),
    /// Infra crossover (Hz).
    pub infra_hz: (f64, f64),
    /// Maximum infra boost (dB).
    pub infra_boost: (f64, f64),
    /// Extra delay of the mains against the subs (ms).
    pub main_delay: (f64, f64),
    /// Rear-row trim of a sub group (ms).
    pub rear_trim: (f64, f64),
    /// MIMO support upper limit (Hz).
    pub fsiso: (f64, f64),
    /// MIMO low limit (Hz) when it is not automatic.
    pub mimo_low: (f64, f64),
    /// Maximum number of MIMO supports (0 = no cap).
    pub mimo_max: (f64, f64),
    /// Protected MIMO onset interval (ms, ADR 0037).
    pub mimo_early_ms: (f64, f64),
    /// Input PEQ band count (None uses the engine default).
    pub peq_bands: (f64, f64),
}

/// Upstream's clamp of a stored RC low edge (calibration.py), kept for reading
/// project files so a stored 10–19 Hz still computes as before (ADR 0024, D48).
pub const RC_LO_READ: (f64, f64) = (10.0, 200.0);

pub const CALIBRATION_LIMITS: CalibrationLimits = CalibrationLimits {
    rc_lo: (20.0, 200.0),
    rc_hi: (100.0, 20000.0),
    rc_band: (20.0, 20000.0),
    rc_bands_max: (0.0, 4.0),
    volume: (0.0, 100000.0),
    dim: (0.0, 100.0),
    dim_valid: (0.5, 100.0),
    boost: (0.0, 10.0),
    input_hp: (10.0, 50.0),
    ultra_ms: (30.0, 200.0),
    xo: (40.0, 200.0),
    xo_peq: (0.0, 1000.0),
    infra_hz: (10.0, 300.0),
    infra_boost: (0.0, 24.0),
    main_delay: (0.0, 100.0),
    rear_trim: (-20.0, 20.0),
    fsiso: (50.0, 150.0),
    mimo_low: (15.0, 150.0),
    mimo_max: (0.0, 12.0),
    mimo_early_ms: (5.0, 60.0),
    peq_bands: (0.0, 64.0),
};

/// Sweep lengths a measurement plan accepts, shortest first.
pub const SWEEP_LENGTHS: [&str; 7] = ["64k", "128k", "256k", "512k", "1M", "2M", "4M"];

/// Start delays before each measured position, as upstream stores them in
/// `extra['start_delay']` (step 4 of the wizard).
pub const START_DELAYS: [&str; 4] = ["끄기", "5 초", "10 초", "15 초"];

/// How many recent projects the app settings keep.
pub const RECENT_PROJECTS_MAX: usize = 10;

/// Configuration groups in the order the service lists them: the upstream
/// (Korean) group name that keys the XO map, then the presentation id.
pub const CONFIG_GROUPS: [(&str, &str); 9] = [
    ("프론트", "front"),
    ("센터", "center"),
    ("서브", "sub"),
    ("서라운드", "surround"),
    ("백", "back"),
    ("와이드", "wide"),
    (TOP_GROUP, "top"),
    (HEIGHT_GROUP, "height"),
    ("기타", "other"),
];

/// Measurement arrangements by position count.
pub const ARRANGEMENT_COUNTS: [usize; 4] = [1, 9, 13, 17];

/// Finished jobs the service keeps in its job list beside every active one
/// (ADR 0009).
pub const JOB_HISTORY_MAX: usize = 32;

/// Calibration fields read directly by `engine_key`; manual-sub fields are in
/// [`MANUAL_KEY_FIELDS`] because `engine_key` includes `manual_key`.
pub const ENGINE_KEY_FIELDS: &[&str] = &[
    "mode",
    "rc_lo",
    "rc_hi",
    "room_volume_m3",
    "xo",
    "ultra_ms",
    "infra_boost_max_db",
    "xo_peq_on",
    "xo_peq_lo",
    "xo_peq_hi",
    "boost_max_db",
    "rc_hi_basis",
    "rc_hi_transition",
    "rc_bands",
    "sample_rate_policy",
    "low_protect",
    "lf_align",
    "mode_decay",
    "hf_phase",
    "dip_limit",
    "input_hp_hz",
    "peq_analytic_derivatives",
    "main_delay_off",
];

/// Calibration fields read by `manual_key`.
pub const MANUAL_KEY_FIELDS: &[&str] = &["subs_manual", "bundle_layouts", "sub_main_delay_ms"];

/// Calibration fields read by `mimo_key`.
pub const MIMO_KEY_FIELDS: &[&str] = &[
    "fsiso",
    "mimo_low",
    "subs_manual",
    "mimo_max_supports",
    "mimo_fir",
    "mimo_iir_analytic_derivatives",
    "mimo_seat_guard",
    "mimo_early_ms",
];

/// Exclusive foreground-job groups and the job-kind wire names in each group.
pub const JOB_GROUPS: &[(&str, &[&str])] = &[
    (
        "device",
        &["measure", "sample_load", "mdat_import", "level_monitor"],
    ),
    ("compute", &["calibrate", "recommend_xo", "mimo"]),
    ("export", &["export", "apply"]),
    ("status_watch", &["status_watch"]),
];

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ScalarEditRule {
    pub kind: &'static str,
    pub field: &'static str,
    pub limit: &'static str,
    pub policy: &'static str,
}

/// Scalar edit rules the mock can apply without copying the service match.
pub use crate::calibration::{KNOWN_KEYS, SCALAR_EDITS};

/// Folder tag used by each `ExportKind` wire name.
pub const EXPORT_FOLDER_TAGS: &[(&str, &str)] = &[
    ("camilla_dsp", "camilla"),
    ("before_after_mdat", "mdat"),
    ("staged_mdat", "mdat"),
    ("filter_text", "text"),
    ("equalizer_apo", "eqapo"),
    ("convolution", "convolution"),
];

pub const MIMO_MIN_POSITIONS: usize = 3;
pub const DESIGN_RATE: u32 = 48_000;
pub const OUTPUT_RATES: [u32; 6] = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000];

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubLimits {
    pub gain: (f64, f64),
    pub delay: (f64, f64),
    pub ap_hz: (f64, f64),
    pub ap_q: (f64, f64),
    pub filter_hz: (f64, f64),
    pub peq_gain: (f64, f64),
}

pub const SUB_LIMITS: SubLimits = SubLimits {
    gain: (-60.0, 20.0),
    delay: (0.0, 500.0),
    ap_hz: (10.0, 500.0),
    ap_q: (0.1, 20.0),
    filter_hz: (10.0, 300.0),
    peq_gain: (-30.0, 12.0),
};
pub const INFRA_DEFAULT_HZ: f64 = 25.0;
pub const ALLPASS_Q_DEFAULT: f64 = 0.71;
pub const TRANSITION_FACTORS: [u8; 3] = [1, 2, 4];
pub const AGREEMENT_MIN_POSITIONS: usize = 3;
pub const REPETITIONS: [u8; 4] = [1, 2, 4, 8];
pub const NEW_PROJECT_PRESETS: [&str; 10] = [
    "2.0", "2.1", "2.2", "4.0", "4.1", "4.2", "5.1", "7.1", "7.1.4", "9.1.6",
];
pub const INPUT_HP_START: f64 = 25.0;

#[cfg(test)]
mod typescript {
    //! `rules.ts`: the values above as TypeScript, deterministic text.
    use serde::Serialize;
    use serde_json::Value;

    use super::*;

    const HEADER: &str = "// This file was generated by `cargo test -p secs-types` from crates/secs-types/src/rules.rs. Do not edit this file manually.\n";

    fn literal(value: &Value) -> String {
        match value {
            Value::Null => "null".into(),
            Value::Bool(value) => value.to_string(),
            // Display prints the shortest round-trip form without a trailing
            // `.0`, as JavaScript does for these magnitudes.
            Value::Number(number) => match (number.as_i64(), number.as_f64()) {
                (Some(int), _) => int.to_string(),
                (None, Some(float)) => float.to_string(),
                (None, None) => number.to_string(),
            },
            Value::String(_) => value.to_string(),
            Value::Array(items) => {
                format!(
                    "[{}]",
                    items.iter().map(literal).collect::<Vec<_>>().join(", ")
                )
            }
            Value::Object(fields) => format!(
                "{{\n{}}}",
                fields
                    .iter()
                    .map(|(key, value)| format!("  {key}: {},\n", literal(value)))
                    .collect::<String>()
            ),
        }
    }

    /// One `export const`; arrays and objects are `as const`.
    pub(super) fn constant(name: &str, ty: Option<&str>, value: &impl Serialize) -> String {
        let value = serde_json::to_value(value).expect("rule constants serialize");
        let annotation = ty.map(|ty| format!(": {ty}")).unwrap_or_default();
        let suffix = if value.is_array() || value.is_object() {
            " as const"
        } else {
            ""
        };
        format!(
            "export const {name}{annotation} = {}{suffix};\n",
            literal(&value)
        )
    }

    pub(super) fn rules_ts() -> String {
        [
            HEADER.to_owned(),
            "import type { CalibrationLimits } from \"./CalibrationLimits\";\n\n".to_owned(),
            constant(
                "CALIBRATION_LIMITS",
                Some("CalibrationLimits"),
                &CALIBRATION_LIMITS,
            ),
            constant("SWEEP_LENGTHS", None, &SWEEP_LENGTHS),
            constant("START_DELAYS", None, &START_DELAYS),
            constant("RECENT_PROJECTS_MAX", None, &RECENT_PROJECTS_MAX),
            constant("CONFIG_GROUPS", None, &CONFIG_GROUPS),
            constant("ARRANGEMENT_COUNTS", None, &ARRANGEMENT_COUNTS),
            constant("JOB_HISTORY_MAX", None, &JOB_HISTORY_MAX),
            constant("ENGINE_KEY_FIELDS", None, &ENGINE_KEY_FIELDS),
            constant("MANUAL_KEY_FIELDS", None, &MANUAL_KEY_FIELDS),
            constant("MIMO_KEY_FIELDS", None, &MIMO_KEY_FIELDS),
            constant("JOB_GROUPS", None, &JOB_GROUPS),
            constant("SCALAR_EDITS", None, &SCALAR_EDITS),
            constant("KNOWN_KEYS", None, &KNOWN_KEYS),
            constant("EXPORT_FOLDER_TAGS", None, &EXPORT_FOLDER_TAGS),
            constant("MIMO_MIN_POSITIONS", None, &MIMO_MIN_POSITIONS),
            constant("DESIGN_RATE", None, &DESIGN_RATE),
            constant("OUTPUT_RATES", None, &OUTPUT_RATES),
            constant("SUB_LIMITS", None, &SUB_LIMITS),
            constant("INFRA_DEFAULT_HZ", None, &INFRA_DEFAULT_HZ),
            constant("ALLPASS_Q_DEFAULT", None, &ALLPASS_Q_DEFAULT),
            constant("TRANSITION_FACTORS", None, &TRANSITION_FACTORS),
            constant("AGREEMENT_MIN_POSITIONS", None, &AGREEMENT_MIN_POSITIONS),
            constant("REPETITIONS", None, &REPETITIONS),
            constant("NEW_PROJECT_PRESETS", None, &NEW_PROJECT_PRESETS),
            constant("INPUT_HP_START", None, &INPUT_HP_START),
        ]
        .concat()
    }
}

#[cfg(test)]
mod tests {
    use super::typescript::{constant, rules_ts};
    use super::*;

    #[test]
    fn renders_constants_as_typescript() {
        assert_eq!(
            constant("RECENT_PROJECTS_MAX", None, &RECENT_PROJECTS_MAX),
            "export const RECENT_PROJECTS_MAX = 10;\n"
        );
        assert_eq!(
            constant("EDGES", None, &[(0.5, 100.0), (-20.0, 0.71)]),
            "export const EDGES = [[0.5, 100], [-20, 0.71]] as const;\n"
        );
        assert_eq!(
            constant("GROUPS", None, &[("탑", "top")]),
            "export const GROUPS = [[\"탑\", \"top\"]] as const;\n"
        );
        let limits = constant("LIMITS", Some("CalibrationLimits"), &CALIBRATION_LIMITS);
        assert!(
            limits.starts_with(
                "export const LIMITS: CalibrationLimits = {\n  rc_lo: [20, 200],\n  rc_hi: [100, 20000],\n  rc_band: [20, 20000],\n  rc_bands_max: [0, 4],\n"
            ),
            "{limits}"
        );
        assert!(
            limits.ends_with("  mimo_max: [0, 12],\n  mimo_early_ms: [5, 60],\n  peq_bands: [0, 64],\n} as const;\n"),
            "{limits}"
        );
    }

    /// Writes `rules.ts` where ts-rs writes the bindings (`TS_RS_EXPORT_DIR`,
    /// `./bindings` by default), so CI's bindings diff compares it too.
    #[test]
    fn exports_architecture_tables_to_typescript() {
        let text = rules_ts();
        assert!(text.contains("peq_bands: [0, 64]"));
        assert!(text.contains("filterHz: [10, 300]"));
        for name in [
            "ENGINE_KEY_FIELDS",
            "MANUAL_KEY_FIELDS",
            "MIMO_KEY_FIELDS",
            "JOB_GROUPS",
            "SCALAR_EDITS",
            "KNOWN_KEYS",
            "EXPORT_FOLDER_TAGS",
            "MIMO_MIN_POSITIONS",
            "DESIGN_RATE",
            "OUTPUT_RATES",
            "SUB_LIMITS",
            "INFRA_DEFAULT_HZ",
            "ALLPASS_Q_DEFAULT",
            "TRANSITION_FACTORS",
            "AGREEMENT_MIN_POSITIONS",
            "REPETITIONS",
            "NEW_PROJECT_PRESETS",
            "INPUT_HP_START",
        ] {
            assert!(text.contains(&format!("export const {name}")), "{name}");
        }
    }

    #[test]
    fn export_rules_ts() {
        let dir = ts_rs::Config::from_env().out_dir().to_path_buf();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("rules.ts"), rules_ts()).unwrap();
    }
}
