//! User-visible calibration settings and their mode-derived engine values.
//!
//! This is the typed form of `calibration.py::CalibrationSettings`. Ordered
//! pairs are used for Python dictionaries so insertion order survives a
//! `.secs` round trip; serde still exposes them as JSON objects.

use std::{collections::BTreeMap, fmt::Write as _};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::chain::BandType;
use crate::rules::CALIBRATION_LIMITS;
use crate::settings_edit::{BandTypeChoice, SubBand, SubLayout, SubModeChoice, SubPatch};

/// The ultra mode's latency budget range (ms).
const ULTRA_MS: (f64, f64) = CALIBRATION_LIMITS.ultra_ms;

fn default_true() -> bool {
    true
}

fn default_mimo_early_ms() -> f64 {
    60.0
}

fn is_true(value: &bool) -> bool {
    *value
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Mode {
    Zero,
    Mixed,
    Linear,
    Ultra,
}

/// Evidence used to choose an automatic room-correction ceiling (ADR 0021).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RcHiBasis {
    #[default]
    Strict,
    Lenient,
    Agreement,
}

impl RcHiBasis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Lenient => "lenient",
            Self::Agreement => "agreement",
        }
    }
}

fn default_rc_hi_transition() -> u8 {
    1
}

/// One extra room-correction band beside `rc_lo..rc_hi`, both edges set by
/// hand (ADR 0024). `20 <= lo_hz < hi_hz <= 20000`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RcBand {
    pub lo_hz: f64,
    pub hi_hz: f64,
}

/// Whether sidecar calibration arrays satisfy the embedded curve rules.
pub fn calibration_curve_is_valid(frequencies: &[f64], gains: &[f64]) -> bool {
    frequencies.len() >= 2
        && frequencies.len() == gains.len()
        && frequencies.iter().zip(gains).all(|(&frequency, &gain)| {
            frequency.is_finite() && frequency > 0.0 && gain.is_finite() && gain.abs() <= 200.0
        })
        && frequencies.windows(2).all(|pair| pair[0] < pair[1])
}

/// The extra bands a calculation may use: finite, inside
/// `CALIBRATION_LIMITS.rc_band` with `lo < hi`, sorted by `lo_hz`, at most
/// `rc_bands_max` (ADR 0024). Settings that did not pass the service's edit
/// checks (a request carrying its own settings) go through this.
pub fn valid_rc_bands(bands: &[RcBand]) -> Vec<RcBand> {
    let limits = crate::rules::CALIBRATION_LIMITS;
    let edge = limits.rc_band.0..=limits.rc_band.1;
    let mut kept: Vec<_> = bands
        .iter()
        .copied()
        .filter(|band| {
            band.lo_hz.is_finite()
                && band.hi_hz.is_finite()
                && edge.contains(&band.lo_hz)
                && edge.contains(&band.hi_hz)
                && band.lo_hz < band.hi_hz
        })
        .collect();
    kept.sort_by(|a, b| {
        a.lo_hz
            .total_cmp(&b.lo_hz)
            .then(a.hi_hz.total_cmp(&b.hi_hz))
    });
    kept.truncate(limits.rc_bands_max.1 as usize);
    kept
}

/// How non-48 kHz measurement responses are prepared for calculation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SampleRatePolicy {
    /// Preserve the existing validation error for mismatched measurements.
    #[default]
    Reject,
    /// Resample calculation copies to 48 kHz, leaving stored measurements intact.
    Resample,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubEntry {
    pub mode: String,
    pub bundle: String,
    pub gain_db: f64,
    pub delay_ms: f64,
    pub polarity: i8,
    pub allpass: Vec<(f64, f64)>,
    pub mimo_support: bool,
    pub hp_hz: f64,
    pub lp_hz: f64,
    pub peq: Vec<(f64, f64, f64)>,
    pub hp_type: String,
    pub lp_type: String,
    pub dba_row: String,
}

impl Default for SubEntry {
    fn default() -> Self {
        Self {
            mode: "auto".into(),
            bundle: "A".into(),
            gain_db: 0.0,
            delay_ms: 0.0,
            polarity: 1,
            allpass: Vec::new(),
            mimo_support: true,
            hp_hz: 0.0,
            lp_hz: 0.0,
            peq: Vec::new(),
            hp_type: "LR24".into(),
            lp_type: "LR24".into(),
            dba_row: "front".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BundleLayoutEntry {
    pub kind: String,
    pub rear_trim_ms: f64,
}

impl Default for BundleLayoutEntry {
    fn default() -> Self {
        Self {
            kind: "values".into(),
            rear_trim_ms: 0.0,
        }
    }
}

// One declaration retains both wire orders: settings fields and edit variants.
macro_rules! calibration_settings {
    (
        fields { $( $(#[$attr:meta])* $field:ident: $ty:ty = $default:expr;
            $check:ident($($args:tt)*); $key:tt; project($read:ident, $write:ident); )* }
        edits { $( $(#[$edit_attr:meta])* $variant:ident {
            $( $(#[$payload_attr:meta])* $payload:ident: $payload_ty:ty, )*
        } $( [ $kind:literal, $rule_field:ident ] )?
            => |$settings:ident| $apply:block; )* }
        project_order { $( $project_key:literal ),* $(,)? }
    ) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
        #[ts(export)]
        pub struct CalibrationSettings { $( $(#[$attr])* pub $field: $ty, )* }
        impl Default for CalibrationSettings {
            fn default() -> Self { Self { $( $field: $default, )* } }
        }
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
        #[serde(tag = "kind", rename_all = "snake_case")]
        #[ts(export)]
        pub enum SettingEdit {
            $( $(#[$edit_attr])* $variant { $( $(#[$payload_attr])* $payload: $payload_ty, )* }, )*
        }
        impl SettingEdit {
            /// Constructs a declared single-payload edit without applying its policy.
            /// Unknown or structured kinds and mismatched payload types return `None`.
            pub fn scalar(kind: &str, value: ScalarValue) -> Option<Self> {
                $( if let Some(edit) = scalar_edit_constructor!(kind, value, [$($kind)?], $variant { $($payload: $payload_ty),* }) {
                    return Some(edit);
                } )*
                None
            }
        }
        #[allow(non_upper_case_globals)]
        mod scalar_rules {
            $( pub const $field: (&str, &str) = scalar_policy!($check($($args)*)); )*
            pub const rear_trim_ms: (&str, &str) = ("rear_trim", "clamp");
        }
        pub const SCALAR_EDITS: &[crate::rules::ScalarEditRule] = &[
            $( $( crate::rules::ScalarEditRule { kind: $kind, field: stringify!($rule_field), limit: scalar_rules::$rule_field.0, policy: scalar_rules::$rule_field.1 }, )? )*
        ];
        /// Project-file order, including the hand-written structured entries.
        pub const KNOWN_KEYS: &[&str] = &[ $( $project_key, )* ];
        impl CalibrationSettings {
            $( fn $field(value: $ty) -> Result<$ty, SettingsRefusal> {
                check_scalar!(value, stringify!($field), $check($($args)*))
            } )*
            /// Applies scalar edits; structured edits remain with their adapter.
            pub fn apply_scalar(&mut self, edit: &SettingEdit) -> Result<Applied, SettingsRefusal> {
                match edit {
                    $( SettingEdit::$variant { $($payload,)* } => {
                        $( let _ = $payload; )*
                        let $settings = self;
                        $apply
                    }, )*
                }
            }
            /// Applies edit policies to a complete request. Refusals are reported
            /// in declaration order, before the hand-written cross-field checks.
            pub fn checked(mut self) -> Result<Self, SettingsRefusal> {
                $( let _ = scalar_rules::$field; self.$field = Self::$field(self.$field)?; )*
                self.check_structured()?;
                Ok(self)
            }
            /// Loads one declared scalar with the legacy project-file read policy.
            /// `empty` distinguishes an empty calibration from a pre-FIR project.
            pub fn load_project_scalar(&mut self, key: &str, input: Option<ProjectScalarInput<'_>>, empty: bool) {
                match key {
                    $( stringify!($field) => { read_project_scalar!(self, $field, input, empty, $default, $check($($args)*), $read); }, )*
                    _ => {},
                }
            }
            /// Returns a scalar in project-file representation, omitting legacy defaults.
            /// Structured keys are handled by the project adapter.
            pub fn project_scalar(&self, key: &str) -> Option<ScalarValue> {
                match key {
                    $( stringify!($field) => write_project_scalar!(&self.$field, $default, $write), )*
                    _ => None,
                }
            }
            fn scalar_engine_key(&self, field: &str) -> String {
                match field { $( stringify!($field) => scalar_key!(&self.$field, $field, $key), )* _ => unreachable!("declared field") }
            }
        }
    };
}
macro_rules! scalar_edit_constructor {
    ($kind:ident, $value:ident, [$wire:literal], $variant:ident { $payload:ident: $ty:ty }) => {
        if $kind == $wire {
            <$ty as ScalarPayload>::from_scalar(&$value).map(|$payload| Self::$variant { $payload })
        } else {
            None
        }
    };
    ($kind:ident, $value:ident, [$($wire:literal)?], $variant:ident { $($payload:ident: $ty:ty),* }) => {
        None::<Self>
    };
}

macro_rules! scalar_key {
    ($value:expr, $field:ident, flag) => {
        if *$value {
            concat!("|", stringify!($field)).into()
        } else {
            String::new()
        }
    };
    ($value:expr, $field:ident, number) => {
        number(*$value)
    };
    ($value:expr, $field:ident, none) => {
        String::new()
    };
    ($value:expr, $field:ident, $prefix:literal) => {
        $value.map_or_else(String::new, |v| format!("|{}{}", $prefix, number(v)))
    };
}
macro_rules! scalar_policy {
    (identity()) => {
        ("-", "-")
    };
    (volume()) => {
        ("volume", "clamp")
    };
    (transition()) => {
        ("-", "refuse")
    };
    (xo()) => {
        ("xo", "clamp")
    };
    (refuse($limit:ident, $($rest:tt)*)) => {
        (stringify!($limit), "refuse")
    };
    (optional_refuse($limit:ident, $($rest:tt)*)) => {
        (stringify!($limit), "refuse")
    };
    ($policy:ident($limit:ident)) => {
        (stringify!($limit), "clamp")
    };
}
macro_rules! check_scalar {
    ($v:ident, $f:expr, xo()) => {
        Ok($v)
    };
    ($v:ident, $f:expr, identity()) => {
        Ok($v)
    };
    ($v:ident, $f:expr, clamp($limit:ident)) => {
        clamp_value($v, CALIBRATION_LIMITS.$limit, $f)
    };
    ($v:ident, $f:expr, optional_clamp($limit:ident)) => {
        $v.map(|v| clamp_value(v, CALIBRATION_LIMITS.$limit, $f))
            .transpose()
    };
    ($v:ident, $f:expr, integer($limit:ident)) => {
        Ok($v.clamp(
            CALIBRATION_LIMITS.$limit.0 as i64,
            CALIBRATION_LIMITS.$limit.1 as i64,
        ))
    };
    ($v:ident, $f:expr, refuse($limit:ident, $field:literal, $reason:literal)) => {
        refuse_value($v, CALIBRATION_LIMITS.$limit, $field, $reason)
    };
    ($v:ident, $f:expr, optional_refuse($limit:ident, $field:literal, $reason:literal)) => {
        $v.map(|v| refuse_value(v, CALIBRATION_LIMITS.$limit, $field, $reason))
            .transpose()
    };
    ($v:ident, $f:expr, volume()) => {
        Ok($v
            .filter(|v| v.is_finite() && *v > CALIBRATION_LIMITS.volume.0)
            .map(|v| v.min(CALIBRATION_LIMITS.volume.1)))
    };
    ($v:ident, $f:expr, transition()) => {
        if crate::rules::TRANSITION_FACTORS.contains(&$v) {
            Ok($v)
        } else {
            Err(SettingsRefusal::new($f, "rc_hi_transition", (1.0, 4.0)))
        }
    };
}

/// Typed scalar values shared by edit constructors and project-file adapters.
#[derive(Debug, Clone, PartialEq)]
pub enum ScalarValue {
    Bool(bool),
    Number(f64),
    OptionalNumber(Option<f64>),
    Integer(i64),
    Text(String),
}

trait ScalarPayload: Sized {
    fn from_scalar(value: &ScalarValue) -> Option<Self>;
}

macro_rules! scalar_payload {
    ($ty:ty, $variant:ident) => {
        impl ScalarPayload for $ty {
            fn from_scalar(value: &ScalarValue) -> Option<Self> {
                match value {
                    ScalarValue::$variant(value) => Some(*value),
                    _ => None,
                }
            }
        }
    };
}
scalar_payload!(bool, Bool);
scalar_payload!(f64, Number);
scalar_payload!(Option<f64>, OptionalNumber);

impl ScalarPayload for u8 {
    fn from_scalar(value: &ScalarValue) -> Option<Self> {
        match value {
            ScalarValue::Number(value) if (0.0..=255.0).contains(value) && value.fract() == 0.0 => {
                Some(*value as u8)
            }
            ScalarValue::Integer(value) => Self::try_from(*value).ok(),
            _ => None,
        }
    }
}

macro_rules! scalar_enum_payload {
    ($($ty:ty),* $(,)?) => {
        $( impl ScalarPayload for $ty {
            fn from_scalar(value: &ScalarValue) -> Option<Self> {
                let ScalarValue::Text(value) = value else { return None; };
                Self::deserialize(serde::de::value::StrDeserializer::<serde::de::value::Error>::new(value)).ok()
            }
        } )*
    };
}
scalar_enum_payload!(Mode, SampleRatePolicy, RcHiBasis);

/// The legacy JSON reader's scalar conversions. Strings may parse as numbers;
/// integer excludes numeric strings, preserving the transition-factor rule.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProjectScalarInput<'a> {
    pub number: Option<f64>,
    pub integer: Option<i64>,
    pub text: Option<&'a str>,
    pub truthy: bool,
}

fn project_number(input: Option<ProjectScalarInput<'_>>, bounds: (f64, f64)) -> Option<f64> {
    input
        .and_then(|v| v.number)
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(bounds.0, bounds.1))
}

fn xo_maps_equal(left: &[(String, f64)], right: &[(String, f64)]) -> bool {
    let mut left = left.to_vec();
    let mut right = right.to_vec();
    left.sort_by(|a, b| a.0.cmp(&b.0));
    right.sort_by(|a, b| a.0.cmp(&b.0));
    left == right
}

macro_rules! project_bounds {
    (refuse(rc_lo, $($rest:tt)*)) => {
        crate::rules::RC_LO_READ
    };
    (volume()) => {
        (5.0, CALIBRATION_LIMITS.volume.1)
    };
    ($policy:ident($limit:ident $(, $rest:tt)*)) => {
        CALIBRATION_LIMITS.$limit
    };
}
macro_rules! read_project_scalar {
    ($s:ident, $field:ident, $input:ident, $empty:ident, $default:expr, $check:ident($($args:tt)*), manual) => {};
    ($s:ident, $field:ident, $input:ident, $empty:ident, $default:expr, $check:ident($($args:tt)*), number) => {
        $s.$field = project_number($input, project_bounds!($check($($args)*))).unwrap_or($default)
    };
    ($s:ident, $field:ident, $input:ident, $empty:ident, $default:expr, $check:ident($($args:tt)*), optional_number) => {
        $s.$field = project_number($input, project_bounds!($check($($args)*)))
    };
    ($s:ident, $field:ident, $input:ident, $empty:ident, $default:expr, $check:ident($($args:tt)*), integer) => {
        $s.$field = project_number($input, project_bounds!($check($($args)*))).map_or($default, |v| v.round_ties_even() as i64)
    };
    ($s:ident, $field:ident, $input:ident, $empty:ident, $default:expr, $check:ident($($args:tt)*), boolean) => {
        $s.$field = $input.map_or($default, |v| v.truthy)
    };
    ($s:ident, $field:ident, $input:ident, $empty:ident, $default:expr, $check:ident($($args:tt)*), legacy_fir) => {
        $s.$field = $input.map_or($empty, |v| v.truthy)
    };
    ($s:ident, $field:ident, $input:ident, $empty:ident, $default:expr, $check:ident($($args:tt)*), include) => {
        $s.$field = $input.map_or_else(|| $s.xo.is_empty() || xo_maps_equal(&$s.xo, &$s.xo_rec), |v| v.truthy)
    };
    ($s:ident, $field:ident, $input:ident, $empty:ident, $default:expr, $check:ident($($args:tt)*), text) => {
        $s.$field = $input.and_then(|v| v.text).unwrap_or_default().to_owned()
    };
    ($s:ident, $field:ident, $input:ident, $empty:ident, $default:expr, $check:ident($($args:tt)*), transition) => {
        $s.$field = $input.and_then(|v| v.integer).filter(|v| matches!(v, 1 | 2 | 4)).map_or($default, |v| v as u8)
    };
    ($s:ident, $field:ident, $input:ident, $empty:ident, $default:expr, $check:ident($($args:tt)*), mode) => {
        $s.$field = match $input.and_then(|v| v.text) { Some("zero") => Mode::Zero, Some("linear") => Mode::Linear, Some("ultra") => Mode::Ultra, _ => $default }
    };
    ($s:ident, $field:ident, $input:ident, $empty:ident, $default:expr, $check:ident($($args:tt)*), policy) => {
        $s.$field = match $input.and_then(|v| v.text) { Some("resample") => SampleRatePolicy::Resample, _ => $default }
    };
    ($s:ident, $field:ident, $input:ident, $empty:ident, $default:expr, $check:ident($($args:tt)*), basis) => {
        $s.$field = match $input.and_then(|v| v.text) { Some("lenient") => RcHiBasis::Lenient, Some("agreement") => RcHiBasis::Agreement, _ => $default }
    };
}
macro_rules! write_project_scalar {
    ($v:expr, $default:expr, manual) => {
        None
    };
    ($v:expr, $default:expr, number) => {
        Some(ScalarValue::Number(*$v))
    };
    ($v:expr, $default:expr, non_default_number) => {
        (*$v != $default).then_some(ScalarValue::Number(*$v))
    };
    ($v:expr, $default:expr, optional_number) => {
        Some(ScalarValue::OptionalNumber(*$v))
    };
    ($v:expr, $default:expr, present_number) => {
        $v.map(ScalarValue::Number)
    };
    ($v:expr, $default:expr, integer) => {
        Some(ScalarValue::Integer(*$v))
    };
    ($v:expr, $default:expr, non_default_integer) => {
        (*$v != $default).then_some(ScalarValue::Integer(i64::from(*$v)))
    };
    ($v:expr, $default:expr, boolean) => {
        Some(ScalarValue::Bool(*$v))
    };
    ($v:expr, $default:expr, true_only) => {
        (*$v).then_some(ScalarValue::Bool(true))
    };
    ($v:expr, $default:expr, false_only) => {
        (!*$v).then_some(ScalarValue::Bool(false))
    };
    ($v:expr, $default:expr, nonempty_text) => {
        (!$v.is_empty()).then(|| ScalarValue::Text($v.clone()))
    };
    ($v:expr, $default:expr, mode) => {
        Some(ScalarValue::Text(mode_tag(*$v).into()))
    };
    ($v:expr, $default:expr, basis) => {
        (*$v != $default).then(|| ScalarValue::Text($v.as_str().into()))
    };
    ($v:expr, $default:expr, policy) => {
        (*$v != $default).then(|| ScalarValue::Text("resample".into()))
    };
}

calibration_settings! {
    fields {

    mode: Mode = Mode::Mixed; identity(); none; project(mode, mode);
    #[serde(default)]
    sample_rate_policy: SampleRatePolicy = SampleRatePolicy::Reject; identity(); none; project(policy, policy);
    #[serde(default)]
    low_protect: bool = false; identity(); flag; project(boolean, true_only);
    /// Common LF arrival for the mains (ADR 0025); off keeps upstream phase.
    #[serde(default)]
    lf_align: bool = false; identity(); flag; project(boolean, true_only);
    /// Shorter long-mode decay through pole-replacing cuts (ADR 0025).
    #[serde(default)]
    mode_decay: bool = false; identity(); flag; project(boolean, true_only);
    /// Unwinds the RC FIR's constant phase above the band (ADR 0030).
    #[serde(default)]
    hf_phase: bool = false; identity(); flag; project(boolean, true_only);
    /// Limits how much one main fills a narrow response dip (ADR 0034).
    #[serde(default)]
    dip_limit: bool = false; identity(); flag; project(boolean, true_only);
    /// Leave the mains without alignment delay between them (ADR 0061).
    #[serde(default)]
    main_delay_off: bool = false; identity(); flag; project(boolean, true_only);
    /// Common 8th-order Butterworth high-pass on every input before the MIMO
    /// fork, in Hz; `None` is off (ADR 0031).
    #[serde(default)]
    input_hp_hz: Option<f64> = None; optional_refuse(input_hp, "input_hp", "input_hp_range"); "input_hp="; project(optional_number, present_number);
    rc_lo: f64 = 20.0; refuse(rc_lo, "rc_lo", "rc_lo_range"); number; project(number, number);
    rc_hi: Option<f64> = None; optional_refuse(rc_hi, "rc_hi", "rc_hi_range"); none; project(optional_number, optional_number);
    #[serde(default)]
    rc_hi_basis: RcHiBasis = RcHiBasis::Strict; identity(); none; project(basis, basis);
    #[serde(default = "default_rc_hi_transition")]
    rc_hi_transition: u8 = 1; transition(); none; project(transition, non_default_integer);
    /// Extra correction bands, sorted by `lo_hz`, at most four (ADR 0024).
    #[serde(default)]
    rc_bands: Vec<RcBand> = Vec::new(); identity(); none; project(manual, manual);
    room_volume_m3: Option<f64> = None; volume(); none; project(optional_number, optional_number);
    room_dims_m: Option<[f64; 3]> = None; identity(); none; project(manual, manual);
    mimo_on: bool = false; identity(); none; project(boolean, boolean);
    fsiso: f64 = 150.0; clamp(fsiso); number; project(number, number);
    mimo_low: Option<f64> = None; optional_clamp(mimo_low); none; project(optional_number, optional_number);
    #[ts(type = "number")]
    mimo_max_supports: i64 = 0; integer(mimo_max); none; project(integer, integer);
    mimo_fir: bool = true; identity(); none; project(legacy_fir, boolean);
    /// Interval after onset protected from MIMO support action (ms, ADR 0037).
    #[serde(default = "default_mimo_early_ms")]
    mimo_early_ms: f64 = default_mimo_early_ms(); refuse(mimo_early_ms, "mimo_early_ms", "mimo_early_range"); none; project(number, non_default_number);
    /// Evaluates MIMO guards at each measured position (ADR 0036).
    #[serde(default)]
    mimo_seat_guard: bool = false; identity(); none; project(boolean, true_only);
    /// Experimental: PEQ fits use closed-form Jacobians (results leave SECS 2).
    #[serde(default)]
    peq_analytic_derivatives: bool = false; identity(); flag; project(boolean, true_only);
    /// Experimental: MIMO IIR fits use closed-form derivatives (results leave SECS 2).
    #[serde(default)]
    mimo_iir_analytic_derivatives: bool = false; identity(); none; project(boolean, true_only);
    #[serde(with = "ordered_f64_map")]
    #[ts(as = "BTreeMap<String, f64>")]
    xo: Vec<(String, f64)> = Vec::new(); xo(); none; project(manual, manual);
    #[serde(with = "ordered_f64_map")]
    #[ts(as = "BTreeMap<String, f64>")]
    xo_rec: Vec<(String, f64)> = Vec::new(); identity(); none; project(manual, manual);
    /// Fingerprint of the inputs used for the stored XO recommendation.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[ts(as = "Option<String>", optional)]
    xo_rec_key: String = String::new(); identity(); none; project(text, nonempty_text);
    /// Recompute the XO recommendation before calibration (ADR 0028).
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    #[ts(as = "Option<bool>", optional)]
    xo_rec_include: bool = true; identity(); none; project(include, false_only);
    ultra_ms: f64 = 200.0; clamp(ultra_ms); none; project(number, number);
    #[serde(with = "ordered_sub_map")]
    #[ts(as = "BTreeMap<String, SubEntry>")]
    subs_manual: Vec<(String, SubEntry)> = Vec::new(); identity(); none; project(manual, manual);
    sub_main_delay_ms: f64 = 0.0; clamp(main_delay); number; project(number, number);
    #[serde(with = "ordered_layout_map")]
    #[ts(as = "BTreeMap<String, BundleLayoutEntry>")]
    bundle_layouts: Vec<(String, BundleLayoutEntry)> = Vec::new(); identity(); none; project(manual, manual);
    infra_boost_max_db: f64 = 24.0; clamp(infra_boost); number; project(number, number);
    xo_peq_on: bool = true; identity(); none; project(boolean, boolean);
    boost_max_db: f64 = 10.0; clamp(boost); number; project(number, number);
    xo_peq_lo: f64 = 0.0; clamp(xo_peq); number; project(number, number);
    xo_peq_hi: f64 = 0.0; clamp(xo_peq); number; project(number, number);
    }
    edits {
    Mode {
        mode: Mode,
        } ["mode", mode] => |s| { s.mode = Self::mode(*mode)?; Ok(Applied::Applied) };
    SampleRatePolicy {
        policy: SampleRatePolicy,
        } ["sample_rate_policy", sample_rate_policy] => |s| { s.sample_rate_policy = Self::sample_rate_policy(*policy)?; Ok(Applied::Applied) };
    UltraMs {
        ms: f64,
        } ["ultra_ms", ultra_ms] => |s| { s.ultra_ms = Self::ultra_ms(*ms)?; Ok(Applied::Applied) };
    LowProtect {
        on: bool,
        } ["low_protect", low_protect] => |s| { s.low_protect = Self::low_protect(*on)?; Ok(Applied::Applied) };
    LfAlign {
        on: bool,
        } ["lf_align", lf_align] => |s| { s.lf_align = Self::lf_align(*on)?; Ok(Applied::Applied) };
    ModeDecay {
        on: bool,
        } ["mode_decay", mode_decay] => |s| { s.mode_decay = Self::mode_decay(*on)?; Ok(Applied::Applied) };
    HfPhase {
        on: bool,
        } ["hf_phase", hf_phase] => |s| { s.hf_phase = Self::hf_phase(*on)?; Ok(Applied::Applied) };
    DipLimit {
        on: bool,
        } ["dip_limit", dip_limit] => |s| { s.dip_limit = Self::dip_limit(*on)?; Ok(Applied::Applied) };
    MainDelayOff {
        on: bool,
        } ["main_delay_off", main_delay_off] => |s| { s.main_delay_off = Self::main_delay_off(*on)?; Ok(Applied::Applied) };
    /// The common input high-pass in Hz, `None` to turn it off (ADR 0031).
    InputHp {
        hz: Option<f64>,
        } ["input_hp", input_hp_hz] => |s| { s.input_hp_hz = Self::input_hp_hz(*hz)?; Ok(Applied::Applied) };
    RcLo {
        hz: f64,
        } ["rc_lo", rc_lo] => |s| { s.rc_lo = Self::rc_lo(*hz)?; Ok(Applied::Applied) };
    RcHi {
        hz: Option<f64>,
        } ["rc_hi", rc_hi] => |s| { s.rc_hi = Self::rc_hi(*hz)?; Ok(Applied::Applied) };
    RcHiBasis {
        basis: RcHiBasis,
        } ["rc_hi_basis", rc_hi_basis] => |s| { s.rc_hi_basis = Self::rc_hi_basis(*basis)?; Ok(Applied::Applied) };
    RcHiTransition {
        factor: u8,
        } ["rc_hi_transition", rc_hi_transition] => |s| { s.rc_hi_transition = Self::rc_hi_transition(*factor)?; Ok(Applied::Applied) };
    /// Replaces the whole list of extra correction bands (ADR 0024); an
    /// empty list removes them. Rejected, not clamped, when a band breaks
    /// `CALIBRATION_LIMITS.rc_band` or the list is longer than `rc_bands_max`.
    RcBands {
        bands: Vec<RcBand>,
        }  => |s| { let _ = s; Ok(Applied::Structured) };
    RoomVolume {
        m3: f64,
        } ["room_volume", room_volume_m3] => |s| { s.room_volume_m3 = Self::room_volume_m3(Some(*m3))?; Ok(Applied::Applied) };
    RoomDims {
        dims: [f64; 3],
        }  => |s| { let _ = s; Ok(Applied::Structured) };
    BoostMax {
        db: f64,
        } ["boost_max", boost_max_db] => |s| { s.boost_max_db = Self::boost_max_db(*db)?; Ok(Applied::Applied) };
    Xo {
        group: String,
        hz: f64,
        } ["xo", xo] => |s| { let _ = s; Ok(Applied::Structured) };
    XoRecInclude {
        on: bool,
        } ["xo_rec_include", xo_rec_include] => |s| { s.xo_rec_include = Self::xo_rec_include(*on)?; Ok(Applied::Applied) };
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
        }  => |s| { if let Some(v) = on { s.xo_peq_on = *v; }
            if let Some(v) = lo { s.xo_peq_lo = Self::xo_peq_lo(*v)?; }
            if let Some(v) = hi { s.xo_peq_hi = Self::xo_peq_hi(*v)?; } Ok(Applied::Applied) };
    Infra {
        role: Option<String>,
        hz: f64,
        #[serde(rename = "type")]
        typ: BandType,
        subs: Vec<String>,
        }  => |s| { let _ = s; Ok(Applied::Structured) };
    InfraBoost {
        db: f64,
        } ["infra_boost", infra_boost_max_db] => |s| { s.infra_boost_max_db = Self::infra_boost_max_db(*db)?; Ok(Applied::Applied) };
    SubMode {
        role: String,
        mode: SubModeChoice,
        }  => |s| { let _ = s; Ok(Applied::Structured) };
    SubLayout {
        bundle: String,
        layout: SubLayout,
        }  => |s| { let _ = s; Ok(Applied::Structured) };
    RearTrim {
        bundle: String,
        ms: f64,
        } ["rear_trim", rear_trim_ms] => |s| { let _ = s; Ok(Applied::Structured) };
    Sub {
        role: String,
        patch: SubPatch,
        }  => |s| { let _ = s; Ok(Applied::Structured) };
    SubBandType {
        role: String,
        band: SubBand,
        #[serde(rename = "type")]
        typ: BandTypeChoice,
        }  => |s| { let _ = s; Ok(Applied::Structured) };
    MainDelay {
        ms: f64,
        } ["main_delay", sub_main_delay_ms] => |s| { s.sub_main_delay_ms = Self::sub_main_delay_ms(*ms)?; Ok(Applied::Applied) };
    MimoOn {
        on: bool,
        } ["mimo_on", mimo_on] => |s| { s.mimo_on = Self::mimo_on(*on)?; Ok(Applied::Applied) };
    Fsiso {
        hz: f64,
        } ["fsiso", fsiso] => |s| { s.fsiso = Self::fsiso(*hz)?; Ok(Applied::Applied) };
    MimoLow {
        hz: Option<f64>,
        } ["mimo_low", mimo_low] => |s| { s.mimo_low = Self::mimo_low(*hz)?; Ok(Applied::Applied) };
    MimoMax {
        count: f64,
        } ["mimo_max", mimo_max_supports] => |s| { s.mimo_max_supports = Self::mimo_max_supports(clamp_value(*count, CALIBRATION_LIMITS.mimo_max, "mimo_max_supports")?.round_ties_even() as i64)?; Ok(Applied::Applied) };
    MimoFir {
        on: bool,
        } ["mimo_fir", mimo_fir] => |s| { s.mimo_fir = Self::mimo_fir(*on)?; Ok(Applied::Applied) };
    MimoEarly {
        ms: f64,
        } ["mimo_early", mimo_early_ms] => |s| { s.mimo_early_ms = Self::mimo_early_ms(*ms)?; Ok(Applied::Applied) };
    MimoSeatGuard {
        on: bool,
        } ["mimo_seat_guard", mimo_seat_guard] => |s| { s.mimo_seat_guard = Self::mimo_seat_guard(*on)?; Ok(Applied::Applied) };
    /// Experimental closed-form derivatives (D43, D44); an absent part keeps its value.
    AnalyticDerivatives {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        peq: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        mimo_iir: Option<bool>,
        }  => |s| { if let Some(v) = peq { s.peq_analytic_derivatives = *v; }
            if let Some(v) = mimo_iir { s.mimo_iir_analytic_derivatives = *v; } Ok(Applied::Applied) };
    }
    project_order { "mode", "rc_lo", "rc_hi", "room_volume_m3", "room_dims_m", "mimo_on", "fsiso", "mimo_low", "mimo_max_supports", "mimo_fir", "xo", "xo_rec", "ultra_ms", "subs_manual", "sub_main_delay_ms", "bundle_layouts", "infra_boost_max_db", "xo_peq_on", "boost_max_db", "xo_peq_lo", "xo_peq_hi", "rc_bands", "low_protect", "lf_align", "mode_decay", "hf_phase", "dip_limit", "main_delay_off", "input_hp_hz", "rc_hi_basis", "rc_hi_transition", "sample_rate_policy", "peq_analytic_derivatives", "mimo_iir_analytic_derivatives", "mimo_seat_guard", "mimo_early_ms", "xo_rec_key", "xo_rec_include" }
}

/// Whether an edit belongs to the scalar declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    Applied,
    Structured,
}
/// A rejected setting and its screen message parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsRefusal {
    pub field: String,
    pub reason: &'static str,
    pub min: f64,
    pub max: f64,
}
impl SettingsRefusal {
    fn new(field: impl Into<String>, reason: &'static str, bounds: (f64, f64)) -> Self {
        Self {
            field: field.into(),
            reason,
            min: bounds.0,
            max: bounds.1,
        }
    }
}
impl From<SettingsRefusal> for crate::ipc::IpcError {
    fn from(value: SettingsRefusal) -> Self {
        let detail = match value.reason {
            "rc_lo_range" => "RC lower limit is outside its range",
            "rc_hi_range" => "RC upper limit is outside its range",
            "input_hp_range" => "input high-pass is outside its range",
            "mimo_early_range" => "MIMO early interval is outside its range",
            "rc_hi_transition" => "RC transition factor must be 1, 2 or 4",
            _ => "calibration setting is outside its range",
        };
        let mut error = Self::new(crate::ipc::ErrorCode::InvalidRequest, detail)
            .with_param("field", value.field);
        // Preserve the existing transition rejection envelope.
        if value.reason != "rc_hi_transition" {
            error = error
                .with_param("reason", value.reason)
                .with_param("min", value.min.to_string())
                .with_param("max", value.max.to_string());
        }
        error
    }
}
fn refuse_value(
    value: f64,
    bounds: (f64, f64),
    field: &str,
    reason: &'static str,
) -> Result<f64, SettingsRefusal> {
    if value.is_finite() && (bounds.0..=bounds.1).contains(&value) {
        Ok(value)
    } else {
        Err(SettingsRefusal::new(field, reason, bounds))
    }
}
fn clamp_value(value: f64, bounds: (f64, f64), field: &str) -> Result<f64, SettingsRefusal> {
    if value.is_nan() {
        Err(SettingsRefusal::new(field, "not_finite", bounds))
    } else {
        Ok(value.clamp(bounds.0, bounds.1))
    }
}
impl CalibrationSettings {
    fn check_structured(&mut self) -> Result<(), SettingsRefusal> {
        if self.rc_hi.is_some_and(|hi| self.rc_lo >= hi) {
            return Err(SettingsRefusal::new(
                "rc_lo",
                "rc_lo_above_hi",
                CALIBRATION_LIMITS.rc_lo,
            ));
        }
        if self.rc_bands.len() > CALIBRATION_LIMITS.rc_bands_max.1 as usize {
            return Err(SettingsRefusal::new(
                "rc_bands",
                "rc_band_count",
                CALIBRATION_LIMITS.rc_bands_max,
            ));
        }
        for band in &self.rc_bands {
            refuse_value(
                band.lo_hz,
                CALIBRATION_LIMITS.rc_band,
                "rc_bands",
                "rc_band_range",
            )?;
            refuse_value(
                band.hi_hz,
                CALIBRATION_LIMITS.rc_band,
                "rc_bands",
                "rc_band_range",
            )?;
            if band.lo_hz >= band.hi_hz {
                return Err(SettingsRefusal::new(
                    "rc_bands",
                    "rc_band_range",
                    CALIBRATION_LIMITS.rc_band,
                ));
            }
        }
        self.rc_bands.sort_by(|a, b| a.lo_hz.total_cmp(&b.lo_hz));
        for (role, sub) in &self.subs_manual {
            for (name, hz) in [("hp_hz", sub.hp_hz), ("lp_hz", sub.lp_hz)] {
                if hz != 0.0 {
                    refuse_value(
                        hz,
                        crate::rules::SUB_LIMITS.filter_hz,
                        &format!("subs_manual.{role}.{name}"),
                        "sub_filter_range",
                    )?;
                }
            }
            if sub.hp_hz != 0.0 && sub.lp_hz != 0.0 && sub.hp_hz >= sub.lp_hz {
                return Err(SettingsRefusal::new(
                    format!("subs_manual.{role}.hp_hz"),
                    "sub_filter_order",
                    crate::rules::SUB_LIMITS.filter_hz,
                ));
            }
        }
        for (_, hz) in &mut self.xo {
            *hz = clamp_value(*hz, CALIBRATION_LIMITS.xo, "xo")?.round_ties_even();
        }
        Ok(())
    }
}

impl CalibrationSettings {
    /// Korean user-visible name of the selected calibration mode.
    pub fn label(&self) -> String {
        mode_label(self.mode, Some(self.ultra_ms))
    }

    /// Stable staleness fingerprint with the same inclusions and exclusions as upstream.
    pub fn engine_key(&self) -> String {
        let mut out = String::new();
        write!(
            &mut out,
            "{:?}|{}|",
            self.mode,
            self.scalar_engine_key("rc_lo")
        )
        .unwrap();
        optional_number(&mut out, self.rc_hi);
        out.push('|');
        optional_number(&mut out, self.room_volume_m3);
        out.push('|');
        sorted_f64_map(&mut out, &self.xo);
        out.push('|');
        if self.mode == Mode::Ultra {
            out.push_str(&number(self.ultra_ms));
        } else {
            out.push('-');
        }
        out.push('|');
        out.push_str(&self.manual_key());
        write!(
            &mut out,
            "|{}|{}|{}|{}|{}",
            self.scalar_engine_key("infra_boost_max_db"),
            self.xo_peq_on,
            self.scalar_engine_key("xo_peq_lo"),
            self.scalar_engine_key("xo_peq_hi"),
            self.scalar_engine_key("boost_max_db")
        )
        .unwrap();
        // Preserve upstream keys for strict ×1, including existing caches.
        if self.rc_hi_basis != RcHiBasis::Strict {
            write!(&mut out, "|rc_hi_basis={}", self.rc_hi_basis.as_str()).unwrap();
        }
        if self.rc_hi_transition != 1 {
            write!(&mut out, "|rc_hi_transition={}", self.rc_hi_transition).unwrap();
        }
        // ADR 0024: only a project with extra bands gets a new key. Sorted, so
        // a list the service stored and the same list in another order match.
        if !self.rc_bands.is_empty() {
            let mut bands = self.rc_bands.clone();
            bands.sort_by(|a, b| {
                a.lo_hz
                    .total_cmp(&b.lo_hz)
                    .then(a.hi_hz.total_cmp(&b.hi_hz))
            });
            out.push_str("|rc_bands=");
            for (index, band) in bands.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write!(&mut out, "{}-{}", number(band.lo_hz), number(band.hi_hz)).unwrap();
            }
        }
        // Keep existing cache fingerprints unchanged for the default policy.
        if self.sample_rate_policy == SampleRatePolicy::Resample {
            out.push_str("|sample_rate_policy=resample");
        }
        out.push_str(&self.scalar_engine_key("low_protect"));
        out.push_str(&self.scalar_engine_key("lf_align"));
        out.push_str(&self.scalar_engine_key("mode_decay"));
        out.push_str(&self.scalar_engine_key("hf_phase"));
        out.push_str(&self.scalar_engine_key("dip_limit"));
        // ADR 0031: the chain is a calibration output, so the cut joins this key.
        out.push_str(&self.scalar_engine_key("input_hp_hz"));
        out.push_str(&self.scalar_engine_key("peq_analytic_derivatives"));
        out.push_str(&self.scalar_engine_key("main_delay_off"));
        out
    }

    /// Stable fingerprint of the sub settings which affect the correction engine.
    pub fn manual_key(&self) -> String {
        let mut entries: Vec<_> = self.subs_manual.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let mut out = String::new();
        for (role, entry) in entries {
            let filters = filter_key(entry);
            if entry.mode == "auto" {
                if entry.hp_hz != 0.0 || entry.lp_hz != 0.0 || !entry.peq.is_empty() {
                    write!(&mut out, "{role}:auto:{filters};").unwrap();
                }
                continue;
            }
            let layout = if entry.mode == "bundle" {
                self.bundle_layouts
                    .iter()
                    .find(|(name, _)| name == &entry.bundle)
                    .map_or("values", |(_, layout)| layout.kind.as_str())
            } else {
                "values"
            };
            if layout != "values" {
                let row = if layout == "dba" {
                    entry.dba_row.as_str()
                } else {
                    ""
                };
                let trim = if layout == "dba" {
                    self.bundle_layouts
                        .iter()
                        .find(|(name, _)| name == &entry.bundle)
                        .map_or(0.0, |(_, value)| value.rear_trim_ms)
                } else {
                    0.0
                };
                write!(
                    &mut out,
                    "{role}:bundle:{}:{layout}:{row}:{}:{filters};",
                    entry.bundle,
                    number(trim)
                )
                .unwrap();
            } else {
                let bundle = if entry.mode == "bundle" {
                    entry.bundle.as_str()
                } else {
                    ""
                };
                write!(
                    &mut out,
                    "{role}:{}:{bundle}:{}:{}:{}:",
                    entry.mode,
                    number(entry.gain_db),
                    number(entry.delay_ms),
                    entry.polarity
                )
                .unwrap();
                for &(frequency, q) in &entry.allpass {
                    write!(&mut out, "{},{};", number(frequency), number(q)).unwrap();
                }
                write!(&mut out, ":{filters};").unwrap();
            }
        }
        let fixed = self.subs_manual.iter().any(|(_, e)| e.mode == "fixed");
        if fixed {
            write!(&mut out, "|{}", self.scalar_engine_key("sub_main_delay_ms")).unwrap();
        } else {
            out.push_str("|-");
        }
        out
    }

    /// Stable fingerprint of settings used only by MIMO.
    pub fn mimo_key(&self) -> String {
        let mut excluded: Vec<_> = self
            .subs_manual
            .iter()
            .filter(|(_, entry)| !entry.mimo_support)
            .map(|(role, _)| role.as_str())
            .collect();
        excluded.sort_unstable();
        let mut out = format!("{}|", self.scalar_engine_key("fsiso"));
        optional_number(&mut out, self.mimo_low);
        write!(
            &mut out,
            "|{}|{}|{}",
            excluded.join(","),
            self.mimo_max_supports,
            self.mimo_fir
        )
        .unwrap();
        if self.mimo_iir_analytic_derivatives {
            out.push_str("|mimo_iir_analytic_derivatives");
        }
        if self.mimo_seat_guard {
            out.push_str("|seat_guard");
        }
        if self.mimo_early_ms != default_mimo_early_ms() {
            write!(&mut out, "|early={}", number(self.mimo_early_ms)).unwrap();
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ModeParams {
    pub taps: usize,
    pub lift_ms: f64,
    pub peq_bands: Option<usize>,
    pub phase_frac: f64,
    pub max_latency_ms: f64,
}

/// Stable ASCII correction-mode tag used by export names and metadata.
pub fn mode_tag(mode: Mode) -> &'static str {
    match mode {
        Mode::Zero => "zero",
        Mode::Mixed => "mixed",
        Mode::Linear => "linear",
        Mode::Ultra => "ultra",
    }
}

/// Korean user-visible mode name, including the clamped ultra latency when supplied.
pub fn mode_label(mode: Mode, ultra_ms: Option<f64>) -> String {
    let name = match mode {
        Mode::Zero => "최소 지연 모드",
        Mode::Mixed => "저지연 모드",
        Mode::Linear => "표준 모드",
        Mode::Ultra => "초고지연 모드",
    };
    if mode == Mode::Ultra
        && let Some(ms) = ultra_ms
    {
        let (lo, hi) = ULTRA_MS;
        format!("{name} {} ms", python_g(ms.clamp(lo, hi)))
    } else {
        name.into()
    }
}

fn python_g(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() { "-0" } else { "0" }.into();
    }
    let mut exponent = value.abs().log10().floor() as i32;
    let places = 5 - exponent;
    let scale = 10_f64.powi(places);
    let rounded = if places >= 0 {
        (value * scale).round_ties_even() / scale
    } else {
        (value / scale.recip()).round_ties_even() * scale.recip()
    };
    if rounded != 0.0 {
        exponent = rounded.abs().log10().floor() as i32;
    }
    if !(-4..6).contains(&exponent) {
        let mut text = format!("{rounded:.5e}");
        let e = text.find('e').expect("scientific format");
        while text[..e].ends_with('0') {
            text.remove(e - 1);
        }
        if text[..text.find('e').unwrap()].ends_with('.') {
            text.remove(text.find('e').unwrap() - 1);
        }
        let e = text.find('e').unwrap();
        let power = text[e + 1..].parse::<i32>().expect("scientific exponent");
        format!(
            "{}e{}{:02}",
            &text[..e],
            if power >= 0 { '+' } else { '-' },
            power.abs()
        )
    } else {
        let places = (5 - exponent).max(0) as usize;
        let mut text = format!("{rounded:.places$}");
        while text.contains('.') && text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
        text
    }
}

pub fn mode_params(mode: Mode, ultra_ms: f64) -> ModeParams {
    match mode {
        Mode::Zero => ModeParams {
            taps: 0,
            lift_ms: 0.0,
            peq_bands: Some(32),
            phase_frac: 6.0,
            max_latency_ms: 0.0,
        },
        Mode::Mixed => ModeParams {
            taps: 2048,
            lift_ms: 8.0,
            peq_bands: None,
            phase_frac: 6.0,
            max_latency_ms: 10.0,
        },
        Mode::Linear => ModeParams {
            taps: 2048,
            lift_ms: 28.0,
            peq_bands: None,
            phase_frac: 24.0,
            max_latency_ms: 30.0,
        },
        Mode::Ultra => {
            let ms = ultra_ms.clamp(ULTRA_MS.0, ULTRA_MS.1);
            let lift_ms = ms - 2.0;
            let mut taps = 2048;
            while taps < 16384 && 0.7 * taps as f64 / 48_000.0 * 1000.0 < lift_ms {
                taps *= 2;
            }
            ModeParams {
                taps,
                lift_ms,
                peq_bands: None,
                phase_frac: 24.0,
                max_latency_ms: ms,
            }
        }
    }
}

fn number(value: f64) -> String {
    if value == 0.0 {
        "0".into()
    } else {
        format!("{:016x}", value.to_bits())
    }
}

fn optional_number(out: &mut String, value: Option<f64>) {
    if let Some(value) = value {
        out.push_str(&number(value));
    } else {
        out.push('-');
    }
}

fn sorted_f64_map(out: &mut String, values: &[(String, f64)]) {
    let mut values: Vec<_> = values.iter().collect();
    values.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, value) in values {
        write!(out, "{key}={};", number(*value)).unwrap();
    }
}

fn filter_key(entry: &SubEntry) -> String {
    let mut out = format!(
        "{}:{}:{}:{}:",
        number(entry.hp_hz),
        if entry.hp_hz != 0.0 {
            &entry.hp_type
        } else {
            ""
        },
        number(entry.lp_hz),
        if entry.lp_hz != 0.0 {
            &entry.lp_type
        } else {
            ""
        }
    );
    for &(frequency, q, gain) in &entry.peq {
        write!(
            &mut out,
            "{},{},{};",
            number(frequency),
            number(q),
            number(gain)
        )
        .unwrap();
    }
    out
}

macro_rules! ordered_map {
    ($module:ident, $value:ty) => {
        mod $module {
            use serde::{Deserialize, Deserializer, Serializer};
            use std::collections::BTreeMap;

            pub fn serialize<S>(
                values: &Vec<(String, $value)>,
                serializer: S,
            ) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                use serde::ser::SerializeMap as _;
                let mut map = serializer.serialize_map(Some(values.len()))?;
                for (key, value) in values {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }

            pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<(String, $value)>, D::Error>
            where
                D: Deserializer<'de>,
            {
                // Deserialization is for IPC values. `.secs` parsing uses PyJson
                // directly to retain insertion order.
                Ok(BTreeMap::<String, $value>::deserialize(deserializer)?
                    .into_iter()
                    .collect())
            }
        }
    };
}

ordered_map!(ordered_f64_map, f64);
ordered_map!(ordered_sub_map, super::SubEntry);
ordered_map!(ordered_layout_map, super::BundleLayoutEntry);

#[cfg(test)]
mod tests {
    use super::*;

    fn band(lo_hz: f64, hi_hz: f64) -> RcBand {
        RcBand { lo_hz, hi_hz }
    }

    #[test]
    fn scalar_constructor_uses_declared_kind_and_payload() {
        assert_eq!(
            SettingEdit::scalar("dip_limit", ScalarValue::Bool(true)),
            Some(SettingEdit::DipLimit { on: true })
        );
        assert_eq!(
            SettingEdit::scalar("mimo_early", ScalarValue::Number(20.0)),
            Some(SettingEdit::MimoEarly { ms: 20.0 })
        );
        for hz in [None, Some(25.0)] {
            assert_eq!(
                SettingEdit::scalar("input_hp", ScalarValue::OptionalNumber(hz)),
                Some(SettingEdit::InputHp { hz })
            );
        }
        assert_eq!(
            SettingEdit::scalar("rc_hi_transition", ScalarValue::Number(2.0)),
            Some(SettingEdit::RcHiTransition { factor: 2 })
        );
        assert_eq!(
            SettingEdit::scalar("mode", ScalarValue::Text("linear".into())),
            Some(SettingEdit::Mode { mode: Mode::Linear })
        );
        assert_eq!(
            SettingEdit::scalar("dip_limit", ScalarValue::Number(1.0)),
            None
        );
        assert_eq!(
            SettingEdit::scalar("unknown", ScalarValue::Bool(true)),
            None
        );
        assert_eq!(SettingEdit::scalar("xo", ScalarValue::Number(80.0)), None);
        assert_eq!(
            SettingEdit::scalar("rc_hi_transition", ScalarValue::Number(2.5)),
            None
        );
        assert_eq!(
            SettingEdit::scalar("rc_hi_transition", ScalarValue::Number(f64::NAN)),
            None
        );
    }

    #[test]
    fn scalar_settings_table_round_trips_defaults_and_keeps_engine_keys() {
        use crate::settings_edit::SettingEdit;
        let defaults = CalibrationSettings::default();
        let json = serde_json::to_value(&defaults).unwrap();
        let round: CalibrationSettings = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(defaults, round);
        let limits = serde_json::to_value(CALIBRATION_LIMITS).unwrap();
        for row in crate::rules::SCALAR_EDITS {
            if row.kind == "rear_trim" {
                continue;
            }
            assert!(
                json.get(row.field).is_some() || row.field == "xo_rec_include",
                "{}",
                row.field
            );
            if row.limit != "-" {
                assert!(limits.get(row.limit).is_some(), "{}", row.limit);
            }
            let mut edit = serde_json::to_value(SettingEdit::DipLimit { on: true }).unwrap();
            edit["kind"] = row.kind.into();
            let error = serde_json::from_value::<SettingEdit>(edit).err();
            assert!(
                !error.is_some_and(|e| e.to_string().contains("unknown variant")),
                "{}",
                row.kind
            );
        }
        assert!(
            crate::rules::SCALAR_EDITS
                .iter()
                .any(|r| r.kind == "dip_limit")
        );
        assert_eq!(
            defaults.engine_key(),
            "Mixed|4034000000000000|-|-||-||-|4038000000000000|true|0|0|4024000000000000"
        );
    }
    #[test]
    fn apply_scalar_clamps_and_refuses_by_policy() {
        use crate::settings_edit::SettingEdit;
        let mut settings = CalibrationSettings::default();
        assert_eq!(
            settings.apply_scalar(&SettingEdit::BoostMax { db: 1e9 }),
            Ok(Applied::Applied)
        );
        assert_eq!(settings.boost_max_db, 10.0);
        assert_eq!(
            settings.apply_scalar(&SettingEdit::RcLo { hz: 5.0 }),
            Err(SettingsRefusal {
                field: "rc_lo".into(),
                reason: "rc_lo_range",
                min: 20.0,
                max: 200.0,
            })
        );
        assert_eq!(
            settings
                .apply_scalar(&SettingEdit::InputHp { hz: Some(f64::NAN) })
                .unwrap_err()
                .field,
            "input_hp"
        );
        settings
            .apply_scalar(&SettingEdit::DipLimit { on: true })
            .unwrap();
        assert!(settings.dip_limit);
    }
    #[test]
    fn checked_applies_every_policy_to_a_whole_value() {
        let mut settings = CalibrationSettings {
            boost_max_db: 1e9,
            ultra_ms: 1.0,
            rc_lo: 5.0,
            ..Default::default()
        };
        assert_eq!(settings.clone().checked().unwrap_err().field, "rc_lo");
        settings.rc_lo = 20.0;
        let checked = settings.checked().unwrap();
        assert_eq!(checked.boost_max_db, 10.0);
        assert_eq!(checked.ultra_ms, 30.0);
    }
    #[test]
    fn limits_table_has_the_two_new_entries() {
        assert_eq!(CALIBRATION_LIMITS.peq_bands, (0.0, 64.0));
        assert_eq!(crate::rules::SUB_LIMITS.filter_hz, (10.0, 300.0));
    }

    #[test]
    fn valid_rc_bands_drop_broken_bands_sort_and_keep_four() {
        let kept = valid_rc_bands(&[
            band(15_000.0, 20_000.0),
            band(10.0, 30.0),
            band(f64::NAN, 100.0),
            band(500.0, 400.0),
            band(300.0, 300.0),
            band(19_000.0, 21_000.0),
            band(1_000.0, 2_000.0),
            band(30.0, 60.0),
            band(5_000.0, 6_000.0),
            band(100.0, 200.0),
        ]);
        assert_eq!(
            kept,
            [
                band(30.0, 60.0),
                band(100.0, 200.0),
                band(1_000.0, 2_000.0),
                band(5_000.0, 6_000.0)
            ]
        );
        assert!(valid_rc_bands(&[]).is_empty());
    }

    #[test]
    fn engine_key_reads_exactly_the_exported_fields() {
        let settings = serde_json::to_value(CalibrationSettings::default()).unwrap();
        let fields = settings
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        let keyed = crate::rules::ENGINE_KEY_FIELDS
            .iter()
            .chain(crate::rules::MANUAL_KEY_FIELDS)
            .chain(crate::rules::MIMO_KEY_FIELDS)
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let excluded = ["room_dims_m", "mimo_on", "xo_rec"]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(fields, keyed.union(&excluded).copied().collect());
    }

    #[test]
    fn engine_key_lists_extra_bands_sorted_and_only_when_present() {
        let baseline = CalibrationSettings::default();
        assert!(!baseline.engine_key().contains("rc_bands"));
        let forward = CalibrationSettings {
            rc_bands: vec![band(100.0, 200.0), band(15_000.0, 20_000.0)],
            ..baseline.clone()
        };
        let backward = CalibrationSettings {
            rc_bands: vec![band(15_000.0, 20_000.0), band(100.0, 200.0)],
            ..baseline
        };
        assert_eq!(forward.engine_key(), backward.engine_key());
        let expected = format!(
            "|rc_bands={}-{},{}-{}",
            number(100.0),
            number(200.0),
            number(15_000.0),
            number(20_000.0)
        );
        assert!(forward.engine_key().ends_with(&expected));
    }

    #[test]
    fn rc_upper_defaults_and_each_setting_change_engine_key() {
        let baseline = CalibrationSettings::default();
        let mut json = serde_json::to_value(&baseline).unwrap();
        json.as_object_mut().unwrap().remove("rc_hi_basis");
        json.as_object_mut().unwrap().remove("rc_hi_transition");
        let legacy: CalibrationSettings = serde_json::from_value(json).unwrap();
        assert_eq!(legacy, baseline);
        for basis in [RcHiBasis::Lenient, RcHiBasis::Agreement] {
            let settings = CalibrationSettings {
                rc_hi_basis: basis,
                ..baseline.clone()
            };
            assert_ne!(settings.engine_key(), baseline.engine_key());
            assert_eq!(settings.mimo_key(), baseline.mimo_key());
            assert_eq!(
                serde_json::to_value(&settings).unwrap()["rc_hi_basis"],
                basis.as_str()
            );
        }
        for factor in [2, 4] {
            let settings = CalibrationSettings {
                rc_hi_transition: factor,
                ..baseline.clone()
            };
            assert_ne!(settings.engine_key(), baseline.engine_key());
            assert_eq!(settings.mimo_key(), baseline.mimo_key());
        }
    }

    #[test]
    fn sample_rate_policy_defaults_for_legacy_settings_and_changes_engine_key() {
        let baseline = CalibrationSettings::default();
        let mut json = serde_json::to_value(&baseline).unwrap();
        assert_eq!(json["sample_rate_policy"], "reject");
        json.as_object_mut().unwrap().remove("sample_rate_policy");
        let legacy: CalibrationSettings = serde_json::from_value(json).unwrap();
        assert_eq!(legacy, baseline);
        let mut resampled = legacy.clone();
        resampled.sample_rate_policy = SampleRatePolicy::Resample;
        assert_ne!(resampled.engine_key(), legacy.engine_key());
        assert_eq!(
            serde_json::to_value(&resampled).unwrap()["sample_rate_policy"],
            "resample"
        );
        resampled.sample_rate_policy = SampleRatePolicy::Reject;
        assert_eq!(resampled.engine_key(), legacy.engine_key());
    }

    #[test]
    fn low_protect_defaults_off_and_preserves_legacy_engine_key() {
        let baseline = CalibrationSettings::default();
        assert!(!baseline.low_protect);
        let key = baseline.engine_key();
        assert!(!key.contains("low_protect"));
        let mut json = serde_json::to_value(&baseline).unwrap();
        json.as_object_mut().unwrap().remove("low_protect");
        let mut legacy: CalibrationSettings = serde_json::from_value(json).unwrap();
        assert_eq!(legacy, baseline);
        legacy.low_protect = true;
        assert_eq!(legacy.engine_key(), format!("{key}|low_protect"));
        legacy.low_protect = false;
        assert_eq!(legacy.engine_key(), key);
        let edit: crate::settings_edit::SettingEdit =
            serde_json::from_value(serde_json::json!({"kind":"low_protect","on":true})).unwrap();
        assert_eq!(
            edit,
            crate::settings_edit::SettingEdit::LowProtect { on: true }
        );
    }

    #[test]
    fn lf_align_and_mode_decay_default_off_and_extend_the_key_only_when_on() {
        let baseline = CalibrationSettings::default();
        assert!(!baseline.lf_align && !baseline.mode_decay);
        let key = baseline.engine_key();
        assert!(!key.contains("lf_align") && !key.contains("mode_decay"));
        let mut json = serde_json::to_value(&baseline).unwrap();
        json.as_object_mut().unwrap().remove("lf_align");
        json.as_object_mut().unwrap().remove("mode_decay");
        let mut legacy: CalibrationSettings = serde_json::from_value(json).unwrap();
        assert_eq!(legacy, baseline);
        legacy.lf_align = true;
        assert_eq!(legacy.engine_key(), format!("{key}|lf_align"));
        legacy.mode_decay = true;
        assert_eq!(legacy.engine_key(), format!("{key}|lf_align|mode_decay"));
        legacy.lf_align = false;
        assert_eq!(legacy.engine_key(), format!("{key}|mode_decay"));
        for (kind, expected) in [
            (
                "lf_align",
                crate::settings_edit::SettingEdit::LfAlign { on: true },
            ),
            (
                "mode_decay",
                crate::settings_edit::SettingEdit::ModeDecay { on: true },
            ),
        ] {
            let edit: crate::settings_edit::SettingEdit =
                serde_json::from_value(serde_json::json!({"kind": kind, "on": true})).unwrap();
            assert_eq!(edit, expected);
        }
    }

    #[test]
    fn hf_phase_defaults_off_and_extends_the_key_only_when_on() {
        let baseline = CalibrationSettings::default();
        assert!(!baseline.hf_phase);
        let key = baseline.engine_key();
        assert!(!key.contains("hf_phase"));
        let mut json = serde_json::to_value(&baseline).unwrap();
        json.as_object_mut().unwrap().remove("hf_phase");
        let mut legacy: CalibrationSettings = serde_json::from_value(json).unwrap();
        assert_eq!(legacy, baseline);
        legacy.hf_phase = true;
        assert_eq!(legacy.engine_key(), format!("{key}|hf_phase"));
        legacy.mode_decay = true;
        assert_eq!(legacy.engine_key(), format!("{key}|mode_decay|hf_phase"));
        let edit: crate::settings_edit::SettingEdit =
            serde_json::from_value(serde_json::json!({"kind": "hf_phase", "on": true})).unwrap();
        assert_eq!(
            edit,
            crate::settings_edit::SettingEdit::HfPhase { on: true }
        );
    }

    #[test]
    fn main_delay_off_defaults_off_and_extends_the_key_only_when_on() {
        let baseline = CalibrationSettings::default();
        assert!(!baseline.main_delay_off);
        let key = baseline.engine_key();
        assert!(!key.contains("main_delay_off"));
        let mut json = serde_json::to_value(&baseline).unwrap();
        json.as_object_mut().unwrap().remove("main_delay_off");
        let mut legacy: CalibrationSettings = serde_json::from_value(json).unwrap();
        assert_eq!(legacy, baseline);
        legacy.main_delay_off = true;
        assert_eq!(legacy.engine_key(), format!("{key}|main_delay_off"));
        legacy.dip_limit = true;
        legacy.peq_analytic_derivatives = true;
        assert_eq!(
            legacy.engine_key(),
            format!("{key}|dip_limit|peq_analytic_derivatives|main_delay_off")
        );
        legacy.main_delay_off = false;
        assert_eq!(
            legacy.engine_key(),
            format!("{key}|dip_limit|peq_analytic_derivatives")
        );
        let edit: crate::settings_edit::SettingEdit =
            serde_json::from_value(serde_json::json!({"kind": "main_delay_off", "on": true}))
                .unwrap();
        assert_eq!(
            edit,
            crate::settings_edit::SettingEdit::MainDelayOff { on: true }
        );
        legacy.apply_scalar(&edit).unwrap();
        assert!(legacy.main_delay_off);
    }

    #[test]
    fn dip_limit_defaults_off_and_preserves_legacy_engine_key() {
        let baseline = CalibrationSettings::default();
        assert!(!baseline.dip_limit);
        let key = baseline.engine_key();
        assert!(!key.contains("dip_limit"));
        let mut json = serde_json::to_value(&baseline).unwrap();
        json.as_object_mut().unwrap().remove("dip_limit");
        let mut legacy: CalibrationSettings = serde_json::from_value(json).unwrap();
        assert_eq!(legacy, baseline);
        legacy.dip_limit = true;
        assert_eq!(legacy.engine_key(), format!("{key}|dip_limit"));
        legacy.dip_limit = false;
        assert_eq!(legacy.engine_key(), key);
        let edit: crate::settings_edit::SettingEdit =
            serde_json::from_value(serde_json::json!({"kind": "dip_limit", "on": true})).unwrap();
        assert_eq!(
            edit,
            crate::settings_edit::SettingEdit::DipLimit { on: true }
        );
    }

    #[test]
    fn input_hp_defaults_off_and_extends_the_key_only_when_set() {
        let baseline = CalibrationSettings::default();
        assert_eq!(baseline.input_hp_hz, None);
        let key = baseline.engine_key();
        assert!(!key.contains("input_hp"));
        let mut json = serde_json::to_value(&baseline).unwrap();
        json.as_object_mut().unwrap().remove("input_hp_hz");
        let mut legacy: CalibrationSettings = serde_json::from_value(json).unwrap();
        assert_eq!(legacy, baseline);
        legacy.input_hp_hz = Some(27.0);
        assert_eq!(
            legacy.engine_key(),
            format!("{key}|input_hp={}", number(27.0))
        );
        legacy.input_hp_hz = Some(27.5);
        assert_ne!(
            legacy.engine_key(),
            format!("{key}|input_hp={}", number(27.0))
        );
        for (hz, expected) in [
            (serde_json::json!(25), Some(25.0)),
            (serde_json::json!(null), None),
        ] {
            let edit: crate::settings_edit::SettingEdit =
                serde_json::from_value(serde_json::json!({"kind": "input_hp", "hz": hz})).unwrap();
            assert_eq!(
                edit,
                crate::settings_edit::SettingEdit::InputHp { hz: expected }
            );
        }
    }

    #[test]
    fn xo_recommendation_metadata_defaults_on_and_does_not_change_engine_key() {
        let baseline = CalibrationSettings::default();
        let key = baseline.engine_key();
        assert!(baseline.xo_rec_include);
        assert!(baseline.xo_rec_key.is_empty());
        let wire = serde_json::to_value(&baseline).unwrap();
        assert!(wire.get("xo_rec_include").is_none());
        assert!(wire.get("xo_rec_key").is_none());

        let legacy: CalibrationSettings = serde_json::from_value(wire).unwrap();
        assert!(legacy.xo_rec_include);
        let changed = CalibrationSettings {
            xo_rec_include: false,
            xo_rec_key: "recommendation".into(),
            ..legacy
        };
        assert_eq!(changed.engine_key(), key);
    }

    #[test]
    fn analytic_derivative_settings_default_off_and_change_only_their_keys() {
        let baseline = CalibrationSettings::default();
        let engine_key = baseline.engine_key();
        let mimo_key = baseline.mimo_key();
        let mut json = serde_json::to_value(&baseline).unwrap();
        let object = json.as_object_mut().unwrap();
        object.remove("peq_analytic_derivatives");
        object.remove("mimo_iir_analytic_derivatives");
        let legacy: CalibrationSettings = serde_json::from_value(json).unwrap();
        assert!(!legacy.peq_analytic_derivatives);
        assert!(!legacy.mimo_iir_analytic_derivatives);
        assert_eq!(legacy.engine_key(), engine_key);
        assert_eq!(legacy.mimo_key(), mimo_key);

        let mut peq = legacy.clone();
        peq.peq_analytic_derivatives = true;
        assert_eq!(
            peq.engine_key(),
            format!("{engine_key}|peq_analytic_derivatives")
        );
        assert_eq!(peq.mimo_key(), mimo_key);

        let mut mimo = legacy;
        mimo.mimo_iir_analytic_derivatives = true;
        assert_eq!(mimo.engine_key(), engine_key);
        assert_eq!(
            mimo.mimo_key(),
            format!("{mimo_key}|mimo_iir_analytic_derivatives")
        );
    }

    #[test]
    fn mimo_seat_guard_defaults_off_and_preserves_legacy_mimo_key() {
        let baseline = CalibrationSettings::default();
        let engine_key = baseline.engine_key();
        let mimo_key = baseline.mimo_key();
        assert!(!baseline.mimo_seat_guard);
        assert!(!mimo_key.contains("seat_guard"));

        let mut json = serde_json::to_value(&baseline).unwrap();
        json.as_object_mut().unwrap().remove("mimo_seat_guard");
        let mut legacy: CalibrationSettings = serde_json::from_value(json).unwrap();
        assert!(!legacy.mimo_seat_guard);
        assert_eq!(legacy.engine_key(), engine_key);
        assert_eq!(legacy.mimo_key(), mimo_key);

        legacy.mimo_seat_guard = true;
        assert_eq!(legacy.engine_key(), engine_key);
        assert_eq!(legacy.mimo_key(), format!("{mimo_key}|seat_guard"));
        let edit: crate::settings_edit::SettingEdit =
            serde_json::from_value(serde_json::json!({"kind": "mimo_seat_guard", "on": true}))
                .unwrap();
        assert_eq!(
            edit,
            crate::settings_edit::SettingEdit::MimoSeatGuard { on: true }
        );
    }

    #[test]
    fn mimo_early_defaults_sixty_and_preserves_legacy_mimo_key() {
        let baseline = CalibrationSettings::default();
        let engine_key = baseline.engine_key();
        let mimo_key = baseline.mimo_key();
        assert_eq!(baseline.mimo_early_ms, 60.0);
        assert!(!mimo_key.contains("|early="));

        let mut json = serde_json::to_value(&baseline).unwrap();
        json.as_object_mut().unwrap().remove("mimo_early_ms");
        let mut legacy: CalibrationSettings = serde_json::from_value(json).unwrap();
        assert_eq!(legacy.mimo_early_ms, 60.0);
        assert_eq!(legacy.engine_key(), engine_key);
        assert_eq!(legacy.mimo_key(), mimo_key);

        legacy.mimo_early_ms = 20.0;
        assert_eq!(legacy.engine_key(), engine_key);
        assert_eq!(
            legacy.mimo_key(),
            format!("{mimo_key}|early={}", number(20.0))
        );
        let edit: crate::settings_edit::SettingEdit =
            serde_json::from_value(serde_json::json!({"kind": "mimo_early", "ms": 20.0})).unwrap();
        assert_eq!(
            edit,
            crate::settings_edit::SettingEdit::MimoEarly { ms: 20.0 }
        );
    }

    #[test]
    fn mode_labels_match_upstream() {
        assert_eq!(mode_label(Mode::Zero, None), "최소 지연 모드");
        assert_eq!(mode_label(Mode::Mixed, Some(120.0)), "저지연 모드");
        assert_eq!(mode_label(Mode::Linear, None), "표준 모드");
        assert_eq!(mode_label(Mode::Ultra, None), "초고지연 모드");
        assert_eq!(mode_label(Mode::Ultra, Some(20.0)), "초고지연 모드 30 ms");
        assert_eq!(
            mode_label(Mode::Ultra, Some(123.456_789)),
            "초고지연 모드 123.457 ms"
        );
        assert_eq!(mode_label(Mode::Ultra, Some(250.0)), "초고지연 모드 200 ms");

        let settings = CalibrationSettings {
            mode: Mode::Ultra,
            ultra_ms: 30.123_456_7,
            ..CalibrationSettings::default()
        };
        assert_eq!(settings.label(), "초고지연 모드 30.1235 ms");
    }

    #[test]
    fn mode_params_match_upstream() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../oracle/goldens/calibration/mode_params.json"
        )))
        .unwrap();
        for row in fixture["outputs"]["rows"].as_array().unwrap() {
            let mode = match row["mode"].as_str().unwrap() {
                "zero" => Mode::Zero,
                "mixed" => Mode::Mixed,
                "linear" => Mode::Linear,
                "ultra" => Mode::Ultra,
                _ => unreachable!(),
            };
            let actual = mode_params(mode, row["ultra_ms"].as_f64().unwrap());
            let expected = &row["params"];
            assert_eq!(actual.taps as u64, expected["taps"].as_u64().unwrap());
            assert_eq!(actual.lift_ms, expected["lift_ms"].as_f64().unwrap());
            assert_eq!(
                actual.peq_bands.map(|value| value as u64),
                expected
                    .get("peq_bands")
                    .and_then(serde_json::Value::as_u64)
            );
            assert_eq!(
                actual.phase_frac,
                expected
                    .get("phase_frac")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(6.0)
            );
            assert_eq!(
                actual.max_latency_ms,
                expected["max_latency_ms"].as_f64().unwrap()
            );
        }
    }

    #[test]
    fn keys_have_upstream_exclusions() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../oracle/goldens/calibration/keys.json"
        )))
        .unwrap();
        let settings: Vec<_> = fixture["inputs"]["settings"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str(),
                    serde_json::from_value::<CalibrationSettings>(value.clone()).unwrap(),
                )
            })
            .collect();

        for (kind, key) in [
            (
                "engine",
                CalibrationSettings::engine_key as fn(&CalibrationSettings) -> String,
            ),
            ("manual", CalibrationSettings::manual_key),
            ("mimo", CalibrationSettings::mimo_key),
        ] {
            let expected = fixture["outputs"][kind].as_object().unwrap();
            for (left_name, left) in &settings {
                for (right_name, right) in &settings {
                    assert_eq!(
                        key(left) == key(right),
                        expected[*left_name] == expected[*right_name],
                        "{kind} key equality differs for {left_name} and {right_name}"
                    );
                }
            }
        }
    }
}
