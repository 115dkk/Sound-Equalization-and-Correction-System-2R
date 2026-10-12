// Port of src/secs/rew_mdat/serialization/writer.py (GPL-3.0-only).
use crate::{
    constants::*,
    nodes::{TextKey, TextSlot},
    parser::python_repr,
    *,
};
use std::collections::BTreeMap;

/// Serialize a valid graph. Panics if a caller constructed an invalid graph
/// or supplies upstream's unsupported `End`/`Reset` content sentinels.
/// Use [`try_write`] for fallible validation.
pub fn write(stream: &Stream) -> Vec<u8> {
    try_write(stream).expect("invalid Java serialization graph")
}
/// Fallible writer; preserves handle assignment and exception resets.
pub fn try_write(stream: &Stream) -> Result<Vec<u8>, JosError> {
    let mut writer = Writer {
        stream,
        out: vec![0xac, 0xed, 0, 5],
        handles: BTreeMap::new(),
        next: BASE_HANDLE,
        tasks: Vec::new(),
    };
    for value in stream.values.iter().rev() {
        writer.tasks.push(Task::Content(value));
    }
    while let Some(task) = writer.tasks.pop() {
        writer.step(task)?;
    }
    Ok(writer.out)
}
enum Task<'a> {
    Content(&'a Value),
    Node(NodeId),
    Descriptor(Option<NodeId>),
    Add(NodeId),
    Fields(NodeId, usize),
    DescriptorTail(NodeId),
    Annotation(&'a [Value]),
    End,
    ObjectBody(NodeId),
    ObjectClass {
        id: NodeId,
        hierarchy: Vec<NodeId>,
        index: usize,
    },
    ObjectField {
        id: NodeId,
        desc: NodeId,
        field: usize,
    },
    ArrayBody(NodeId),
    Reset,
}
struct Writer<'a> {
    stream: &'a Stream,
    out: Vec<u8>,
    handles: BTreeMap<NodeId, u32>,
    next: u32,
    tasks: Vec<Task<'a>>,
}
impl<'a> Writer<'a> {
    fn node(&self, id: NodeId) -> Result<&'a Node, JosError> {
        self.stream
            .nodes
            .get(id)
            .ok_or_else(|| JosError::format(format!("invalid node id {id}")))
    }
    fn desc(&self, id: NodeId) -> Result<&'a ClassDescriptor, JosError> {
        self.stream
            .descriptor(id)
            .ok_or_else(|| JosError::format("node is not a class descriptor"))
    }
    fn reference(&mut self, id: NodeId) -> bool {
        if let Some(h) = self.handles.get(&id) {
            self.out.push(TC_REFERENCE);
            self.out.extend(h.to_be_bytes());
            true
        } else {
            false
        }
    }
    fn add(&mut self, id: NodeId) -> Result<(), JosError> {
        self.handles.insert(id, self.next);
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| JosError::format("handle number overflow"))?;
        Ok(())
    }
    fn utf(&mut self, value: &str, key: Option<TextKey>, long: bool) -> Result<(), JosError> {
        let encoded = key
            .and_then(|key| self.stream.raw_text.get(&key))
            .filter(|(text, _)| text == value)
            .map(|(_, bytes)| bytes.clone())
            .unwrap_or_else(|| encode_modified_utf8(value));
        if long {
            self.out.extend((encoded.len() as i64).to_be_bytes());
        } else {
            let len = u16::try_from(encoded.len()).map_err(|_| {
                JosError::format(format!(
                    "Modified UTF-8 value is too long for writeUTF: {} bytes",
                    encoded.len()
                ))
            })?;
            self.out.extend(len.to_be_bytes());
        }
        self.out.extend(encoded);
        Ok(())
    }
    fn step(&mut self, task: Task<'a>) -> Result<(), JosError> {
        match task {
            Task::Content(value) => match value {
                Value::Null => self.out.push(TC_NULL),
                Value::Node(id) => self.tasks.push(Task::Node(*id)),
                _ => return Err(JosError::format("cannot serialize node object")),
            },
            Task::Node(id) => match self.node(id)? {
                Node::String { value, long } => {
                    if !self.reference(id) {
                        let long = *long || encode_modified_utf8(value).len() > 0xffff;
                        self.out.push(if long { TC_LONGSTRING } else { TC_STRING });
                        self.utf(value, Some(TextKey(id, TextSlot::String)), long)?;
                        self.add(id)?;
                    }
                }
                Node::ClassDesc(_) => self.tasks.push(Task::Descriptor(Some(id))),
                Node::Object(object) => {
                    if !self.reference(id) {
                        self.out.push(TC_OBJECT);
                        self.tasks.push(Task::ObjectBody(id));
                        self.tasks.push(Task::Add(id));
                        self.tasks.push(Task::Descriptor(Some(object.classdesc)));
                    }
                }
                Node::Array { classdesc, .. } => {
                    if !self.reference(id) {
                        self.out.push(TC_ARRAY);
                        self.tasks.push(Task::ArrayBody(id));
                        self.tasks.push(Task::Add(id));
                        self.tasks.push(Task::Descriptor(Some(*classdesc)));
                    }
                }
                Node::Class { classdesc } => {
                    if !self.reference(id) {
                        self.out.push(TC_CLASS);
                        self.tasks.push(Task::Add(id));
                        self.tasks.push(Task::Descriptor(Some(*classdesc)));
                    }
                }
                Node::Enum {
                    classdesc,
                    constant,
                } => {
                    if !self.reference(id) {
                        self.out.push(TC_ENUM);
                        self.tasks.push(Task::Content(constant));
                        self.tasks.push(Task::Add(id));
                        self.tasks.push(Task::Descriptor(Some(*classdesc)));
                    }
                }
                Node::BlockData { data, long } => {
                    let long = *long || data.len() > 255;
                    self.out
                        .push(if long { TC_BLOCKDATALONG } else { TC_BLOCKDATA });
                    if long {
                        self.length(data.len())?;
                    } else {
                        self.out.push(data.len() as u8);
                    }
                    self.out.extend(data);
                }
                Node::Exception { exception } => {
                    self.out.push(TC_EXCEPTION);
                    self.tasks.push(Task::Reset);
                    self.tasks.push(Task::Content(exception));
                }
            },
            Task::Descriptor(id) => {
                if let Some(id) = id {
                    if !self.reference(id) {
                        let d = self.desc(id)?;
                        if d.is_proxy {
                            self.out.push(TC_PROXYCLASSDESC);
                            self.add(id)?;
                            self.length(d.interfaces.len())?;
                            for (i, name) in d.interfaces.iter().enumerate() {
                                self.utf(name, Some(TextKey(id, TextSlot::Interface(i))), false)?;
                            }
                        } else {
                            self.out.push(TC_CLASSDESC);
                            self.utf(&d.name, Some(TextKey(id, TextSlot::Name)), false)?;
                            self.out.extend(d.serial_version_uid.to_be_bytes());
                            self.add(id)?;
                            self.out.push(d.flags);
                            self.out.extend(
                                u16::try_from(d.fields.len())
                                    .map_err(|_| JosError::format("too many class fields"))?
                                    .to_be_bytes(),
                            );
                        }
                        self.tasks.push(Task::DescriptorTail(id));
                        if !d.is_proxy {
                            self.tasks.push(Task::Fields(id, 0));
                        }
                    }
                } else {
                    self.out.push(TC_NULL);
                }
            }
            Task::Fields(id, index) => {
                let d = self.desc(id)?;
                if let Some(field) = d.fields.get(index) {
                    self.out.push(
                        u8::try_from(field.typecode)
                            .map_err(|_| JosError::format("invalid field typecode"))?,
                    );
                    self.utf(
                        &field.name,
                        Some(TextKey(id, TextSlot::Field(index))),
                        false,
                    )?;
                    self.tasks.push(Task::Fields(id, index + 1));
                    if matches!(field.typecode, 'L' | '[') {
                        if let Some(id) = field.type_string_node {
                            self.tasks.push(Task::Node(id));
                        } else if let Some(text) = &field.type_string {
                            // The immutable writer cannot install Python's synthesized node.
                            // Each missing signature slot nevertheless receives its own handle.
                            let long = encode_modified_utf8(text).len() > 65535;
                            self.out.push(if long { TC_LONGSTRING } else { TC_STRING });
                            self.utf(text, None, long)?;
                            self.next = self
                                .next
                                .checked_add(1)
                                .ok_or_else(|| JosError::format("handle number overflow"))?;
                        } else {
                            self.out.push(TC_NULL);
                        }
                    }
                }
            }
            Task::DescriptorTail(id) => {
                let d = self.desc(id)?;
                self.tasks.push(Task::Descriptor(d.super_desc));
                self.tasks.push(Task::Annotation(&d.annotations));
            }
            Task::Add(id) => self.add(id)?,
            Task::Annotation(values) => {
                self.tasks.push(Task::End);
                for v in values.iter().rev() {
                    self.tasks.push(Task::Content(v));
                }
            }
            Task::End => self.out.push(TC_ENDBLOCKDATA),
            Task::Reset => {
                self.handles.clear();
                self.next = BASE_HANDLE;
            }
            Task::ObjectBody(id) => {
                let Node::Object(o) = self.node(id)? else {
                    unreachable!("object task");
                };
                let mut hierarchy = self.stream.hierarchy(o.classdesc).ok_or_else(|| {
                    JosError::format("cyclic or invalid class descriptor hierarchy")
                })?;
                hierarchy.reverse();
                self.tasks.push(Task::ObjectClass {
                    id,
                    hierarchy,
                    index: 0,
                });
            }
            Task::ObjectClass {
                id,
                hierarchy,
                index,
            } => {
                if let Some(&desc) = hierarchy.get(index) {
                    let d = self.desc(desc)?;
                    let Node::Object(o) = self.node(id)? else {
                        unreachable!("object task");
                    };
                    self.tasks.push(Task::ObjectClass {
                        id,
                        hierarchy,
                        index: index + 1,
                    });
                    if d.flags & SC_SERIALIZABLE != 0 {
                        if d.flags & SC_WRITE_METHOD != 0 {
                            self.tasks.push(Task::Annotation(
                                o.annotations.get(&d.name).map(Vec::as_slice).unwrap_or(&[]),
                            ));
                        }
                        self.tasks.push(Task::ObjectField { id, desc, field: 0 });
                    } else if d.flags & SC_EXTERNALIZABLE != 0 {
                        if d.flags & SC_BLOCK_DATA == 0 {
                            return Err(JosError::Unsupported {
                                message: format!(
                                    "externalizable class {} lacks block-data framing",
                                    python_repr(&d.name)
                                ),
                                offset: None,
                            });
                        }
                        self.tasks.push(Task::Annotation(
                            o.annotations.get(&d.name).map(Vec::as_slice).unwrap_or(&[]),
                        ));
                    }
                }
            }
            Task::ObjectField { id, desc, field } => {
                let d = self.desc(desc)?;
                if let Some(f) = d.fields.get(field) {
                    let Node::Object(o) = self.node(id)? else {
                        unreachable!("object task");
                    };
                    let value = o
                        .field_data
                        .get(&d.name)
                        .and_then(|fields| fields.get(&f.name))
                        .unwrap_or(&Value::Null);
                    self.tasks.push(Task::ObjectField {
                        id,
                        desc,
                        field: field + 1,
                    });
                    if matches!(f.typecode, 'L' | '[') {
                        self.tasks.push(Task::Content(value));
                    } else {
                        self.primitive(f.typecode, value)?;
                    }
                }
            }
            Task::ArrayBody(id) => {
                let Node::Array { data, .. } = self.node(id)? else {
                    unreachable!("array task");
                };
                macro_rules! numeric {
                    ($v:expr) => {{
                        self.length($v.len())?;
                        for item in $v {
                            self.out.extend(item.to_be_bytes());
                        }
                    }};
                }
                match data {
                    ArrayData::Byte(v) => {
                        self.length(v.len())?;
                        self.out.extend(v.iter().map(|b| *b as u8));
                    }
                    ArrayData::Bool(v) => {
                        self.length(v.len())?;
                        self.out.extend(v.iter().map(|b| u8::from(*b)));
                    }
                    ArrayData::Char(v) => numeric!(v),
                    ArrayData::Short(v) => numeric!(v),
                    ArrayData::Int(v) => numeric!(v),
                    ArrayData::Long(v) => numeric!(v),
                    ArrayData::Float(v) => numeric!(v),
                    ArrayData::Double(v) => numeric!(v),
                    ArrayData::Object(v) => {
                        self.length(v.len())?;
                        for item in v.iter().rev() {
                            self.tasks.push(Task::Content(item));
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn length(&mut self, n: usize) -> Result<(), JosError> {
        self.out.extend(
            i32::try_from(n)
                .map_err(|_| JosError::format("length exceeds signed 32-bit range"))?
                .to_be_bytes(),
        );
        Ok(())
    }
    fn primitive(&mut self, code: char, value: &Value) -> Result<(), JosError> {
        match (code, value) {
            ('B', Value::Byte(v)) => self.out.push(*v as u8),
            ('C', Value::Char(v)) => self.out.extend(v.to_be_bytes()),
            ('D', Value::Double(v)) => self.out.extend(v.to_be_bytes()),
            ('F', Value::Float(v)) => self.out.extend(v.to_be_bytes()),
            ('I', Value::Int(v)) => self.out.extend(v.to_be_bytes()),
            ('J', Value::Long(v)) => self.out.extend(v.to_be_bytes()),
            ('S', Value::Short(v)) => self.out.extend(v.to_be_bytes()),
            ('Z', Value::Bool(v)) => self.out.push(u8::from(*v)),
            _ => {
                return Err(JosError::format(format!(
                    "invalid value for field typecode {}",
                    python_repr(&code.to_string())
                )));
            }
        }
        Ok(())
    }
}
