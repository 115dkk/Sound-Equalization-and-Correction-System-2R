// Port of src/secs/rew_mdat/limits.py (GPL-3.0-only).
/// Resource limits for untrusted input. Defaults match upstream ParseLimits.
#[derive(Clone, Debug)]
pub struct Limits {
    pub max_file_bytes: usize,
    pub max_array_bytes: usize,
    pub max_array_elements: usize,
    pub max_string_bytes: usize,
    pub max_depth: usize,
    pub max_handles: usize,
    pub max_fields_per_class: usize,
    pub max_top_level_values: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_file_bytes: 536_870_912,
            max_array_bytes: 268_435_456,
            max_array_elements: 50_000_000,
            max_string_bytes: 16_777_216,
            max_depth: 256,
            max_handles: 2_000_000,
            max_fields_per_class: 4096,
            max_top_level_values: 100_000,
        }
    }
}
