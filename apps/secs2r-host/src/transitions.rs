use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

use secs_types::ipc::{HostLogLine, HostManaged, IpcError, ManagedCamilla, ManagedEqApo};

const MAX_REPAIR_ATTEMPTS: u8 = 3;
const REPAIR_DELAYS: [Duration; MAX_REPAIR_ATTEMPTS as usize - 1] =
    [Duration::from_secs(1), Duration::from_secs(2)];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEvent {
    Logon,
    Logoff,
}

#[derive(Debug, Clone)]
pub enum Input {
    SetManaged {
        update: HostManaged,
        requester_session: u32,
        requester_sid: String,
    },
    StopManaging,
    Session {
        event: SessionEvent,
        session_id: u32,
        owner_matches: bool,
    },
    ConfigChanged {
        owner_session: Option<u32>,
    },
    RestartTick {
        owner_session: Option<u32>,
    },
    RepairRetry,
}

#[derive(Debug, Clone)]
pub enum Effect {
    SaveState(HostManaged),
    ReplaceWatcher(Option<ManagedEqApo>),
    RepairEqapo {
        session: u32,
    },
    StopChild,
    LaunchChild {
        session: u32,
        target: ManagedCamilla,
    },
    Log(HostLogLine),
}

#[derive(Debug, Clone)]
pub struct State {
    pub managed: HostManaged,
    pub running_child_session: Option<u32>,
    pub restart_scheduled: bool,
    pub eqapo_error: Option<String>,
    repair_attempts: u8,
    repair_retry_after: Option<Duration>,
}

impl State {
    pub fn new(managed: HostManaged, running_child_session: Option<u32>) -> Self {
        Self {
            managed,
            running_child_session,
            restart_scheduled: false,
            eqapo_error: None,
            repair_attempts: 0,
            repair_retry_after: None,
        }
    }

    pub fn take_repair_retry(&mut self) -> Option<Duration> {
        self.repair_retry_after.take()
    }
}

pub trait Platform {
    fn save_state(&mut self, managed: &HostManaged) -> Result<(), IpcError>;
    fn replace_watcher(&mut self, target: Option<&ManagedEqApo>) -> Result<(), IpcError>;
    fn repair_eqapo(&mut self, session: u32) -> Result<(), IpcError>;
    fn stop_child(&mut self) -> Result<(), IpcError>;
    fn launch_child(&mut self, session: u32, target: &ManagedCamilla) -> Result<(), IpcError>;
    fn log(&mut self, line: &HostLogLine) -> Result<(), IpcError>;
}

pub fn decide(state: &State, input: Input) -> (State, Vec<Effect>) {
    let mut next = state.clone();
    let mut effects = Vec::new();
    match input {
        Input::SetManaged {
            update,
            requester_session,
            requester_sid,
        } => {
            let previous_camilla = next.managed.camilla.clone();
            let owner_changed = next.managed.owner_user_sid.as_deref() != Some(&requester_sid);
            if owner_changed {
                next.managed = HostManaged::default();
            }
            if update.camilla.is_some() {
                next.managed.camilla = update.camilla;
            }
            if update.eqapo.is_some() {
                next.managed.eqapo = update.eqapo;
            }
            next.managed.owner_session_id = Some(requester_session);
            next.managed.owner_user_sid = Some(requester_sid);
            next.eqapo_error = None;
            next.repair_attempts = 0;
            next.repair_retry_after = None;
            effects.push(Effect::SaveState(next.managed.clone()));
            effects.push(Effect::ReplaceWatcher(next.managed.eqapo.clone()));
            if next.managed.eqapo.is_some() {
                effects.push(Effect::RepairEqapo {
                    session: requester_session,
                });
            }
            if previous_camilla != next.managed.camilla && next.running_child_session.is_some() {
                effects.push(Effect::StopChild);
            }
            if let Some(target) = next.managed.camilla.clone()
                && (next.running_child_session.is_none()
                    || previous_camilla != next.managed.camilla)
            {
                effects.push(Effect::LaunchChild {
                    session: requester_session,
                    target,
                });
            }
        }
        Input::StopManaging => {
            next.managed = HostManaged::default();
            next.restart_scheduled = false;
            next.eqapo_error = None;
            next.repair_attempts = 0;
            next.repair_retry_after = None;
            effects.push(Effect::SaveState(next.managed.clone()));
            effects.push(Effect::ReplaceWatcher(None));
            if next.running_child_session.is_some() {
                effects.push(Effect::StopChild);
            }
        }
        Input::Session {
            event,
            session_id,
            owner_matches,
        } => {
            effects.push(Effect::Log(HostLogLine {
                timestamp: chrono::Utc::now().to_rfc3339(),
                event: match event {
                    SessionEvent::Logon => "session_logon",
                    SessionEvent::Logoff => "session_logoff",
                }
                .into(),
                fields: BTreeMap::from([("session".into(), session_id.to_string())]),
            }));
            match event {
                SessionEvent::Logon if owner_matches => {
                    next.managed.owner_session_id = Some(session_id);
                    if let Some(target) = next.managed.camilla.clone()
                        && next.running_child_session.is_none()
                    {
                        effects.push(Effect::LaunchChild {
                            session: session_id,
                            target,
                        });
                    }
                    if next.managed.eqapo.is_some() {
                        effects.push(Effect::RepairEqapo {
                            session: session_id,
                        });
                    }
                }
                SessionEvent::Logon => {
                    if next.running_child_session.is_some() {
                        effects.push(Effect::StopChild);
                    }
                    next.restart_scheduled = false;
                }
                SessionEvent::Logoff => {
                    if next.running_child_session == Some(session_id) {
                        effects.push(Effect::StopChild);
                        next.restart_scheduled = false;
                    }
                }
            }
        }
        Input::ConfigChanged { owner_session } => {
            next.repair_attempts = 0;
            next.repair_retry_after = None;
            if next.managed.eqapo.is_some()
                && let Some(session) = owner_session
            {
                effects.push(Effect::RepairEqapo { session });
            }
        }
        Input::RestartTick { owner_session } => {
            next.restart_scheduled = false;
            if let (Some(session), Some(target)) = (owner_session, next.managed.camilla.clone()) {
                effects.push(Effect::LaunchChild { session, target });
            }
        }
        Input::RepairRetry => {
            if next.managed.eqapo.is_some()
                && let Some(session) = next.managed.owner_session_id
            {
                effects.push(Effect::RepairEqapo { session });
            }
        }
    }
    (next, effects)
}

fn repair_failed_line(error: &IpcError, attempt: u8) -> HostLogLine {
    HostLogLine {
        timestamp: chrono::Utc::now().to_rfc3339(),
        event: "repair_failed".into(),
        fields: BTreeMap::from([
            ("attempt".into(), attempt.to_string()),
            ("detail".into(), error.detail.clone()),
        ]),
    }
}

pub fn drive(
    state: &mut State,
    effects: Vec<Effect>,
    platform: &mut impl Platform,
) -> Result<(), IpcError> {
    let mut effects = VecDeque::from(effects);
    while let Some(effect) = effects.pop_front() {
        match effect {
            Effect::SaveState(managed) => platform.save_state(&managed)?,
            Effect::ReplaceWatcher(target) => platform.replace_watcher(target.as_ref())?,
            Effect::RepairEqapo { session } => match platform.repair_eqapo(session) {
                Ok(()) => {
                    state.eqapo_error = None;
                    state.repair_attempts = 0;
                    state.repair_retry_after = None;
                }
                Err(error) => {
                    state.repair_attempts = state.repair_attempts.saturating_add(1);
                    state.eqapo_error = Some(error.detail.clone());
                    let _ = platform.log(&repair_failed_line(&error, state.repair_attempts));
                    state.repair_retry_after = if state.repair_attempts < MAX_REPAIR_ATTEMPTS {
                        Some(REPAIR_DELAYS[usize::from(state.repair_attempts - 1)])
                    } else {
                        None
                    };
                }
            },
            Effect::StopChild => {
                platform.stop_child()?;
                state.running_child_session = None;
            }
            Effect::LaunchChild { session, target } => {
                platform.launch_child(session, &target)?;
                state.running_child_session = Some(session);
            }
            Effect::Log(line) => platform.log(&line)?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use secs_types::ipc::{ErrorCode, ManagedCamilla, ManagedEqApo};

    use super::*;

    #[derive(Default)]
    struct FakePlatform {
        launches: Vec<(u32, String)>,
        repair_calls: usize,
        fail_repairs: bool,
        stops: usize,
    }

    impl Platform for FakePlatform {
        fn save_state(&mut self, _managed: &HostManaged) -> Result<(), IpcError> {
            Ok(())
        }

        fn replace_watcher(&mut self, _target: Option<&ManagedEqApo>) -> Result<(), IpcError> {
            Ok(())
        }

        fn repair_eqapo(&mut self, _session: u32) -> Result<(), IpcError> {
            self.repair_calls += 1;
            if self.fail_repairs {
                Err(IpcError::new(ErrorCode::ApplyFailed, "repair failed"))
            } else {
                Ok(())
            }
        }

        fn stop_child(&mut self) -> Result<(), IpcError> {
            self.stops += 1;
            Ok(())
        }

        fn launch_child(&mut self, session: u32, target: &ManagedCamilla) -> Result<(), IpcError> {
            self.launches.push((session, target.exe_path.clone()));
            Ok(())
        }

        fn log(&mut self, _line: &HostLogLine) -> Result<(), IpcError> {
            Ok(())
        }
    }

    fn camilla(exe: &str) -> ManagedCamilla {
        ManagedCamilla {
            exe_path: exe.into(),
            config_path: "room.yml".into(),
            port: 1234,
            design_id: "camilla-design".into(),
        }
    }

    fn eqapo() -> ManagedEqApo {
        ManagedEqApo {
            config_path: "apo".into(),
            design_id: "apo-design".into(),
        }
    }

    #[test]
    fn changing_owner_discards_the_previous_users_targets() {
        let mut state = State::new(HostManaged::default(), None);
        let mut platform = FakePlatform::default();
        let (next, effects) = decide(
            &state,
            Input::SetManaged {
                update: HostManaged {
                    camilla: Some(camilla("A-camilladsp.exe")),
                    ..HostManaged::default()
                },
                requester_session: 7,
                requester_sid: "sid-a".into(),
            },
        );
        state = next;
        drive(&mut state, effects, &mut platform).unwrap();
        state.running_child_session = None;
        platform.launches.clear();

        let (next, effects) = decide(
            &state,
            Input::SetManaged {
                update: HostManaged {
                    eqapo: Some(eqapo()),
                    ..HostManaged::default()
                },
                requester_session: 8,
                requester_sid: "sid-b".into(),
            },
        );
        state = next;
        drive(&mut state, effects, &mut platform).unwrap();

        assert_eq!(state.managed.camilla, None);
        assert_eq!(state.managed.eqapo, Some(eqapo()));
        assert!(platform.launches.is_empty());
    }

    #[test]
    fn same_owner_keeps_targets_omitted_from_an_update() {
        let target = camilla("A-camilladsp.exe");
        let state = State::new(
            HostManaged {
                camilla: Some(target.clone()),
                owner_session_id: Some(7),
                owner_user_sid: Some("sid-a".into()),
                ..HostManaged::default()
            },
            Some(7),
        );

        let (next, _effects) = decide(
            &state,
            Input::SetManaged {
                update: HostManaged {
                    eqapo: Some(eqapo()),
                    ..HostManaged::default()
                },
                requester_session: 9,
                requester_sid: "sid-a".into(),
            },
        );

        assert_eq!(next.managed.camilla, Some(target));
        assert_eq!(next.managed.eqapo, Some(eqapo()));
    }

    #[test]
    fn logoff_only_stops_a_child_running_in_that_session() {
        let mut state = State::new(
            HostManaged {
                camilla: Some(camilla("camilladsp.exe")),
                owner_session_id: Some(7),
                owner_user_sid: Some("sid-a".into()),
                ..HostManaged::default()
            },
            Some(7),
        );
        state.restart_scheduled = true;
        let mut platform = FakePlatform::default();

        let (next, effects) = decide(
            &state,
            Input::Session {
                event: SessionEvent::Logoff,
                session_id: 8,
                owner_matches: false,
            },
        );
        state = next;
        drive(&mut state, effects, &mut platform).unwrap();

        assert_eq!(state.running_child_session, Some(7));
        assert!(state.restart_scheduled);
        assert_eq!(platform.stops, 0);

        let (next, effects) = decide(
            &state,
            Input::Session {
                event: SessionEvent::Logoff,
                session_id: 7,
                owner_matches: true,
            },
        );
        state = next;
        drive(&mut state, effects, &mut platform).unwrap();
        assert_eq!(state.running_child_session, None);
        assert!(!state.restart_scheduled);
        assert_eq!(platform.stops, 1);
    }

    #[test]
    fn failed_eqapo_repair_does_not_stop_camilla_or_the_driver() {
        let mut state = State::new(
            HostManaged {
                camilla: Some(camilla("camilladsp.exe")),
                eqapo: Some(eqapo()),
                owner_session_id: Some(7),
                owner_user_sid: Some("sid-a".into()),
            },
            Some(7),
        );
        let mut platform = FakePlatform {
            fail_repairs: true,
            ..FakePlatform::default()
        };
        let (next, effects) = decide(
            &state,
            Input::ConfigChanged {
                owner_session: Some(7),
            },
        );
        state = next;

        assert!(drive(&mut state, effects, &mut platform).is_ok());
        while state.take_repair_retry().is_some() {
            let (next, effects) = decide(&state, Input::RepairRetry);
            state = next;
            drive(&mut state, effects, &mut platform).unwrap();
        }

        assert_eq!(state.running_child_session, Some(7));
        assert_eq!(state.eqapo_error.as_deref(), Some("repair failed"));
        assert_eq!(platform.repair_calls, 3);
        assert_eq!(platform.stops, 0);
    }
}
