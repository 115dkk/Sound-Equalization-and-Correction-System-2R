// Port of src/secs/rew_mdat/serialization/parser.py (GPL-3.0-only).
use crate::{
    constants::*,
    nodes::{TextKey, TextSlot},
    reader::Reader,
    *,
};
use std::collections::BTreeMap;

/// Parse an untrusted stream without recursive calls or class execution.
pub fn parse(data: &[u8], limits: &Limits) -> Result<Stream, JosError> {
    if data.len() > limits.max_file_bytes {
        return Err(JosError::limit(
            format!(
                "file size {} exceeds limit {}",
                data.len(),
                limits.max_file_bytes
            ),
            0,
        ));
    }
    let mut p = Parser {
        r: Reader {
            data,
            offset: 0,
            limits,
            allocated: 0,
        },
        stream: Stream::default(),
        handles: Vec::new(),
        tasks: Vec::new(),
        last: Value::Null,
    };
    if p.r.u2()? != 0xaced || p.r.u2()? != 5 {
        return Err(JosError::format("invalid Java serialization header").at(0));
    }
    while p.r.offset < data.len() {
        if p.stream.values.len() >= limits.max_top_level_values {
            return Err(JosError::limit(
                "top-level value limit exceeded",
                p.r.offset,
            ));
        }
        p.schedule(Kind::Content, false, 1, p.r.offset);
        while let Some(task) = p.tasks.pop() {
            let origin = task.origin;
            p.step(task).map_err(|e| e.at(origin))?;
        }
        p.r.charge(2 * std::mem::size_of::<Value>())?;
        p.stream.values.push(p.last.clone());
    }
    p.stream.bytes_consumed = p.r.offset;
    Ok(p.stream)
}
struct Task {
    kind: Kind,
    descriptor: bool,
    depth: usize,
    origin: usize,
}
enum Kind {
    Content,
    Fields {
        id: NodeId,
        remaining: usize,
    },
    Signature {
        id: NodeId,
        field: FieldDescriptor,
        remaining: usize,
    },
    Annotations {
        id: NodeId,
        owner: Option<String>,
    },
    AnnotationValue {
        id: NodeId,
        owner: Option<String>,
    },
    Super(NodeId),
    SuperValue(NodeId),
    New(u8),
    ObjectClass {
        id: NodeId,
        hierarchy: Vec<NodeId>,
        index: usize,
    },
    ObjectField {
        id: NodeId,
        hierarchy: Vec<NodeId>,
        index: usize,
        field: usize,
    },
    FieldValue {
        id: NodeId,
        hierarchy: Vec<NodeId>,
        index: usize,
        field: usize,
    },
    ArrayItem {
        id: NodeId,
        remaining: usize,
    },
    ArrayValue {
        id: NodeId,
        remaining: usize,
    },
    EnumValue(NodeId),
    ExceptionValue,
    Return(NodeId),
}
struct Parser<'a> {
    r: Reader<'a>,
    stream: Stream,
    handles: Vec<NodeId>,
    tasks: Vec<Task>,
    last: Value,
}
impl Parser<'_> {
    fn schedule(&mut self, kind: Kind, descriptor: bool, depth: usize, origin: usize) {
        self.tasks.push(Task {
            kind,
            descriptor,
            depth,
            origin,
        });
    }
    fn add(&mut self, node: Node) -> Result<NodeId, JosError> {
        if self.stream.handle_count >= self.r.limits.max_handles {
            return Err(JosError::limit("handle limit exceeded", self.r.offset));
        }
        self.r
            .charge(2 * (std::mem::size_of::<Node>() + std::mem::size_of::<NodeId>()))?;
        let id = self.stream.push(node);
        self.handles.push(id);
        self.stream.handle_count += 1;
        Ok(id)
    }
    fn desc_mut(&mut self, id: NodeId) -> &mut ClassDescriptor {
        match &mut self.stream.nodes[id] {
            Node::ClassDesc(d) => d,
            _ => unreachable!("internal descriptor task"),
        }
    }
    fn text(&mut self, id: NodeId, slot: TextSlot, long: bool) -> Result<String, JosError> {
        let (text, raw) = self.r.utf(long)?;
        // Only non-scalar UTF-16 needs auxiliary storage.
        if text.contains('\u{fffd}') && encode_modified_utf8(&text) != raw {
            self.stream
                .raw_text
                .insert(TextKey(id, slot), (text.clone(), raw));
        }
        Ok(text)
    }
    fn step(&mut self, task: Task) -> Result<(), JosError> {
        let Task {
            kind,
            descriptor,
            depth,
            origin,
        } = task;
        match kind {
            Kind::Content => {
                if depth > self.r.limits.max_depth {
                    return Err(JosError::limit(
                        "nesting depth limit exceeded",
                        self.r.offset,
                    ));
                }
                let token = self.r.u1()?;
                self.r.charge(4 * std::mem::size_of::<Task>())?;
                if descriptor
                    && !matches!(
                        token,
                        TC_NULL | TC_REFERENCE | TC_CLASSDESC | TC_PROXYCLASSDESC
                    )
                {
                    return Err(JosError::format(format!(
                        "expected class descriptor, got token 0x{token:02x}"
                    )));
                }
                match token {
                    TC_NULL => self.last = Value::Null,
                    TC_REFERENCE => {
                        let handle = self.r.u4()?;
                        let id = handle
                            .checked_sub(BASE_HANDLE)
                            .and_then(|i| self.handles.get(i as usize))
                            .copied();
                        if descriptor && !id.is_some_and(|id| self.stream.descriptor(id).is_some())
                        {
                            return Err(JosError::InvalidHandle {
                                message: format!("handle 0x{handle:08x} is not a class descriptor"),
                                offset: None,
                            });
                        }
                        self.last = Value::Node(id.ok_or_else(|| JosError::InvalidHandle {
                            message: format!("invalid or reset handle 0x{handle:08x}"),
                            offset: None,
                        })?);
                    }
                    TC_STRING | TC_LONGSTRING => {
                        let id = self.stream.nodes.len();
                        let long = token == TC_LONGSTRING;
                        let value = self.text(id, TextSlot::String, long)?;
                        self.last = Value::Node(self.add(Node::String { value, long })?);
                    }
                    TC_CLASSDESC | TC_PROXYCLASSDESC => {
                        let proxy = token == TC_PROXYCLASSDESC;
                        let id = self.stream.nodes.len();
                        let (name, uid) = if proxy {
                            ("<proxy>".into(), 0)
                        } else {
                            (self.text(id, TextSlot::Name, false)?, self.r.i8()?)
                        };
                        self.add(Node::ClassDesc(ClassDescriptor {
                            name,
                            serial_version_uid: uid,
                            is_proxy: proxy,
                            ..Default::default()
                        }))?;
                        self.schedule(Kind::Super(id), false, depth, origin);
                        self.schedule(Kind::Annotations { id, owner: None }, false, depth, origin);
                        if proxy {
                            self.desc_mut(id).flags = SC_SERIALIZABLE;
                            let count = self.r.i4()?;
                            if count < 0 || count as usize > self.r.limits.max_fields_per_class {
                                return Err(JosError::limit(
                                    "proxy interface limit exceeded",
                                    self.r.offset,
                                ));
                            }
                            for i in 0..count as usize {
                                let name = self.text(id, TextSlot::Interface(i), false)?;
                                self.desc_mut(id).interfaces.push(name);
                            }
                        } else {
                            let flags = self.r.u1()?;
                            self.desc_mut(id).flags = flags;
                            let count = usize::from(self.r.u2()?);
                            if count > self.r.limits.max_fields_per_class {
                                return Err(JosError::limit(
                                    "class field limit exceeded",
                                    self.r.offset,
                                ));
                            }
                            self.schedule(
                                Kind::Fields {
                                    id,
                                    remaining: count,
                                },
                                false,
                                depth,
                                origin,
                            );
                        }
                    }
                    TC_OBJECT | TC_ARRAY | TC_CLASS | TC_ENUM => {
                        self.schedule(Kind::New(token), false, depth, origin);
                        self.schedule(Kind::Content, true, depth, origin);
                    }
                    TC_BLOCKDATA | TC_BLOCKDATALONG => {
                        let long = token == TC_BLOCKDATALONG;
                        let length = if long {
                            self.r.i4()?
                        } else {
                            i32::from(self.r.u1()?)
                        };
                        if length < 0 {
                            return Err(JosError::format(format!("negative byte count {length}"))
                                .at(self.r.offset));
                        }
                        if long && length as usize > self.r.limits.max_array_bytes {
                            return Err(JosError::limit(
                                "block-data byte limit exceeded",
                                self.r.offset,
                            ));
                        }
                        let raw = self.r.read(length as usize)?;
                        self.r.charge(raw.len() + 2 * std::mem::size_of::<Node>())?;
                        let data = raw.to_vec();
                        self.last = Value::Node(self.stream.push(Node::BlockData { data, long }));
                    }
                    TC_ENDBLOCKDATA => self.last = Value::End,
                    TC_RESET => {
                        self.handles.clear();
                        self.last = Value::Reset;
                    }
                    TC_EXCEPTION => {
                        self.schedule(Kind::ExceptionValue, false, depth, origin);
                        self.schedule(Kind::Content, false, depth + 1, self.r.offset);
                    }
                    _ => {
                        return Err(JosError::format(format!(
                            "unknown serialization token 0x{token:02x}"
                        )));
                    }
                }
            }
            Kind::Fields { id, remaining } => {
                if remaining != 0 {
                    let typecode = char::from(self.r.u1()?);
                    if !"BCDFIJSZL[".contains(typecode) {
                        return Err(JosError::format(format!(
                            "invalid field typecode {}",
                            python_repr(&typecode.to_string())
                        )));
                    }
                    let index = self.desc_mut(id).fields.len();
                    let name = self.text(id, TextSlot::Field(index), false)?;
                    self.r.charge(2 * std::mem::size_of::<FieldDescriptor>())?;
                    let field = FieldDescriptor {
                        typecode,
                        name,
                        type_string: None,
                        type_string_node: None,
                    };
                    if matches!(typecode, 'L' | '[') {
                        self.schedule(
                            Kind::Signature {
                                id,
                                field,
                                remaining: remaining - 1,
                            },
                            false,
                            depth,
                            origin,
                        );
                        self.schedule(Kind::Content, false, depth + 1, self.r.offset);
                    } else {
                        self.desc_mut(id).fields.push(field);
                        self.schedule(
                            Kind::Fields {
                                id,
                                remaining: remaining - 1,
                            },
                            false,
                            depth,
                            origin,
                        );
                    }
                }
            }
            Kind::Signature {
                id,
                mut field,
                remaining,
            } => {
                match &self.last {
                    Value::Null => {}
                    Value::Node(n) => match &self.stream.nodes[*n] {
                        Node::String { value, .. } => {
                            self.r.charge(value.len())?;
                            field.type_string = Some(value.clone());
                            field.type_string_node = Some(*n);
                        }
                        _ => return Err(JosError::format("field type signature is not a string")),
                    },
                    _ => return Err(JosError::format("field type signature is not a string")),
                }
                self.desc_mut(id).fields.push(field);
                self.schedule(Kind::Fields { id, remaining }, false, depth, origin);
            }
            Kind::Annotations { id, owner } => {
                self.schedule(Kind::AnnotationValue { id, owner }, false, depth, origin);
                self.schedule(Kind::Content, false, depth + 1, self.r.offset);
            }
            Kind::AnnotationValue { id, owner } => {
                if self.last != Value::End {
                    if self.last != Value::Reset {
                        self.r.charge(128 + owner.as_ref().map_or(0, String::len))?;
                        let value = self.last.clone();
                        match &mut self.stream.nodes[id] {
                            Node::ClassDesc(d) => d.annotations.push(value),
                            Node::Object(o) => o
                                .annotations
                                .entry(owner.clone().unwrap_or_default())
                                .or_default()
                                .push(value),
                            _ => unreachable!("internal annotation task"),
                        }
                    }
                    self.schedule(Kind::Annotations { id, owner }, false, depth, origin);
                }
            }
            Kind::Super(id) => {
                self.schedule(Kind::SuperValue(id), false, depth, origin);
                self.schedule(Kind::Content, true, depth + 1, origin);
            }
            Kind::SuperValue(id) => {
                self.desc_mut(id).super_desc = match self.last {
                    Value::Node(n) => Some(n),
                    _ => None,
                };
                let hierarchy = self
                    .stream
                    .hierarchy(id)
                    .ok_or_else(|| JosError::format("cyclic class descriptor hierarchy"))?;
                if hierarchy.len().saturating_add(depth - 1) > self.r.limits.max_depth {
                    return Err(JosError::limit(
                        "nesting depth limit exceeded",
                        self.r.offset,
                    ));
                }
                self.last = Value::Node(id);
            }
            Kind::New(token) => {
                let name = match token {
                    TC_OBJECT => "object has a null class descriptor",
                    TC_ARRAY => "array has a null class descriptor",
                    TC_CLASS => "class token has a null descriptor",
                    _ => "enum token has a null descriptor",
                };
                let Value::Node(classdesc) = self.last else {
                    return Err(JosError::format(name));
                };
                match token {
                    TC_OBJECT => {
                        let id = self.add(Node::Object(JavaObject {
                            classdesc,
                            field_data: BTreeMap::new(),
                            annotations: BTreeMap::new(),
                        }))?;
                        let mut hierarchy = self
                            .stream
                            .hierarchy(classdesc)
                            .ok_or_else(|| JosError::format("cyclic class descriptor hierarchy"))?;
                        if hierarchy.len().saturating_add(depth - 1) > self.r.limits.max_depth {
                            return Err(JosError::limit(
                                "nesting depth limit exceeded",
                                self.r.offset,
                            ));
                        }
                        hierarchy.reverse();
                        self.schedule(Kind::Return(id), false, depth, origin);
                        self.schedule(
                            Kind::ObjectClass {
                                id,
                                hierarchy,
                                index: 0,
                            },
                            false,
                            depth,
                            origin,
                        );
                    }
                    TC_ARRAY => {
                        let id = self.add(Node::Array {
                            classdesc,
                            data: ArrayData::Object(Vec::new()),
                        })?;
                        let count = self.r.i4()?;
                        if count < 0 {
                            return Err(JosError::format(format!("negative array length {count}")));
                        }
                        let count = count as usize;
                        if count > self.r.limits.max_array_elements {
                            return Err(JosError::limit(
                                "array element limit exceeded",
                                self.r.offset,
                            ));
                        }
                        let name = &self
                            .stream
                            .descriptor(classdesc)
                            .expect("validated descriptor")
                            .name;
                        let code = if name.starts_with('[') {
                            name.chars().nth(1).ok_or_else(|| {
                                JosError::format("array descriptor has no component type")
                            })?
                        } else {
                            'L'
                        };
                        if let Some(width) = primitive_width(code) {
                            let bytes = count
                                .checked_mul(width)
                                .filter(|n| *n <= self.r.limits.max_array_bytes)
                                .ok_or_else(|| {
                                    JosError::limit("array byte limit exceeded", self.r.offset)
                                })?;
                            let raw = self.r.read(bytes)?;
                            self.r.charge(bytes)?;
                            let data = primitive_array(code, raw);
                            self.stream.nodes[id] = Node::Array { classdesc, data };
                            self.last = Value::Node(id);
                        } else {
                            self.schedule(Kind::Return(id), false, depth, origin);
                            self.schedule(
                                Kind::ArrayItem {
                                    id,
                                    remaining: count,
                                },
                                false,
                                depth,
                                origin,
                            );
                        }
                    }
                    TC_CLASS => self.last = Value::Node(self.add(Node::Class { classdesc })?),
                    _ => {
                        let id = self.add(Node::Enum {
                            classdesc,
                            constant: Value::Null,
                        })?;
                        self.schedule(Kind::EnumValue(id), false, depth, origin);
                        self.schedule(Kind::Content, false, depth + 1, self.r.offset);
                    }
                }
            }
            Kind::ObjectClass {
                id,
                hierarchy,
                index,
            } => {
                if let Some(&current) = hierarchy.get(index) {
                    let d = self
                        .stream
                        .descriptor(current)
                        .expect("validated hierarchy");
                    let flags = d.flags;
                    self.r.charge(256 + d.name.len().saturating_mul(2))?;
                    let name = d.name.clone();
                    if flags & SC_SERIALIZABLE != 0 {
                        if let Node::Object(o) = &mut self.stream.nodes[id] {
                            o.field_data.insert(name.clone(), BTreeMap::new());
                        }
                        self.schedule(
                            Kind::ObjectField {
                                id,
                                hierarchy,
                                index,
                                field: 0,
                            },
                            false,
                            depth,
                            origin,
                        );
                    } else {
                        if flags & SC_EXTERNALIZABLE != 0 && flags & SC_BLOCK_DATA == 0 {
                            return Err(JosError::Unsupported {
                                message: format!(
                                    "externalizable class {} lacks block-data framing",
                                    python_repr(&name)
                                ),
                                offset: None,
                            });
                        }
                        self.schedule(
                            Kind::ObjectClass {
                                id,
                                hierarchy,
                                index: index + 1,
                            },
                            false,
                            depth,
                            origin,
                        );
                        if flags & SC_EXTERNALIZABLE != 0 {
                            self.schedule(
                                Kind::Annotations {
                                    id,
                                    owner: Some(name),
                                },
                                false,
                                depth,
                                origin,
                            );
                        }
                    }
                }
            }
            Kind::ObjectField {
                id,
                hierarchy,
                index,
                field,
            } => {
                let d = self
                    .stream
                    .descriptor(hierarchy[index])
                    .expect("validated hierarchy");
                if let Some(f) = d.fields.get(field) {
                    let code = f.typecode;
                    self.schedule(
                        Kind::FieldValue {
                            id,
                            hierarchy,
                            index,
                            field,
                        },
                        false,
                        depth,
                        origin,
                    );
                    if matches!(code, 'L' | '[') {
                        self.schedule(Kind::Content, false, depth + 1, self.r.offset);
                    } else {
                        self.last = self.primitive(code)?;
                    }
                } else {
                    self.r.charge(d.name.len())?;
                    let name = d.name.clone();
                    let annotations = d.flags & SC_WRITE_METHOD != 0;
                    self.schedule(
                        Kind::ObjectClass {
                            id,
                            hierarchy,
                            index: index + 1,
                        },
                        false,
                        depth,
                        origin,
                    );
                    if annotations {
                        self.schedule(
                            Kind::Annotations {
                                id,
                                owner: Some(name),
                            },
                            false,
                            depth,
                            origin,
                        );
                    }
                }
            }
            Kind::FieldValue {
                id,
                hierarchy,
                index,
                field,
            } => {
                let d = self
                    .stream
                    .descriptor(hierarchy[index])
                    .expect("validated hierarchy");
                self.r
                    .charge(128 + d.name.len() + d.fields[field].name.len())?;
                let (owner, name) = (d.name.clone(), d.fields[field].name.clone());
                if let Node::Object(o) = &mut self.stream.nodes[id] {
                    o.field_data
                        .entry(owner)
                        .or_default()
                        .insert(name, self.last.clone());
                }
                self.schedule(
                    Kind::ObjectField {
                        id,
                        hierarchy,
                        index,
                        field: field + 1,
                    },
                    false,
                    depth,
                    origin,
                );
            }
            Kind::ArrayItem { id, remaining } => {
                if remaining != 0 {
                    self.schedule(
                        Kind::ArrayValue {
                            id,
                            remaining: remaining - 1,
                        },
                        false,
                        depth,
                        origin,
                    );
                    self.schedule(Kind::Content, false, depth + 1, self.r.offset);
                }
            }
            Kind::ArrayValue { id, remaining } => {
                self.r.charge(2 * std::mem::size_of::<Value>())?;
                if let Node::Array {
                    data: ArrayData::Object(values),
                    ..
                } = &mut self.stream.nodes[id]
                {
                    values.push(self.last.clone());
                }
                self.schedule(Kind::ArrayItem { id, remaining }, false, depth, origin);
            }
            Kind::EnumValue(id) => {
                if let Node::Enum { constant, .. } = &mut self.stream.nodes[id] {
                    *constant = self.last.clone();
                }
                self.last = Value::Node(id);
            }
            Kind::ExceptionValue => {
                self.r.charge(2 * std::mem::size_of::<Node>())?;
                let exception = self.last.clone();
                self.last = Value::Node(self.stream.push(Node::Exception { exception }));
                self.handles.clear();
            }
            Kind::Return(id) => self.last = Value::Node(id),
        }
        Ok(())
    }
    fn primitive(&mut self, code: char) -> Result<Value, JosError> {
        Ok(match code {
            'B' => Value::Byte(self.r.u1()? as i8),
            'C' => Value::Char(self.r.u2()?),
            'D' => Value::Double(f64::from_be_bytes(self.r.fixed()?)),
            'F' => Value::Float(f32::from_be_bytes(self.r.fixed()?)),
            'I' => Value::Int(self.r.i4()?),
            'J' => Value::Long(self.r.i8()?),
            'S' => Value::Short(i16::from_be_bytes(self.r.fixed()?)),
            'Z' => Value::Bool(self.r.u1()? != 0),
            _ => return Err(JosError::format("invalid primitive type")),
        })
    }
}
fn primitive_width(code: char) -> Option<usize> {
    match code {
        'B' | 'Z' => Some(1),
        'C' | 'S' => Some(2),
        'F' | 'I' => Some(4),
        'D' | 'J' => Some(8),
        _ => None,
    }
}
fn primitive_array(code: char, raw: &[u8]) -> ArrayData {
    macro_rules! array {
        ($variant:ident,$ty:ty,$width:literal) => {
            ArrayData::$variant(
                raw.chunks_exact($width)
                    .map(|b| {
                        let mut a = [0; $width];
                        a.copy_from_slice(b);
                        <$ty>::from_be_bytes(a)
                    })
                    .collect(),
            )
        };
    }
    match code {
        'B' => ArrayData::Byte(raw.iter().map(|b| *b as i8).collect()),
        'Z' => ArrayData::Bool(raw.iter().map(|b| *b != 0).collect()),
        'C' => array!(Char, u16, 2),
        'S' => array!(Short, i16, 2),
        'F' => array!(Float, f32, 4),
        'I' => array!(Int, i32, 4),
        'D' => array!(Double, f64, 8),
        'J' => array!(Long, i64, 8),
        _ => unreachable!("primitive code"),
    }
}
pub(crate) fn python_repr(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut result = String::from(quote);
    for c in value.chars() {
        match c {
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c == quote => {
                result.push('\\');
                result.push(c);
            }
            c if c.is_control() => {
                let n = c as u32;
                if n <= 255 {
                    result.push_str(&format!("\\x{n:02x}"));
                } else {
                    result.push_str(&format!("\\u{n:04x}"));
                }
            }
            _ => result.push(c),
        }
    }
    result.push(quote);
    result
}
