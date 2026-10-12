// Port of src/secs/rew_mdat/serialization/nodes.py (GPL-3.0-only).
use std::collections::{BTreeMap, BTreeSet};

pub type NodeId = usize;
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Node(NodeId),
    Byte(i8),
    Char(u16),
    Double(f64),
    Float(f32),
    Int(i32),
    Long(i64),
    Short(i16),
    Bool(bool),
    End,
    Reset,
}
#[derive(Clone, Debug)]
pub enum Node {
    String { value: String, long: bool },
    ClassDesc(ClassDescriptor),
    Object(JavaObject),
    Array { classdesc: NodeId, data: ArrayData },
    Class { classdesc: NodeId },
    Enum { classdesc: NodeId, constant: Value },
    BlockData { data: Vec<u8>, long: bool },
    Exception { exception: Value },
}
#[derive(Clone, Debug)]
pub enum ArrayData {
    Byte(Vec<i8>),
    Char(Vec<u16>),
    Double(Vec<f64>),
    Float(Vec<f32>),
    Int(Vec<i32>),
    Long(Vec<i64>),
    Short(Vec<i16>),
    Bool(Vec<bool>),
    Object(Vec<Value>),
}
#[derive(Clone, Debug)]
pub struct FieldDescriptor {
    pub typecode: char,
    pub name: String,
    pub type_string: Option<String>,
    pub type_string_node: Option<NodeId>,
}
#[derive(Clone, Debug, Default)]
pub struct ClassDescriptor {
    pub name: String,
    pub serial_version_uid: i64,
    pub flags: u8,
    pub fields: Vec<FieldDescriptor>,
    pub annotations: Vec<Value>,
    pub super_desc: Option<NodeId>,
    pub interfaces: Vec<String>,
    pub is_proxy: bool,
}
#[derive(Clone, Debug)]
pub struct JavaObject {
    pub classdesc: NodeId,
    pub field_data: BTreeMap<String, BTreeMap<String, Value>>,
    pub annotations: BTreeMap<String, Vec<Value>>,
}
#[derive(Clone, Debug, Default)]
pub struct Stream {
    pub values: Vec<Value>,
    pub nodes: Vec<Node>,
    pub bytes_consumed: usize,
    pub handle_count: usize,
    pub(crate) raw_text: BTreeMap<TextKey, (String, Vec<u8>)>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct TextKey(pub NodeId, pub TextSlot);
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TextSlot {
    String,
    Name,
    Field(usize),
    Interface(usize),
}
impl Stream {
    pub fn push(&mut self, node: Node) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(node);
        id
    }
    pub(crate) fn descriptor(&self, id: NodeId) -> Option<&ClassDescriptor> {
        match self.nodes.get(id)? {
            Node::ClassDesc(d) => Some(d),
            _ => None,
        }
    }
    pub(crate) fn hierarchy(&self, id: NodeId) -> Option<Vec<NodeId>> {
        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        let mut next = Some(id);
        while let Some(id) = next {
            if !seen.insert(id) {
                return None;
            }
            let descriptor = self.descriptor(id)?;
            result.push(id);
            next = descriptor.super_desc;
        }
        Some(result)
    }
    /// Most-derived class wins when a field shadows an inherited field.
    pub fn get(&self, obj: NodeId, name: &str) -> Option<&Value> {
        let Node::Object(object) = self.nodes.get(obj)? else {
            return None;
        };
        for id in self.hierarchy(object.classdesc)? {
            if let Some(value) = object
                .field_data
                .get(&self.descriptor(id)?.name)
                .and_then(|fields| fields.get(name))
            {
                return Some(value);
            }
        }
        None
    }
    /// Update only an existing owning slot, never synthesize a field.
    pub fn set(&mut self, obj: NodeId, name: &str, value: Value) -> bool {
        let Some(Node::Object(object)) = self.nodes.get(obj) else {
            return false;
        };
        let Some(hierarchy) = self.hierarchy(object.classdesc) else {
            return false;
        };
        let owner = hierarchy
            .into_iter()
            .filter_map(|id| self.descriptor(id))
            .find(|d| {
                object
                    .field_data
                    .get(&d.name)
                    .is_some_and(|f| f.contains_key(name))
            })
            .map(|d| d.name.clone());
        if let (Some(owner), Some(Node::Object(object))) = (owner, self.nodes.get_mut(obj))
            && let Some(slot) = object
                .field_data
                .get_mut(&owner)
                .and_then(|fields| fields.get_mut(name))
        {
            *slot = value;
            return true;
        }
        false
    }
    pub fn class_name(&self, obj: NodeId) -> Option<&str> {
        let id = match self.nodes.get(obj)? {
            Node::ClassDesc(d) => return Some(&d.name),
            Node::Object(o) => o.classdesc,
            Node::Array { classdesc, .. }
            | Node::Class { classdesc }
            | Node::Enum { classdesc, .. } => *classdesc,
            _ => return None,
        };
        Some(&self.descriptor(id)?.name)
    }
    /// Base-first insertion order, with shadowed names retaining their position.
    pub fn field_names(&self, obj: NodeId) -> Vec<String> {
        let Some(Node::Object(object)) = self.nodes.get(obj) else {
            return Vec::new();
        };
        let Some(hierarchy) = self.hierarchy(object.classdesc) else {
            return Vec::new();
        };
        let mut names = Vec::new();
        let mut seen = BTreeSet::new();
        for id in hierarchy.into_iter().rev() {
            let Some(d) = self.descriptor(id) else {
                continue;
            };
            for field in &d.fields {
                if object
                    .field_data
                    .get(&d.name)
                    .is_some_and(|f| f.contains_key(&field.name))
                    && seen.insert(field.name.clone())
                {
                    names.push(field.name.clone());
                }
            }
        }
        names
    }
    /// Deep-copy one connected graph with a shared memo, including cycles,
    /// class descriptors and signature strings. Arena IDs outside this stream
    /// are left untouched (callers must supply valid IDs).
    pub fn clone_values(&mut self, values: &[Value]) -> Vec<Value> {
        let mut memo = BTreeMap::new();
        let mut pending = Vec::new();
        let result = values
            .iter()
            .map(|v| remap_value(v, &mut memo, &mut pending, &mut self.nodes))
            .collect();
        while let Some((old, new)) = pending.pop() {
            let mut node = self.nodes[old].clone();
            let mut remap = |id: &mut NodeId| {
                *id = remap_id(*id, &mut memo, &mut pending, &mut self.nodes);
            };
            match &mut node {
                Node::ClassDesc(d) => {
                    if let Some(id) = &mut d.super_desc {
                        remap(id);
                    }
                    for field in &mut d.fields {
                        if let Some(id) = &mut field.type_string_node {
                            remap(id);
                        }
                    }
                    visit_values(&mut d.annotations, &mut remap);
                }
                Node::Object(o) => {
                    remap(&mut o.classdesc);
                    for fields in o.field_data.values_mut() {
                        for v in fields.values_mut() {
                            visit_value(v, &mut remap);
                        }
                    }
                    for values in o.annotations.values_mut() {
                        visit_values(values, &mut remap);
                    }
                }
                Node::Array { classdesc, data } => {
                    remap(classdesc);
                    if let ArrayData::Object(v) = data {
                        visit_values(v, &mut remap);
                    }
                }
                Node::Class { classdesc } => remap(classdesc),
                Node::Enum {
                    classdesc,
                    constant,
                } => {
                    remap(classdesc);
                    visit_value(constant, &mut remap);
                }
                Node::Exception { exception } => visit_value(exception, &mut remap),
                _ => {}
            }
            self.nodes[new] = node;
            let texts: Vec<_> = self
                .raw_text
                .range(
                    TextKey(old, TextSlot::String)..=TextKey(old, TextSlot::Interface(usize::MAX)),
                )
                .map(|(k, v)| (TextKey(new, k.1.clone()), v.clone()))
                .collect();
            self.raw_text.extend(texts);
        }
        result
    }
}
fn visit_value(v: &mut Value, f: &mut impl FnMut(&mut NodeId)) {
    if let Value::Node(id) = v {
        f(id);
    }
}
fn visit_values(values: &mut [Value], f: &mut impl FnMut(&mut NodeId)) {
    for v in values {
        visit_value(v, f);
    }
}
fn remap_id(
    id: NodeId,
    memo: &mut BTreeMap<NodeId, NodeId>,
    pending: &mut Vec<(NodeId, NodeId)>,
    nodes: &mut Vec<Node>,
) -> NodeId {
    if let Some(new) = memo.get(&id) {
        return *new;
    }
    if id >= nodes.len() {
        return id;
    }
    let new = nodes.len();
    nodes.push(Node::Exception {
        exception: Value::Null,
    });
    memo.insert(id, new);
    pending.push((id, new));
    new
}
fn remap_value(
    value: &Value,
    memo: &mut BTreeMap<NodeId, NodeId>,
    pending: &mut Vec<(NodeId, NodeId)>,
    nodes: &mut Vec<Node>,
) -> Value {
    match value {
        Value::Node(id) => Value::Node(remap_id(*id, memo, pending, nodes)),
        _ => value.clone(),
    }
}
