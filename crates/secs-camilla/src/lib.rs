#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! CamillaDSP v4 export.
//!
//! Upstream source: `export_camilladsp.py`.
//!
//! Reads one signal-chain value (not engine internals) and writes
//! `configs/*.yml` and `coeffs/*.wav` in the CamillaGUI folder layout, with
//! Python's float formatting and half-to-even delay rounding, ASCII-only
//! (the CamillaGUI server reads CP949). Every check (ASCII, finite numbers)
//! runs before the first file is written.

pub mod pyfmt;

use pyfmt::{format_fixed, format_g, format_i3, format_s30, repr_f64, round_ndigits};
use secs_types::chain::{BandType, MainChannel, Peq, SignalChain, SubChannel};
use std::fmt::Write as _;
use std::fs;
use std::path::{Component, Path, PathBuf};

const SAMPLE_RATE: u32 = 48_000;
const CONFIG_DIR: &str = "configs";
const COEFF_DIR: &str = "coeffs";

/// A device map as CamillaDSP takes it, keys in insertion order.
#[derive(Debug, Clone, PartialEq)]
pub struct Device(pub Vec<(String, DeviceValue)>);

/// One device-map value.
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceValue {
    /// JSON string.
    Str(String),
    /// JSON integer.
    Int(i64),
    /// JSON float.
    Float(f64),
    /// JSON boolean.
    Bool(bool),
}

/// Export options (upstream `make_config` / `write_config` keyword arguments).
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// Capture device; `None` selects a file placeholder.
    pub capture: Option<Device>,
    /// Playback device; `None` selects a file placeholder.
    pub playback: Option<Device>,
    /// CamillaDSP chunksize.
    pub chunksize: u32,
    /// ASCII head-comment note (non-ASCII characters are dropped).
    pub note: String,
    /// MIMO upper frequency; retained for the M3 path.
    pub fsiso: f64,
    /// LFE gain, or `None` to omit the LFE input.
    pub lfe_gain_db: Option<f64>,
    /// LFE low-pass frequency.
    pub lfe_lp_hz: f64,
    /// Filter latency shown in instructions.
    pub latency_ms: Option<f64>,
    /// Mode shown in title and instructions.
    pub mode: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            capture: None,
            playback: None,
            chunksize: 1024,
            note: String::new(),
            fsiso: 150.0,
            lfe_gain_db: Some(10.0),
            lfe_lp_hz: 120.0,
            latency_ms: None,
            mode: String::new(),
        }
    }
}

/// Configuration text and coefficient files, with no filesystem effects yet.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Complete ASCII YAML.
    pub yaml: String,
    /// Coefficient paths and complete WAV bytes.
    pub coeffs: Vec<(PathBuf, Vec<u8>)>,
}

/// Labels needed by the calibration-level export wrapper.
#[derive(Debug, Clone, PartialEq)]
pub struct RunLabels {
    /// Correction mode.
    pub mode: String,
    /// Ultra-mode latency budget.
    pub ultra_ms: f64,
    /// Total filter latency.
    pub latency_ms: f64,
    /// Room-correction upper bound.
    pub rc_hi_hz: f64,
}

/// CamillaDSP export failure.
#[derive(Debug, thiserror::Error)]
pub enum CamillaError {
    /// A role or another structural name cannot be represented safely as YAML keys and filenames.
    #[error("non-ASCII export identifier: {0}")]
    NonAscii(String),
    /// A numeric signal-chain or option value is NaN or infinite.
    #[error("non-finite export value: {0}")]
    NonFinite(String),
    /// The signal chain violates the writer contract.
    #[error("invalid signal chain: {0}")]
    Invalid(String),
    /// Filesystem operation failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
enum Json {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

fn object<const N: usize>(entries: [(&str, Json); N]) -> Json {
    Json::Object(
        entries
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}

fn int(value: usize) -> Json {
    Json::Int(i64::try_from(value).expect("channel counts fit i64"))
}

#[cfg(test)]
fn json_string(value: &str) -> String {
    let mut out = String::new();
    json_string_into(value, &mut out);
    out
}

fn json_string_into(value: &str, out: &mut String) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0'..='\u{1f}' => write!(out, "\\u{:04x}", ch as u32).expect("String write"),
            ch if ch.is_ascii() => out.push(ch),
            ch if (ch as u32) <= 0xffff => {
                write!(out, "\\u{:04x}", ch as u32).expect("String write")
            }
            ch => {
                let code = ch as u32 - 0x1_0000;
                write!(
                    out,
                    "\\u{:04x}\\u{:04x}",
                    0xd800 + (code >> 10),
                    0xdc00 + (code & 0x3ff)
                )
                .expect("String write");
            }
        }
    }
    out.push('"');
}

fn json(value: &Json) -> String {
    let mut out = String::new();
    json_into(value, &mut out);
    out
}

fn json_into(value: &Json, out: &mut String) {
    match value {
        Json::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        Json::Int(value) => write!(out, "{value}").expect("String write"),
        Json::Float(value) => out.push_str(&repr_f64(*value)),
        Json::Str(value) => json_string_into(value, out),
        Json::Array(values) => {
            out.push('[');
            for (i, value) in values.iter().enumerate() {
                if i != 0 {
                    out.push_str(", ");
                }
                json_into(value, out);
            }
            out.push(']');
        }
        Json::Object(values) => {
            out.push('{');
            for (i, (key, value)) in values.iter().enumerate() {
                if i != 0 {
                    out.push_str(", ");
                }
                json_string_into(key, out);
                out.push_str(": ");
                json_into(value, out);
            }
            out.push('}');
        }
    }
}

fn device_json(device: &Device) -> Json {
    Json::Object(
        device
            .0
            .iter()
            .map(|(key, value)| {
                let value = match value {
                    DeviceValue::Str(value) => Json::Str(value.clone()),
                    DeviceValue::Int(value) => Json::Int(*value),
                    DeviceValue::Float(value) => Json::Float(*value),
                    DeviceValue::Bool(value) => Json::Bool(*value),
                };
                (key.clone(), value)
            })
            .collect(),
    )
}

fn one(dest: usize, channel: usize, inverted: bool) -> Json {
    object([
        ("dest", int(dest)),
        (
            "sources",
            Json::Array(vec![object([
                ("channel", int(channel)),
                ("gain", Json::Int(0)),
                ("inverted", Json::Bool(inverted)),
            ])]),
        ),
    ])
}

fn channels(input: usize, output: usize) -> Json {
    object([("in", int(input)), ("out", int(output))])
}

fn filter_spec(kind: &str, parameters: Json) -> Json {
    object([("type", Json::Str(kind.into())), ("parameters", parameters)])
}

fn peaking(frequency: f64, q: f64, gain: f64) -> Json {
    filter_spec(
        "Biquad",
        object([
            ("type", Json::Str("Peaking".into())),
            ("freq", Json::Float(round_ndigits(frequency, 4))),
            ("q", Json::Float(round_ndigits(q, 5))),
            ("gain", Json::Float(round_ndigits(gain, 4))),
        ]),
    )
}

fn combo(kind: &str, frequency: f64, order: i64) -> Json {
    filter_spec(
        "BiquadCombo",
        object([
            ("type", Json::Str(kind.into())),
            ("freq", Json::Float(frequency)),
            ("order", Json::Int(order)),
        ]),
    )
}

fn delay(milliseconds: f64) -> Json {
    filter_spec(
        "Delay",
        object([
            (
                "delay",
                Json::Int(round_ties_even_i64(
                    milliseconds * f64::from(SAMPLE_RATE) / 1000.0,
                )),
            ),
            ("unit", Json::Str("samples".into())),
            ("subsample", Json::Bool(false)),
        ]),
    )
}

fn round_ties_even_i64(value: f64) -> i64 {
    let floor = value.floor();
    let fraction = value - floor;
    let rounded = if fraction < 0.5 {
        floor
    } else if fraction > 0.5 {
        floor + 1.0
    } else if (floor as i64) & 1 == 0 {
        floor
    } else {
        floor + 1.0
    };
    rounded as i64
}

fn wav(data: &[f64], rate: u32) -> Vec<u8> {
    let data_len = u32::try_from(data.len() * 4).expect("FIR WAV fits u32");
    let mut out = Vec::with_capacity(44 + data.len() * 4);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&3_u16.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4_u16.to_le_bytes());
    out.extend_from_slice(&32_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for value in data {
        out.extend_from_slice(&(*value as f32).to_le_bytes());
    }
    out
}

fn absolute(path: &Path) -> Result<PathBuf, CamillaError> {
    if path.is_absolute() {
        Ok(normalize(path))
    } else {
        Ok(normalize(&std::env::current_dir()?.join(path)))
    }
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn posix(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn relative(path: &Path, start: &Path) -> Result<PathBuf, CamillaError> {
    let path = absolute(path)?;
    let start = absolute(start)?;
    let a: Vec<_> = path.components().collect();
    let b: Vec<_> = start.components().collect();
    let common = a
        .iter()
        .zip(&b)
        .take_while(|(left, right)| left == right)
        .count();
    let mut out = PathBuf::new();
    for _ in common..b.len() {
        out.push("..");
    }
    for component in &a[common..] {
        out.push(component.as_os_str());
    }
    Ok(out)
}

fn file_devices(out: &Path, n_in: usize, n_out: usize) -> Result<(Device, Device), CamillaError> {
    let input = posix(&absolute(&out.join("in.raw"))?);
    let output = posix(&absolute(&out.join("out.raw"))?);
    Ok((
        Device(vec![
            ("type".into(), DeviceValue::Str("RawFile".into())),
            ("filename".into(), DeviceValue::Str(input)),
            ("format".into(), DeviceValue::Str("F32_LE".into())),
            ("channels".into(), DeviceValue::Int(n_in as i64)),
            ("extra_samples".into(), DeviceValue::Int(96_000)),
        ]),
        Device(vec![
            ("type".into(), DeviceValue::Str("File".into())),
            ("filename".into(), DeviceValue::Str(output)),
            ("format".into(), DeviceValue::Str("F32_LE".into())),
            ("channels".into(), DeviceValue::Int(n_out as i64)),
        ]),
    ))
}

fn check_ascii_identifier(name: &str) -> Result<(), CamillaError> {
    if !name.is_ascii() {
        return Err(CamillaError::NonAscii(name.into()));
    }
    if name.is_empty() || name.contains(['\n', '\r', ':']) {
        return Err(CamillaError::Invalid(format!("unsafe identifier {name:?}")));
    }
    Ok(())
}

fn finite(name: &str, value: f64) -> Result<(), CamillaError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(CamillaError::NonFinite(name.into()))
    }
}

fn validate_peq(prefix: &str, bands: &[Peq]) -> Result<(), CamillaError> {
    for (index, (frequency, q, gain)) in bands.iter().enumerate() {
        finite(&format!("{prefix}[{index}].frequency"), *frequency)?;
        finite(&format!("{prefix}[{index}].q"), *q)?;
        finite(&format!("{prefix}[{index}].gain"), *gain)?;
    }
    Ok(())
}

fn validate(chain: &SignalChain, options: &Options) -> Result<(), CamillaError> {
    if chain.sample_rate != SAMPLE_RATE {
        return Err(CamillaError::Invalid(format!(
            "sample rate {} (expected {SAMPLE_RATE})",
            chain.sample_rate
        )));
    }
    finite("options.fsiso", options.fsiso)?;
    finite("options.lfe_lp_hz", options.lfe_lp_hz)?;
    if !options.mode.is_ascii() {
        return Err(CamillaError::NonAscii(options.mode.clone()));
    }
    if options.mode.contains(['\n', '\r']) {
        return Err(CamillaError::Invalid(format!(
            "unsafe mode identifier {:?}",
            options.mode
        )));
    }
    if let Some(value) = options.lfe_gain_db {
        finite("options.lfe_gain_db", value)?;
    }
    if let Some(value) = options.latency_ms {
        finite("options.latency_ms", value)?;
    }
    for device in [&options.capture, &options.playback].into_iter().flatten() {
        for (key, value) in &device.0 {
            if !key.is_ascii() {
                return Err(CamillaError::NonAscii(key.clone()));
            }
            if let DeviceValue::Float(number) = value {
                finite(&format!("device.{key}"), *number)?;
            }
        }
    }
    finite("chain.main_extra_ms", chain.main_extra_ms)?;
    finite("chain.lift_ms", chain.lift_ms)?;
    finite("chain.latency_ms", chain.latency_ms)?;
    for MainChannel {
        role,
        input_peq,
        fir,
        gain_db,
        delay_ms,
        xo_hz,
        top_peq,
        align_delay_ms,
    } in &chain.mains
    {
        check_ascii_identifier(role)?;
        validate_peq(&format!("main.{role}.input_peq"), input_peq)?;
        validate_peq(&format!("main.{role}.top_peq"), top_peq)?;
        finite(&format!("main.{role}.gain_db"), *gain_db)?;
        finite(&format!("main.{role}.delay_ms"), *delay_ms)?;
        finite(&format!("main.{role}.align_delay_ms"), *align_delay_ms)?;
        if let Some(value) = xo_hz {
            finite(&format!("main.{role}.xo_hz"), *value)?;
        }
        for (index, value) in fir.iter().enumerate() {
            finite(&format!("main.{role}.fir[{index}]"), *value)?;
        }
    }
    for SubChannel {
        role,
        gain_db,
        delay_ms,
        polarity,
        allpass,
        hp_hz,
        lp_hz,
        peq,
        mode,
        ..
    } in &chain.subs
    {
        check_ascii_identifier(role)?;
        finite(&format!("sub.{role}.gain_db"), *gain_db)?;
        finite(&format!("sub.{role}.delay_ms"), *delay_ms)?;
        finite(&format!("sub.{role}.hp_hz"), *hp_hz)?;
        finite(&format!("sub.{role}.lp_hz"), *lp_hz)?;
        if !matches!(polarity, -1 | 1) {
            return Err(CamillaError::Invalid(format!(
                "sub {role} polarity {polarity}"
            )));
        }
        if !mode.is_ascii() {
            return Err(CamillaError::NonAscii(mode.clone()));
        }
        for (index, (frequency, q)) in allpass.iter().enumerate() {
            finite(
                &format!("sub.{role}.allpass[{index}].frequency"),
                *frequency,
            )?;
            finite(&format!("sub.{role}.allpass[{index}].q"), *q)?;
        }
        validate_peq(&format!("sub.{role}.peq"), peq)?;
    }
    validate_peq("sub_bus_peq", &chain.sub_bus_peq)
}

fn band_name(kind: BandType, highpass: bool) -> (&'static str, i64) {
    let prefix = match kind {
        BandType::Lr12 | BandType::Lr24 | BandType::Lr48 => "LinkwitzRiley",
        BandType::Bw12 | BandType::Bw24 => "Butterworth",
    };
    let order = match kind {
        BandType::Lr12 | BandType::Bw12 => 2,
        BandType::Lr24 | BandType::Bw24 => 4,
        BandType::Lr48 => 8,
    };
    (
        if highpass {
            if prefix == "LinkwitzRiley" {
                "LinkwitzRileyHighpass"
            } else {
                "ButterworthHighpass"
            }
        } else if prefix == "LinkwitzRiley" {
            "LinkwitzRileyLowpass"
        } else {
            "ButterworthLowpass"
        },
        order,
    )
}

fn fixed_filters(role: &str, sub: &SubChannel, filters: &mut Vec<(String, Json)>) -> Vec<String> {
    let mut names = Vec::new();
    if sub.hp_hz != 0.0 {
        let name = format!("sub_{role}_fixed_hp");
        let (kind, order) = band_name(sub.hp_type, true);
        filters.push((name.clone(), combo(kind, sub.hp_hz, order)));
        names.push(name);
    }
    if sub.lp_hz != 0.0 {
        let name = format!("sub_{role}_fixed_lp");
        let (kind, order) = band_name(sub.lp_type, false);
        filters.push((name.clone(), combo(kind, sub.lp_hz, order)));
        names.push(name);
    }
    for (index, (frequency, q, gain)) in sub.peq.iter().enumerate() {
        let name = format!("sub_{role}_fixed_peq{index}");
        filters.push((name.clone(), peaking(*frequency, *q, *gain)));
        names.push(name);
    }
    names
}

fn pipeline_mixer(name: &str) -> Json {
    object([
        ("type", Json::Str("Mixer".into())),
        ("name", Json::Str(name.into())),
    ])
}

fn pipeline_filter(channel: usize, names: Vec<String>) -> Json {
    object([
        ("type", Json::Str("Filter".into())),
        ("channels", Json::Array(vec![int(channel)])),
        (
            "names",
            Json::Array(names.into_iter().map(Json::Str).collect()),
        ),
    ])
}

fn order_table(inputs: &[String], outputs: &[String]) -> Vec<String> {
    let mut rows =
        vec!["#    ch  input (capture -> route_in)   output (route_out -> playback)".into()];
    for index in 0..inputs.len().max(outputs.len()) {
        let input = inputs.get(index).map_or("-", String::as_str);
        let output = outputs.get(index).map_or("-", String::as_str);
        rows.push(format!(
            "#   {}  {}  {output}",
            format_i3(index),
            format_s30(input)
        ));
    }
    rows
}

fn howto_lines(
    inputs: &[String],
    outputs: &[String],
    latency_ms: Option<f64>,
    mode: &str,
) -> Vec<String> {
    let mut lines = vec![
        "# ---- HOW TO USE (CamillaDSP 4.1.x + CamillaGUI 4.1) ----".into(),
        "# 1) Files: this file goes to camilladsp/configs, the wav files to camilladsp/coeffs".into(),
        "#    (same layout as this export). CamillaGUI: Files tab, upload the wav files under Filters FIRST,".into(),
        "#    then this file under Configs. FIR paths are relative to this file (../coeffs/...).".into(),
        "# 2) Devices: capture/playback below are FILE placeholders (in.raw / out.raw) - change them:".into(),
        "#    capture = loopback (Windows: Wasapi \"CABLE Output (VB-Audio Virtual Cable)\", Mac: CoreAudio".into(),
        "#    \"BlackHole 16ch\"), playback = your audio interface. format: empty or F32 (F32_LE is for files only).".into(),
        "#    samplerate 48000 (Windows: set both devices' default format to 48000 Hz too).".into(),
        "#    Different clocks (loopback -> interface): enable_rate_adjust; Windows also resampler AsyncSinc.".into(),
        "#    Capture channels = route_in \"in\", playback channels = route_out \"out\".".into(),
        "# 3) Channel numbers start at 0 (interface output 1 = 0). Default routing is 1:1:".into(),
    ];
    lines.extend(order_table(inputs, outputs));
    lines.extend([
        "#    To move a channel change only the Mixers route_in (source \"channel\") or route_out (\"dest\").".into(),
        "#    More interface outputs: raise route_out \"out\" and the playback channels to the same number.".into(),
        "#    Players send LFE 4th (Windows 7.1: FL FR FC LFE BL BR SL SR) - reorder them in route_in.".into(),
    ]);
    if let Some(milliseconds) = latency_ms {
        lines.push(format!(
            "# 4) Latency: filters {} ms{} + CamillaDSP buffering (grows with chunksize).",
            format_fixed(milliseconds, 1, false),
            if mode.is_empty() {
                String::new()
            } else {
                format!(" (mode {mode})")
            }
        ));
        lines.push(format!(
            "#    Video: set the player audio delay to about -{} ms (plus the buffering) to keep lip sync.",
            format_fixed(milliseconds, 0, false)
        ));
    }
    lines
}

/// Build a CamillaDSP v4 configuration without writing anything.
pub fn make_config(
    chain: &SignalChain,
    out: &Path,
    config_dir: &Path,
    fir_prefix: &str,
    options: &Options,
) -> Result<Config, CamillaError> {
    make_config_observed(chain, out, config_dir, fir_prefix, options, &|_| {})
}

/// Audit-only stage boundaries; callbacks run after construction and serialization.
#[doc(hidden)]
pub fn make_config_observed(
    chain: &SignalChain,
    out: &Path,
    config_dir: &Path,
    fir_prefix: &str,
    options: &Options,
    boundary: &dyn Fn(&str),
) -> Result<Config, CamillaError> {
    validate(chain, options)?;
    let mains = &chain.mains;
    let subs = &chain.subs;
    let lfe = !subs.is_empty() && options.lfe_gain_db.is_some();
    let n_main = mains.len();
    let n_sub = subs.len();
    let n_in = n_main + usize::from(lfe);
    let n_out = n_main + n_sub;
    let (default_capture, default_playback) = file_devices(out, n_in, n_out)?;
    let capture = options.capture.as_ref().unwrap_or(&default_capture);
    let playback = options.playback.as_ref().unwrap_or(&default_playback);

    let mut filters: Vec<(String, Json)> = Vec::new();
    let mut pipeline = vec![pipeline_mixer("route_in")];
    let mut coeffs = Vec::new();
    let coeff_dir = out.join(COEFF_DIR);

    for (channel, main) in mains.iter().enumerate() {
        let mut names = Vec::new();
        for (index, (frequency, q, gain)) in main.input_peq.iter().enumerate() {
            let name = format!("in_{}_peq{index}", main.role);
            filters.push((name.clone(), peaking(*frequency, *q, *gain)));
            names.push(name);
        }
        if main.fir.len() > 1 {
            let file = coeff_dir.join(format!("{fir_prefix}{}_fir.wav", main.role));
            let filename = posix(&relative(&file, config_dir)?);
            coeffs.push((file, wav(&main.fir, SAMPLE_RATE)));
            let name = format!("in_{}_fir", main.role);
            filters.push((
                name.clone(),
                filter_spec(
                    "Conv",
                    object([
                        ("type", Json::Str("Wav".into())),
                        ("filename", Json::Str(filename)),
                    ]),
                ),
            ));
            names.push(name);
        }
        if !names.is_empty() {
            pipeline.push(pipeline_filter(channel, names));
        }
    }
    if let Some(gain) = options.lfe_gain_db.filter(|_| lfe) {
        filters.push((
            "lfe_gain".into(),
            filter_spec(
                "Gain",
                object([
                    ("gain", Json::Float(round_ndigits(gain, 4))),
                    ("inverted", Json::Bool(false)),
                ]),
            ),
        ));
        filters.push((
            "lfe_lp".into(),
            combo("LinkwitzRileyLowpass", options.lfe_lp_hz, 4),
        ));
        pipeline.push(pipeline_filter(
            n_main,
            vec!["lfe_gain".into(), "lfe_lp".into()],
        ));
    }

    let split = object([
        ("channels", channels(n_in, 2 * n_in)),
        (
            "mapping",
            Json::Array(
                (0..n_in)
                    .map(|k| one(k, k, false))
                    .chain((0..n_in).map(|k| one(n_in + k, k, false)))
                    .collect(),
            ),
        ),
    ]);
    pipeline.push(pipeline_mixer("split"));
    for (channel, main) in mains.iter().enumerate() {
        let mut names = Vec::new();
        let gain_name = format!("main_{}_gain", main.role);
        filters.push((
            gain_name.clone(),
            filter_spec(
                "Gain",
                object([
                    ("gain", Json::Float(round_ndigits(main.gain_db, 4))),
                    ("inverted", Json::Bool(false)),
                ]),
            ),
        ));
        names.push(gain_name);
        for (index, (frequency, q, gain)) in main.top_peq.iter().enumerate() {
            let name = format!("main_{}_xopeq{index}", main.role);
            filters.push((name.clone(), peaking(*frequency, *q, *gain)));
            names.push(name);
        }
        if let Some(xo) = main.xo_hz.filter(|value| *value != 0.0) {
            let high = format!("main_{}_hp", main.role);
            filters.push((high.clone(), combo("LinkwitzRileyHighpass", xo, 4)));
            names.push(high);
            let low = format!("send_{}_lp", main.role);
            filters.push((low.clone(), combo("LinkwitzRileyLowpass", xo, 4)));
            pipeline.push(pipeline_filter(n_in + channel, vec![low]));
        }
        let delay_name = format!("main_{}_delay", main.role);
        filters.push((delay_name.clone(), delay(main.delay_ms)));
        names.push(delay_name);
        pipeline.push(pipeline_filter(channel, names));
    }

    let mut bus_mapping: Vec<Json> = (0..n_main).map(|k| one(k, k, false)).collect();
    for (sub_index, sub) in subs.iter().enumerate() {
        let sources = (0..n_in)
            .filter(|index| {
                *index == n_main
                    || mains
                        .get(*index)
                        .and_then(|main| main.xo_hz)
                        .is_some_and(|value| value != 0.0)
            })
            .map(|index| {
                object([
                    ("channel", int(n_in + index)),
                    ("gain", Json::Float(round_ndigits(sub.gain_db, 4))),
                    ("inverted", Json::Bool(sub.polarity < 0)),
                ])
            })
            .collect::<Vec<_>>();
        if !sources.is_empty() {
            bus_mapping.push(object([
                ("dest", int(n_main + sub_index)),
                ("sources", Json::Array(sources)),
            ]));
        }
    }
    let bus = object([
        ("channels", channels(2 * n_in, n_out)),
        ("mapping", Json::Array(bus_mapping)),
    ]);
    pipeline.push(pipeline_mixer("bus"));
    for (sub_index, sub) in subs.iter().enumerate() {
        let mut names = Vec::new();
        for (index, (frequency, q, gain)) in chain.sub_bus_peq.iter().enumerate() {
            let name = format!("sub_{}_peq{index}", sub.role);
            filters.push((name.clone(), peaking(*frequency, *q, *gain)));
            names.push(name);
        }
        names.extend(fixed_filters(&sub.role, sub, &mut filters));
        for (index, (frequency, q)) in sub.allpass.iter().enumerate() {
            let name = format!("sub_{}_ap{index}", sub.role);
            filters.push((
                name.clone(),
                filter_spec(
                    "Biquad",
                    object([
                        ("type", Json::Str("Allpass".into())),
                        ("freq", Json::Float(round_ndigits(*frequency, 4))),
                        ("q", Json::Float(round_ndigits(*q, 5))),
                    ]),
                ),
            ));
            names.push(name);
        }
        let name = format!("sub_{}_delay", sub.role);
        filters.push((name.clone(), delay(sub.delay_ms)));
        names.push(name);
        pipeline.push(pipeline_filter(n_main + sub_index, names));
    }
    pipeline.push(pipeline_mixer("route_out"));

    let route_in = object([
        (
            "description",
            Json::Str(
                "capture channel -> SECS input (edit source channel to move an input)".into(),
            ),
        ),
        ("channels", channels(n_in, n_in)),
        (
            "mapping",
            Json::Array((0..n_in).map(|k| one(k, k, false)).collect()),
        ),
    ]);
    let route_out = object([
        (
            "description",
            Json::Str("SECS output -> playback channel (edit dest to move an output)".into()),
        ),
        ("channels", channels(n_out, n_out)),
        (
            "mapping",
            Json::Array((0..n_out).map(|k| one(k, k, false)).collect()),
        ),
    ]);

    let mut inputs: Vec<String> = mains.iter().map(|main| main.role.clone()).collect();
    if lfe {
        inputs.push("LFE".into());
    }
    let outputs: Vec<String> = mains
        .iter()
        .map(|main| main.role.clone())
        .chain(subs.iter().map(|sub| sub.role.clone()))
        .collect();
    let mut lines = vec![
        "# SECS 2 calibration -> CamillaDSP v4 (generated)".into(),
        format!(
            "# inputs {n_in}: {} / outputs {n_out}: {}",
            inputs.join(", "),
            outputs.join(", ")
        ),
    ];
    if !options.note.is_empty() {
        lines.push(format!(
            "# {}",
            options
                .note
                .chars()
                .filter(char::is_ascii)
                .collect::<String>()
        ));
    }
    if let Some(gain) = options.lfe_gain_db.filter(|_| lfe) {
        lines.push(format!(
            "# LFE: gain {} dB, LR24 lowpass {} Hz -> every sub bus (same sub gain/polarity/filters/delay as the bass-managed mains), no MIMO",
            format_g(gain, true),
            format_g(options.lfe_lp_hz, false)
        ));
    }
    for sub in subs {
        let allpass = sub
            .allpass
            .iter()
            .map(|(frequency, q)| {
                format!(
                    ", allpass {} Hz Q {}",
                    format_fixed(*frequency, 1, false),
                    format_fixed(*q, 3, false)
                )
            })
            .collect::<String>();
        let mut fixed = String::new();
        if sub.hp_hz != 0.0 {
            write!(
                fixed,
                ", fixed HP {} Hz {}",
                format_g(sub.hp_hz, false),
                sub.hp_type.name()
            )
            .expect("String write");
        }
        if sub.lp_hz != 0.0 {
            write!(
                fixed,
                ", fixed LP {} Hz {}",
                format_g(sub.lp_hz, false),
                sub.lp_type.name()
            )
            .expect("String write");
        }
        for (frequency, q, gain) in &sub.peq {
            write!(
                fixed,
                ", fixed PEQ {} Hz Q {} {} dB",
                format_fixed(*frequency, 1, false),
                format_fixed(*q, 3, false),
                format_fixed(*gain, 2, true)
            )
            .expect("String write");
        }
        lines.push(format!(
            "# sub {}: {} gain {} dB, delay {} ms, polarity {}{allpass}{fixed}",
            sub.role,
            sub.mode,
            format_fixed(sub.gain_db, 3, true),
            format_fixed(sub.delay_ms, 3, false),
            if sub.polarity > 0 {
                "normal"
            } else {
                "inverted"
            }
        ));
    }
    lines.push("#".into());
    lines.extend(howto_lines(
        &inputs,
        &outputs,
        options.latency_ms,
        &options.mode,
    ));

    let title = format!(
        "SECS 2{}{}",
        if options.mode.is_empty() {
            String::new()
        } else {
            format!(" {}", options.mode)
        },
        options.latency_ms.map_or_else(String::new, |value| format!(
            " ({} ms)",
            format_fixed(value, 1, false)
        ))
    );
    let description = format!(
        "inputs {} | outputs {} | numbers start at 0, change routing in Mixers route_in / route_out | devices are file placeholders: set capture = loopback, playback = interface",
        inputs
            .iter()
            .enumerate()
            .map(|(index, role)| format!("{index} {role}"))
            .collect::<Vec<_>>()
            .join(", "),
        outputs
            .iter()
            .enumerate()
            .map(|(index, role)| format!("{index} {role}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    boundary("build");
    lines.extend([
        format!("title: {}", json(&Json::Str(title))),
        format!("description: {}", json(&Json::Str(description))),
        "devices:".into(),
        format!("  samplerate: {SAMPLE_RATE}"),
        format!("  chunksize: {}", options.chunksize),
        format!("  capture: {}", json(&device_json(capture))),
        format!("  playback: {}", json(&device_json(playback))),
        "filters:".into(),
    ]);
    for (name, spec) in filters {
        lines.push(format!("  {name}: {}", json(&spec)));
    }
    lines.extend([
        "mixers:".into(),
        format!("  route_in: {}", json(&route_in)),
        format!("  route_out: {}", json(&route_out)),
        format!("  split: {}", json(&split)),
        format!("  bus: {}", json(&bus)),
        "pipeline:".into(),
    ]);
    for stage in pipeline {
        lines.push(format!("  - {}", json(&stage)));
    }
    let config = Config {
        yaml: lines.join("\n") + "\n",
        coeffs,
    };
    boundary("serialize");
    Ok(config)
}

fn safe_name(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii() && (ch.is_ascii_alphanumeric() || "-_.".contains(ch)) {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if safe.is_empty() {
        "secs_calibration".into()
    } else {
        safe
    }
}

/// Write `configs/<name>.yml`, coefficient WAVs, and a placeholder `in.raw` when needed.
pub fn write_config(
    chain: &SignalChain,
    out: &Path,
    name: &str,
    options: &Options,
) -> Result<Vec<PathBuf>, CamillaError> {
    let safe = safe_name(name);
    if safe == "." || safe == ".." {
        return Err(CamillaError::Invalid(format!(
            "unsafe output name {name:?}"
        )));
    }
    let config_dir = out.join(CONFIG_DIR);
    let config = make_config(chain, out, &config_dir, &format!("{safe}_"), options)?;
    let n_in =
        chain.mains.len() + usize::from(!chain.subs.is_empty() && options.lfe_gain_db.is_some());

    // All validation and pure construction have completed before the first write.
    fs::create_dir_all(out)?;
    if options.capture.is_none() {
        let input = out.join("in.raw");
        if !input.exists() {
            fs::write(&input, vec![0_u8; 1024 * n_in * 4])?;
        }
    }
    for (path, bytes) in &config.coeffs {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bytes)?;
    }
    fs::create_dir_all(&config_dir)?;
    let config_path = config_dir.join(format!("{safe}.yml"));
    fs::write(&config_path, config.yaml.as_bytes())?;
    let mut written = vec![config_path];
    let mut coeff_paths: Vec<_> = config.coeffs.into_iter().map(|(path, _)| path).collect();
    coeff_paths.sort();
    written.extend(coeff_paths);
    Ok(written)
}

/// Calibration-level export wrapper, without MIMO.
pub fn export(
    chain: &SignalChain,
    folder: &Path,
    labels: &RunLabels,
) -> Result<Vec<PathBuf>, CamillaError> {
    finite("labels.ultra_ms", labels.ultra_ms)?;
    finite("labels.latency_ms", labels.latency_ms)?;
    finite("labels.rc_hi_hz", labels.rc_hi_hz)?;
    let note = format!(
        "mode {}, total latency {} ms, RC upper {} Hz; no MIMO",
        labels.mode,
        format_fixed(labels.latency_ms, 1, false),
        format_fixed(labels.rc_hi_hz, 0, false)
    );
    let mode = if labels.mode == "ultra" {
        format!("ultra{}", format_g(labels.ultra_ms, false))
    } else {
        labels.mode.clone()
    };
    let options = Options {
        note,
        latency_ms: Some(labels.latency_ms),
        mode: mode.clone(),
        ..Options::default()
    };
    write_config(chain, folder, &format!("secs_{mode}"), &options)
}

#[cfg(test)]
mod perf_tests {
    use super::*;

    #[test]
    fn streaming_json_matches_recursive_bytes() {
        fn old(value: &Json) -> String {
            match value {
                Json::Bool(v) => v.to_string(),
                Json::Int(v) => v.to_string(),
                Json::Float(v) => repr_f64(*v),
                Json::Str(v) => json_string(v),
                Json::Array(v) => format!("[{}]", v.iter().map(old).collect::<Vec<_>>().join(", ")),
                Json::Object(v) => format!(
                    "{{{}}}",
                    v.iter()
                        .map(|(k, v)| format!("{}: {}", json_string(k), old(v)))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }
        }
        let values = [
            Json::Array(vec![]),
            Json::Object(vec![]),
            object([
                ("quote\"\\\n", Json::Str("한글\t\u{0}😀".into())),
                (
                    "values",
                    Json::Array(vec![
                        Json::Bool(true),
                        Json::Bool(false),
                        Json::Int(i64::MIN),
                        Json::Float(-0.0),
                        Json::Float(1e-9),
                        object([("nested", Json::Array(vec![Json::Int(42)]))]),
                    ]),
                ),
            ]),
        ];
        for value in values {
            assert_eq!(json(&value).as_bytes(), old(&value).as_bytes());
        }
    }
}
