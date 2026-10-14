//! Available-memory query through sysinfo's safe interface.

use secs_compute::MemoryProbe;
use sysinfo::{MemoryRefreshKind, RefreshKind, System};

/// Operating-system implementation of the compute memory seam.
#[derive(Clone, Copy, Debug, Default)]
pub struct OsMemoryProbe;

impl MemoryProbe for OsMemoryProbe {
    fn available_bytes(&self) -> u64 {
        System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::everything()),
        )
        .available_memory()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_probe_reports_available_memory() {
        assert!(OsMemoryProbe.available_bytes() > 0);
    }
}
