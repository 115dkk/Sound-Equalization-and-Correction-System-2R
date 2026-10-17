// Port of src/secs/rew_mdat/rew/adapters.py (SECS 2, GPL-3.0-only).
use secs_jos::{ArrayData, Limits, Node, NodeId, Stream, Value};

use crate::{MdatError, models::*};

pub(crate) fn node_id(value: Option<&Value>) -> Option<NodeId> {
    match value {
        Some(Value::Node(id)) => Some(*id),
        _ => None,
    }
}

pub(crate) fn object(stream: &Stream, value: Option<&Value>) -> Option<NodeId> {
    let id = node_id(value)?;
    matches!(stream.nodes.get(id), Some(Node::Object(_))).then_some(id)
}

pub(crate) fn string(stream: &Stream, value: Option<&Value>) -> String {
    match node_id(value).and_then(|id| stream.nodes.get(id)) {
        Some(Node::String { value, .. }) => value.clone(),
        _ => String::new(),
    }
}

pub(crate) fn number(value: Option<&Value>, label: &str) -> Result<f64, MdatError> {
    match value {
        Some(Value::Double(v)) => Ok(*v),
        Some(Value::Float(v)) => Ok(f64::from(*v)),
        Some(Value::Long(v)) => Ok(*v as f64),
        Some(Value::Int(v)) => Ok(f64::from(*v)),
        Some(Value::Short(v)) => Ok(f64::from(*v)),
        Some(Value::Byte(v)) => Ok(f64::from(*v)),
        Some(Value::Bool(v)) => Ok(f64::from(u8::from(*v))),
        _ => Err(MdatError::Schema(format!(
            "required numeric field {label:?} is absent"
        ))),
    }
}

fn integer(value: Option<&Value>, label: &str) -> Result<i64, MdatError> {
    if let Some(Value::Long(v)) = value {
        return Ok(*v);
    }
    let v = number(value, label)?;
    if !v.is_finite() || v < i64::MIN as f64 || v >= -(i64::MIN as f64) {
        return Err(MdatError::Schema(format!(
            "invalid integer field {label:?}"
        )));
    }
    Ok(v as i64)
}

pub(crate) fn block<'a>(
    stream: &'a Stream,
    value: Option<&Value>,
    len: usize,
    label: &str,
) -> Result<&'a [u8], MdatError> {
    match node_id(value).and_then(|id| stream.nodes.get(id)) {
        Some(Node::BlockData { data, .. }) if data.len() == len => Ok(data),
        _ => Err(MdatError::Schema(format!(
            "{label} is not a {len}-byte block"
        ))),
    }
}

pub(crate) fn int_block(
    stream: &Stream,
    value: Option<&Value>,
    label: &str,
) -> Result<i32, MdatError> {
    let bytes = block(stream, value, 4, label)?;
    Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn meanings(fields: &[(&str, &str)]) -> Meanings {
    fields
        .iter()
        .map(|(name, source)| {
            (
                (*name).into(),
                FieldMeaning {
                    status: FieldStatus::Confirmed,
                    source_field: Some((*source).into()),
                    note: None,
                },
            )
        })
        .collect()
}

fn unknown(stream: &Stream, obj: NodeId, known: &[&str]) -> Vec<String> {
    stream
        .field_names(obj)
        .into_iter()
        .filter(|name| !known.contains(&name.as_str()))
        .collect()
}

struct Adapter<'a> {
    stream: &'a Stream,
    limits: &'a Limits,
    remaining: usize,
}
impl<'a> Adapter<'a> {
    fn charge(&mut self, bytes: usize) -> Result<(), MdatError> {
        self.remaining = self
            .remaining
            .checked_sub(bytes)
            .ok_or_else(|| MdatError::Schema("adapted data exceeds allocation limit".into()))?;
        Ok(())
    }

    fn charge_object(&mut self, id: NodeId) -> Result<(), MdatError> {
        // Charge borrowed slots before field_names() or string() can clone them.
        // The fixed allowance covers models, meanings and vector growth.
        self.charge(4096)?;
        if let Some(Node::Object(obj)) = self.stream.nodes.get(id) {
            for fields in obj.field_data.values() {
                for (name, value) in fields {
                    self.charge(name.len().saturating_add(2 * size_of::<String>()))?;
                    if let Some(Node::String { value, .. }) =
                        node_id(Some(value)).and_then(|id| self.stream.nodes.get(id))
                    {
                        self.charge(value.len())?;
                    }
                }
            }
        }
        Ok(())
    }

    fn array(&mut self, value: Option<&Value>, label: &str) -> Result<Vec<f64>, MdatError> {
        let data = match node_id(value).and_then(|id| self.stream.nodes.get(id)) {
            Some(Node::Array { data, .. }) => data,
            _ => {
                return Err(MdatError::Schema(format!(
                    "required REW array '{label}' is absent"
                )));
            }
        };
        let n = match data {
            ArrayData::Byte(v) => v.len(),
            ArrayData::Char(v) => v.len(),
            ArrayData::Double(v) => v.len(),
            ArrayData::Float(v) => v.len(),
            ArrayData::Int(v) => v.len(),
            ArrayData::Long(v) => v.len(),
            ArrayData::Short(v) => v.len(),
            ArrayData::Bool(v) => v.len(),
            ArrayData::Object(v) => v.len(),
        };
        let bytes = n
            .checked_mul(8)
            .ok_or_else(|| MdatError::Schema("array size overflow".into()))?;
        if n > self.limits.max_array_elements
            || bytes > self.limits.max_array_bytes
            || bytes > self.remaining
        {
            return Err(MdatError::Schema(
                "adapted arrays exceed allocation limit".into(),
            ));
        }
        self.remaining -= bytes;
        let mut result = Vec::new();
        result
            .try_reserve_exact(n)
            .map_err(|_| MdatError::Schema("cannot allocate adapted array".into()))?;
        match data {
            ArrayData::Byte(v) => result.extend(v.iter().map(|v| f64::from(*v))),
            ArrayData::Char(_) => {
                return Err(MdatError::Schema(format!(
                    "REW array '{label}' contains characters, not numbers"
                )));
            }
            ArrayData::Double(v) => result.extend_from_slice(v),
            ArrayData::Float(v) => result.extend(v.iter().map(|v| f64::from(*v))),
            ArrayData::Int(v) => result.extend(v.iter().map(|v| f64::from(*v))),
            ArrayData::Long(v) => result.extend(v.iter().map(|v| *v as f64)),
            ArrayData::Short(v) => result.extend(v.iter().map(|v| f64::from(*v))),
            ArrayData::Bool(v) => result.extend(v.iter().map(|v| f64::from(u8::from(*v)))),
            ArrayData::Object(v) => {
                for value in v {
                    result.push(number(Some(value), label)?);
                }
            }
        }
        Ok(result)
    }

    fn boxed_int(&self, mut value: Option<&'a Value>, default: i32) -> Result<i64, MdatError> {
        for _ in 0..=self.limits.max_depth.min(self.stream.nodes.len()) {
            match node_id(value).and_then(|id| self.stream.nodes.get(id).map(|node| (id, node))) {
                Some((id, Node::Object(_)))
                    if self
                        .stream
                        .class_name(id)
                        .is_some_and(|n| n.starts_with("java.lang.")) =>
                {
                    value = self.stream.get(id, "value")
                }
                Some((_, Node::Enum { constant, .. })) => value = Some(constant),
                Some((_, Node::String { value, .. })) => {
                    return value
                        .parse()
                        .map_err(|_| MdatError::Schema("invalid boxed integer".into()));
                }
                _ => {
                    return if value.is_none() || matches!(value, Some(Value::Null)) {
                        Ok(i64::from(default))
                    } else {
                        integer(value, "boxed version")
                    };
                }
            }
        }
        Err(MdatError::Schema(
            "boxed value nesting exceeds limit".into(),
        ))
    }

    fn measurement(&mut self, obj: NodeId, version: (i32, i32)) -> Result<Measurement, MdatError> {
        let s = self.stream;
        let get = |name| s.get(obj, name);
        let missing: Vec<_> = [
            "splValues",
            "phaseValues",
            "unwPhaseValues",
            "gdValues",
            "irData",
        ]
        .into_iter()
        .filter(|name| matches!(get(name), None | Some(Value::Null)))
        .collect();
        if !missing.is_empty() {
            return Err(MdatError::Schema(format!(
                "MeasData is missing required fields: {}",
                missing.join(", ")
            )));
        }
        self.charge_object(obj)?;
        let num = |name| number(get(name), name);
        let int = |name| integer(get(name), name);
        let spacing = FrequencySpacing {
            count: int("dataLength")?,
            start_hz: num("startFreq")?,
            end_hz: num("endFreq")?,
            step_hz: num("freqStep")?,
            is_log_spaced: num("isLogSpaced")? != 0.0,
            points_per_octave: int("ppo")?,
            valid_start_hz: num("validStartFreq")?,
            valid_end_hz: num("validEndFreq")?,
        };
        let response = FrequencyResponse {
            spacing,
            spl_db: self.array(get("splValues"), "splValues")?,
            phase_deg: self.array(get("phaseValues"), "phaseValues")?,
            unwrapped_phase_deg: self.array(get("unwPhaseValues"), "unwPhaseValues")?,
            group_delay_s: self.array(get("gdValues"), "gdValues")?,
            meanings: meanings(&[
                ("spl_db", "splValues"),
                ("phase_deg", "phaseValues"),
                ("unwrapped_phase_deg", "unwPhaseValues"),
                ("group_delay_s", "gdValues"),
            ]),
        };
        let ir = object(s, get("irData"))
            .filter(|id| s.class_name(*id) == Some("roomeqwizard.IRData"))
            .ok_or_else(|| {
                MdatError::Schema("irData does not contain roomeqwizard.IRData".into())
            })?;
        let sampled = object(s, s.get(ir, "ir"))
            .filter(|id| s.class_name(*id) == Some("roomeqwizard.SampledData"))
            .ok_or_else(|| {
                MdatError::Schema("IRData.ir does not contain roomeqwizard.SampledData".into())
            })?;
        let impulse = ImpulseResponse {
            samples: self.array(s.get(sampled, "data"), "IRData.ir.data")?,
            sample_interval_s: number(s.get(sampled, "T"), "T")?,
            start_time_s: number(s.get(sampled, "startTime"), "startTime")?,
            measurement_delay_s: number(s.get(ir, "measurementDelay"), "measurementDelay")?,
            spl_offset_db: number(s.get(ir, "splOffset"), "splOffset")?,
            peak_index: integer(s.get(sampled, "absMaxIndex"), "absMaxIndex")?,
            meanings: meanings(&[
                ("samples", "IRData.ir.data"),
                ("measurement_delay_s", "IRData.measurementDelay"),
                ("spl_offset_db", "IRData.splOffset"),
            ]),
        };
        let distortion = if let Some(d) = object(s, get("distortionData")) {
            self.charge_object(d)?;
            let mut harmonic_arrays = Vec::new();
            if let Some(Node::Array {
                data: ArrayData::Object(items),
                ..
            }) = node_id(s.get(d, "harmData")).and_then(|id| s.nodes.get(id))
            {
                self.charge(items.len().saturating_mul(2 * size_of::<Vec<f64>>()))?;
                for item in items {
                    harmonic_arrays.push(if matches!(item, Value::Null) {
                        vec![]
                    } else {
                        self.array(Some(item), "harmData")?
                    });
                }
            }
            let noise = match s.get(d, "noiseFloorSPL") {
                None | Some(Value::Null) => None,
                v => Some(self.array(v, "noiseFloorSPL")?),
            };
            let mut m = meanings(&[("frequency_hz", "DistortionData.freqs")]);
            m.insert(
                "harmonic_arrays".into(),
                FieldMeaning {
                    status: FieldStatus::Inferred,
                    source_field: Some("DistortionData.harmData".into()),
                    note: Some(
                        "array identity is preserved; individual harmonic order is not asserted"
                            .into(),
                    ),
                },
            );
            Some(DistortionDataset {
                frequency_hz: self.array(s.get(d, "freqs"), "freqs")?,
                harmonic_arrays,
                points_per_octave: s
                    .get(d, "ppo")
                    .map(|v| integer(Some(v), "ppo"))
                    .transpose()?
                    .unwrap_or(0),
                noise_floor_spl_db: noise,
                meanings: m,
                unknown_fields: unknown(s, d, &["noiseFloorSPL", "harmData", "ppo", "freqs"]),
            })
        } else {
            None
        };
        Ok(Measurement {
            source_object: obj,
            name: string(s, get("shortDesc")),
            sample_rate_hz: int("sampleRate")?,
            rew_version: RewVersion {
                major: self.boxed_int(get("rewVersion"), version.0)?,
                minor: self.boxed_int(get("rewSubVersion"), version.1)?,
                suffix: string(s, get("versionSt")),
            },
            room_dimensions: RoomDimensions {
                length_m: num("roomLength")?,
                width_m: num("roomWidth")?,
                height_m: num("roomHeight")?,
            },
            frequency_response: response,
            impulse_response: impulse,
            distortion,
            notes: string(s, get("measNotes")),
            unknown_fields: unknown(
                s,
                obj,
                &[
                    "dataLength",
                    "irData",
                    "rewVersion",
                    "validEndFreq",
                    "distortionData",
                    "roomHeight",
                    "splValues",
                    "validStartFreq",
                    "sampleRate",
                    "measNotes",
                    "endFreq",
                    "phaseValues",
                    "freqStep",
                    "ppo",
                    "versionSt",
                    "isLogSpaced",
                    "rewSubVersion",
                    "gdValues",
                    "shortDesc",
                    "unwPhaseValues",
                    "roomWidth",
                    "roomLength",
                    "startFreq",
                ],
            ),
        })
    }
}

pub(crate) fn adapt(
    stream: Stream,
    file_size: usize,
    limits: &Limits,
) -> Result<MdatFile, MdatError> {
    if stream.values.len() < 4 {
        return Err(MdatError::Schema(
            "MDAT top-level sequence is incomplete".into(),
        ));
    }
    let version = block(&stream, stream.values.get(1), 8, "REW version")?;
    let version = (
        i32::from_be_bytes([version[0], version[1], version[2], version[3]]),
        i32::from_be_bytes([version[4], version[5], version[6], version[7]]),
    );
    let mut adapter = Adapter {
        stream: &stream,
        limits,
        remaining: limits.max_file_bytes,
    };
    adapter.charge(stream.values.len().saturating_mul(3 * size_of::<Value>()))?;
    for value in [stream.values.first(), stream.values.get(2)] {
        if let Some(Node::String { value, .. }) = node_id(value).and_then(|id| stream.nodes.get(id))
        {
            adapter.charge(value.len())?;
        }
    }
    let mut measurements = Vec::new();
    let mut unknown_top_level = Vec::new();
    for (index, value) in stream.values.iter().enumerate() {
        if let Some(id) = object(&stream, Some(value))
            .filter(|id| stream.class_name(*id) == Some("roomeqwizard.MeasData"))
        {
            measurements.push(adapter.measurement(id, version)?);
        } else if index > 3 {
            unknown_top_level.push(value.clone());
        }
    }
    Ok(MdatFile {
        title: string(&stream, stream.values.first()),
        rew_version_major: version.0,
        rew_version_minor: version.1,
        notes: string(&stream, stream.values.get(2)),
        preview_count: int_block(&stream, stream.values.get(3), "preview count")?,
        measurements,
        bytes_consumed: stream.bytes_consumed,
        file_size,
        top_level_count: stream.values.len(),
        handle_count: stream.handle_count,
        raw_top_level: stream.values.clone(),
        unknown_top_level,
        stream,
    })
}
