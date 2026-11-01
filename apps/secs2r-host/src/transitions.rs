use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

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
        managing_session: Option<u32>,
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
    RepairRetry {
        managing_session: Option<u32>,
    },
}

#[derive(Debug, Clone)]
pub enum Effect {
    SaveState(HostManaged),
    ReplaceWatcher(Option<ManagedEqApo>),
    RepairEqapo {
        session: u32,
    },
    StopChild,
    ClearCamillaStatus,
    ClearEqapoStatus,
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
    repair_retry_pending: bool,
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
            repair_retry_pending: false,
        }
    }

    pub fn sync_repair_retry_deadline(&mut self, deadline: &mut Option<Instant>, now: Instant) {
        if !self.repair_retry_pending {
            *deadline = None;
        } else if deadline.is_none()
            && let Some(delay) = self.repair_retry_after.take()
        {
            *deadline = Some(now + delay);
        }
    }

    fn clear_repair_retry(&mut self) {
        self.repair_retry_after = None;
        self.repair_retry_pending = false;
    }
}

pub trait Platform {
    fn save_state(&mut self, managed: &HostManaged) -> Result<(), IpcError>;
    fn replace_watcher(&mut self, target: Option<&ManagedEqApo>) -> Result<(), IpcError>;
    fn repair_eqapo(&mut self, session: u32) -> Result<(), IpcError>;
    fn stop_child(&mut self) -> Result<(), IpcError>;
    fn clear_camilla_status(&mut self);
    fn clear_eqapo_status(&mut self);
    fn launch_child(&mut self, session: u32, target: &ManagedCamilla) -> Result<bool, IpcError>;
    fn log(&mut self, line: &HostLogLine) -> Result<(), IpcError>;
}

pub fn managed_after_update(
    state: &State,
    update: &HostManaged,
    requester_session: u32,
    requester_sid: &str,
) -> HostManaged {
    let owner_changed = state.managed.owner_user_sid.as_deref() != Some(requester_sid);
    let mut managed = if owner_changed {
        HostManaged::default()
    } else {
        state.managed.clone()
    };
    if update.camilla.is_some() {
        managed.camilla.clone_from(&update.camilla);
    }
    if update.eqapo.is_some() {
        managed.eqapo.clone_from(&update.eqapo);
    }
    managed.owner_session_id = Some(requester_session);
    managed.owner_user_sid = Some(requester_sid.to_owned());
    managed
}

pub fn decide(state: &State, input: Input) -> (State, Vec<Effect>) {
    let mut next = state.clone();
    let mut effects = Vec::new();
    match input {
        Input::SetManaged {
            update,
            requester_session,
            requester_sid,
            managing_session,
        } => {
            let previous_camilla = next.managed.camilla.clone();
            let previous_eqapo = next.managed.eqapo.clone();
            next.managed = managed_after_update(state, &update, requester_session, &requester_sid);
            next.eqapo_error = None;
            next.repair_attempts = 0;
            next.clear_repair_retry();
            effects.push(Effect::SaveState(next.managed.clone()));
            effects.push(Effect::ReplaceWatcher(next.managed.eqapo.clone()));
            if previous_camilla.is_some() && next.managed.camilla.is_none() {
                effects.push(Effect::ClearCamillaStatus);
            }
            if previous_eqapo.is_some() && next.managed.eqapo.is_none() {
                effects.push(Effect::ClearEqapoStatus);
            }
            if managing_session == Some(requester_session) && next.managed.eqapo.is_some() {
                effects.push(Effect::RepairEqapo {
                    session: requester_session,
                });
            }
            if previous_camilla != next.managed.camilla && next.running_child_session.is_some() {
                effects.push(Effect::StopChild);
            }
            if managing_session == Some(requester_session)
                && let Some(target) = next.managed.camilla.clone()
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
            next.clear_repair_retry();
            effects.push(Effect::SaveState(next.managed.clone()));
            effects.push(Effect::ReplaceWatcher(None));
            effects.push(Effect::ClearCamillaStatus);
            effects.push(Effect::ClearEqapoStatus);
            // The platform stop also cancels a pending real restart deadline,
            // even when no child is currently tracked.
            effects.push(Effect::StopChild);
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
                    if next.managed.owner_session_id != Some(session_id) {
                        next.managed.owner_session_id = Some(session_id);
                        effects.push(Effect::SaveState(next.managed.clone()));
                    }
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
                SessionEvent::Logoff if owner_matches => {
                    if next.running_child_session.is_some() || next.restart_scheduled {
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
            next.clear_repair_retry();
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
        Input::RepairRetry { managing_session } => {
            next.clear_repair_retry();
            if next.managed.eqapo.is_some()
                && let Some(session) = managing_session
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

#[derive(Debug, Default)]
pub struct DriveOutcome {
    pub repair_error: Option<IpcError>,
}

pub fn drive(
    state: &mut State,
    effects: Vec<Effect>,
    platform: &mut impl Platform,
) -> Result<DriveOutcome, IpcError> {
    let mut outcome = DriveOutcome::default();
    let mut effects = VecDeque::from(effects);
    while let Some(effect) = effects.pop_front() {
        match effect {
            Effect::SaveState(managed) => platform.save_state(&managed)?,
            Effect::ReplaceWatcher(target) => platform.replace_watcher(target.as_ref())?,
            Effect::RepairEqapo { session } => match platform.repair_eqapo(session) {
                Ok(()) => {
                    state.eqapo_error = None;
                    state.repair_attempts = 0;
                    state.clear_repair_retry();
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
                    state.repair_retry_pending = state.repair_retry_after.is_some();
                    outcome.repair_error = Some(error);
                }
            },
            Effect::StopChild => {
                // The real adapter takes ownership of the child before stop can
                // fail, so the transition must forget it on both outcomes.
                state.running_child_session = None;
                platform.stop_child()?;
            }
            Effect::ClearCamillaStatus => platform.clear_camilla_status(),
            Effect::ClearEqapoStatus => platform.clear_eqapo_status(),
            Effect::LaunchChild { session, target } => {
                state.running_child_session =
                    platform.launch_child(session, &target)?.then_some(session);
            }
            Effect::Log(line) => platform.log(&line)?,
        }
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use secs_types::ipc::{ErrorCode, ManagedCamilla, ManagedEqApo};

    use super::*;

    struct FakePlatform {
        saved_states: Vec<HostManaged>,
        launches: Vec<(u32, String)>,
        launch_starts_child: bool,
        repair_calls: usize,
        repair_sessions: Vec<u32>,
        fail_repairs: bool,
        stops: usize,
        fail_stop: bool,
        camilla_status_clears: usize,
        eqapo_status_clears: usize,
    }

    impl Default for FakePlatform {
        fn default() -> Self {
            Self {
                saved_states: Vec::new(),
                launches: Vec::new(),
                launch_starts_child: true,
                repair_calls: 0,
                repair_sessions: Vec::new(),
                fail_repairs: false,
                stops: 0,
                fail_stop: false,
                camilla_status_clears: 0,
                eqapo_status_clears: 0,
            }
        }
    }

    impl Platform for FakePlatform {
        fn save_state(&mut self, managed: &HostManaged) -> Result<(), IpcError> {
            self.saved_states.push(managed.clone());
            Ok(())
        }

        fn replace_watcher(&mut self, _target: Option<&ManagedEqApo>) -> Result<(), IpcError> {
            Ok(())
        }

        fn repair_eqapo(&mut self, session: u32) -> Result<(), IpcError> {
            self.repair_calls += 1;
            self.repair_sessions.push(session);
            if self.fail_repairs {
                Err(IpcError::new(ErrorCode::ApplyFailed, "repair failed"))
            } else {
                Ok(())
            }
        }

        fn stop_child(&mut self) -> Result<(), IpcError> {
            self.stops += 1;
            if self.fail_stop {
                Err(IpcError::new(ErrorCode::ApplyFailed, "stop failed"))
            } else {
                Ok(())
            }
        }

        fn clear_camilla_status(&mut self) {
            self.camilla_status_clears += 1;
        }

        fn clear_eqapo_status(&mut self) {
            self.eqapo_status_clears += 1;
        }

        fn launch_child(
            &mut self,
            session: u32,
            target: &ManagedCamilla,
        ) -> Result<bool, IpcError> {
            self.launches.push((session, target.exe_path.clone()));
            Ok(self.launch_starts_child)
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
                managing_session: Some(7),
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
                managing_session: Some(8),
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
                managing_session: Some(9),
            },
        );

        assert_eq!(next.managed.camilla, Some(target));
        assert_eq!(next.managed.eqapo, Some(eqapo()));
    }

    #[test]
    fn owner_change_clears_status_for_every_dropped_target() {
        let mut state = State::new(
            HostManaged {
                camilla: Some(camilla("A-camilladsp.exe")),
                eqapo: Some(eqapo()),
                owner_session_id: Some(7),
                owner_user_sid: Some("sid-a".into()),
            },
            None,
        );
        let mut platform = FakePlatform::default();
        let (next, effects) = decide(
            &state,
            Input::SetManaged {
                update: HostManaged::default(),
                requester_session: 8,
                requester_sid: "sid-b".into(),
                managing_session: Some(8),
            },
        );
        state = next;

        drive(&mut state, effects, &mut platform).unwrap();

        assert_eq!(platform.camilla_status_clears, 1);
        assert_eq!(platform.eqapo_status_clears, 1);
    }

    #[test]
    fn set_managed_does_not_launch_or_repair_without_a_managing_session() {
        let mut state = State::new(
            HostManaged {
                camilla: Some(camilla("camilladsp.exe")),
                eqapo: Some(eqapo()),
                owner_session_id: Some(7),
                owner_user_sid: Some("sid-a".into()),
            },
            None,
        );
        let mut platform = FakePlatform::default();
        let (next, effects) = decide(
            &state,
            Input::SetManaged {
                update: HostManaged::default(),
                requester_session: 7,
                requester_sid: "sid-a".into(),
                managing_session: None,
            },
        );
        state = next;

        drive(&mut state, effects, &mut platform).unwrap();

        assert!(platform.launches.is_empty());
        assert_eq!(platform.repair_calls, 0);
    }

    #[test]
    fn stop_managing_calls_platform_stop_without_a_tracked_child() {
        let mut state = State::new(HostManaged::default(), None);
        let mut platform = FakePlatform::default();
        let (next, effects) = decide(&state, Input::StopManaging);
        state = next;

        drive(&mut state, effects, &mut platform).unwrap();

        assert_eq!(platform.stops, 1);
    }

    #[test]
    fn launch_that_does_not_create_a_child_is_not_recorded_as_running() {
        let mut state = State::new(
            HostManaged {
                camilla: Some(camilla("camilladsp.exe")),
                owner_session_id: Some(7),
                owner_user_sid: Some("sid-a".into()),
                ..HostManaged::default()
            },
            None,
        );
        let mut platform = FakePlatform {
            launch_starts_child: false,
            ..FakePlatform::default()
        };
        let (next, effects) = decide(
            &state,
            Input::RestartTick {
                owner_session: Some(7),
            },
        );
        state = next;

        drive(&mut state, effects, &mut platform).unwrap();

        assert_eq!(state.running_child_session, None);
    }

    #[test]
    fn failed_stop_still_clears_the_tracked_child() {
        let mut state = State::new(HostManaged::default(), Some(7));
        let mut platform = FakePlatform {
            fail_stop: true,
            ..FakePlatform::default()
        };
        let (next, effects) = decide(&state, Input::StopManaging);
        state = next;

        assert!(drive(&mut state, effects, &mut platform).is_err());
        assert_eq!(state.running_child_session, None);
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
    fn owner_logoff_cancels_backoff_without_a_running_child() {
        let mut state = State::new(
            HostManaged {
                camilla: Some(camilla("camilladsp.exe")),
                owner_session_id: Some(7),
                owner_user_sid: Some("sid-a".into()),
                ..HostManaged::default()
            },
            None,
        );
        state.restart_scheduled = true;
        let mut platform = FakePlatform::default();

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

        assert!(!state.restart_scheduled);
        assert_eq!(platform.stops, 1);
    }

    #[test]
    fn owner_logon_persists_a_changed_session_id() {
        let mut state = State::new(
            HostManaged {
                owner_session_id: Some(7),
                owner_user_sid: Some("sid-a".into()),
                ..HostManaged::default()
            },
            None,
        );
        let mut platform = FakePlatform::default();

        let (next, effects) = decide(
            &state,
            Input::Session {
                event: SessionEvent::Logon,
                session_id: 9,
                owner_matches: true,
            },
        );
        state = next;
        drive(&mut state, effects, &mut platform).unwrap();

        assert_eq!(state.managed.owner_session_id, Some(9));
        assert_eq!(platform.saved_states.len(), 1);
        assert_eq!(platform.saved_states[0].owner_session_id, Some(9));
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
        for _ in 0..2 {
            let mut deadline = None;
            state.sync_repair_retry_deadline(&mut deadline, std::time::Instant::now());
            assert!(deadline.is_some());
            let (next, effects) = decide(
                &state,
                Input::RepairRetry {
                    managing_session: Some(7),
                },
            );
            state = next;
            drive(&mut state, effects, &mut platform).unwrap();
        }

        assert_eq!(state.running_child_session, Some(7));
        assert_eq!(state.eqapo_error.as_deref(), Some("repair failed"));
        assert_eq!(platform.repair_calls, 3);
        assert_eq!(platform.stops, 0);
    }

    #[test]
    fn repair_retry_uses_the_current_managing_session() {
        let mut state = State::new(
            HostManaged {
                eqapo: Some(eqapo()),
                owner_session_id: Some(7),
                owner_user_sid: Some("sid-a".into()),
                ..HostManaged::default()
            },
            None,
        );
        let mut platform = FakePlatform::default();
        let (next, effects) = decide(
            &state,
            Input::RepairRetry {
                managing_session: Some(9),
            },
        );
        state = next;

        drive(&mut state, effects, &mut platform).unwrap();

        assert_eq!(platform.repair_sessions, vec![9]);
    }

    #[test]
    fn repair_retry_is_cancelled_without_a_managing_session() {
        let mut state = State::new(
            HostManaged {
                eqapo: Some(eqapo()),
                owner_session_id: Some(7),
                owner_user_sid: Some("sid-a".into()),
                ..HostManaged::default()
            },
            None,
        );
        let mut platform = FakePlatform::default();
        let (next, effects) = decide(
            &state,
            Input::RepairRetry {
                managing_session: None,
            },
        );
        state = next;

        drive(&mut state, effects, &mut platform).unwrap();

        assert!(platform.repair_sessions.is_empty());
    }

    #[test]
    fn resetting_repair_state_clears_the_production_deadline() {
        let mut state = State::new(
            HostManaged {
                eqapo: Some(eqapo()),
                owner_session_id: Some(7),
                owner_user_sid: Some("sid-a".into()),
                ..HostManaged::default()
            },
            None,
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
        drive(&mut state, effects, &mut platform).unwrap();

        let mut deadline = None;
        state.sync_repair_retry_deadline(&mut deadline, std::time::Instant::now());
        assert!(deadline.is_some());

        let (next, _effects) = decide(&state, Input::StopManaging);
        state = next;
        state.sync_repair_retry_deadline(&mut deadline, std::time::Instant::now());

        assert_eq!(deadline, None);
    }

    #[test]
    fn set_managed_reports_repair_error_after_running_later_effects() {
        let mut state = State::new(HostManaged::default(), None);
        let mut platform = FakePlatform {
            fail_repairs: true,
            ..FakePlatform::default()
        };
        let (next, effects) = decide(
            &state,
            Input::SetManaged {
                update: HostManaged {
                    camilla: Some(camilla("camilladsp.exe")),
                    eqapo: Some(eqapo()),
                    ..HostManaged::default()
                },
                requester_session: 7,
                requester_sid: "sid-a".into(),
                managing_session: Some(7),
            },
        );
        state = next;

        let outcome = drive(&mut state, effects, &mut platform).unwrap();

        assert_eq!(
            outcome
                .repair_error
                .as_ref()
                .map(|error| error.detail.as_str()),
            Some("repair failed")
        );
        assert_eq!(platform.launches.len(), 1);
    }
}
