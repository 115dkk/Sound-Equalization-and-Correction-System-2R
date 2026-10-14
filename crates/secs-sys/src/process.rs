//! Cross-platform process enumeration through sysinfo's safe interface.

use secs_rew::{RewError, launch::ProcessProbe};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

/// Operating-system implementation of REW's process-list seam.
#[derive(Clone, Copy, Debug, Default)]
pub struct OsProcessProbe;

impl ProcessProbe for OsProcessProbe {
    fn process_names(&self) -> Result<Vec<String>, RewError> {
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().without_tasks(),
        );
        Ok(system
            .processes()
            .values()
            .map(|process| process.name().to_string_lossy().into_owned())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_probe_contains_the_test_process() {
        let names = OsProcessProbe.process_names().unwrap();
        let current = std::env::current_exe()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(
            names
                .iter()
                .any(|name| !name.is_empty() && (name == &current || current.starts_with(name))),
            "{current:?} not in {names:?}"
        );
    }
}
