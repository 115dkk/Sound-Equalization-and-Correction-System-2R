use crate::pyjson::Shape;
use crate::{ProjectError, PyJson, SpeakerConfig};
use secs_types::ipc::OutputRateChoice;
use std::collections::BTreeMap;

/// The `dsp` object [`DspProfile::to_json`] writes (F16).
pub(crate) static DSP_SHAPE: Shape = Shape::Record {
    optional: &[],
    nested: &[],
};

/// The output rate a preset runs at (ADR 0017): miniDSP 96 kHz, every other
/// preset and any unknown profile 48 kHz.
fn preset_output_rate(profile: &str) -> OutputRateChoice {
    OutputRateChoice::Fixed(if profile == "minidsp" { 96_000 } else { 48_000 })
}

fn output_rate(
    value: Option<&PyJson>,
    fallback: OutputRateChoice,
) -> Result<OutputRateChoice, ProjectError> {
    let choice = match value {
        None => fallback,
        Some(PyJson::Str(value)) if value == "auto" => OutputRateChoice::Auto,
        Some(PyJson::Int(value)) => {
            value
                .parse::<u32>()
                .map(OutputRateChoice::Fixed)
                .map_err(|_| {
                    ProjectError::Invalid("dsp.output_rate must be 'auto' or an integer".into())
                })?
        }
        Some(_) => {
            return Err(ProjectError::Invalid(
                "dsp.output_rate must be 'auto' or an integer".into(),
            ));
        }
    };
    if let OutputRateChoice::Fixed(rate) = choice
        && ![44_100, 48_000, 88_200, 96_000, 176_400, 192_000].contains(&rate)
    {
        return Err(ProjectError::Invalid(format!(
            "dsp.output_rate {rate} is not supported; expected 44100, 48000, 88200, 96000, 176400, or 192000"
        )));
    }
    Ok(choice)
}

#[derive(Debug, Clone, PartialEq)]
pub struct DspProfile {
    pub profile: String,
    pub output_rate: OutputRateChoice,
    pub fir_taps: i64,
    pub fir_budget: i64,
    pub iir_bands: i64,
    pub support_mode: String,
    pub support_taps: i64,
    pub allpass_bands: i64,
    pub bm_path_delays: i64,
    pub bm_path_polarity: bool,
    pub bm_path_allpass: i64,
    pub path_peq_bands: i64,
    pub path_allpass_bands: i64,
    pub per_output_lp: bool,
    pub fir_taps_max: i64,
    pub fir_taps_auto: Option<bool>,
    pub support_taps_auto: Option<bool>,
    pub name: String,
}
impl Default for DspProfile {
    fn default() -> Self {
        Self {
            profile: "tesira".into(),
            output_rate: OutputRateChoice::Fixed(48_000),
            fir_taps: 2048,
            fir_budget: 32768,
            iir_bands: 16,
            support_mode: "iir".into(),
            support_taps: 1024,
            allpass_bands: 16,
            bm_path_delays: 16,
            bm_path_polarity: false,
            bm_path_allpass: 0,
            path_peq_bands: 0,
            path_allpass_bands: 0,
            per_output_lp: false,
            fir_taps_max: 2048,
            fir_taps_auto: Some(true),
            support_taps_auto: Some(true),
            name: String::new(),
        }
    }
}
impl DspProfile {
    pub fn presets() -> BTreeMap<&'static str, Self> {
        let rows = [
            (
                "camilla",
                "PC · CamillaDSP",
                65536,
                65536 * 32,
                64,
                64,
                64,
                true,
                4,
                16,
                8,
                true,
            ),
            (
                "tesira",
                "Biamp Tesira SERVER (DSP-2 ×3)",
                2048,
                51200,
                16,
                16,
                16,
                false,
                0,
                0,
                0,
                false,
            ),
            (
                "qsys",
                "Q-SYS Core 8 Flex / 110f",
                16384,
                65536,
                24,
                16,
                16,
                true,
                2,
                4,
                2,
                true,
            ),
            (
                "minidsp",
                "miniDSP Flex / 2x4 HD",
                2048,
                4096,
                10,
                0,
                0,
                false,
                0,
                0,
                0,
                false,
            ),
            (
                "custom",
                "사용자 정의",
                2048,
                32768,
                16,
                16,
                16,
                false,
                0,
                0,
                0,
                false,
            ),
        ];
        rows.into_iter()
            .map(
                |(key, name, taps, budget, iir, ap, bmd, pol, bmap, ppeq, pap, lp)| {
                    (
                        key,
                        Self {
                            profile: key.into(),
                            output_rate: preset_output_rate(key),
                            fir_taps: taps,
                            fir_taps_max: taps,
                            fir_budget: budget,
                            iir_bands: iir,
                            support_mode: "iir".into(),
                            support_taps: 1024,
                            allpass_bands: ap,
                            bm_path_delays: bmd,
                            bm_path_polarity: pol,
                            bm_path_allpass: bmap,
                            path_peq_bands: ppeq,
                            path_allpass_bands: pap,
                            per_output_lp: lp,
                            fir_taps_auto: Some(true),
                            support_taps_auto: Some(true),
                            name: name.into(),
                        },
                    )
                },
            )
            .collect()
    }
    pub fn allocate_taps(
        &self,
        config: &SpeakerConfig,
        phase: &str,
        path_count: i64,
        min_taps: i64,
    ) -> (i64, i64) {
        let bands = config
            .speakers
            .iter()
            .map(|s| s.bands.len() as i64)
            .sum::<i64>()
            .max(1);
        let chan = if phase == "mixed" { bands } else { 0 };
        let sup = if ["hybrid", "fir"].contains(&self.support_mode.as_str()) {
            path_count
        } else {
            0
        };
        let consumers = chan + sup;
        if consumers <= 0 {
            return (self.fir_taps, self.support_taps);
        }
        let ceiling = self.fir_taps_max.max(min_taps);
        let ceiling_pow = 1_i64 << (63 - ceiling.leading_zeros());
        let target = self.fir_budget / consumers;
        let mut taps = if target >= 1 {
            1_i64 << (63 - target.leading_zeros())
        } else {
            min_taps
        };
        taps = taps.max(min_taps).min(ceiling_pow);
        while taps > min_taps && taps * consumers > self.fir_budget {
            taps /= 2;
        }
        (
            if chan > 0 { taps } else { self.fir_taps },
            if sup > 0 { taps } else { self.support_taps },
        )
    }
    pub fn to_json(&self) -> PyJson {
        let mut fields = vec![("profile".into(), PyJson::Str(self.profile.clone()))];
        // Written only when it differs from the preset's rate, so a project
        // that keeps the default saves the same bytes as upstream (ADR 0004)
        // and loads back to the same rate.
        if self.output_rate != preset_output_rate(&self.profile) {
            fields.push((
                "output_rate".into(),
                match self.output_rate {
                    OutputRateChoice::Auto => PyJson::Str("auto".into()),
                    OutputRateChoice::Fixed(rate) => PyJson::Int(rate.to_string()),
                },
            ));
        }
        fields.extend(vec![
            ("fir_taps".into(), PyJson::Int(self.fir_taps.to_string())),
            (
                "fir_taps_max".into(),
                PyJson::Int(self.fir_taps_max.to_string()),
            ),
            (
                "fir_budget".into(),
                PyJson::Int(self.fir_budget.to_string()),
            ),
            ("iir_bands".into(), PyJson::Int(self.iir_bands.to_string())),
            (
                "support_mode".into(),
                PyJson::Str(self.support_mode.clone()),
            ),
            (
                "support_taps".into(),
                PyJson::Int(self.support_taps.to_string()),
            ),
            (
                "allpass_bands".into(),
                PyJson::Int(self.allpass_bands.to_string()),
            ),
            (
                "bm_path_delays".into(),
                PyJson::Int(self.bm_path_delays.to_string()),
            ),
            (
                "bm_path_polarity".into(),
                PyJson::Bool(self.bm_path_polarity),
            ),
            (
                "bm_path_allpass".into(),
                PyJson::Int(self.bm_path_allpass.to_string()),
            ),
            (
                "path_peq_bands".into(),
                PyJson::Int(self.path_peq_bands.to_string()),
            ),
            (
                "path_allpass_bands".into(),
                PyJson::Int(self.path_allpass_bands.to_string()),
            ),
            ("per_output_lp".into(), PyJson::Bool(self.per_output_lp)),
            (
                "fir_taps_auto".into(),
                PyJson::Bool(self.fir_taps_auto.unwrap_or(false)),
            ),
            (
                "support_taps_auto".into(),
                PyJson::Bool(self.support_taps_auto.unwrap_or(false)),
            ),
            ("name".into(), PyJson::Str(self.name.clone())),
        ]);
        PyJson::Object(fields)
    }
    pub fn from_json(value: PyJson) -> Result<Self, ProjectError> {
        let object = value
            .object()
            .ok_or_else(|| ProjectError::Invalid("dsp must be an object".into()))?;
        let get = |key: &str| {
            object
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value)
        };
        let profile = crate::project::string(get("profile"), "tesira");
        let preset = Self::presets().remove(profile.as_str());
        let fir_taps = crate::project::integer(get("fir_taps"), 2048, "dsp.fir_taps")?;
        let fallback = preset.as_ref().cloned().unwrap_or_default();
        Ok(Self {
            profile,
            output_rate: output_rate(get("output_rate"), fallback.output_rate)?,
            fir_taps,
            fir_taps_max: crate::project::integer(
                get("fir_taps_max"),
                preset.as_ref().map_or(fir_taps, |v| v.fir_taps_max),
                "dsp.fir_taps_max",
            )?,
            fir_budget: crate::project::integer(get("fir_budget"), 32768, "dsp.fir_budget")?,
            iir_bands: crate::project::integer(get("iir_bands"), 16, "dsp.iir_bands")?,
            support_mode: crate::project::string(get("support_mode"), "iir"),
            support_taps: crate::project::integer(get("support_taps"), 1024, "dsp.support_taps")?,
            allpass_bands: crate::project::integer(
                get("allpass_bands"),
                fallback.allpass_bands,
                "dsp.allpass_bands",
            )?,
            bm_path_delays: crate::project::integer(
                get("bm_path_delays"),
                fallback.bm_path_delays,
                "dsp.bm_path_delays",
            )?,
            fir_taps_auto: crate::project::optional_bool(get("fir_taps_auto")),
            support_taps_auto: crate::project::optional_bool(get("support_taps_auto")),
            bm_path_polarity: get("bm_path_polarity")
                .and_then(PyJson::bool)
                .unwrap_or(fallback.bm_path_polarity),
            bm_path_allpass: crate::project::integer(
                get("bm_path_allpass"),
                fallback.bm_path_allpass,
                "dsp.bm_path_allpass",
            )?,
            path_peq_bands: crate::project::integer(
                get("path_peq_bands"),
                fallback.path_peq_bands,
                "dsp.path_peq_bands",
            )?,
            path_allpass_bands: crate::project::integer(
                get("path_allpass_bands"),
                fallback.path_allpass_bands,
                "dsp.path_allpass_bands",
            )?,
            per_output_lp: get("per_output_lp")
                .and_then(PyJson::bool)
                .unwrap_or(fallback.per_output_lp),
            name: crate::project::string(get("name"), ""),
        })
    }

    pub fn fir_used(
        &self,
        config: &SpeakerConfig,
        phase: &str,
        path_count: i64,
    ) -> BTreeMap<&'static str, i64> {
        let bands = config
            .speakers
            .iter()
            .map(|s| s.bands.len() as i64)
            .sum::<i64>();
        let chan = if phase == "minphase" {
            0
        } else {
            bands * self.fir_taps
        };
        let sup = if self.support_mode == "iir" {
            0
        } else {
            path_count * self.support_taps
        };
        [("chan", chan), ("sup", sup), ("total", chan + sup)]
            .into_iter()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(fields: Vec<(&str, PyJson)>) -> PyJson {
        PyJson::Object(
            fields
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
        )
    }

    #[test]
    fn output_rate_defaults_follow_presets_and_unknown_profiles_use_48k() {
        for (profile, expected) in [
            ("camilla", 48_000),
            ("tesira", 48_000),
            ("qsys", 48_000),
            ("minidsp", 96_000),
            ("custom", 48_000),
            ("future", 48_000),
        ] {
            let dsp =
                DspProfile::from_json(object(vec![("profile", PyJson::Str(profile.to_owned()))]))
                    .unwrap();
            assert_eq!(
                dsp.output_rate,
                OutputRateChoice::Fixed(expected),
                "{profile}"
            );
        }
    }

    #[test]
    fn output_rate_round_trips_auto_and_every_fixed_rate() {
        let choices = std::iter::once(OutputRateChoice::Auto).chain(
            [44_100, 48_000, 88_200, 96_000, 176_400, 192_000]
                .into_iter()
                .map(OutputRateChoice::Fixed),
        );
        for choice in choices {
            let dsp = DspProfile {
                output_rate: choice,
                ..DspProfile::default()
            };
            let restored = DspProfile::from_json(dsp.to_json()).unwrap();
            assert_eq!(restored.output_rate, choice);
        }
    }

    #[test]
    fn output_rate_rejects_unsupported_values() {
        let error = DspProfile::from_json(object(vec![
            ("profile", PyJson::Str("camilla".into())),
            ("output_rate", PyJson::Int("32000".into())),
        ]))
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("dsp.output_rate 32000 is not supported")
        );
    }
}
