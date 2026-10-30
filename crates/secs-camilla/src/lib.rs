#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! CamillaDSP v4 export.
//!
//! Upstream source: `export_camilladsp.py`.
//!
//! Reads one signal-chain value (not engine internals) through its export
//! graph and writes `configs/*.yml` and `coeffs/*.wav` in the CamillaGUI
//! folder layout, with Python's float formatting and half-to-even delay
//! rounding, ASCII-only (the CamillaGUI server reads CP949). Building the
//! [`ExportGraph`] validates the chain and its MIMO paths; the writer checks
//! only its own options. Every check runs before the first file is written.

pub mod note;
pub mod pyfmt;

use pyfmt::{format_fixed, format_g, format_i3, format_s30, repr_f64, round_ndigits};
use secs_graph::{ExportGraph, GraphError, LfeChoice};
use secs_types::chain::{BandType, MimoFilter, MimoPath, MimoPaths, SignalChain, SubChannel};
use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};

// The export graph owns the rate, the LFE defaults, the delay rounding and
// the file helpers every writer shares; they stay reachable here under their
// old names.
pub use secs_graph::files::{safe_name, wav_bytes, write_bundle_observed};
pub use secs_graph::{LFE_GAIN_DB, LFE_LP_HZ, SAMPLE_RATE, is_negative_delay, round_ties_even_i64};

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

/// Design identity shown by CamillaDSP controllers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignLabels {
    /// Open project name, omitted when unavailable.
    pub project_name: Option<String>,
    /// User-facing design name.
    pub design_name: String,
    /// Full lowercase SHA-256 design identity.
    pub design_id: String,
    /// Whether this is the correction-free companion configuration.
    pub bypass: bool,
}

/// Export options (upstream `make_config` / `write_config` keyword arguments).
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// Capture device; `None` selects a file placeholder.
    pub capture: Option<Device>,
    /// Playback device; `None` selects a file placeholder.
    pub playback: Option<Device>,
    /// Output sample rate in Hz.
    pub output_rate: u32,
    /// CamillaDSP chunksize.
    pub chunksize: u32,
    /// ASCII head-comment note (non-ASCII characters are dropped).
    pub note: String,
    /// LFE gain, or `None` to omit the LFE input.
    pub lfe_gain_db: Option<f64>,
    /// LFE low-pass frequency.
    pub lfe_lp_hz: f64,
    /// Filter latency shown in instructions.
    pub latency_ms: Option<f64>,
    /// Mode shown in title and instructions.
    pub mode: String,
    /// Design identity and controller-facing names. Absent preserves upstream bytes.
    pub design: Option<DesignLabels>,
    /// Add a stable rounded headroom gain as the first pipeline step.
    pub headroom: bool,
    /// Add capture-channel role labels.
    pub labels: bool,
    /// Write the correction-free companion configuration.
    pub bypass: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            capture: None,
            playback: None,
            output_rate: SAMPLE_RATE,
            chunksize: 1024,
            note: String::new(),
            lfe_gain_db: Some(10.0),
            lfe_lp_hz: 120.0,
            latency_ms: None,
            mode: String::new(),
            design: None,
            headroom: false,
            labels: false,
            bypass: false,
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
    /// Mode name ([`note::mode_name`]), also the configuration's file name.
    pub mode: String,
    /// ASCII header note ([`note::camilla_note`]).
    pub note: String,
    /// Total filter latency.
    pub latency_ms: f64,
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

impl From<GraphError> for CamillaError {
    fn from(error: GraphError) -> Self {
        match error {
            GraphError::NonAscii(detail) => Self::NonAscii(detail),
            GraphError::NonFinite(detail) => Self::NonFinite(detail),
            GraphError::Invalid(detail) => Self::Invalid(detail),
        }
    }
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
    device_json_with_labels(device, None)
}

fn device_json_with_labels(device: &Device, labels: Option<&[String]>) -> Json {
    let mut values = device
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
        .collect::<Vec<_>>();
    if let Some(labels) = labels {
        values.retain(|(key, _)| key != "labels");
        values.push((
            "labels".into(),
            Json::Array(labels.iter().cloned().map(Json::Str).collect()),
        ));
    }
    Json::Object(values)
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

fn delay(milliseconds: f64, rate: u32) -> Json {
    delay_samples(secs_dsp::rate::delay_samples(milliseconds, rate))
}

fn delay_samples(samples: i64) -> Json {
    filter_spec(
        "Delay",
        object([
            ("delay", Json::Int(samples)),
            ("unit", Json::Str("samples".into())),
            ("subsample", Json::Bool(false)),
        ]),
    )
}

/// [`wav_bytes`] for one exported FIR, naming its owner in a range error.
fn fir_wav(owner: &str, taps: &[f64], rate: u32) -> Result<Vec<u8>, CamillaError> {
    wav_bytes(taps, rate).map_err(|error| match error {
        GraphError::NonFinite(name) => CamillaError::NonFinite(format!("{owner}.{name}")),
        other => other.into(),
    })
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

/// The coefficient filename prefix (`secs_mixed_`), the writer's own identifier.
fn check_ascii_identifier(name: &str) -> Result<(), CamillaError> {
    if !name.is_ascii() {
        return Err(CamillaError::NonAscii(name.into()));
    }
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
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

/// The writer's own options; the chain and its MIMO paths are the export
/// graph's to check.
fn validate_options(options: &Options) -> Result<(), CamillaError> {
    if !secs_dsp::rate::is_output_rate(options.output_rate) {
        return Err(CamillaError::Invalid(
            "unsupported output sample rate".into(),
        ));
    }
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
    if let Some(design) = &options.design {
        if design.design_name.chars().any(char::is_control)
            || design
                .project_name
                .as_deref()
                .is_some_and(|name| name.chars().any(char::is_control))
        {
            return Err(CamillaError::Invalid(
                "control character in design labels".into(),
            ));
        }
        if design.design_id.len() != 64
            || !design
                .design_id
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(CamillaError::Invalid("invalid design id".into()));
        }
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
    Ok(())
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
    pipeline_mixer_meta(name, None)
}

fn pipeline_mixer_meta(name: &str, description: Option<&str>) -> Json {
    let mut fields = vec![
        ("type".into(), Json::Str("Mixer".into())),
        ("name".into(), Json::Str(name.into())),
    ];
    if let Some(description) = description {
        fields.push(("description".into(), Json::Str(description.into())));
    }
    Json::Object(fields)
}

fn pipeline_filter(channel: usize, names: Vec<String>) -> Json {
    pipeline_filter_meta(channel, names, None, None)
}

fn pipeline_filter_meta(
    channel: usize,
    names: Vec<String>,
    description: Option<&str>,
    bypassed: Option<bool>,
) -> Json {
    let mut fields = vec![
        ("type".into(), Json::Str("Filter".into())),
        ("channels".into(), Json::Array(vec![int(channel)])),
        (
            "names".into(),
            Json::Array(names.into_iter().map(Json::Str).collect()),
        ),
    ];
    if let Some(description) = description {
        fields.push(("description".into(), Json::Str(description.into())));
    }
    if let Some(bypassed) = bypassed {
        fields.push(("bypassed".into(), Json::Bool(bypassed)));
    }
    Json::Object(fields)
}

fn pipeline_all_filter(name: &str, description: &str, channels: usize) -> Json {
    Json::Object(vec![
        ("type".into(), Json::Str("Filter".into())),
        (
            "channels".into(),
            Json::Array((0..channels).map(int).collect()),
        ),
        ("names".into(), Json::Array(vec![Json::Str(name.into())])),
        ("description".into(), Json::Str(description.into())),
    ])
}

fn allpass(frequency: f64, q: f64) -> Json {
    filter_spec(
        "Biquad",
        object([
            ("type", Json::Str("Allpass".into())),
            ("freq", Json::Float(round_ndigits(frequency, 4))),
            ("q", Json::Float(round_ndigits(q, 5))),
        ]),
    )
}

fn path_tag(path: &MimoPath) -> String {
    format!("mimo_{}_{}", path.input, path.support)
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

fn device_is(device: &Device, kind: &str) -> bool {
    device.0.iter().any(|(key, value)| {
        key == "type" && matches!(value, DeviceValue::Str(value) if value == kind)
    })
}

fn howto_lines(
    inputs: &[String],
    outputs: &[String],
    latency_ms: Option<f64>,
    mode: &str,
    output_rate: u32,
    file_placeholders: bool,
    asio: bool,
) -> Vec<String> {
    let mut lines = vec![
        "# ---- HOW TO USE (CamillaDSP 4.1.x + CamillaGUI 4.1) ----".into(),
        "# 1) Files: this file goes to camilladsp/configs, the wav files to camilladsp/coeffs".into(),
        "#    (same layout as this export). CamillaGUI: Files tab, upload the wav files under Filters FIRST,".into(),
        "#    then this file under Configs. FIR paths are relative to this file (../coeffs/...).".into(),
    ];
    if file_placeholders {
        lines.extend([
            "# 2) Devices: capture/playback below are FILE placeholders (in.raw / out.raw) - change them:".into(),
            "#    capture = loopback (Windows: Wasapi \"CABLE Output (VB-Audio Virtual Cable)\", Mac: CoreAudio".into(),
            "#    \"BlackHole 16ch\"), playback = your audio interface. format: empty or F32 (F32_LE is for files only).".into(),
        ]);
    } else {
        lines.push(
            "# 2) Devices: capture/playback below are the devices selected for this export.".into(),
        );
    }
    lines.extend([
        format!("#    samplerate {output_rate} (Windows: set both devices' default format to {output_rate} Hz too)."),
        "#    Different clocks (loopback -> interface): enable_rate_adjust; Windows also resampler AsyncSinc.".into(),
        "#    Capture channels = route_in \"in\", playback channels = route_out \"out\".".into(),
    ]);
    if asio {
        lines.push("#    ASIO opens only in CamillaDSP's ASIO build (camilladsp-windows-asio-amd64.zip in its releases); the plain Windows build has no ASIO.".into());
    }
    lines.push(
        "# 3) Channel numbers start at 0 (interface output 1 = 0). Default routing is 1:1:".into(),
    );
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
    mimo: Option<&MimoPaths>,
) -> Result<Config, CamillaError> {
    make_config_observed(chain, out, config_dir, fir_prefix, options, mimo, &|_| {})
}

/// Audit-only stage boundaries; callbacks run after construction and serialization.
#[doc(hidden)]
pub fn make_config_observed(
    chain: &SignalChain,
    out: &Path,
    config_dir: &Path,
    fir_prefix: &str,
    options: &Options,
    mimo: Option<&MimoPaths>,
    boundary: &dyn Fn(&str),
) -> Result<Config, CamillaError> {
    validate_options(options)?;
    let lfe_choice = options
        .lfe_gain_db
        .map_or(LfeChoice::Omit, |gain_db| LfeChoice::Include {
            gain_db,
            lp_hz: options.lfe_lp_hz,
        });
    let graph = ExportGraph::build(chain, mimo, lfe_choice)?;
    if !fir_prefix.is_empty() {
        check_ascii_identifier(fir_prefix)?;
    }
    let chain = graph.chain();
    let mimo = graph.mimo();
    let mains = &chain.mains;
    let subs = &chain.subs;
    // Main roles, then `LFE` when the chain has subs and the options keep it.
    let inputs = &graph.plan().inputs;
    // Main roles, then sub roles.
    let outputs = &graph.plan().outputs;
    let n_main = mains.len();
    let n_sub = subs.len();
    let n_in = inputs.len();
    let lfe = n_in > n_main;
    let n_out = outputs.len();
    let paths = mimo.map_or(&[][..], |value| value.paths.as_slice());
    let n_paths = paths.len();
    let (default_capture, default_playback) = file_devices(out, n_in, n_out)?;
    let capture = options.capture.as_ref().unwrap_or(&default_capture);
    let playback = options.playback.as_ref().unwrap_or(&default_playback);

    let mut filters: Vec<(String, Json)> = Vec::new();
    let mut pipeline = Vec::new();
    if options.headroom {
        let headroom = (secs_graph::headroom_db(graph.plan())? * 2.0).ceil() / 2.0;
        filters.push((
            "secs_headroom".into(),
            filter_spec(
                "Gain",
                object([
                    ("gain", Json::Float(-headroom)),
                    ("inverted", Json::Bool(false)),
                ]),
            ),
        ));
        pipeline.push(pipeline_all_filter("secs_headroom", "secs:headroom", n_in));
    }
    pipeline.push(pipeline_mixer("route_in"));
    let mut coeffs = Vec::new();
    let coeff_dir = out.join(COEFF_DIR);
    let fork = if paths.is_empty() {
        None
    } else {
        let mapping = (0..n_in)
            .map(|channel| one(channel, channel, false))
            .chain(paths.iter().enumerate().map(|(index, path)| {
                let input = mains
                    .iter()
                    .position(|main| main.role == path.input)
                    .expect("validated MIMO input");
                one(n_in + index, input, false)
            }))
            .collect();
        pipeline.push(pipeline_mixer_meta(
            "mimo_fork",
            options.design.as_ref().map(|_| "secs:mimo"),
        ));
        Some(object([
            ("channels", channels(n_in, n_in + n_paths)),
            ("mapping", Json::Array(mapping)),
        ]))
    };

    for (channel, main) in mains.iter().enumerate() {
        let mut names = Vec::new();
        for (index, (frequency, q, gain)) in main.input_peq.iter().enumerate() {
            let name = format!("in_{}_peq{index}", main.role);
            filters.push((name.clone(), peaking(*frequency, *q, *gain)));
            names.push(name);
        }
        if main.fir.len() > 1 {
            let file = coeff_dir.join(format!("{fir_prefix}{}_fir.wav", main.role));
            if file.parent() != Some(coeff_dir.as_path()) {
                return Err(CamillaError::Invalid(format!(
                    "coefficient path escapes export folder: {}",
                    file.display()
                )));
            }
            let filename = posix(&relative(&file, config_dir)?);
            let taps = secs_dsp::rate::fir_to_rate(&main.fir, options.output_rate)
                .map_err(|error| CamillaError::Invalid(error.to_string()))?;
            coeffs.push((
                file,
                fir_wav(&format!("main.{}", main.role), &taps, options.output_rate)?,
            ));
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
        if options.design.is_some() {
            for name in names {
                pipeline.push(pipeline_filter_meta(
                    channel,
                    vec![name],
                    Some("secs:correction"),
                    Some(false),
                ));
            }
        } else if !names.is_empty() {
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

    // Corrected inputs that share a crossover can be summed before their
    // common LR4 low-pass. Raw MIMO copies were taken before correction.
    let mut xo_groups: Vec<(f64, Vec<usize>)> = Vec::new();
    if n_sub > 0 {
        for (channel, main) in mains.iter().enumerate() {
            if let Some(xo) = main.xo_hz.filter(|value| *value != 0.0) {
                if let Some((_, members)) = xo_groups.iter_mut().find(|(f, _)| *f == xo) {
                    members.push(channel);
                } else {
                    xo_groups.push((xo, vec![channel]));
                }
            }
        }
    }
    let n_xo = xo_groups.len();
    let split_width = n_in + n_xo + n_paths;
    let sum = |dest: usize, sources: Vec<usize>| {
        object([
            ("dest", int(dest)),
            (
                "sources",
                Json::Array(
                    sources
                        .into_iter()
                        .map(|channel| {
                            object([
                                ("channel", int(channel)),
                                ("gain", Json::Int(0)),
                                ("inverted", Json::Bool(false)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    };
    let mut mixers = Vec::<(String, Json)>::new();
    let split_mapping = (0..n_in)
        .map(|channel| one(channel, channel, false))
        .chain(
            xo_groups
                .iter()
                .enumerate()
                .map(|(group, (_, members))| sum(n_in + group, members.clone())),
        )
        .chain((0..n_paths).map(|path| one(n_in + n_xo + path, n_in + path, false)))
        .collect();
    mixers.push((
        "split".into(),
        object([
            ("channels", channels(n_in + n_paths, split_width)),
            ("mapping", Json::Array(split_mapping)),
        ]),
    ));
    pipeline.push(pipeline_mixer("split"));
    for (group, (frequency, _)) in xo_groups.iter().enumerate() {
        let name = format!("bass_xo{group}_lp");
        filters.push((name.clone(), combo("LinkwitzRileyLowpass", *frequency, 4)));
        pipeline.push(pipeline_filter(n_in + group, vec![name]));
    }
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
        }
        let name = format!("main_{}_delay", main.role);
        filters.push((name.clone(), delay(main.delay_ms, options.output_rate)));
        names.push(name);
        pipeline.push(pipeline_filter(channel, names));
    }

    let bass_width = n_main + usize::from(n_sub > 0) + n_paths;
    let mut bass_mapping: Vec<_> = (0..n_main)
        .map(|channel| one(channel, channel, false))
        .collect();
    if n_sub > 0 {
        let sources = (0..n_xo)
            .map(|group| n_in + group)
            .chain(lfe.then_some(n_main))
            .collect::<Vec<_>>();
        if !sources.is_empty() {
            bass_mapping.push(sum(n_main, sources));
        }
    }
    bass_mapping.extend((0..n_paths).map(|path| {
        one(
            n_main + usize::from(n_sub > 0) + path,
            n_in + n_xo + path,
            false,
        )
    }));
    mixers.push((
        "bass_sum".into(),
        object([
            ("channels", channels(split_width, bass_width)),
            ("mapping", Json::Array(bass_mapping)),
        ]),
    ));
    pipeline.push(pipeline_mixer("bass_sum"));
    if n_sub > 0 && !chain.sub_bus_peq.is_empty() {
        let names = chain
            .sub_bus_peq
            .iter()
            .enumerate()
            .map(|(index, &(frequency, q, gain))| {
                let name = format!("bass_bus_peq{index}");
                filters.push((name.clone(), peaking(frequency, q, gain)));
                name
            })
            .collect();
        pipeline.push(pipeline_filter(n_main, names));
    }
    // Apply each sub's gain/polarity after the shared sum and bus PEQ.
    let mut bus_mapping: Vec<_> = (0..n_main)
        .map(|channel| one(channel, channel, false))
        .collect();
    bus_mapping.extend(subs.iter().enumerate().map(|(index, sub)| {
        object([
            ("dest", int(n_main + index)),
            (
                "sources",
                Json::Array(vec![object([
                    ("channel", int(n_main)),
                    ("gain", Json::Float(round_ndigits(sub.gain_db, 4))),
                    ("inverted", Json::Bool(sub.polarity < 0)),
                ])]),
            ),
        ])
    }));
    bus_mapping.extend(
        (0..n_paths).map(|path| one(n_out + path, n_main + usize::from(n_sub > 0) + path, false)),
    );
    mixers.push((
        "bus".into(),
        object([
            ("channels", channels(bass_width, n_out + n_paths)),
            ("mapping", Json::Array(bus_mapping)),
        ]),
    ));
    pipeline.push(pipeline_mixer("bus"));
    for (sub_index, sub) in subs.iter().enumerate() {
        let mut names = Vec::new();
        for (index, &(frequency, q)) in sub.allpass.iter().enumerate() {
            let name = format!("sub_{}_ap{index}", sub.role);
            filters.push((name.clone(), allpass(frequency, q)));
            names.push(name);
        }
        let name = format!("sub_{}_delay", sub.role);
        filters.push((name.clone(), delay(sub.delay_ms, options.output_rate)));
        names.push(name);
        pipeline.push(pipeline_filter(n_main + sub_index, names));
    }

    struct SupportGroup {
        output: usize,
        low_hz: f64,
        fir: bool,
        delay_samples: i64,
        members: Vec<usize>,
    }
    let path_delays: Vec<_> = paths
        .iter()
        .map(|path| {
            let extra = match &path.filter {
                MimoFilter::Iir { delay_ms, .. } => *delay_ms,
                _ => 0.0,
            };
            // Round the old complete delay exactly once, then factor integer samples.
            secs_dsp::rate::delay_samples(path.delay_ms + extra, options.output_rate)
        })
        .collect();
    // The graph groups support paths by output, family and exact low edge.
    // CamillaDSP writes the high-pass at the low edge rounded to three places,
    // so graph groups whose rounded cutoff coincides share one channel. The
    // delay split is this writer's own (F13): each path's total delay is
    // rounded to whole samples once, the group carries the smallest and each
    // path the rest, so no path's delay is rounded twice. Equalizer APO keeps
    // the identical rendered milliseconds instead.
    let mut groups: Vec<SupportGroup> = Vec::new();
    for shared in graph.support_groups() {
        let low_hz = round_ndigits(shared.low_hz, 3);
        if let Some(group) = groups.iter_mut().find(|group| {
            group.output == shared.output && group.low_hz == low_hz && group.fir == shared.fir
        }) {
            group.members.extend_from_slice(&shared.members);
        } else {
            groups.push(SupportGroup {
                output: shared.output,
                low_hz,
                fir: shared.fir,
                delay_samples: 0,
                members: shared.members.clone(),
            });
        }
    }
    let mut path_groups = vec![0; n_paths];
    for (index, group) in groups.iter_mut().enumerate() {
        group.members.sort_unstable();
        group.delay_samples = group
            .members
            .iter()
            .map(|&path| path_delays[path])
            .min()
            .expect("a support group has a path");
        for &path in &group.members {
            path_groups[path] = index;
        }
    }
    for (index, path) in paths.iter().enumerate() {
        let tag = path_tag(path);
        let mut names = Vec::new();
        let residual = path_delays[index] - groups[path_groups[index]].delay_samples;
        if residual != 0 {
            let name = format!("{tag}_delay");
            filters.push((name.clone(), delay_samples(residual)));
            names.push(name);
        }
        match &path.filter {
            MimoFilter::Fir { .. } | MimoFilter::FirLowRate { .. } => {
                let taps = match &path.filter {
                    MimoFilter::Fir { taps_48k } => {
                        secs_dsp::rate::fir_to_rate(taps_48k, options.output_rate)
                    }
                    MimoFilter::FirLowRate { taps_low, fs_low } => {
                        secs_dsp::rate::lowrate_to_rate(taps_low, *fs_low, options.output_rate)
                    }
                    MimoFilter::Iir { .. } => unreachable!(),
                }
                .map_err(|error| CamillaError::Invalid(error.to_string()))?;
                let file = coeff_dir.join(format!("{fir_prefix}{tag}.wav"));
                if file.parent() != Some(coeff_dir.as_path()) {
                    return Err(CamillaError::Invalid(format!(
                        "coefficient path escapes export folder: {}",
                        file.display()
                    )));
                }
                let filename = posix(&relative(&file, config_dir)?);
                coeffs.push((
                    file,
                    fir_wav(&format!("mimo.paths[{index}]"), &taps, options.output_rate)?,
                ));
                let name = format!("{tag}_fir");
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
            MimoFilter::Iir {
                gain_db,
                polarity,
                peq,
                allpass: path_allpass,
                ..
            } => {
                let name = format!("{tag}_gain");
                filters.push((
                    name.clone(),
                    filter_spec(
                        "Gain",
                        object([
                            ("gain", Json::Float(round_ndigits(*gain_db, 4))),
                            ("inverted", Json::Bool(*polarity < 0)),
                        ]),
                    ),
                ));
                names.push(name);
                for (band, &(frequency, q, gain)) in peq.iter().enumerate() {
                    let name = format!("{tag}_peq{band}");
                    filters.push((name.clone(), peaking(frequency, q, gain)));
                    names.push(name);
                }
                for (band, &(frequency, q)) in path_allpass.iter().enumerate() {
                    let name = format!("{tag}_ap{band}");
                    filters.push((name.clone(), allpass(frequency, q)));
                    names.push(name);
                }
            }
        }
        if !names.is_empty() {
            pipeline.push(if options.design.is_some() {
                pipeline_filter_meta(n_out + index, names, Some("secs:mimo"), None)
            } else {
                pipeline_filter(n_out + index, names)
            });
        }
    }
    if !groups.is_empty() {
        // A group shares its exported HP (and IIR LP). Different support limits
        // or filter families remain separate even in caller-constructed plans.
        let mapping = (0..n_out)
            .map(|channel| one(channel, channel, false))
            .chain(groups.iter().enumerate().map(|(group, value)| {
                sum(
                    n_out + group,
                    value.members.iter().map(|path| n_out + path).collect(),
                )
            }))
            .collect();
        mixers.push((
            "mimo_groups".into(),
            object([
                ("channels", channels(n_out + n_paths, n_out + groups.len())),
                ("mapping", Json::Array(mapping)),
            ]),
        ));
        pipeline.push(pipeline_mixer_meta(
            "mimo_groups",
            options.design.as_ref().map(|_| "secs:mimo"),
        ));
        for (index, group) in groups.iter().enumerate() {
            let mut names = Vec::new();
            if group.delay_samples != 0 {
                let name = format!("mimo_group{index}_delay");
                filters.push((name.clone(), delay_samples(group.delay_samples)));
                names.push(name);
            }
            let name = format!("mimo_group{index}_hp");
            filters.push((
                name.clone(),
                combo("LinkwitzRileyHighpass", group.low_hz, 4),
            ));
            names.push(name);
            if !group.fir {
                let name = format!("mimo_group{index}_lp");
                filters.push((
                    name.clone(),
                    combo(
                        "LinkwitzRileyLowpass",
                        mimo.expect("MIMO container").fsiso,
                        4,
                    ),
                ));
                names.push(name);
            }
            pipeline.push(if options.design.is_some() {
                pipeline_filter_meta(n_out + index, names, Some("secs:mimo"), None)
            } else {
                pipeline_filter(n_out + index, names)
            });
        }
        let mapping = (0..n_out)
            .map(|output| {
                sum(
                    output,
                    std::iter::once(output)
                        .chain(
                            groups
                                .iter()
                                .enumerate()
                                .filter(|(_, group)| group.output == output)
                                .map(|(index, _)| n_out + index),
                        )
                        .collect(),
                )
            })
            .collect();
        mixers.push((
            "mimo_merge".into(),
            object([
                ("channels", channels(n_out + groups.len(), n_out)),
                ("mapping", Json::Array(mapping)),
            ]),
        ));
        pipeline.push(pipeline_mixer_meta(
            "mimo_merge",
            options.design.as_ref().map(|_| "secs:mimo"),
        ));
    }
    // The fixed speaker filter belongs after base + support summation. It is
    // identical on both branches; apply it once to the physical output.
    for (sub_index, sub) in subs.iter().enumerate() {
        let names = fixed_filters(&sub.role, sub, &mut filters);
        if !names.is_empty() {
            pipeline.push(pipeline_filter(n_main + sub_index, names));
        }
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
        inputs,
        outputs,
        options.latency_ms,
        &options.mode,
        options.output_rate,
        options.design.is_none() || (options.capture.is_none() && options.playback.is_none()),
        options.design.is_some() && (device_is(capture, "Asio") || device_is(playback, "Asio")),
    ));

    let title = if let Some(design) = &options.design {
        let base = design.project_name.as_deref().map_or_else(
            || format!("SECS 2R · {}", design.design_name),
            |project| format!("SECS 2R · {project} · {}", design.design_name),
        );
        if design.bypass {
            format!("{base} (우회)")
        } else {
            base
        }
    } else {
        format!(
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
        )
    };
    let mut description = format!(
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
    if let Some(design) = &options.design {
        write!(
            description,
            " | secs-design {}{}",
            design.design_id,
            if design.bypass { " bypass" } else { "" }
        )
        .expect("String write");
    }
    boundary("build");
    lines.extend([
        format!("title: {}", json(&Json::Str(title))),
        format!("description: {}", json(&Json::Str(description))),
        "devices:".into(),
        format!("  samplerate: {}", options.output_rate),
        format!("  chunksize: {}", options.chunksize),
        format!(
            "  capture: {}",
            json(&if options.labels {
                device_json_with_labels(capture, Some(inputs))
            } else {
                device_json(capture)
            })
        ),
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
    ]);
    if let Some(fork) = fork {
        lines.push(format!("  mimo_fork: {}", json(&fork)));
    }
    for (name, mixer) in mixers {
        lines.push(format!("  {name}: {}", json(&mixer)));
    }
    lines.push("pipeline:".into());
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

/// Atomically replace one export file through a sibling temporary file.
pub fn atomic_write(destination: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    secs_types::fs::write_atomic_bytes(destination, bytes)
}

fn without_correction(chain: &SignalChain) -> SignalChain {
    let mut chain = chain.clone();
    for main in &mut chain.mains {
        main.input_peq.clear();
        main.fir = vec![1.0];
    }
    chain
}

/// Write `configs/<name>.yml`, coefficient WAVs, and a placeholder `in.raw` when needed.
pub fn write_config(
    chain: &SignalChain,
    out: &Path,
    name: &str,
    options: &Options,
    mimo: Option<&MimoPaths>,
) -> Result<Vec<PathBuf>, CamillaError> {
    write_config_observed(chain, out, name, options, mimo, &|_, _| Ok(()))
}

/// Audit-only staging seam invoked after each complete staged file.
#[doc(hidden)]
pub fn write_config_observed(
    chain: &SignalChain,
    out: &Path,
    name: &str,
    options: &Options,
    mimo: Option<&MimoPaths>,
    after_stage_write: &dyn Fn(usize, &Path) -> std::io::Result<()>,
) -> Result<Vec<PathBuf>, CamillaError> {
    let safe = safe_name(name);
    if safe == "." || safe == ".." {
        return Err(CamillaError::Invalid(format!(
            "unsafe output name {name:?}"
        )));
    }
    let config_dir = out.join(CONFIG_DIR);
    let config = make_config(chain, out, &config_dir, &format!("{safe}_"), options, mimo)?;
    let bypass = if options.bypass {
        let bypass_chain = without_correction(chain);
        let mut bypass_options = options.clone();
        bypass_options.bypass = false;
        if let Some(design) = &mut bypass_options.design {
            design.bypass = true;
        }
        Some(make_config(
            &bypass_chain,
            out,
            &config_dir,
            &format!("{safe}_"),
            &bypass_options,
            None,
        )?)
    } else {
        None
    };
    let n_in =
        chain.mains.len() + usize::from(!chain.subs.is_empty() && options.lfe_gain_db.is_some());

    // All validation and pure construction have completed before the first write.
    let config_relative = PathBuf::from(CONFIG_DIR).join(format!("{safe}.yml"));
    let bypass_relative = PathBuf::from(CONFIG_DIR).join(format!("{safe}_bypass.yml"));
    let mut bundle = Vec::new();
    if options.capture.is_none() && !out.join("in.raw").exists() {
        bundle.push((PathBuf::from("in.raw"), vec![0_u8; 1024 * n_in * 4]));
    }
    for (path, bytes) in &config.coeffs {
        let relative = path.strip_prefix(out).map_err(|_| {
            CamillaError::Invalid(format!(
                "coefficient path escapes export folder: {}",
                path.display()
            ))
        })?;
        bundle.push((relative.to_path_buf(), bytes.clone()));
    }
    if let Some(bypass) = &bypass {
        if !bypass.coeffs.is_empty() {
            return Err(CamillaError::Invalid(
                "bypass configuration unexpectedly needs FIR coefficients".into(),
            ));
        }
        bundle.push((bypass_relative.clone(), bypass.yaml.as_bytes().to_vec()));
    }
    bundle.push((config_relative.clone(), config.yaml.into_bytes()));
    write_bundle_observed(out, &bundle, &config_relative, after_stage_write)?;

    let config_path = out.join(&config_relative);
    let mut written = vec![config_path];
    if bypass.is_some() {
        written.push(out.join(&bypass_relative));
    }
    let mut coeff_paths: Vec<_> = config.coeffs.into_iter().map(|(path, _)| path).collect();
    coeff_paths.sort();
    written.extend(coeff_paths);
    Ok(written)
}

/// Calibration-level export wrapper.
pub fn export(
    chain: &SignalChain,
    folder: &Path,
    labels: &RunLabels,
    mimo: Option<&MimoPaths>,
) -> Result<Vec<PathBuf>, CamillaError> {
    export_with_options(chain, folder, labels, mimo, Options::default())
}

/// Calibration-level export with service-selected devices and metadata.
pub fn export_with_options(
    chain: &SignalChain,
    folder: &Path,
    labels: &RunLabels,
    mimo: Option<&MimoPaths>,
    mut options: Options,
) -> Result<Vec<PathBuf>, CamillaError> {
    finite("labels.latency_ms", labels.latency_ms)?;
    options.note = labels.note.clone();
    options.latency_ms = Some(labels.latency_ms);
    options.mode = labels.mode.clone();
    write_config(
        chain,
        folder,
        &format!("secs_{}", labels.mode),
        &options,
        mimo,
    )
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
