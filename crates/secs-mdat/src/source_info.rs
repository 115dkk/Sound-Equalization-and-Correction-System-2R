// Port of src/secs/rew_mdat/source_info.py (SECS 2, GPL-3.0-only).
use std::collections::BTreeMap;

use secs_jos::{
    ArrayData, ClassDescriptor, FieldDescriptor, JavaObject, Node, NodeId, Stream, Value,
};

use crate::{AllocationBudget, MdatError};

const STRING_FIELDS: [&str; 7] = [
    "outputName",
    "sourceFileName",
    "timingRefName",
    "osName",
    "osVersion",
    "jre",
    "sigGenLevelSt",
];
const PERSONAL_BOXED: [&str; 3] = ["inputVolume", "outputVolume", "micSensitivity"];
const BOXED_FIELDS: [&str; 8] = [
    "inputVolume",
    "outputVolume",
    "micSensitivity",
    "splCalOffset",
    "dBVInputOffset",
    "dBVOutputOffset",
    "usesEmbeddedCalData",
    "calDataLimitApplied",
];
const CAL_FIELDS: [&str; 2] = ["meterCal", "scCal"];
const CAL_ARRAYS: [&str; 3] = ["freqArray", "gainArray", "phaseArray"];
const CAL_STRINGS: [&str; 2] = ["sourceName", "sourcePath"];
const CAL_BOXED: [&str; 4] = ["sensitivitydB", "sensitivitydBFS", "serialNum", "revision"];

const DOUBLE_UID: i64 = -9_172_774_392_245_257_468;
const INTEGER_UID: i64 = 1_360_826_667_806_852_920;
const BOOLEAN_UID: i64 = -3_665_804_199_014_368_530;
const NUMBER_UID: i64 = -8_742_448_824_652_078_965;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SourceInfo {
    pub strings: Vec<(String, String)>,
    pub boxed: Vec<(String, Boxed)>,
    pub cal: Vec<(String, CalInfo)>,
    pub date: Option<SourceDate>,
    pub notes: Option<String>,
    pub format: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SourceDate {
    Int(i64),
    Float(f64),
}

impl SourceDate {
    pub fn value(self) -> f64 {
        match self {
            Self::Int(value) => value as f64,
            Self::Float(value) => value,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Boxed {
    Double(f64),
    Integer(i64),
    Boolean(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CalInfo {
    pub inverse_c_selected: bool,
    pub source_name: String,
    pub source_path: String,
    pub freq: Option<Vec<f32>>,
    pub gain: Option<Vec<f32>>,
    pub phase: Option<Vec<f32>>,
    pub boxed: Vec<(String, Boxed)>,
}

#[derive(Clone, Debug)]
pub struct CalFile {
    pub path: String,
    pub text: Option<Vec<u8>>,
}

#[derive(Debug, Default)]
pub struct Boxes {
    descriptors: BTreeMap<&'static str, NodeId>,
    number: Option<NodeId>,
    float_array: Option<NodeId>,
}

fn node_id(value: Option<&Value>) -> Option<NodeId> {
    match value {
        Some(Value::Node(id)) => Some(*id),
        _ => None,
    }
}

fn object(stream: &Stream, value: Option<&Value>) -> Option<NodeId> {
    let id = node_id(value)?;
    matches!(stream.nodes.get(id), Some(Node::Object(_))).then_some(id)
}

fn boxed_value(stream: &Stream, value: Option<&Value>) -> Option<Boxed> {
    let id = object(stream, value)?;
    match (stream.class_name(id)?, stream.get(id, "value")?) {
        ("java.lang.Double", Value::Double(value)) => Some(Boxed::Double(*value)),
        ("java.lang.Integer", Value::Int(value)) => Some(Boxed::Integer(i64::from(*value))),
        ("java.lang.Boolean", Value::Bool(value)) => Some(Boxed::Boolean(*value)),
        _ => None,
    }
}

/// Copy a string node after charging its bytes.
fn string(
    stream: &Stream,
    value: Option<&Value>,
    budget: &mut AllocationBudget,
) -> Result<Option<String>, MdatError> {
    match node_id(value).and_then(|id| stream.nodes.get(id)) {
        Some(Node::String { value, .. }) => {
            budget.charge(value.len())?;
            Ok(Some(value.clone()))
        }
        _ => Ok(None),
    }
}

/// Copy a float array node after charging its bytes.
fn float_array(
    stream: &Stream,
    value: Option<&Value>,
    budget: &mut AllocationBudget,
) -> Result<Option<Vec<f32>>, MdatError> {
    match node_id(value).and_then(|id| stream.nodes.get(id)) {
        Some(Node::Array {
            data: ArrayData::Float(values),
            ..
        }) => {
            budget.charge(values.len().saturating_mul(size_of::<f32>()))?;
            Ok(Some(values.clone()))
        }
        _ => Ok(None),
    }
}

/// Copy one measurement's source information, charging `budget` before every
/// string and array copy. Several measurements may reference the same
/// calibration array; each copy is charged.
pub fn extract(
    stream: &Stream,
    measurement: NodeId,
    notes: &str,
    budget: &mut AllocationBudget,
) -> Result<SourceInfo, MdatError> {
    // A fixed 4 KiB covers the field-name keys, boxed values, CalInfo records
    // and their vector growth; strings and arrays are charged as copied.
    budget.charge(4096)?;
    let mut info = SourceInfo::default();
    if let Some(Value::Long(value)) = stream.get(measurement, "sourceFileDate")
        && *value > 0
    {
        info.date = Some(SourceDate::Int(*value));
    }
    if !notes.is_empty() {
        budget.charge(notes.len())?;
        info.notes = Some(notes.to_owned());
    }
    info.format = string(stream, stream.get(measurement, "sourceFileFormat"), budget)?
        .filter(|value| !value.is_empty());
    for name in STRING_FIELDS {
        if let Some(value) = string(stream, stream.get(measurement, name), budget)? {
            info.strings.push((name.into(), value));
        }
    }
    for name in BOXED_FIELDS {
        if let Some(value) = boxed_value(stream, stream.get(measurement, name)) {
            info.boxed.push((name.into(), value));
        }
    }
    for name in CAL_FIELDS {
        let Some(cal) = object(stream, stream.get(measurement, name)) else {
            continue;
        };
        let inverse_c_selected =
            matches!(stream.get(cal, "inverseCSelected"), Some(Value::Bool(true)));
        let source_name =
            string(stream, stream.get(cal, "sourceName"), budget)?.unwrap_or_default();
        let source_path =
            string(stream, stream.get(cal, "sourcePath"), budget)?.unwrap_or_default();
        let [freq, gain, phase] = [
            float_array(stream, stream.get(cal, CAL_ARRAYS[0]), budget)?,
            float_array(stream, stream.get(cal, CAL_ARRAYS[1]), budget)?,
            float_array(stream, stream.get(cal, CAL_ARRAYS[2]), budget)?,
        ];
        let mut boxed = Vec::new();
        for key in CAL_BOXED {
            if let Some(value) = boxed_value(stream, stream.get(cal, key)) {
                boxed.push((key.into(), value));
            }
        }
        info.cal.push((
            name.into(),
            CalInfo {
                inverse_c_selected,
                source_name,
                source_path,
                freq,
                gain,
                phase,
                boxed,
            },
        ));
    }
    Ok(info)
}

fn put(stream: &mut Stream, object: NodeId, name: &str, value: Value) {
    stream.set(object, name, value);
}

fn put_string(stream: &mut Stream, object: NodeId, name: &str, value: &str) {
    let id = stream.push(Node::String {
        value: value.into(),
        long: false,
    });
    put(stream, object, name, Value::Node(id));
}

pub fn scrub(stream: &mut Stream, measurement: NodeId) -> Result<(), MdatError> {
    for name in STRING_FIELDS {
        put_string(stream, measurement, name, "");
    }
    for name in PERSONAL_BOXED {
        put(stream, measurement, name, Value::Null);
    }
    if let Some(flag) = object(stream, stream.get(measurement, "usesEmbeddedCalData")) {
        let classdesc = match stream.nodes.get(flag) {
            Some(Node::Object(value)) => value.classdesc,
            _ => return Err(MdatError::Schema("boxed Boolean object is invalid".into())),
        };
        let class_name = stream.class_name(flag).unwrap_or_default().to_owned();
        let mut fields = BTreeMap::new();
        fields.insert("value".into(), Value::Bool(false));
        let id = stream.push(Node::Object(JavaObject {
            classdesc,
            field_data: BTreeMap::from([(class_name, fields)]),
            annotations: BTreeMap::new(),
        }));
        put(stream, measurement, "usesEmbeddedCalData", Value::Node(id));
    }
    for name in CAL_FIELDS {
        let Some(cal) = object(stream, stream.get(measurement, name)) else {
            continue;
        };
        put(stream, cal, "inverseCSelected", Value::Bool(false));
        for key in CAL_STRINGS {
            put_string(stream, cal, key, "");
        }
        for key in [
            "freqArray",
            "gainArray",
            "phaseArray",
            "sensitivitydB",
            "sensitivitydBFS",
            "serialNum",
            "revision",
            "freqList",
            "gainList",
            "phaseList",
        ] {
            put(stream, cal, key, Value::Null);
        }
    }
    Ok(())
}

impl Boxes {
    fn learn(&mut self, stream: &Stream, value: Option<&Value>) {
        let Some(id) = object(stream, value) else {
            return;
        };
        let Some(kind) = stream.class_name(id) else {
            return;
        };
        let key = match kind {
            "java.lang.Double" => "java.lang.Double",
            "java.lang.Integer" => "java.lang.Integer",
            "java.lang.Boolean" => "java.lang.Boolean",
            _ => return,
        };
        self.descriptors
            .entry(key)
            .or_insert_with(|| match &stream.nodes[id] {
                Node::Object(value) => value.classdesc,
                _ => unreachable!(),
            });
        if key != "java.lang.Boolean" && self.number.is_none() {
            let descriptor = self.descriptors[key];
            if let Some(Node::ClassDesc(desc)) = stream.nodes.get(descriptor)
                && let Some(super_desc) = desc.super_desc
                && stream.class_name(super_desc) == Some("java.lang.Number")
            {
                self.number = Some(super_desc);
            }
        }
    }

    fn make(&mut self, stream: &mut Stream, value: Boxed) -> NodeId {
        let (kind, uid, code, primitive) = match value {
            Boxed::Double(value) => ("java.lang.Double", DOUBLE_UID, 'D', Value::Double(value)),
            Boxed::Integer(value) => (
                "java.lang.Integer",
                INTEGER_UID,
                'I',
                Value::Int(value as i32),
            ),
            Boxed::Boolean(value) => ("java.lang.Boolean", BOOLEAN_UID, 'Z', Value::Bool(value)),
        };
        let classdesc = if let Some(id) = self.descriptors.get(kind) {
            *id
        } else {
            let super_desc = if kind == "java.lang.Boolean" {
                None
            } else {
                Some(if let Some(id) = self.number {
                    id
                } else {
                    let id = stream.push(Node::ClassDesc(ClassDescriptor {
                        name: "java.lang.Number".into(),
                        serial_version_uid: NUMBER_UID,
                        flags: 2,
                        ..ClassDescriptor::default()
                    }));
                    self.number = Some(id);
                    id
                })
            };
            let id = stream.push(Node::ClassDesc(ClassDescriptor {
                name: kind.into(),
                serial_version_uid: uid,
                flags: 2,
                fields: vec![FieldDescriptor {
                    typecode: code,
                    name: "value".into(),
                    type_string: None,
                    type_string_node: None,
                }],
                super_desc,
                ..ClassDescriptor::default()
            }));
            self.descriptors.insert(kind, id);
            id
        };
        let mut field_data = BTreeMap::new();
        if kind != "java.lang.Boolean" {
            field_data.insert("java.lang.Number".into(), BTreeMap::new());
        }
        field_data.insert(kind.into(), BTreeMap::from([("value".into(), primitive)]));
        stream.push(Node::Object(JavaObject {
            classdesc,
            field_data,
            annotations: BTreeMap::new(),
        }))
    }
}

fn scan(stream: &Stream, measurement: NodeId, boxes: &mut Boxes) {
    for name in BOXED_FIELDS {
        boxes.learn(stream, stream.get(measurement, name));
    }
    for name in CAL_FIELDS {
        let Some(cal) = object(stream, stream.get(measurement, name)) else {
            continue;
        };
        for key in CAL_BOXED {
            boxes.learn(stream, stream.get(cal, key));
        }
        if boxes.float_array.is_none() {
            for key in CAL_ARRAYS {
                if let Some(id) = node_id(stream.get(cal, key))
                    && matches!(
                        stream.nodes.get(id),
                        Some(Node::Array {
                            data: ArrayData::Float(_),
                            ..
                        })
                    )
                {
                    boxes.float_array = match stream.nodes.get(id) {
                        Some(Node::Array { classdesc, .. }) => Some(*classdesc),
                        _ => None,
                    };
                    break;
                }
            }
        }
    }
    if boxes.float_array.is_none() {
        for name in ["splValues", "phaseValues"] {
            if let Some(id) = node_id(stream.get(measurement, name))
                && let Some(Node::Array {
                    classdesc,
                    data: ArrayData::Float(_),
                }) = stream.nodes.get(id)
            {
                boxes.float_array = Some(*classdesc);
                break;
            }
        }
    }
}

pub fn boxes_for(stream: &Stream, prototype: NodeId) -> Boxes {
    let mut boxes = Boxes::default();
    scan(stream, prototype, &mut boxes);
    boxes
}

pub fn apply(
    stream: &mut Stream,
    measurement: NodeId,
    info: Option<&SourceInfo>,
    boxes: &mut Boxes,
) -> Result<(), MdatError> {
    scan(stream, measurement, boxes);
    scrub(stream, measurement)?;
    let Some(info) = info else {
        return Ok(());
    };
    for (name, value) in &info.strings {
        if STRING_FIELDS.contains(&name.as_str()) {
            put_string(stream, measurement, name, value);
        }
    }
    for (name, value) in &info.boxed {
        if BOXED_FIELDS.contains(&name.as_str()) {
            let made = boxes.make(stream, *value);
            put(stream, measurement, name, Value::Node(made));
        }
    }
    for (name, item) in &info.cal {
        let Some(cal) = CAL_FIELDS
            .contains(&name.as_str())
            .then(|| object(stream, stream.get(measurement, name)))
            .flatten()
        else {
            continue;
        };
        put(
            stream,
            cal,
            "inverseCSelected",
            Value::Bool(item.inverse_c_selected),
        );
        put_string(stream, cal, "sourceName", &item.source_name);
        put_string(stream, cal, "sourcePath", &item.source_path);
        for (key, values) in [
            ("freqArray", item.freq.as_ref()),
            ("gainArray", item.gain.as_ref()),
            ("phaseArray", item.phase.as_ref()),
        ] {
            if let (Some(values), Some(classdesc)) = (values, boxes.float_array) {
                let id = stream.push(Node::Array {
                    classdesc,
                    data: ArrayData::Float(values.clone()),
                });
                put(stream, cal, key, Value::Node(id));
            }
        }
        for key in CAL_BOXED {
            let value = item.boxed.iter().find(|(name, _)| name == key);
            if let Some((_, value)) = value {
                let made = boxes.make(stream, *value);
                put(stream, cal, key, Value::Node(made));
            } else {
                put(stream, cal, key, Value::Null);
            }
        }
    }
    Ok(())
}

pub fn from_rew_measurement(
    cal: Option<&CalFile>,
    output_device: &str,
    input_device: &str,
    output_channel: &str,
    now_ms: i64,
) -> Result<Option<SourceInfo>, MdatError> {
    let mut info = SourceInfo {
        date: Some(SourceDate::Int(now_ms)),
        ..SourceInfo::default()
    };
    if !output_device.is_empty() {
        let output = if output_channel.is_empty() {
            output_device.to_owned()
        } else {
            format!("{output_device}, {output_channel}")
        };
        info.strings.push(("outputName".into(), output));
    }
    if !input_device.is_empty() {
        info.strings
            .push(("sourceFileName".into(), input_device.into()));
    }
    if let Some(cal) = cal {
        let mut item = CalInfo {
            inverse_c_selected: false,
            source_name: platform_basename(&cal.path),
            source_path: cal.path.clone(),
            freq: None,
            gain: None,
            phase: None,
            boxed: Vec::new(),
        };
        if let Some(bytes) = &cal.text {
            let text = String::from_utf8_lossy(bytes)
                .replace("\r\n", "\n")
                .replace('\r', "\n");
            let mut rows = Vec::new();
            let mut sensitivity = None;
            for line in text.lines() {
                if let Some(value) = sensitivity_value(line)? {
                    sensitivity = Some(value);
                }
                let replaced = line.replace([',', ';'], " ");
                let mut values = Vec::new();
                let mut valid = true;
                for token in replaced.split_whitespace().take(3) {
                    match python_float(token) {
                        Some(value) => values.push(value),
                        None => {
                            valid = false;
                            break;
                        }
                    }
                }
                if valid && values.len() >= 2 && values[0] > 0.0 {
                    values.resize(3, 0.0);
                    rows.push(values);
                }
            }
            if !rows.is_empty() {
                item.freq = Some(rows.iter().map(|row| row[0] as f32).collect());
                item.gain = Some(rows.iter().map(|row| row[1] as f32).collect());
                item.phase = Some(rows.iter().map(|row| row[2] as f32).collect());
                info.boxed
                    .push(("usesEmbeddedCalData".into(), Boxed::Boolean(true)));
            }
            if let Some(value) = sensitivity {
                item.boxed
                    .push(("sensitivitydB".into(), Boxed::Double(value)));
            }
        }
        info.cal.push(("meterCal".into(), item));
    }
    Ok((!info.strings.is_empty() || !info.cal.is_empty()).then_some(info))
}

fn sensitivity_value(line: &str) -> Result<Option<f64>, MdatError> {
    let lower = line.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut last = None;
    let mut at = 0;
    while at < bytes.len() {
        let Some(found) = lower[at..].find("sens") else {
            break;
        };
        let mut index = at + found + 4;
        skip_ascii_space(bytes, &mut index);
        if !lower[index..].starts_with("factor") {
            at += found + 4;
            continue;
        }
        index += 6;
        skip_ascii_space(bytes, &mut index);
        if bytes.get(index) != Some(&b'=') {
            at += found + 4;
            continue;
        }
        index += 1;
        skip_ascii_space(bytes, &mut index);
        let start = index;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        while matches!(bytes.get(index), Some(b'0'..=b'9' | b'.')) {
            index += 1;
        }
        if index > start && !matches!(&lower[start..index], "+" | "-") {
            let raw = &lower[start..index];
            let value = raw
                .parse::<f64>()
                .map_err(|_| MdatError::Input(format!("invalid Sens Factor value {raw:?}")))?;
            last = Some(value);
        }
        at = index.max(at + found + 4);
    }
    Ok(last)
}

fn skip_ascii_space(bytes: &[u8], index: &mut usize) {
    while bytes.get(*index).is_some_and(u8::is_ascii_whitespace) {
        *index += 1;
    }
}

fn python_float(token: &str) -> Option<f64> {
    let bytes = token.as_bytes();
    let mut cleaned = String::with_capacity(token.len());
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b'_' {
            if index == 0
                || index + 1 == bytes.len()
                || !bytes[index - 1].is_ascii_digit()
                || !bytes[index + 1].is_ascii_digit()
            {
                return None;
            }
        } else {
            cleaned.push(char::from(byte));
        }
    }
    match cleaned.to_ascii_lowercase().as_str() {
        "inf" | "+inf" | "infinity" | "+infinity" => Some(f64::INFINITY),
        "-inf" | "-infinity" => Some(f64::NEG_INFINITY),
        "nan" | "+nan" | "-nan" => Some(f64::NAN),
        _ => cleaned.parse().ok(),
    }
}

fn platform_basename(path: &str) -> String {
    basename_for(path, cfg!(windows))
}

fn basename_for(path: &str, windows: bool) -> String {
    path.rsplit(|ch| ch == '/' || (windows && ch == '\\'))
        .next()
        .unwrap_or_default()
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basename_follows_host_separator_rules() {
        assert_eq!(basename_for("/a/b/mic.txt", false), "mic.txt");
        assert_eq!(basename_for("C:\\a\\mic.txt", false), "C:\\a\\mic.txt");
        assert_eq!(basename_for("C:\\a/mic.txt", true), "mic.txt");
        assert_eq!(basename_for("C:\\a\\", true), "");
    }

    #[test]
    fn python_float_accepts_python_spellings() {
        assert_eq!(python_float("1_0"), Some(10.0));
        assert_eq!(python_float("-Infinity"), Some(f64::NEG_INFINITY));
        assert!(python_float("+NaN").unwrap().is_nan());
        assert_eq!(python_float("1__0"), None);
    }

    #[test]
    fn extract_charges_every_copy_of_a_shared_calibration_array() {
        let mut stream = Stream::default();
        let class = |stream: &mut Stream, name: &str| {
            stream.push(Node::ClassDesc(ClassDescriptor {
                name: name.into(),
                ..ClassDescriptor::default()
            }))
        };
        let float = class(&mut stream, "[F");
        let cal_class = class(&mut stream, "roomeqwizard.CalData");
        let measurement_class = class(&mut stream, "roomeqwizard.MeasData");
        let array = stream.push(Node::Array {
            classdesc: float,
            data: ArrayData::Float(vec![0.5; 1024]),
        });
        let object = |stream: &mut Stream, classdesc, fields: &[(&str, NodeId)]| {
            let name = stream.class_name(classdesc).unwrap().to_owned();
            let fields = fields
                .iter()
                .map(|(field, id)| ((*field).to_owned(), Value::Node(*id)))
                .collect();
            stream.push(Node::Object(JavaObject {
                classdesc,
                field_data: BTreeMap::from([(name, fields)]),
                annotations: BTreeMap::new(),
            }))
        };
        let cal = object(
            &mut stream,
            cal_class,
            &[("freqArray", array), ("gainArray", array)],
        );
        let measurement = object(
            &mut stream,
            measurement_class,
            &[("meterCal", cal), ("scCal", cal)],
        );

        // The fixed allowance plus four 4 KiB copies of the one shared array.
        let needed = 4096 + 4 * 1024 * size_of::<f32>();
        let limits = |max_file_bytes| secs_jos::Limits {
            max_file_bytes,
            ..secs_jos::Limits::default()
        };
        let mut budget = AllocationBudget::new(&limits(needed));
        let info = extract(&stream, measurement, "", &mut budget).unwrap();
        assert_eq!(budget.remaining(), 0);
        assert_eq!(info.cal.len(), 2);
        for (_, cal) in &info.cal {
            assert_eq!(cal.freq.as_deref(), Some(&[0.5; 1024][..]));
            assert_eq!(cal.gain.as_deref(), Some(&[0.5; 1024][..]));
            assert_eq!(cal.phase, None);
        }

        let mut budget = AllocationBudget::new(&limits(needed - 1));
        assert!(matches!(
            extract(&stream, measurement, "", &mut budget),
            Err(MdatError::Schema(ref message)) if message == "adapted data exceeds allocation limit"
        ));
        assert_eq!(budget.remaining(), 4096 - 1);
    }

    #[test]
    fn invalid_sensitivity_is_an_error() {
        assert!(sensitivity_value("Sens Factor = 1.2.3").is_err());
        assert_eq!(
            sensitivity_value("sens factor = -1.337dB").unwrap(),
            Some(-1.337)
        );
    }
}
