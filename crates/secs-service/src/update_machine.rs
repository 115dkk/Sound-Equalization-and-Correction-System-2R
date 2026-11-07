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

    fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }

    fn begin_check(&mut self) -> Vec<UpdateCommand> {
        let generation = self.next_generation();
        self.found = None;
        self.status.phase = UpdatePhase::Checking;
        vec![UpdateCommand::Check { generation }]
    }

    fn begin_download(&mut self, release: FoundRelease) -> Vec<UpdateCommand> {
        let generation = self.next_generation();
        self.status.phase = UpdatePhase::Downloading {
            version: release.version.clone(),
            received: 0,
            total: None,
        };
        vec![UpdateCommand::Download {
            generation,
            release,
        }]
    }

    fn fail(&mut self, failure: UpdateFailure) -> Vec<UpdateCommand> {
        self.status.phase = UpdatePhase::Failed {
            reason: failure.reason.into(),
        };
        vec![UpdateCommand::Log {
            level: LogLevel::Error,
            line: failure.cause,
        }]
    }

    pub fn step(&mut self, event: UpdateEvent) -> Result<Vec<UpdateCommand>, IpcError> {
        match event {
            UpdateEvent::Start { policy, notice } => {
                self.status.policy = policy;
                self.status.phase = if policy == UpdatePolicy::Off {
                    UpdatePhase::Off
                } else {
                    UpdatePhase::Idle
                };
                self.status.notice = notice;
                self.stopped = false;
                Ok(Vec::new())
            }
            UpdateEvent::Tick => {
                if self.stopped
                    || self.status.policy == UpdatePolicy::Off
                    || matches!(
                        self.status.phase,
                        UpdatePhase::Checking
                            | UpdatePhase::Downloading { .. }
                            | UpdatePhase::Ready { .. }
                            | UpdatePhase::Installing { .. }
                    )
                {
                    Ok(Vec::new())
                } else {
                    Ok(self.begin_check())
                }
            }
            UpdateEvent::Check => {
                if self.stopped || matches!(self.status.phase, UpdatePhase::Installing { .. }) {
                    Err(invalid("The app is closing"))
                } else {
                    Ok(self.begin_check())
                }
            }
            UpdateEvent::CheckResult { generation, .. }
                if generation != self.generation || self.stopped =>
            {
                Ok(Vec::new())
            }
            UpdateEvent::CheckResult { result, .. } => {
                self.status.checked_at = Some(chrono::Utc::now().to_rfc3339());
                match result {
                    Ok(Some(release)) => {
                        self.found = Some(release.clone());
                        if self.portable {
                            self.status.phase = UpdatePhase::Portable {
                                version: release.version.clone(),
                                url: format!("{RELEASES}{}", release.version),
                            };
                            Ok(Vec::new())
                        } else if self.status.policy == UpdatePolicy::Auto {
                            Ok(self.begin_download(release))
                        } else {
                            self.status.phase = UpdatePhase::Available {
                                version: release.version,
                            };
                            Ok(Vec::new())
                        }
                    }
                    Ok(None) => {
                        self.found = None;
                        self.status.phase = UpdatePhase::UpToDate;
                        Ok(Vec::new())
                    }
                    Err(failure) => {
                        self.found = None;
                        Ok(self.fail(failure))
                    }
                }
            }
            UpdateEvent::Download => {
                if self.stopped {
                    return Err(invalid("The app is closing"));
                }
                if !matches!(self.status.phase, UpdatePhase::Available { .. }) {
                    return Err(invalid("No update is available to download"));
                }
                let release = self
                    .found
                    .clone()
                    .ok_or_else(|| invalid("No update is available to download"))?;
                Ok(self.begin_download(release))
            }
            UpdateEvent::DownloadProgress { generation, .. }
                if generation != self.generation || self.stopped =>
            {
                Ok(Vec::new())
            }
            UpdateEvent::DownloadProgress {
                received, total, ..
            } => {
                if !matches!(self.status.phase, UpdatePhase::Downloading { .. }) {
                    return Ok(Vec::new());
                }
                let exceeds_total = total.is_some_and(|total| received > total);
                let exceeds_cap = received > MAX_DOWNLOAD_BYTES
                    || total.is_some_and(|total| total > MAX_DOWNLOAD_BYTES);
                if exceeds_total || exceeds_cap {
                    self.next_generation();
                    self.status.phase = UpdatePhase::Failed {
                        reason: "The update is larger than allowed".into(),
                    };
                    return Ok(vec![UpdateCommand::Cancel]);
                }
                if let UpdatePhase::Downloading {
                    received: current,
                    total: expected,
                    ..
                } = &mut self.status.phase
                {
                    *current = received;
                    *expected = total;
                }
                Ok(Vec::new())
            }
            UpdateEvent::DownloadResult { generation, .. }
                if generation != self.generation || self.stopped =>
            {
                Ok(Vec::new())
            }
            UpdateEvent::DownloadResult { result, .. } => match result {
                Ok(()) => {
                    let Some(release) = self.found.clone() else {
                        return Ok(Vec::new());
                    };
                    self.status.phase = UpdatePhase::Ready {
                        version: release.version.clone(),
                        install: self.install,
                    };
                    Ok(vec![UpdateCommand::SavePending(PendingUpdate {
                        from: self.status.current.clone(),
                        to: release.version,
                        notes: release.notes,
                        previous: release.previous,
                    })])
                }
                Err(failure) => Ok(self.fail(failure)),
            },
            UpdateEvent::Restart => {
                if !matches!(self.status.phase, UpdatePhase::Ready { .. }) {
                    return Err(invalid("No update is ready to install"));
                }
                if self.stopped {
                    return Err(invalid("The app is closing"));
                }
                let version = self
                    .found
                    .as_ref()
                    .map_or_else(String::new, |release| release.version.clone());
                self.status.phase = UpdatePhase::Installing { version };
                Ok(vec![UpdateCommand::Install { restart: true }])
            }
            UpdateEvent::DismissNotice => {
                self.status.notice = None;
                if matches!(
                    self.status.phase,
                    UpdatePhase::Ready { .. } | UpdatePhase::Installing { .. }
                ) {
                    Ok(Vec::new())
                } else {
                    Ok(vec![UpdateCommand::RemovePending])
                }
            }
            UpdateEvent::SetPolicy(policy) => {
                self.status.policy = policy;
                if self.stopped || matches!(self.status.phase, UpdatePhase::Installing { .. }) {
                    return Ok(Vec::new());
                }
                if policy == UpdatePolicy::Off {
                    self.next_generation();
                    self.found = None;
                    self.status.phase = UpdatePhase::Off;
                    return Ok(vec![UpdateCommand::Cancel, UpdateCommand::RemovePending]);
                }
                if policy == UpdatePolicy::Notify
                    && matches!(self.status.phase, UpdatePhase::Downloading { .. })
                {
                    self.next_generation();
                    self.status.phase = self.found.as_ref().map_or(UpdatePhase::Idle, |release| {
                        UpdatePhase::Available {
                            version: release.version.clone(),
                        }
                    });
                    return Ok(vec![UpdateCommand::Cancel]);
                }
                if policy == UpdatePolicy::Auto
                    && matches!(self.status.phase, UpdatePhase::Available { .. })
                    && let Some(release) = self.found.clone()
                {
                    return Ok(self.begin_download(release));
                }
                if matches!(self.status.phase, UpdatePhase::Off) {
                    return Ok(self.begin_check());
                }
                Ok(Vec::new())
            }
            UpdateEvent::Exit => {
                if self.stopped {
                    return Ok(Vec::new());
                }
                self.stopped = true;
                self.next_generation();
                let mut commands = vec![UpdateCommand::Cancel];
                if let UpdatePhase::Ready {
                    version,
                    install: UpdateInstall::OnExit,
                } = &self.status.phase
                {
                    let version = version.clone();
                    self.status.phase = UpdatePhase::Installing { version };
                    commands.push(UpdateCommand::Install { restart: false });
                }
                Ok(commands)
            }
        }
    }
}

fn invalid(reason: &str) -> IpcError {
    use secs_types::ipc::ErrorCode;

    IpcError::new(ErrorCode::InvalidRequest, reason)
}

pub fn notice_from_pending(pending: PendingUpdate, current: &str) -> Option<UpdateNotice> {
    if pending.to != current {
        return None;
    }
    Some(UpdateNotice {
        skipped: pending
            .previous
            .as_ref()
            .is_some_and(|previous| previous != &pending.from),
        release_url: format!("{RELEASES}{}", pending.to),
        from: pending.from,
        to: pending.to,
        notes: pending.notes,
    })
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
        let commands = machine
            .step(UpdateEvent::CheckResult {
                generation,
                result: Ok(Some(release("1.1.0"))),
            })
            .unwrap();
        let generation = match commands.as_slice() {
            [UpdateCommand::Download { generation, .. }] => *generation,
            other => panic!("expected one download command, got {other:?}"),
        };
        (machine, generation)
    }

    fn release(version: &str) -> FoundRelease {
        FoundRelease {
            version: version.into(),
            notes: "Changes".into(),
            previous: Some("1.0.5".into()),
        }
    }

    fn checking(
        policy: UpdatePolicy,
        portable: bool,
        install: UpdateInstall,
    ) -> (UpdateMachine, u64) {
        let mut machine = UpdateMachine::new("1.0.0", portable, install);
        machine
            .step(UpdateEvent::Start {
                policy,
                notice: None,
            })
            .unwrap();
        let commands = machine.step(UpdateEvent::Tick).unwrap();
        let generation = match commands.as_slice() {
            [UpdateCommand::Check { generation }] => *generation,
            other => panic!("expected one check command, got {other:?}"),
        };
        (machine, generation)
    }

    fn ready(install: UpdateInstall) -> (UpdateMachine, u64) {
        let (mut machine, generation) = downloading();
        machine.install = install;
        let commands = machine
            .step(UpdateEvent::DownloadResult {
                generation,
                result: Ok(()),
            })
            .unwrap();
        assert!(matches!(
            commands.as_slice(),
            [UpdateCommand::SavePending(_)]
        ));
        (machine, generation)
    }

    #[test]
    fn auto_policy_downloads_a_found_release_at_once() {
        let (mut machine, generation) = checking(UpdatePolicy::Auto, false, UpdateInstall::OnExit);

        let commands = machine
            .step(UpdateEvent::CheckResult {
                generation,
                result: Ok(Some(release("1.1.0"))),
            })
            .unwrap();

        assert!(matches!(
            commands.as_slice(),
            [UpdateCommand::Download {
                generation: _,
                release: FoundRelease { version, .. },
            }] if version == "1.1.0"
        ));
        assert_eq!(
            machine.status().phase,
            UpdatePhase::Downloading {
                version: "1.1.0".into(),
                received: 0,
                total: None,
            }
        );
        assert!(machine.status().checked_at.is_some());
    }

    #[test]
    fn notify_policy_only_announces_a_found_release() {
        let (mut machine, generation) =
            checking(UpdatePolicy::Notify, false, UpdateInstall::OnExit);

        assert!(
            machine
                .step(UpdateEvent::CheckResult {
                    generation,
                    result: Ok(Some(release("1.1.0"))),
                })
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            machine.status().phase,
            UpdatePhase::Available {
                version: "1.1.0".into()
            }
        );
    }

    #[test]
    fn portable_build_reports_the_release_url() {
        let (mut machine, generation) = checking(UpdatePolicy::Auto, true, UpdateInstall::OnExit);

        assert!(
            machine
                .step(UpdateEvent::CheckResult {
                    generation,
                    result: Ok(Some(release("1.1.0"))),
                })
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            machine.status().phase,
            UpdatePhase::Portable {
                version: "1.1.0".into(),
                url: format!("{RELEASES}1.1.0"),
            }
        );
    }

    #[test]
    fn tick_is_ignored_while_checking_downloading_ready_or_installing() {
        let (mut checking_machine, _) = checking(UpdatePolicy::Auto, false, UpdateInstall::OnExit);
        let (mut downloading_machine, _) = downloading();
        let (mut ready_machine, _) = ready(UpdateInstall::OnExit);
        let (mut installing_machine, _) = ready(UpdateInstall::OnExit);
        installing_machine.step(UpdateEvent::Restart).unwrap();

        for machine in [
            &mut checking_machine,
            &mut downloading_machine,
            &mut ready_machine,
            &mut installing_machine,
        ] {
            let before = machine.status().phase.clone();
            assert!(machine.step(UpdateEvent::Tick).unwrap().is_empty());
            assert_eq!(machine.status().phase, before);
        }
    }

    #[test]
    fn late_check_result_with_an_old_generation_is_ignored() {
        let (mut machine, old_generation) =
            checking(UpdatePolicy::Notify, false, UpdateInstall::OnExit);
        let commands = machine.step(UpdateEvent::Check).unwrap();
        let new_generation = match commands.as_slice() {
            [UpdateCommand::Check { generation }] => *generation,
            other => panic!("expected one check command, got {other:?}"),
        };
        assert_ne!(old_generation, new_generation);

        assert!(
            machine
                .step(UpdateEvent::CheckResult {
                    generation: old_generation,
                    result: Ok(Some(release("9.9.9"))),
                })
                .unwrap()
                .is_empty()
        );
        assert_eq!(machine.status().phase, UpdatePhase::Checking);
    }

    #[test]
    fn set_policy_off_cancels_removes_pending_and_goes_off() {
        let (mut machine, generation) = downloading();

        assert_eq!(
            machine
                .step(UpdateEvent::SetPolicy(UpdatePolicy::Off))
                .unwrap(),
            vec![UpdateCommand::Cancel, UpdateCommand::RemovePending]
        );
        assert_eq!(machine.status().policy, UpdatePolicy::Off);
        assert_eq!(machine.status().phase, UpdatePhase::Off);
        assert!(
            machine
                .step(UpdateEvent::DownloadResult {
                    generation,
                    result: Ok(()),
                })
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn notify_while_downloading_cancels_back_to_available() {
        let (mut machine, _) = downloading();

        assert_eq!(
            machine
                .step(UpdateEvent::SetPolicy(UpdatePolicy::Notify))
                .unwrap(),
            vec![UpdateCommand::Cancel]
        );
        assert_eq!(
            machine.status().phase,
            UpdatePhase::Available {
                version: "1.1.0".into()
            }
        );
    }

    #[test]
    fn auto_while_available_starts_the_download() {
        let (mut machine, generation) =
            checking(UpdatePolicy::Notify, false, UpdateInstall::OnExit);
        machine
            .step(UpdateEvent::CheckResult {
                generation,
                result: Ok(Some(release("1.1.0"))),
            })
            .unwrap();

        let commands = machine
            .step(UpdateEvent::SetPolicy(UpdatePolicy::Auto))
            .unwrap();

        assert!(matches!(
            commands.as_slice(),
            [UpdateCommand::Download {
                generation: _,
                release: FoundRelease { version, .. },
            }] if version == "1.1.0"
        ));
        assert!(matches!(
            machine.status().phase,
            UpdatePhase::Downloading { .. }
        ));
    }

    #[test]
    fn exit_installs_only_a_ready_on_exit_release() {
        let (mut on_exit, _) = ready(UpdateInstall::OnExit);
        let (mut on_request, _) = ready(UpdateInstall::OnRequest);

        assert_eq!(
            on_exit.step(UpdateEvent::Exit).unwrap(),
            vec![
                UpdateCommand::Cancel,
                UpdateCommand::Install { restart: false }
            ]
        );
        assert!(matches!(
            on_exit.status().phase,
            UpdatePhase::Installing { .. }
        ));
        assert_eq!(
            on_request.step(UpdateEvent::Exit).unwrap(),
            vec![UpdateCommand::Cancel]
        );
        assert!(matches!(
            on_request.status().phase,
            UpdatePhase::Ready {
                install: UpdateInstall::OnRequest,
                ..
            }
        ));
    }

    #[test]
    fn failures_keep_the_user_sentence_and_log_the_cause_td164() {
        let (mut machine, generation) = checking(UpdatePolicy::Auto, false, UpdateInstall::OnExit);
        let failure = UpdateFailure {
            reason: "Could not check for updates",
            cause: "connection refused at 127.0.0.1:1".into(),
        };

        let commands = machine
            .step(UpdateEvent::CheckResult {
                generation,
                result: Err(failure),
            })
            .unwrap();

        assert_eq!(
            machine.status().phase,
            UpdatePhase::Failed {
                reason: "Could not check for updates".into()
            }
        );
        assert!(commands.iter().any(|command| matches!(
            command,
            UpdateCommand::Log {
                level: LogLevel::Error,
                line,
            } if line.contains("connection refused at 127.0.0.1:1")
        )));
    }

    #[test]
    fn update_policy_and_portable_decisions() {
        for (policy, phase) in [
            (UpdatePolicy::Off, UpdatePhase::Off),
            (UpdatePolicy::Auto, UpdatePhase::Idle),
            (UpdatePolicy::Notify, UpdatePhase::Idle),
        ] {
            let mut machine = UpdateMachine::new("1.0.0", false, UpdateInstall::OnExit);
            machine
                .step(UpdateEvent::Start {
                    policy,
                    notice: None,
                })
                .unwrap();
            assert_eq!(machine.status().phase, phase);
        }

        let (mut portable, generation) = checking(UpdatePolicy::Auto, true, UpdateInstall::OnExit);
        portable
            .step(UpdateEvent::CheckResult {
                generation,
                result: Ok(Some(release("1.2.3"))),
            })
            .unwrap();
        assert_eq!(
            portable.status().phase,
            UpdatePhase::Portable {
                version: "1.2.3".into(),
                url: format!("{RELEASES}1.2.3"),
            }
        );

        let (on_exit, _) = ready(UpdateInstall::OnExit);
        let (on_request, _) = ready(UpdateInstall::OnRequest);
        assert!(matches!(
            on_exit.status().phase,
            UpdatePhase::Ready {
                install: UpdateInstall::OnExit,
                ..
            }
        ));
        assert!(matches!(
            on_request.status().phase,
            UpdatePhase::Ready {
                install: UpdateInstall::OnRequest,
                ..
            }
        ));
    }

    #[test]
    fn update_actions_reject_wrong_phases() {
        for event in [UpdateEvent::Download, UpdateEvent::Restart] {
            let mut machine = UpdateMachine::new("1.0.0", false, UpdateInstall::OnExit);
            machine
                .step(UpdateEvent::Start {
                    policy: UpdatePolicy::Notify,
                    notice: None,
                })
                .unwrap();
            assert_eq!(
                machine.step(event).unwrap_err().code,
                secs_types::ipc::ErrorCode::InvalidRequest
            );
        }

        let (mut available, generation) =
            checking(UpdatePolicy::Notify, false, UpdateInstall::OnExit);
        available
            .step(UpdateEvent::CheckResult {
                generation,
                result: Ok(Some(release("1.2.3"))),
            })
            .unwrap();
        assert!(available.step(UpdateEvent::Download).is_ok());
        let (mut ready, _) = ready(UpdateInstall::OnExit);
        assert!(ready.step(UpdateEvent::Restart).is_ok());
    }

    #[test]
    fn update_notice_matches_running_version_and_tracks_skipped_releases() {
        let mut pending = PendingUpdate {
            from: "1.0.0".into(),
            to: "1.2.0".into(),
            notes: "Changes".into(),
            previous: Some("1.1.0".into()),
        };
        let notice = notice_from_pending(pending.clone(), "1.2.0").unwrap();
        assert!(notice.skipped);
        assert_eq!(notice.notes, "Changes");
        assert_eq!(notice.release_url, format!("{RELEASES}1.2.0"));
        assert!(notice_from_pending(pending.clone(), "1.0.0").is_none());
        pending.previous = Some(pending.from.clone());
        assert!(
            !notice_from_pending(pending.clone(), "1.2.0")
                .unwrap()
                .skipped
        );
        pending.previous = None;
        assert!(!notice_from_pending(pending, "1.2.0").unwrap().skipped);
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
