use secs_types::ipc::{
    IpcError, PendingUpdate, UpdateInstall, UpdateNotice, UpdatePhase, UpdatePolicy, UpdateStatus,
};

use crate::LogLevel;

pub const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;
const RELEASES: &str =
    "https://github.com/115dkk/Sound-Equalization-and-Correction-System-2R/releases/tag/v";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundRelease {
    pub version: String,
    pub notes: String,
    pub previous: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateFailure {
    pub reason: &'static str,
    pub cause: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateEvent {
    Start {
        policy: UpdatePolicy,
        notice: Option<UpdateNotice>,
    },
    Tick,
    Check,
    CheckResult {
        generation: u64,
        result: Result<Option<FoundRelease>, UpdateFailure>,
    },
    Download,
    DownloadProgress {
        generation: u64,
        received: u64,
        total: Option<u64>,
    },
    DownloadResult {
        generation: u64,
        result: Result<(), UpdateFailure>,
    },
    Restart,
    DismissNotice,
    SetPolicy(UpdatePolicy),
    Exit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCommand {
    Check {
        generation: u64,
    },
    Download {
        generation: u64,
        release: FoundRelease,
    },
    Cancel,
    SavePending(PendingUpdate),
    RemovePending,
    Install {
        restart: bool,
    },
    Log {
        level: LogLevel,
        line: String,
    },
}

#[derive(Debug)]
pub struct UpdateMachine {
    status: UpdateStatus,
    portable: bool,
    install: UpdateInstall,
    found: Option<FoundRelease>,
    generation: u64,
    stopped: bool,
}

impl UpdateMachine {
    pub fn new(current: &str, portable: bool, install: UpdateInstall) -> Self {
        Self {
            status: UpdateStatus {
                current: current.to_owned(),
                policy: UpdatePolicy::Off,
                phase: UpdatePhase::Off,
                checked_at: None,
                notice: None,
            },
            portable,
            install,
            found: None,
            generation: 0,
            stopped: false,
        }
    }

    pub fn status(&self) -> &UpdateStatus {
        &self.status
    }

    pub fn step(&mut self, event: UpdateEvent) -> Result<Vec<UpdateCommand>, IpcError> {
        let commands = match event {
            UpdateEvent::Start { policy, notice } => {
                self.status.policy = policy;
                self.status.phase = if policy == UpdatePolicy::Off {
                    UpdatePhase::Off
                } else {
                    UpdatePhase::Idle
                };
                self.status.notice = notice;
                let _install = self.install;
                Vec::new()
            }
            UpdateEvent::Tick | UpdateEvent::Check => {
                self.generation = self.generation.wrapping_add(1);
                self.status.phase = UpdatePhase::Checking;
                vec![UpdateCommand::Check {
                    generation: self.generation,
                }]
            }
            UpdateEvent::CheckResult {
                generation,
                result: Ok(Some(release)),
            } if generation == self.generation && !self.stopped => {
                self.found = Some(release.clone());
                if self.portable {
                    self.status.phase = UpdatePhase::Portable {
                        version: release.version.clone(),
                        url: format!("{RELEASES}{}", release.version),
                    };
                    Vec::new()
                } else if self.status.policy == UpdatePolicy::Auto {
                    self.status.phase = UpdatePhase::Downloading {
                        version: release.version.clone(),
                        received: 0,
                        total: None,
                    };
                    vec![UpdateCommand::Download {
                        generation,
                        release,
                    }]
                } else {
                    self.status.phase = UpdatePhase::Available {
                        version: release.version,
                    };
                    Vec::new()
                }
            }
            UpdateEvent::DownloadProgress {
                generation,
                received,
                total,
            } if generation == self.generation
                && !self.stopped
                && matches!(self.status.phase, UpdatePhase::Downloading { .. }) =>
            {
                if received > MAX_DOWNLOAD_BYTES || total.is_some_and(|total| received > total) {
                    self.generation = self.generation.wrapping_add(1);
                    self.status.phase = UpdatePhase::Failed {
                        reason: "The update is larger than allowed".into(),
                    };
                    vec![UpdateCommand::Cancel]
                } else {
                    if let UpdatePhase::Downloading {
                        received: current,
                        total: expected,
                        ..
                    } = &mut self.status.phase
                    {
                        *current = received;
                        *expected = total;
                    }
                    Vec::new()
                }
            }
            UpdateEvent::DownloadResult { .. }
            | UpdateEvent::DownloadProgress { .. }
            | UpdateEvent::CheckResult { .. }
            | UpdateEvent::Download
            | UpdateEvent::Restart
            | UpdateEvent::DismissNotice
            | UpdateEvent::SetPolicy(_)
            | UpdateEvent::Exit => Vec::new(),
        };
        Ok(commands)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn downloading() -> (UpdateMachine, u64) {
        let mut machine = UpdateMachine::new("1.0.0", false, UpdateInstall::OnExit);
        machine
            .step(UpdateEvent::Start {
                policy: UpdatePolicy::Auto,
                notice: None,
            })
            .unwrap();
        let commands = machine.step(UpdateEvent::Tick).unwrap();
        let generation = match commands.as_slice() {
            [UpdateCommand::Check { generation }] => *generation,
            other => panic!("expected one check command, got {other:?}"),
        };
        machine
            .step(UpdateEvent::CheckResult {
                generation,
                result: Ok(Some(FoundRelease {
                    version: "1.1.0".into(),
                    notes: "Changes".into(),
                    previous: None,
                })),
            })
            .unwrap();
        (machine, generation)
    }

    #[test]
    fn download_past_the_cap_cancels_and_fails_before_buffering_td204() {
        let (mut machine, generation) = downloading();

        let commands = machine
            .step(UpdateEvent::DownloadProgress {
                generation,
                received: MAX_DOWNLOAD_BYTES + 1,
                total: None,
            })
            .unwrap();

        assert_eq!(commands, vec![UpdateCommand::Cancel]);
        assert_eq!(
            machine.status().phase,
            UpdatePhase::Failed {
                reason: "The update is larger than allowed".into()
            }
        );
        assert!(
            machine
                .step(UpdateEvent::DownloadResult {
                    generation,
                    result: Ok(()),
                })
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            machine.status().phase,
            UpdatePhase::Failed {
                reason: "The update is larger than allowed".into()
            }
        );
    }
}
