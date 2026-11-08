include!("record/core.rs");

/// Parses a measurement source sidecar with the file format's allocation limit.
///
/// This entry point lets fuzz tests exercise the sidecar codec without file I/O.
pub fn source_info_from_text(text: &str) -> Option<SourceInfo> {
    crate::pyjson::loads_with_budget(text, MAX_SOURCE_BYTES as usize)
        .ok()
        .and_then(|value| source::source_from_json(&value))
}
