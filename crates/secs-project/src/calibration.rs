//! Read and write the calibration half of an upstream-compatible `.secs` file.

use secs_types::calibration::{
    BundleLayoutEntry, CalibrationSettings, Mode, RcBand, RcHiBasis, SampleRatePolicy, SubEntry,
};
use secs_types::rules::{CALIBRATION_LIMITS, RC_LO_READ};

use crate::pyjson::Shape;
use crate::{Project, PyJson};

static SUB_ENTRY_SHAPE: Shape = Shape::Record {
    optional: &[],
    nested: &[],
};
static BUNDLE_LAYOUT_SHAPE: Shape = Shape::Record {
    optional: &[],
    nested: &[],
};
/// The `calibration` object [`store_calibration`] writes (F16). Its `xo` and
/// `xo_rec` maps are replaced; `subs_manual` and `bundle_layouts` keep only
/// the entries the settings still have.
pub(crate) static CALIBRATION_SHAPE: Shape = Shape::Record {
    // Written only for non-default policies, so stored values must not return.
    optional: &[
        "sample_rate_policy",
        "low_protect",
        "lf_align",
        "mode_decay",
        "hf_phase",
        "input_hp_hz",
        "rc_hi_basis",
        "rc_hi_transition",
        "rc_bands",
        "peq_analytic_derivatives",
        "mimo_iir_analytic_derivatives",
        "xo_rec_key",
        "xo_rec_include",
    ],
    nested: &[
        ("subs_manual", Shape::Map(&SUB_ENTRY_SHAPE)),
        ("bundle_layouts", Shape::Map(&BUNDLE_LAYOUT_SHAPE)),
    ],
};

pub fn calibration_from_project(project: &Project) -> CalibrationSettings {
    let Some(raw) = project
        .extra
        .iter()
        .find(|(key, _)| key == "calibration")
        .map(|(_, value)| value)
        .and_then(PyJson::object)
    else {
        return CalibrationSettings::default();
    };
    let mut out = CalibrationSettings::default();
    let get = |key: &str| {
        raw.iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    };

    out.mode = match get("mode").and_then(PyJson::str) {
        Some("zero") => Mode::Zero,
        Some("linear") => Mode::Linear,
        Some("ultra") => Mode::Ultra,
        _ => Mode::Mixed,
    };
    out.sample_rate_policy = match get("sample_rate_policy").and_then(PyJson::str) {
        Some("resample") => SampleRatePolicy::Resample,
        _ => SampleRatePolicy::Reject,
    };
    out.low_protect = get("low_protect").is_some_and(PyJson::is_truthy);
    out.lf_align = get("lf_align").is_some_and(PyJson::is_truthy);
    out.mode_decay = get("mode_decay").is_some_and(PyJson::is_truthy);
    out.hf_phase = get("hf_phase").is_some_and(PyJson::is_truthy);
    out.input_hp_hz = num(
        get("input_hp_hz"),
        CALIBRATION_LIMITS.input_hp.0,
        CALIBRATION_LIMITS.input_hp.1,
    );
    out.rc_lo = num(get("rc_lo"), RC_LO_READ.0, RC_LO_READ.1).unwrap_or(20.0);
    out.rc_hi = num(
        get("rc_hi"),
        CALIBRATION_LIMITS.rc_hi.0,
        CALIBRATION_LIMITS.rc_hi.1,
    );
    out.rc_hi_basis = match get("rc_hi_basis").and_then(PyJson::str) {
        Some("lenient") => RcHiBasis::Lenient,
        Some("agreement") => RcHiBasis::Agreement,
        _ => RcHiBasis::Strict,
    };
    out.rc_hi_transition = match get("rc_hi_transition") {
        Some(PyJson::Int(value)) => value.parse::<u8>().ok(),
        Some(PyJson::Float(1.0)) => Some(1),
        Some(PyJson::Float(2.0)) => Some(2),
        Some(PyJson::Float(4.0)) => Some(4),
        _ => None,
    }
    .filter(|value| matches!(value, 1 | 2 | 4))
    .unwrap_or(1);
    out.rc_bands = rc_bands(get("rc_bands"));
    out.room_volume_m3 = num(get("room_volume_m3"), 5.0, 100_000.0);
    out.room_dims_m = room_dims(get("room_dims_m"));
    out.mimo_on = get("mimo_on").is_some_and(PyJson::is_truthy);
    out.fsiso = num(get("fsiso"), 50.0, 150.0).unwrap_or(150.0);
    out.mimo_low = num(get("mimo_low"), 15.0, 150.0);
    out.mimo_max_supports =
        num(get("mimo_max_supports"), 0.0, 12.0).map_or(0, |value| value.round_ties_even() as i64);
    // Upstream 1.3.0: a stored calibration without the key predates FIR
    // supports (1.2.0) and stays IIR; an empty one keeps the FIR default.
    out.mimo_fir = get("mimo_fir").map_or(raw.is_empty(), PyJson::is_truthy);
    out.peq_analytic_derivatives = get("peq_analytic_derivatives").is_some_and(PyJson::is_truthy);
    out.mimo_iir_analytic_derivatives =
        get("mimo_iir_analytic_derivatives").is_some_and(PyJson::is_truthy);
    out.xo = xo_map(get("xo"));
    out.xo_rec = xo_map(get("xo_rec"));
    out.xo_rec_key = get("xo_rec_key")
        .and_then(PyJson::str)
        .unwrap_or_default()
        .to_owned();
    out.xo_rec_include = get("xo_rec_include").map_or_else(
        || out.xo.is_empty() || xo_maps_equal(&out.xo, &out.xo_rec),
        PyJson::is_truthy,
    );
    out.ultra_ms = num(get("ultra_ms"), 30.0, 200.0).unwrap_or(200.0);
    if let Some(manual) = get("subs_manual").and_then(PyJson::object) {
        out.subs_manual = manual
            .iter()
            .filter_map(|(role, value)| {
                value.object().map(|entry| (role.clone(), sub_entry(entry)))
            })
            .collect();
    }
    out.sub_main_delay_ms = num(get("sub_main_delay_ms"), 0.0, 100.0).unwrap_or(0.0);
    out.xo_peq_on = get("xo_peq_on").is_none_or(PyJson::is_truthy);
    out.boost_max_db = num(get("boost_max_db"), 0.0, 10.0).unwrap_or(10.0);
    out.xo_peq_lo = num(get("xo_peq_lo"), 0.0, 1000.0).unwrap_or(0.0);
    out.xo_peq_hi = num(get("xo_peq_hi"), 0.0, 1000.0).unwrap_or(0.0);
    out.infra_boost_max_db = num(get("infra_boost_max_db"), 0.0, 24.0).unwrap_or(24.0);
    if let Some(layouts) = get("bundle_layouts").and_then(PyJson::object) {
        out.bundle_layouts = layouts
            .iter()
            .filter(|(bundle, value)| {
                matches!(bundle.as_str(), "A" | "B") && value.object().is_some()
            })
            .map(|(bundle, value)| (bundle.clone(), bundle_layout_entry(value.object().unwrap())))
            .collect();
    }
    out
}

pub fn store_calibration(project: &mut Project, settings: &CalibrationSettings) {
    let value = calibration_json(settings);
    if let Some((_, old)) = project
        .extra
        .iter_mut()
        .find(|(key, _)| key == "calibration")
    {
        *old = value;
    } else {
        project.extra.push(("calibration".into(), value));
    }
}

fn calibration_json(settings: &CalibrationSettings) -> PyJson {
    let mut values = vec![
        (
            "mode".into(),
            PyJson::Str(secs_types::calibration::mode_tag(settings.mode).into()),
        ),
        ("rc_lo".into(), PyJson::Float(settings.rc_lo)),
        ("rc_hi".into(), optional_float(settings.rc_hi)),
        (
            "room_volume_m3".into(),
            optional_float(settings.room_volume_m3),
        ),
        (
            "room_dims_m".into(),
            settings.room_dims_m.map_or(PyJson::Null, |dims| {
                PyJson::Array(dims.into_iter().map(PyJson::Float).collect())
            }),
        ),
        ("mimo_on".into(), PyJson::Bool(settings.mimo_on)),
        ("fsiso".into(), PyJson::Float(settings.fsiso)),
        ("mimo_low".into(), optional_float(settings.mimo_low)),
        (
            "mimo_max_supports".into(),
            PyJson::Int(settings.mimo_max_supports.to_string()),
        ),
        ("mimo_fir".into(), PyJson::Bool(settings.mimo_fir)),
        ("xo".into(), f64_map(&settings.xo)),
        ("xo_rec".into(), f64_map(&settings.xo_rec)),
        ("ultra_ms".into(), PyJson::Float(settings.ultra_ms)),
        (
            "subs_manual".into(),
            PyJson::Object(
                settings
                    .subs_manual
                    .iter()
                    .map(|(role, entry)| (role.clone(), sub_json(entry)))
                    .collect(),
            ),
        ),
        (
            "sub_main_delay_ms".into(),
            PyJson::Float(settings.sub_main_delay_ms),
        ),
        (
            "bundle_layouts".into(),
            PyJson::Object(
                settings
                    .bundle_layouts
                    .iter()
                    .map(|(bundle, entry)| {
                        (
                            bundle.clone(),
                            PyJson::Object(vec![
                                ("kind".into(), PyJson::Str(entry.kind.clone())),
                                ("rear_trim_ms".into(), PyJson::Float(entry.rear_trim_ms)),
                            ]),
                        )
                    })
                    .collect(),
            ),
        ),
        (
            "infra_boost_max_db".into(),
            PyJson::Float(settings.infra_boost_max_db),
        ),
        ("xo_peq_on".into(), PyJson::Bool(settings.xo_peq_on)),
        ("boost_max_db".into(), PyJson::Float(settings.boost_max_db)),
        ("xo_peq_lo".into(), PyJson::Float(settings.xo_peq_lo)),
        ("xo_peq_hi".into(), PyJson::Float(settings.xo_peq_hi)),
    ];
    // Omit defaults to preserve the upstream saved-byte goldens.
    if !settings.rc_bands.is_empty() {
        values.push((
            "rc_bands".into(),
            PyJson::Array(
                settings
                    .rc_bands
                    .iter()
                    .map(|band| {
                        PyJson::Array(vec![PyJson::Float(band.lo_hz), PyJson::Float(band.hi_hz)])
                    })
                    .collect(),
            ),
        ));
    }
    if settings.low_protect {
        values.push(("low_protect".into(), PyJson::Bool(true)));
    }
    if settings.lf_align {
        values.push(("lf_align".into(), PyJson::Bool(true)));
    }
    if settings.mode_decay {
        values.push(("mode_decay".into(), PyJson::Bool(true)));
    }
    if settings.hf_phase {
        values.push(("hf_phase".into(), PyJson::Bool(true)));
    }
    if let Some(hz) = settings.input_hp_hz {
        values.push(("input_hp_hz".into(), PyJson::Float(hz)));
    }
    if settings.rc_hi_basis != RcHiBasis::Strict {
        values.push((
            "rc_hi_basis".into(),
            PyJson::Str(settings.rc_hi_basis.as_str().into()),
        ));
    }
    if settings.rc_hi_transition != 1 {
        values.push((
            "rc_hi_transition".into(),
            PyJson::Int(settings.rc_hi_transition.to_string()),
        ));
    }
    if settings.sample_rate_policy == SampleRatePolicy::Resample {
        values.push(("sample_rate_policy".into(), PyJson::Str("resample".into())));
    }
    if settings.peq_analytic_derivatives {
        values.push(("peq_analytic_derivatives".into(), PyJson::Bool(true)));
    }
    if settings.mimo_iir_analytic_derivatives {
        values.push(("mimo_iir_analytic_derivatives".into(), PyJson::Bool(true)));
    }
    if !settings.xo_rec_key.is_empty() {
        values.push((
            "xo_rec_key".into(),
            PyJson::Str(settings.xo_rec_key.clone()),
        ));
    }
    if !settings.xo_rec_include {
        values.push(("xo_rec_include".into(), PyJson::Bool(false)));
    }
    PyJson::Object(values)
}

fn sub_json(entry: &SubEntry) -> PyJson {
    PyJson::Object(vec![
        ("mode".into(), PyJson::Str(entry.mode.clone())),
        ("bundle".into(), PyJson::Str(entry.bundle.clone())),
        ("gain_db".into(), PyJson::Float(entry.gain_db)),
        ("delay_ms".into(), PyJson::Float(entry.delay_ms)),
        ("polarity".into(), PyJson::Int(entry.polarity.to_string())),
        (
            "allpass".into(),
            PyJson::Array(
                entry
                    .allpass
                    .iter()
                    .map(|&(frequency, q)| {
                        PyJson::Array(vec![PyJson::Float(frequency), PyJson::Float(q)])
                    })
                    .collect(),
            ),
        ),
        ("mimo_support".into(), PyJson::Bool(entry.mimo_support)),
        ("hp_hz".into(), PyJson::Float(entry.hp_hz)),
        ("lp_hz".into(), PyJson::Float(entry.lp_hz)),
        (
            "peq".into(),
            PyJson::Array(
                entry
                    .peq
                    .iter()
                    .map(|&(frequency, q, gain)| {
                        PyJson::Array(vec![
                            PyJson::Float(frequency),
                            PyJson::Float(q),
                            PyJson::Float(gain),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("hp_type".into(), PyJson::Str(entry.hp_type.clone())),
        ("lp_type".into(), PyJson::Str(entry.lp_type.clone())),
        ("dba_row".into(), PyJson::Str(entry.dba_row.clone())),
    ])
}

fn sub_entry(value: &[(String, PyJson)]) -> SubEntry {
    let get = |key: &str| {
        value
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    };
    let mode = get("mode")
        .and_then(PyJson::str)
        .filter(|mode| matches!(*mode, "auto" | "fixed" | "bundle"))
        .unwrap_or("auto");
    let bundle = get("bundle")
        .and_then(PyJson::str)
        .filter(|bundle| matches!(*bundle, "A" | "B"))
        .unwrap_or("A");
    let mut allpass = Vec::new();
    if let Some(items) = get("allpass").and_then(PyJson::array) {
        for item in items {
            let Some(values) = item.array() else { continue };
            if values.len() < 2 {
                continue;
            }
            let (Some(frequency), Some(q)) = (as_float(&values[0]), as_float(&values[1])) else {
                continue;
            };
            if frequency.is_finite() && q.is_finite() && frequency > 0.0 {
                allpass.push((frequency.clamp(10.0, 500.0), q.clamp(0.1, 20.0)));
            }
        }
    }
    allpass.truncate(2);
    let mut peq = Vec::new();
    if let Some(items) = get("peq").and_then(PyJson::array) {
        for item in items {
            let Some(values) = item.array() else { continue };
            if values.len() < 3 {
                continue;
            }
            let (Some(frequency), Some(q), Some(gain)) = (
                as_float(&values[0]),
                as_float(&values[1]),
                as_float(&values[2]),
            ) else {
                continue;
            };
            if frequency.is_finite() && q.is_finite() && gain.is_finite() && frequency > 0.0 {
                peq.push((
                    frequency.clamp(10.0, 500.0),
                    q.clamp(0.1, 20.0),
                    gain.clamp(-30.0, 12.0),
                ));
            }
        }
    }
    peq.truncate(3);
    let corner = |key| {
        let value = num(get(key), 0.0, 300.0).unwrap_or(0.0);
        if value <= 0.0 { 0.0 } else { value.max(10.0) }
    };
    SubEntry {
        mode: mode.into(),
        bundle: bundle.into(),
        gain_db: num(get("gain_db"), -60.0, 20.0).unwrap_or(0.0),
        delay_ms: num(get("delay_ms"), 0.0, 500.0).unwrap_or(0.0),
        polarity: if num(get("polarity"), -1.0, 1.0).unwrap_or(1.0) < 0.0 {
            -1
        } else {
            1
        },
        allpass,
        mimo_support: get("mimo_support").is_none_or(PyJson::is_truthy),
        hp_hz: corner("hp_hz"),
        lp_hz: corner("lp_hz"),
        peq,
        hp_type: band_type(get("hp_type")).into(),
        lp_type: band_type(get("lp_type")).into(),
        dba_row: match get("dba_row").and_then(PyJson::str) {
            Some("rear") => "rear",
            _ => "front",
        }
        .into(),
    }
}

fn bundle_layout_entry(value: &[(String, PyJson)]) -> BundleLayoutEntry {
    let get = |key: &str| {
        value
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    };
    BundleLayoutEntry {
        kind: get("kind")
            .and_then(PyJson::str)
            .filter(|kind| matches!(*kind, "values" | "dba" | "multi"))
            .unwrap_or("values")
            .into(),
        rear_trim_ms: num(get("rear_trim_ms"), -20.0, 20.0).unwrap_or(0.0),
    }
}

fn rc_bands(value: Option<&PyJson>) -> Vec<RcBand> {
    let mut bands: Vec<_> = value
        .and_then(PyJson::array)
        .into_iter()
        .flatten()
        .filter_map(|value| {
            let edges = value.array()?;
            if edges.len() != 2 {
                return None;
            }
            let lo_hz = as_float(&edges[0])?;
            let hi_hz = as_float(&edges[1])?;
            (lo_hz.is_finite()
                && hi_hz.is_finite()
                && (CALIBRATION_LIMITS.rc_band.0..=CALIBRATION_LIMITS.rc_band.1).contains(&lo_hz)
                && (CALIBRATION_LIMITS.rc_band.0..=CALIBRATION_LIMITS.rc_band.1).contains(&hi_hz)
                && lo_hz < hi_hz)
                .then_some(RcBand { lo_hz, hi_hz })
        })
        .collect();
    bands.sort_by(|left, right| left.lo_hz.total_cmp(&right.lo_hz));
    bands.truncate(CALIBRATION_LIMITS.rc_bands_max.1 as usize);
    bands
}

fn num(value: Option<&PyJson>, low: f64, high: f64) -> Option<f64> {
    let value = as_float(value?)?;
    value.is_finite().then(|| value.clamp(low, high))
}

fn as_float(value: &PyJson) -> Option<f64> {
    match value {
        PyJson::Int(value) | PyJson::Str(value) => value.parse().ok(),
        PyJson::Float(value) => Some(*value),
        PyJson::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
        _ => None,
    }
}

fn room_dims(value: Option<&PyJson>) -> Option<[f64; 3]> {
    let value = value?.array()?;
    if value.len() != 3 {
        return None;
    }
    let dims = [
        as_float(&value[0])?,
        as_float(&value[1])?,
        as_float(&value[2])?,
    ];
    dims.iter()
        .all(|dim| dim.is_finite() && (0.5..=100.0).contains(dim))
        .then_some(dims)
}

fn xo_map(value: Option<&PyJson>) -> Vec<(String, f64)> {
    value
        .and_then(PyJson::object)
        .into_iter()
        .flatten()
        .filter_map(|(group, value)| {
            as_float(value)
                .filter(|value| value.is_finite())
                .map(|value| (group.clone(), value.clamp(40.0, 200.0)))
        })
        .collect()
}

fn xo_maps_equal(left: &[(String, f64)], right: &[(String, f64)]) -> bool {
    let mut left = left.to_vec();
    let mut right = right.to_vec();
    left.sort_by(|a, b| a.0.cmp(&b.0));
    right.sort_by(|a, b| a.0.cmp(&b.0));
    left == right
}

fn band_type(value: Option<&PyJson>) -> &'static str {
    match value.and_then(PyJson::str) {
        Some("LR12") => "LR12",
        Some("LR48") => "LR48",
        Some("BW12") => "BW12",
        Some("BW24") => "BW24",
        _ => "LR24",
    }
}

fn optional_float(value: Option<f64>) -> PyJson {
    value.map_or(PyJson::Null, PyJson::Float)
}

fn f64_map(values: &[(String, f64)]) -> PyJson {
    PyJson::Object(
        values
            .iter()
            .map(|(key, value)| (key.clone(), PyJson::Float(*value)))
            .collect(),
    )
}
