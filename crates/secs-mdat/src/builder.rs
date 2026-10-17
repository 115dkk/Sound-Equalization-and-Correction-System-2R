// Port of src/secs/rew_mdat/builder.py (SECS 2, GPL-3.0-only).
use std::f64::consts::PI;

use secs_dsp::fft::rfft;
use secs_jos::{ArrayData, Limits, Node, NodeId, Stream, Value};

use crate::{
    MdatError, REW_MDAT_IR_SCALE,
    adapter::{int_block, node_id, number, object},
    source_info::{self, Boxes, SourceDate, SourceInfo},
};

#[derive(Clone, Debug)]
pub struct MeasurementInput {
    pub title: String,
    pub notes: String,
    pub sample_rate_hz: u32,
    pub samples: Vec<f64>,
    pub start_time_s: f64,
    pub spl_offset_db: Option<f64>,
    pub source_info: Option<SourceInfo>,
    pub computed: bool,
}

fn set(s: &mut Stream, obj: NodeId, name: &str, value: Value) -> Result<(), MdatError> {
    if s.set(obj, name, value) {
        Ok(())
    } else {
        Err(MdatError::Schema(format!(
            "template object has no field {name:?}"
        )))
    }
}

fn set_number(s: &mut Stream, obj: NodeId, name: &str, value: f64) -> Result<(), MdatError> {
    let v = match s.get(obj, name) {
        Some(Value::Double(_)) => Value::Double(value),
        Some(Value::Float(_)) => Value::Float(value as f32),
        Some(Value::Int(_)) => Value::Int(value as i32),
        Some(Value::Long(_)) => Value::Long(value as i64),
        Some(Value::Short(_)) => Value::Short(value as i16),
        Some(Value::Byte(_)) => Value::Byte(value as i8),
        Some(Value::Bool(_)) => Value::Bool(value != 0.0),
        _ => {
            return Err(MdatError::Schema(format!(
                "template numeric field {name:?} is absent"
            )));
        }
    };
    set(s, obj, name, v)
}

fn set_string(s: &mut Stream, obj: NodeId, name: &str, value: &str) -> Result<(), MdatError> {
    let id = s.push(Node::String {
        value: value.into(),
        long: false,
    });
    set(s, obj, name, Value::Node(id))
}

fn array_set(s: &mut Stream, obj: NodeId, name: &str, data: ArrayData) -> Result<(), MdatError> {
    let id = node_id(s.get(obj, name))
        .ok_or_else(|| MdatError::Schema(format!("template field {name:?} is not an array")))?;
    match s.nodes.get_mut(id) {
        Some(Node::Array { data: old, .. }) => {
            *old = data;
            Ok(())
        }
        _ => Err(MdatError::Schema(format!(
            "template field {name:?} is not an array"
        ))),
    }
}

fn is_array(s: &Stream, obj: NodeId, name: &str) -> bool {
    matches!(
        node_id(s.get(obj, name)).and_then(|id| s.nodes.get(id)),
        Some(Node::Array { .. })
    )
}

fn block(s: &mut Stream, value: i32) -> Value {
    Value::Node(s.push(Node::BlockData {
        data: value.to_be_bytes().to_vec(),
        long: false,
    }))
}

struct FrequencyData {
    freq: Vec<f64>,
    spl: Vec<f64>,
    phase: Vec<f64>,
    unwrapped: Vec<f64>,
    gd: Vec<f64>,
    real: Vec<f64>,
    imag: Vec<f64>,
}

fn frequency_data(samples: &[f32], rate: u32, spl_offset: f64) -> Result<FrequencyData, MdatError> {
    let low = 10.0;
    let high = 20000.0_f64.min(f64::from(rate) * 0.5 * 0.999);
    let count = (((high / low).log2() * 96.0).floor() as i64 + 1).max(2) as usize;
    let freq: Vec<f64> = (0..count)
        .map(|i| low * 2.0_f64.powf(i as f64 / 96.0))
        .filter(|v| *v <= high)
        .collect();
    if freq.len() < 2 {
        return Err(MdatError::Input(
            "sample rate gives fewer than two frequency points".into(),
        ));
    }
    let size = samples
        .len()
        .max(2)
        .checked_next_power_of_two()
        .ok_or_else(|| MdatError::Input("FFT size overflow".into()))?;
    let mut padded = vec![0.0; size];
    for (out, sample) in padded.iter_mut().zip(samples) {
        *out = f64::from(*sample);
    }
    let spectrum = rfft(&padded);
    // np.fft.rfftfreq computes val first, then arange * val (not i*rate/n).
    let step = 1.0 / (size as f64 * (1.0 / f64::from(rate)));
    let bins: Vec<f64> = (0..spectrum.len()).map(|i| i as f64 * step).collect();
    let real_bins: Vec<_> = spectrum.iter().map(|v| v.re).collect();
    let imag_bins: Vec<_> = spectrum.iter().map(|v| v.im).collect();
    let real = interp(&freq, &bins, &real_bins);
    let imag = interp(&freq, &bins, &imag_bins);
    let mut spl = Vec::with_capacity(freq.len());
    let mut angles = Vec::with_capacity(freq.len());
    for (&re, &im) in real.iter().zip(&imag) {
        // NumPy constructs real + 1j*imag; addition erases negative zero real.
        let re = re + 0.0 * im;
        let im = 0.0 + im;
        let magnitude = re.hypot(im);
        let clipped = if magnitude < 1e-30 { 1e-30 } else { magnitude };
        spl.push(20.0 * clipped.log10() + spl_offset);
        angles.push(im.atan2(re));
    }
    let phase: Vec<_> = angles.iter().map(|v| v * (180.0 / PI)).collect();
    let unwrapped: Vec<_> = unwrap(&angles).iter().map(|v| v * (180.0 / PI)).collect();
    let radians: Vec<_> = unwrapped.iter().map(|v| v * (PI / 180.0)).collect();
    let omega: Vec<_> = freq.iter().map(|v| 2.0 * PI * v).collect();
    let gd = gradient(&radians, &omega).into_iter().map(|v| -v).collect();
    Ok(FrequencyData {
        freq,
        spl,
        phase,
        unwrapped,
        gd,
        real,
        imag,
    })
}

fn interp(x: &[f64], xp: &[f64], fp: &[f64]) -> Vec<f64> {
    x.iter()
        .map(|&x| {
            if x <= xp[0] {
                return fp[0];
            }
            if x >= xp[xp.len() - 1] {
                return fp[fp.len() - 1];
            }
            let i = xp.partition_point(|v| *v <= x) - 1;
            if x == xp[i] {
                return fp[i];
            }
            let slope = (fp[i + 1] - fp[i]) / (xp[i + 1] - xp[i]);
            let mut result = slope * (x - xp[i]) + fp[i];
            if result.is_nan() {
                result = slope * (x - xp[i + 1]) + fp[i + 1];
                if result.is_nan() && fp[i] == fp[i + 1] {
                    result = fp[i];
                }
            }
            result
        })
        .collect()
}

fn unwrap(angles: &[f64]) -> Vec<f64> {
    let mut out = Vec::with_capacity(angles.len());
    let mut correction = 0.0;
    for (i, &v) in angles.iter().enumerate() {
        if i > 0 {
            let diff = v - angles[i - 1];
            let mut modulo = (diff + PI).rem_euclid(2.0 * PI) - PI;
            if modulo == -PI && diff > 0.0 {
                modulo = PI;
            }
            let delta = if diff.abs() < PI { 0.0 } else { modulo - diff };
            correction += delta;
        }
        out.push(v + correction);
    }
    out
}

fn gradient(values: &[f64], coordinates: &[f64]) -> Vec<f64> {
    let n = values.len();
    let dx: Vec<_> = coordinates.windows(2).map(|w| w[1] - w[0]).collect();
    let mut out = vec![0.0; n];
    out[0] = (values[1] - values[0]) / dx[0];
    for i in 1..n - 1 {
        let dx1 = dx[i - 1];
        let dx2 = dx[i];
        let a = -dx2 / (dx1 * (dx1 + dx2));
        let b = (dx2 - dx1) / (dx1 * dx2);
        let c = dx1 / (dx2 * (dx1 + dx2));
        out[i] = a * values[i - 1] + b * values[i] + c * values[i + 1];
    }
    out[n - 1] = (values[n - 1] - values[n - 2]) / dx[n - 2];
    out
}

fn replace(
    s: &mut Stream,
    obj: NodeId,
    item: &MeasurementInput,
    fallback_offset: Option<f64>,
    now_ms: i64,
    boxes: &mut Boxes,
    uuid: &mut dyn FnMut() -> u128,
) -> Result<(), MdatError> {
    let samples: Vec<f32> = item
        .samples
        .iter()
        .map(|v| (v * REW_MDAT_IR_SCALE) as f32)
        .collect();
    let rate = item.sample_rate_hz;
    source_info::apply(s, obj, item.source_info.as_ref(), boxes)?;
    let mut notes = item.notes.clone();
    if !item.computed
        && let Some(original) = item
            .source_info
            .as_ref()
            .and_then(|info| info.notes.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    {
        notes.push_str("\n\n[원 측정 메모]\n");
        notes.push_str(original);
    }
    set_string(s, obj, "shortDesc", &item.title)?;
    set_string(s, obj, "measNotes", &notes)?;
    set_number(s, obj, "sampleRate", f64::from(rate))?;
    // Upstream `int(date)`: an integer stays exact, a float truncates.
    let original_date = match item.source_info.as_ref().and_then(|info| info.date) {
        Some(SourceDate::Int(date)) if date > 0 => Some(date),
        Some(SourceDate::Float(date)) if date > 0.0 => Some(date as i64),
        _ => None,
    };
    let date = if item.computed {
        now_ms
    } else {
        original_date.unwrap_or(now_ms)
    };
    set_number(s, obj, "sourceFileDate", date as f64)?;
    if matches!(
        node_id(s.get(obj, "sourceFileFormat")).and_then(|id| s.nodes.get(id)),
        Some(Node::String { .. })
    ) {
        let format = if item.computed {
            "SECS2 correction result (computed, not measured)"
        } else {
            item.source_info
                .as_ref()
                .and_then(|info| info.format.as_deref())
                .filter(|value| !value.is_empty())
                .unwrap_or("SECS2 measurement")
        };
        set_string(s, obj, "sourceFileFormat", format)?;
    }
    let uid = uuid();
    if let Some(id) = object(s, s.get(obj, "uuid")) {
        let high = (uid >> 64) as i64;
        let low = uid as i64;
        set(s, id, "mostSigBits", Value::Long(high))?;
        set(s, id, "leastSigBits", Value::Long(low))?;
        if let Some(id) = object(s, s.get(obj, "uuid2")).filter(|id| is_array(s, *id, "value")) {
            array_set(s, id, "value", ArrayData::Long(vec![high, low]))?;
        }
    }
    let ir = object(s, s.get(obj, "irData"))
        .ok_or_else(|| MdatError::Schema("template MeasData.irData is absent".into()))?;
    let sampled = object(s, s.get(ir, "ir"))
        .ok_or_else(|| MdatError::Schema("template IRData.ir is absent".into()))?;
    let interval = 1.0 / f64::from(rate);
    let start = item.start_time_s;
    let mut peak = 0;
    for i in 1..samples.len() {
        if !samples[peak].is_nan()
            && (samples[i].is_nan() || samples[i].abs() > samples[peak].abs())
        {
            peak = i;
        }
    }
    let peak_value = f64::from(samples[peak].abs());
    let end = start + (samples.len() - 1) as f64 * interval;
    for (name, value) in [
        ("T", interval),
        ("n", samples.len() as f64),
        ("startTime", start),
        ("endTime", end),
        ("absMaxIndex", peak as f64),
        ("absMaxInterpIndex", peak as f64),
        ("absMaxVal", peak_value),
        ("normVal", peak_value),
    ] {
        set_number(s, sampled, name, value)?;
    }
    array_set(s, sampled, "data", ArrayData::Float(samples.clone()))?;
    if let Some(boxed) = object(s, s.get(sampled, "absMaxDoubleIndex")) {
        if !s
            .class_name(boxed)
            .is_some_and(|n| n.starts_with("java.lang."))
        {
            return Err(MdatError::Schema("template boxed value is absent".into()));
        }
        set_number(s, boxed, "value", peak as f64)?;
    }
    set_number(s, ir, "origPeakTime", start + peak as f64 * interval)?;
    if let Some(windows) = object(s, s.get(ir, "windows")) {
        let pre = peak - peak.min(rate as usize / 3);
        let post = (samples.len() - 1).min(peak + rate as usize / 2);
        for (name, value) in [
            ("T", interval),
            ("n", samples.len() as f64),
            ("startTime", start),
            ("winRefIndex", peak as f64),
            ("winRefTime", start + peak as f64 * interval),
            ("preImpulseIndex", pre as f64),
            ("postImpulseIndex", post as f64),
            ("preLength", (peak - pre).max(1) as f64),
            ("postLength", (post - peak + 1).max(1) as f64),
        ] {
            set_number(s, windows, name, value)?;
        }
    }
    if let Some(offset) = item.spl_offset_db.or(fallback_offset) {
        set_number(s, ir, "splOffset", offset)?;
    }
    let offset = match s.get(ir, "splOffset") {
        None | Some(Value::Null) => 0.0,
        v => number(v, "splOffset")?,
    };
    let cache = frequency_data(&samples, rate, offset)?;
    let n = cache.freq.len();
    let ratio = cache.freq[1] / cache.freq[0];
    for (name, value) in [
        ("dataLength", n as f64),
        ("sampleRate", f64::from(rate)),
        ("startFreq", cache.freq[0]),
        ("endFreq", cache.freq[n - 1]),
        ("validStartFreq", cache.freq[0]),
        ("validEndFreq", cache.freq[n - 1]),
        ("freqStep", cache.freq[1] - cache.freq[0]),
        ("isLogSpaced", 1.0),
        ("ppo", 96.0),
        ("logStep", ratio),
        ("logStepLog", ratio.ln()),
    ] {
        set_number(s, obj, name, value)?;
    }
    set_number(s, ir, "startFreq", cache.freq[0])?;
    set_number(s, ir, "endFreq", cache.freq[n - 1])?;
    for (name, data) in [
        ("splValues", &cache.spl),
        ("rawValues", &cache.spl),
        ("phaseValues", &cache.phase),
        ("unwPhaseValues", &cache.unwrapped),
        ("rawUnwPhaseValues", &cache.unwrapped),
        ("gdValues", &cache.gd),
        ("rawGDValues", &cache.gd),
        ("fdwReal", &cache.real),
        ("fdwImag", &cache.imag),
    ] {
        if is_array(s, obj, name) {
            array_set(
                s,
                obj,
                name,
                ArrayData::Float(data.iter().map(|v| *v as f32).collect()),
            )?;
        }
    }
    if is_array(s, obj, "unwPhaseRefPeak") {
        array_set(s, obj, "unwPhaseRefPeak", ArrayData::Float(vec![0.0; n]))?;
    }
    if s.get(obj, "distortionData").is_some() {
        set(s, obj, "distortionData", Value::Null)?;
    }
    Ok(())
}

fn add_text_allocation(bytes: &mut usize, value: &str) -> Option<()> {
    *bytes = bytes.checked_add(value.len().checked_mul(3)?)?;
    Some(())
}

fn source_allocation(info: &SourceInfo) -> Option<usize> {
    let mut bytes = 0usize;
    for (name, value) in &info.strings {
        add_text_allocation(&mut bytes, name)?;
        add_text_allocation(&mut bytes, value)?;
    }
    for (name, _) in &info.boxed {
        add_text_allocation(&mut bytes, name)?;
    }
    for (name, cal) in &info.cal {
        add_text_allocation(&mut bytes, name)?;
        add_text_allocation(&mut bytes, &cal.source_name)?;
        add_text_allocation(&mut bytes, &cal.source_path)?;
        for values in [&cal.freq, &cal.gain, &cal.phase].into_iter().flatten() {
            bytes = bytes.checked_add(values.len().checked_mul(4)?)?;
        }
        for (name, _) in &cal.boxed {
            add_text_allocation(&mut bytes, name)?;
        }
    }
    for value in [info.notes.as_deref(), info.format.as_deref()]
        .into_iter()
        .flatten()
    {
        add_text_allocation(&mut bytes, value)?;
    }
    Some(bytes)
}

/// Builds the upstream graph, with a separate deepcopy memo for each preview
/// pair and each measurement. UUIDs are consumed exactly once per measurement.
pub fn build(
    measurements: &[MeasurementInput],
    template: &[u8],
    spl_offset_db: Option<f64>,
    now_ms: i64,
    uuid: &mut dyn FnMut() -> u128,
) -> Result<Vec<u8>, MdatError> {
    let limits = Limits::default();
    if measurements.is_empty() {
        return Err(MdatError::Input(
            "at least one measurement is required".into(),
        ));
    }
    if measurements.len() > (limits.max_top_level_values.saturating_sub(5)) / 4
        || measurements.len() > i32::MAX as usize
    {
        return Err(MdatError::Input("too many measurements".into()));
    }
    let mut allocation = 0usize;
    for item in measurements {
        // Retained output per sample is the IR float plus six float cache arrays.
        // FFT work and serialized bytes are transient and must not be charged as
        // if they all remained allocated for every measurement.
        allocation = item
            .samples
            .len()
            .checked_mul(28)
            .and_then(|n| {
                item.title
                    .len()
                    .checked_mul(3)
                    .and_then(|s| n.checked_add(s))
            })
            .and_then(|n| {
                item.notes
                    .len()
                    .checked_mul(3)
                    .and_then(|s| n.checked_add(s))
            })
            .and_then(|n| {
                item.source_info
                    .as_ref()
                    .map_or(Some(0), source_allocation)
                    .and_then(|source| n.checked_add(source))
            })
            .and_then(|n| n.checked_add(128 * 1024))
            .and_then(|n| allocation.checked_add(n))
            .ok_or_else(|| MdatError::Input("measurement allocation overflow".into()))?;
        if item.samples.is_empty() {
            return Err(MdatError::Input(format!(
                "{}: IR samples are empty",
                item.title
            )));
        }
        if item.samples.len() > limits.max_array_elements.min(limits.max_array_bytes / 32)
            || allocation > limits.max_file_bytes
        {
            return Err(MdatError::Input(
                "IR samples exceed allocation limit".into(),
            ));
        }
        if item.sample_rate_hz == 0 {
            return Err(MdatError::Input(format!(
                "{}: sample rate must be positive",
                item.title
            )));
        }
        if item.sample_rate_hz > i32::MAX as u32 {
            return Err(MdatError::Input(format!(
                "{}: unsupported sample rate",
                item.title
            )));
        }
        if item.title.len() > limits.max_string_bytes / 3
            || item.notes.len() > limits.max_string_bytes / 3
        {
            return Err(MdatError::Input("measurement text exceeds limit".into()));
        }
    }
    let mut s = secs_jos::parse(template, &limits)?;
    let prototype = s
        .values
        .iter()
        .find_map(|v| {
            object(&s, Some(v)).filter(|id| s.class_name(*id) == Some("roomeqwizard.MeasData"))
        })
        .ok_or_else(|| MdatError::Schema("template has no roomeqwizard.MeasData object".into()))?;
    if int_block(&s, s.values.get(3), "template preview header")? < 1 || s.values.len() < 6 {
        return Err(MdatError::Schema("template needs one preview entry".into()));
    }
    let mut boxes = source_info::boxes_for(&s, prototype);
    // Bound the aggregate clone arena even for attacker-supplied templates.
    let clones = measurements
        .len()
        .checked_mul(2)
        .and_then(|v| v.checked_add(2))
        .ok_or_else(|| MdatError::Input("clone count overflow".into()))?;
    if s.nodes
        .len()
        .checked_mul(clones)
        .is_none_or(|n| n > limits.max_handles)
        || template
            .len()
            .checked_mul(clones)
            .and_then(|n| n.checked_add(allocation))
            .is_none_or(|n| n > limits.max_file_bytes)
    {
        return Err(MdatError::Input(
            "template clones exceed allocation limit".into(),
        ));
    }
    let header = s.values[..3].to_vec();
    let preview = s.values[4..6].to_vec();
    let mut values = s.clone_values(&header);
    values.push(block(&mut s, measurements.len() as i32));
    for _ in measurements {
        values.extend(s.clone_values(&preview));
    }
    values.push(block(&mut s, 3));
    for (index, item) in measurements.iter().enumerate() {
        let copied = s.clone_values(&[Value::Node(prototype)]);
        let id = object(&s, copied.first())
            .ok_or_else(|| MdatError::Schema("cannot clone measurement".into()))?;
        replace(&mut s, id, item, spl_offset_db, now_ms, &mut boxes, uuid)?;
        values.push(Value::Node(id));
        values.push(block(
            &mut s,
            if index + 1 == measurements.len() {
                0
            } else {
                3
            },
        ));
    }
    s.values = values;
    Ok(secs_jos::try_write(&s)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nonfinite_frequency_values_propagate_like_numpy() {
        let cache = frequency_data(&[f32::NAN, 1.0], 48000, 120.0).unwrap();
        assert!(cache.spl.iter().all(|v| v.is_nan()));
        assert!(cache.phase.iter().all(|v| v.is_nan()));
        assert!(cache.gd.iter().all(|v| v.is_nan()));
        assert_eq!(
            interp(&[0.5], &[0.0, 1.0], &[f64::INFINITY, f64::INFINITY]),
            [f64::INFINITY]
        );
    }

    #[test]
    fn interpolation_unwrap_and_nonuniform_gradient() {
        assert_eq!(
            interp(&[-1.0, 0.0, 0.5, 2.0], &[0.0, 1.0], &[2.0, 4.0]),
            [2.0, 2.0, 3.0, 4.0]
        );
        assert_eq!(unwrap(&[0.0, PI, 0.0]), [0.0, PI, 0.0]);
        let unwrapped = unwrap(&[3.0, -3.0]);
        assert_eq!(unwrapped[1], -3.0 + 2.0 * PI);
        assert_eq!(
            gradient(&[0.0, 1.0, 9.0], &[0.0, 1.0, 3.0]),
            [1.0, 2.0, 4.0]
        );
    }
}
