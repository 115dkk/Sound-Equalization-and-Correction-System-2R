//! User-visible calibration settings and their mode-derived engine values.
//!
//! This is the typed form of `calibration.py::CalibrationSettings`. Ordered
//! pairs are used for Python dictionaries so insertion order survives a
//! `.secs` round trip; serde still exposes them as JSON objects.

use std::{collections::BTreeMap, fmt::Write as _};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::rules::CALIBRATION_LIMITS;

/// The ultra mode's latency budget range (ms).
const ULTRA_MS: (f64, f64) = CALIBRATION_LIMITS.ultra_ms;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Mode {
    Zero,
    Mixed,
    Linear,
    Ultra,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CalibrationSettings {
    pub mode: Mode,
    #[serde(default)]
    pub sample_rate_policy: SampleRatePolicy,
    pub rc_lo: f64,
    pub rc_hi: Option<f64>,
    pub room_volume_m3: Option<f64>,
    pub room_dims_m: Option<[f64; 3]>,
    pub mimo_on: bool,
    pub fsiso: f64,
    pub mimo_low: Option<f64>,
    #[ts(type = "number")]
    pub mimo_max_supports: i64,
    pub mimo_fir: bool,
    /// Experimental: PEQ fits use closed-form Jacobians (results leave SECS 2).
    #[serde(default)]
    pub peq_analytic_derivatives: bool,
    /// Experimental: MIMO IIR fits use closed-form derivatives (results leave SECS 2).
    #[serde(default)]
    pub mimo_iir_analytic_derivatives: bool,
    #[serde(with = "ordered_f64_map")]
    #[ts(as = "BTreeMap<String, f64>")]
    pub xo: Vec<(String, f64)>,
    #[serde(with = "ordered_f64_map")]
    #[ts(as = "BTreeMap<String, f64>")]
    pub xo_rec: Vec<(String, f64)>,
    pub ultra_ms: f64,
    #[serde(with = "ordered_sub_map")]
    #[ts(as = "BTreeMap<String, SubEntry>")]
    pub subs_manual: Vec<(String, SubEntry)>,
    pub sub_main_delay_ms: f64,
    #[serde(with = "ordered_layout_map")]
    #[ts(as = "BTreeMap<String, BundleLayoutEntry>")]
    pub bundle_layouts: Vec<(String, BundleLayoutEntry)>,
    pub infra_boost_max_db: f64,
    pub xo_peq_on: bool,
    pub boost_max_db: f64,
    pub xo_peq_lo: f64,
    pub xo_peq_hi: f64,
}

impl Default for CalibrationSettings {
    fn default() -> Self {
        Self {
            mode: Mode::Mixed,
            sample_rate_policy: SampleRatePolicy::Reject,
            rc_lo: 20.0,
            rc_hi: None,
            room_volume_m3: None,
            room_dims_m: None,
            mimo_on: false,
            fsiso: 150.0,
            mimo_low: None,
            mimo_max_supports: 0,
            // Beta 1.3.0: MIMO supports start as FIR in a new project.
            mimo_fir: true,
            peq_analytic_derivatives: false,
            mimo_iir_analytic_derivatives: false,
            xo: Vec::new(),
            xo_rec: Vec::new(),
            ultra_ms: 200.0,
            subs_manual: Vec::new(),
            sub_main_delay_ms: 0.0,
            bundle_layouts: Vec::new(),
            infra_boost_max_db: 24.0,
            xo_peq_on: true,
            boost_max_db: 10.0,
            xo_peq_lo: 0.0,
            xo_peq_hi: 0.0,
        }
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
        write!(&mut out, "{:?}|{}|", self.mode, number(self.rc_lo)).unwrap();
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
            number(self.infra_boost_max_db),
            self.xo_peq_on,
            number(self.xo_peq_lo),
            number(self.xo_peq_hi),
            number(self.boost_max_db)
        )
        .unwrap();
        // Keep existing cache fingerprints unchanged for the default policy.
        if self.sample_rate_policy == SampleRatePolicy::Resample {
            out.push_str("|sample_rate_policy=resample");
        }
        if self.peq_analytic_derivatives {
            out.push_str("|peq_analytic_derivatives");
        }
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
            write!(&mut out, "|{}", number(self.sub_main_delay_ms)).unwrap();
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
        let mut out = format!("{}|", number(self.fsiso));
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
