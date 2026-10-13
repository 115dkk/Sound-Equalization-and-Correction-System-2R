#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Java Object Serialization graphs, ported from SECS 2's
//! `src/secs/rew_mdat/serialization` (GPL-3.0-only).
//!
//! [`parse`] never loads classes and uses an explicit work stack and [`Limits`].
//! In addition to upstream's eight limits, conservative cumulative allocation
//! accounting uses `max_file_bytes` as the decoded-memory budget. Cyclic
//! superclass graphs are rejected and superclass chains consume depth. These
//! guards also bound repeated signature/name copies reached through references.
//! Nodes have arena identity: clone a [`Value`] to retain a reference, or use
//! [`Stream::clone_values`] to deep-copy a graph, including its descriptors.
//! [`write()`] serializes valid, caller-edited graphs in descriptor field order.
//! Use [`try_write`] when constructing graphs that may be invalid. `End` and
//! `Reset` are parser sentinels; upstream's writer rejects them as content.
//!
//! Rust strings cannot contain Java's lone UTF-16 surrogates. The text API
//! exposes these as U+FFFD; parsed streams privately retain their original
//! encodings so unedited text still round-trips exactly. Use the UTF-16 unit
//! codec when lone surrogates must be inspected or constructed losslessly.

pub mod constants;
mod error;
mod limits;
pub mod modified_utf8;
mod nodes;
mod parser;
mod reader;
mod writer;

pub use error::JosError;
pub use limits::Limits;
pub use modified_utf8::{decode_modified_utf8, encode_modified_utf8};
pub use nodes::{
    ArrayData, ClassDescriptor, FieldDescriptor, JavaObject, Node, NodeId, Stream, Value,
};
pub use parser::parse;
pub use writer::{try_write, write};

#[cfg(test)]
mod tests;
