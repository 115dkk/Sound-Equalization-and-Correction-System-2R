use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use secs_types::ipc::{
    HostLogLine, HostManaged, IpcError, ManagedCamilla, ManagedEqApo, ManagedProcessState,
};

const MAX_REPAIR_ATTEMPTS: u8 = 3;

const RESTART_DELAYS: [Duration; 4] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
];
const RESTART_WINDOW: Duration = Duration::from_secs(5 * 60);
const MAX_RESTART_FAILURES: usize = 5;
const GIVE_UP_REASON: &str = "five exits within five minutes";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestartDecision {
    RestartAfter(Duration),
    GiveUp,
}

#[derive(Debug, Clone)]
struct Supervisor {
    failures: Vec<Instant>,
}

impl Supervisor {
    fn new() -> Self {
        Self {
            failures: Vec::new(),
        }
    }

    fn on_exit(&mut self, now: Instant) -> RestartDecision {
        self.failures.retain(|failure| {
            now.checked_duration_since(*failure)
                .is_none_or(|age| age < RESTART_WINDOW)
        });
        self.failures.push(now);
        if self.failures.len() >= MAX_RESTART_FAILURES {
            return RestartDecision::GiveUp;
        }
        let index = self.failures.len().saturating_sub(1);
        let delay = RESTART_DELAYS
            .get(index)
            .or_else(|| RESTART_DELAYS.last())
            .copied()
            .unwrap_or(Duration::ZERO);
        RestartDecision::RestartAfter(delay)
    }

    fn on_stable(&mut self) {
        self.failures.clear();
    }
}
const REPAIR_DELAYS: [Duration; MAX_REPAIR_ATTEMPTS as usize - 1] =
    [Duration::from_secs(1), Duration::from_secs(2)];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEvent {
    Logon,
    Logoff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleEvent {
    Connect,
    Disconnect,
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
    Console {
        event: ConsoleEvent,
        session_id: u32,
        owner_matches: bool,
    },
    ConfigChanged {
        owner_session: Option<u32>,
    },
    ChildExited {
        pid: Option<u32>,
        exit_code: i32,
        now: Instant,
    },
    PortBusy {
        reason: &'static str,
        detail: String,
        now: Instant,
    },
    TakeoverFinished {
        session: Option<u32>,
    },
    WatcherFailed {
        detail: String,
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
    SetCamillaState(ManagedProcessState),
    CountRestart,
    ClearCamillaStatus,
    ClearEqapoStatus,
    LaunchChild {
        session: u32,
        target: ManagedCamilla,
    },
    Log(HostLogLine),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingTakeover {
    pub session: u32,
    pub target: ManagedCamilla,
}

#[derive(Debug, Clone)]
pub struct State {
    pub managed: HostManaged,
    running_child_session: Option<u32>,
    pub restart_due: Option<Instant>,
    restart: Supervisor,
    pub eqapo_error: Option<String>,
    pub pending_takeover: Option<PendingTakeover>,
    pub other_user: bool,
    repair_attempts: u8,
    repair_retry_after: Option<Duration>,
    repair_retry_pending: bool,
}

impl State {
    pub fn new(managed: HostManaged, running_child_session: Option<u32>) -> Self {
        Self {
            managed,
            running_child_session,
            restart_due: None,
            restart: Supervisor::new(),
            eqapo_error: None,
            pending_takeover: None,
            other_user: false,
            repair_attempts: 0,
            repair_retry_after: None,
            repair_retry_pending: false,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn running_child_session(&self) -> Option<u32> {
        self.running_child_session
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
    fn set_camilla_state(&mut self, state: ManagedProcessState);
    fn count_restart(&mut self);
    fn clear_camilla_status(&mut self);
    fn clear_eqapo_status(&mut self);
    fn launch_child(&mut self, session: u32, target: &ManagedCamilla) -> Result<(), IpcError>;
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

fn schedule_takeover(
    state: &mut State,
    effects: &mut Vec<Effect>,
    session: u32,
    target: ManagedCamilla,
) {
    let pending = PendingTakeover {
        session,
        target: target.clone(),
    };
    if state.pending_takeover.as_ref() != Some(&pending) {
        state.pending_takeover = Some(pending);
        effects.push(Effect::LaunchChild { session, target });
    }
}

fn event_line(event: &str, fields: impl IntoIterator<Item = (String, String)>) -> HostLogLine {
    HostLogLine {
        timestamp: chrono::Utc::now().to_rfc3339(),
        event: event.into(),
        fields: fields.into_iter().collect(),
    }
}

fn schedule_restart(state: &mut State, effects: &mut Vec<Effect>, now: Instant) {
    match state.restart.on_exit(now) {
        RestartDecision::RestartAfter(delay) => {
            state.restart_due = Some(now + delay);
            // Kept only until service_loop is routed through restart_due in the next step.
            let until = (chrono::Utc::now()
                + chrono::Duration::from_std(delay).unwrap_or_default())
            .to_rfc3339();
            effects.push(Effect::SetCamillaState(ManagedProcessState::BackingOff {
                until,
            }));
            effects.push(Effect::CountRestart);
            effects.push(Effect::Log(event_line(
                "restart",
                [("after_ms".into(), delay.as_millis().to_string())],
            )));
        }
        RestartDecision::GiveUp => {
            state.restart_due = None;
            effects.push(Effect::SetCamillaState(ManagedProcessState::GivenUp {
                reason: GIVE_UP_REASON.into(),
            }));
            effects.push(Effect::Log(event_line(
                "given_up",
                [("reason".into(), GIVE_UP_REASON.into())],
            )));
        }
    }
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
            if previous_camilla != next.managed.camilla
                && (next.running_child_session.is_some() || next.pending_takeover.is_some())
            {
                effects.push(Effect::StopChild);
                next.pending_takeover = None;
            }
            if managing_session == Some(requester_session)
                && let Some(target) = next.managed.camilla.clone()
                && (next.running_child_session.is_none()
                    || previous_camilla != next.managed.camilla)
            {
                schedule_takeover(&mut next, &mut effects, requester_session, target);
            } else if managing_session != Some(requester_session) {
                next.pending_takeover = None;
            }
        }
        Input::StopManaging => {
            next.managed = HostManaged::default();
            next.restart_due = None;
            next.eqapo_error = None;
            next.pending_takeover = None;
            next.other_user = false;
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
                    next.other_user = false;
                    next.restart.on_stable();
                    if next.managed.owner_session_id != Some(session_id) {
                        next.managed.owner_session_id = Some(session_id);
                        effects.push(Effect::SaveState(next.managed.clone()));
                    }
                    if let Some(target) = next.managed.camilla.clone()
                        && next.running_child_session.is_none()
                    {
                        schedule_takeover(&mut next, &mut effects, session_id, target);
                    }
                    if next.managed.eqapo.is_some() {
                        effects.push(Effect::RepairEqapo {
                            session: session_id,
                        });
                    }
                }
                SessionEvent::Logon => {
                    next.other_user = true;
                    if next.running_child_session.is_some() || next.pending_takeover.is_some() {
                        effects.push(Effect::StopChild);
                    }
                    next.pending_takeover = None;
                    next.restart_due = None;
                    effects.push(Effect::SetCamillaState(ManagedProcessState::OtherUser));
                }
                SessionEvent::Logoff if owner_matches => {
                    next.other_user = false;
                    if next.running_child_session.is_some()
                        || next.restart_due.is_some()
                        || next.pending_takeover.is_some()
                    {
                        effects.push(Effect::StopChild);
                    }
                    next.pending_takeover = None;
                    next.restart_due = None;
                    effects.push(Effect::SetCamillaState(ManagedProcessState::NoSession));
                }
                SessionEvent::Logoff => {
                    let child_ended = next.running_child_session == Some(session_id)
                        || next.pending_takeover.as_ref().map(|work| work.session)
                            == Some(session_id);
                    if child_ended {
                        effects.push(Effect::StopChild);
                        next.pending_takeover = None;
                        next.restart_due = None;
                    }
                    if child_ended {
                        effects.push(Effect::SetCamillaState(ManagedProcessState::NoSession));
                    }
                }
            }
        }
        Input::Console {
            event,
            session_id,
            owner_matches,
        } => {
            effects.push(Effect::Log(event_line(
                match event {
                    ConsoleEvent::Connect => "console_connect",
                    ConsoleEvent::Disconnect => "console_disconnect",
                },
                [("session".into(), session_id.to_string())],
            )));
            match event {
                ConsoleEvent::Connect if owner_matches => {
                    next.other_user = false;
                    next.restart.on_stable();
                    if next.managed.owner_session_id != Some(session_id) {
                        next.managed.owner_session_id = Some(session_id);
                        effects.push(Effect::SaveState(next.managed.clone()));
                    }
                    if let Some(target) = next.managed.camilla.clone()
                        && next.running_child_session.is_none()
                    {
                        schedule_takeover(&mut next, &mut effects, session_id, target);
                    }
                    if next.managed.eqapo.is_some() {
                        effects.push(Effect::RepairEqapo {
                            session: session_id,
                        });
                    }
                }
                ConsoleEvent::Connect => {
                    next.other_user = true;
                    if next.running_child_session.take().is_some()
                        || next.pending_takeover.is_some()
                    {
                        effects.push(Effect::StopChild);
                    }
                    next.pending_takeover = None;
                    next.restart_due = None;
                    effects.push(Effect::SetCamillaState(ManagedProcessState::OtherUser));
                }
                ConsoleEvent::Disconnect if owner_matches => {
                    next.other_user = false;
                    if next.running_child_session.take().is_some()
                        || next.restart_due.is_some()
                        || next.pending_takeover.is_some()
                    {
                        effects.push(Effect::StopChild);
                    }
                    next.pending_takeover = None;
                    next.restart_due = None;
                    effects.push(Effect::SetCamillaState(ManagedProcessState::NoSession));
                }
                ConsoleEvent::Disconnect => {
                    next.other_user = false;
                    if next.running_child_session == Some(session_id)
                        || next.pending_takeover.as_ref().map(|work| work.session)
                            == Some(session_id)
                    {
                        next.running_child_session = None;
                        next.pending_takeover = None;
                        next.restart_due = None;
                        effects.push(Effect::StopChild);
                    }
                    effects.push(Effect::SetCamillaState(ManagedProcessState::NoSession));
                }
            }
        }
        Input::ChildExited {
            pid,
            exit_code,
            now,
        } => {
            let had_session = next.running_child_session.take().is_some();
            next.pending_takeover = None;
            effects.push(Effect::Log(event_line(
                "exit",
                [
                    (
                        "pid".into(),
                        pid.map_or_else(String::new, |pid| pid.to_string()),
                    ),
                    ("code".into(), exit_code.to_string()),
                ],
            )));
            if had_session {
                schedule_restart(&mut next, &mut effects, now);
            }
        }
        Input::PortBusy {
            reason,
            detail,
            now,
        } => {
            next.pending_takeover = None;
            effects.push(Effect::Log(event_line(
                "port_busy",
                [("reason".into(), reason.into()), ("detail".into(), detail)],
            )));
            if next.managed.camilla.is_some() {
                schedule_restart(&mut next, &mut effects, now);
            }
        }
        Input::TakeoverFinished { session } => {
            next.pending_takeover = None;
            next.running_child_session = session;
            effects.push(Effect::SetCamillaState(if session.is_some() {
                ManagedProcessState::Running
            } else {
                ManagedProcessState::NoSession
            }));
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
        Input::WatcherFailed { detail } => {
            next.eqapo_error = Some(detail.clone());
            effects.push(Effect::Log(HostLogLine {
                timestamp: chrono::Utc::now().to_rfc3339(),
                event: "watcher_failed".into(),
                fields: BTreeMap::from([("detail".into(), detail)]),
            }));
        }
        Input::RestartTick { owner_session } => {
            next.restart_due = None;
            if let (Some(session), Some(target)) = (owner_session, next.managed.camilla.clone()) {
                schedule_takeover(&mut next, &mut effects, session, target);
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

pub fn finish_repair(state: &mut State, result: &Result<(), IpcError>) -> Option<HostLogLine> {
    match result {
        Ok(()) => {
            state.eqapo_error = None;
            state.repair_attempts = 0;
            state.clear_repair_retry();
            None
        }
        Err(error) => {
            state.repair_attempts = state.repair_attempts.saturating_add(1);
            state.eqapo_error = Some(error.detail.clone());
            state.repair_retry_after = if state.repair_attempts < MAX_REPAIR_ATTEMPTS {
                Some(REPAIR_DELAYS[usize::from(state.repair_attempts - 1)])
            } else {
                None
            };
            state.repair_retry_pending = state.repair_retry_after.is_some();
            Some(repair_failed_line(error, state.repair_attempts))
        }
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
    let outcome = DriveOutcome::default();
    let mut effects = VecDeque::from(effects);
    while let Some(effect) = effects.pop_front() {
        match effect {
            Effect::SaveState(managed) => platform.save_state(&managed)?,
            Effect::ReplaceWatcher(target) => {
                if let Err(error) = platform.replace_watcher(target.as_ref()) {
                    let (next, follow_up) = decide(
                        state,
                        Input::WatcherFailed {
                            detail: error.detail,
                        },
                    );
                    *state = next;
                    for effect in follow_up.into_iter().rev() {
                        effects.push_front(effect);
                    }
                }
            }
            Effect::RepairEqapo { session } => platform.repair_eqapo(session)?,
            Effect::SetCamillaState(state) => platform.set_camilla_state(state),
            Effect::CountRestart => platform.count_restart(),
            Effect::StopChild => {
                // The real adapter takes ownership of the child before stop can
                // fail, so the transition must forget it on both outcomes.
                state.running_child_session = None;
                platform.stop_child()?;
            }
            Effect::ClearCamillaStatus => platform.clear_camilla_status(),
            Effect::ClearEqapoStatus => platform.clear_eqapo_status(),
            Effect::LaunchChild { session, target } => {
                platform.launch_child(session, &target)?;
            }
            Effect::Log(line) => platform.log(&line)?,
        }
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use secs_types::ipc::{ErrorCode, HostResponse, ManagedCamilla, ManagedEqApo};

    use super::*;

    #[derive(Default)]
    struct FakePlatform {
        saved_states: Vec<HostManaged>,
        launches: Vec<(u32, String)>,
        repair_calls: usize,
        repair_sessions: Vec<u32>,
        stops: usize,
        fail_stop: bool,
        fail_watcher: bool,
        logs: Vec<HostLogLine>,
        camilla_states: Vec<ManagedProcessState>,
        restart_counts: usize,
        camilla_status_clears: usize,
        eqapo_status_clears: usize,
    }

    impl Platform for FakePlatform {
        fn save_state(&mut self, managed: &HostManaged) -> Result<(), IpcError> {
            self.saved_states.push(managed.clone());
            Ok(())
        }

        fn replace_watcher(&mut self, _target: Option<&ManagedEqApo>) -> Result<(), IpcError> {
            if self.fail_watcher {
                Err(IpcError::new(ErrorCode::ApplyFailed, "watcher failed"))
            } else {
                Ok(())
            }
        }

        fn repair_eqapo(&mut self, session: u32) -> Result<(), IpcError> {
            self.repair_calls += 1;
            self.repair_sessions.push(session);
            Ok(())
        }

        fn stop_child(&mut self) -> Result<(), IpcError> {
            self.stops += 1;
            if self.fail_stop {
                Err(IpcError::new(ErrorCode::ApplyFailed, "stop failed"))
            } else {
                Ok(())
            }
        }

        fn set_camilla_state(&mut self, state: ManagedProcessState) {
            self.camilla_states.push(state);
        }

        fn count_restart(&mut self) {
            self.restart_counts += 1;
        }

        fn clear_camilla_status(&mut self) {
            self.camilla_status_clears += 1;
        }

        fn clear_eqapo_status(&mut self) {
            self.eqapo_status_clears += 1;
        }

        fn launch_child(&mut self, session: u32, target: &ManagedCamilla) -> Result<(), IpcError> {
            self.launches.push((session, target.exe_path.clone()));
            Ok(())
        }

        fn log(&mut self, line: &HostLogLine) -> Result<(), IpcError> {
            self.logs.push(line.clone());
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

    fn managed_camilla() -> HostManaged {
        HostManaged {
            camilla: Some(camilla("camilladsp.exe")),
            owner_session_id: Some(7),
            owner_user_sid: Some("sid-a".into()),
            ..HostManaged::default()
        }
    }

    #[test]
    fn one_child_exit_schedules_the_first_backoff_and_counts_it() {
        let now = Instant::now();
        let state = State::new(managed_camilla(), Some(7));

        let (next, effects) = decide(
            &state,
            Input::ChildExited {
                pid: Some(42),
                exit_code: 7,
                now,
            },
        );

        assert_eq!(next.restart_due, Some(now + Duration::from_secs(1)));
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::SetCamillaState(ManagedProcessState::BackingOff { .. })
        )));
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::CountRestart))
        );
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::Log(line)
                if line.event == "exit"
                    && line.fields.get("pid").map(String::as_str) == Some("42")
                    && line.fields.get("code").map(String::as_str) == Some("7")
        )));
    }

    #[test]
    fn five_exits_within_five_minutes_give_up() {
        let now = Instant::now();
        let mut state = State::new(managed_camilla(), Some(7));
        let mut effects = Vec::new();
        for second in 0_u32..5 {
            state.running_child_session = Some(7);
            (state, effects) = decide(
                &state,
                Input::ChildExited {
                    pid: Some(40 + second),
                    exit_code: 1,
                    now: now + Duration::from_secs(u64::from(second)),
                },
            );
        }

        assert_eq!(state.restart_due, None);
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::SetCamillaState(ManagedProcessState::GivenUp { reason })
                if reason == "five exits within five minutes"
        )));
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::Log(line) if line.event == "given_up"
        )));
    }

    #[test]
    fn exit_without_a_session_schedules_nothing() {
        let state = State::new(managed_camilla(), None);

        let (next, effects) = decide(
            &state,
            Input::ChildExited {
                pid: Some(42),
                exit_code: 0,
                now: Instant::now(),
            },
        );

        assert_eq!(next.restart_due, None);
        assert!(!effects.iter().any(|effect| matches!(
            effect,
            Effect::CountRestart | Effect::SetCamillaState(ManagedProcessState::BackingOff { .. })
        )));
    }

    #[test]
    fn restart_tick_after_restart_due_launches() {
        let mut state = State::new(managed_camilla(), None);
        state.restart_due = Some(Instant::now());

        let (next, effects) = decide(
            &state,
            Input::RestartTick {
                owner_session: Some(7),
            },
        );

        assert_eq!(next.restart_due, None);
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::LaunchChild { session: 7, .. }))
        );
    }

    #[test]
    fn fake_platform_records_no_session_after_owner_logoff() {
        let mut state = State::new(managed_camilla(), Some(7));
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

        assert!(
            platform
                .camilla_states
                .contains(&ManagedProcessState::NoSession)
        );
    }

    #[test]
    fn owner_logoff_and_stop_managing_clear_restart_due_and_stop() {
        let mut state = State::new(managed_camilla(), None);
        state.restart_due = Some(Instant::now());
        let (logged_off, logoff_effects) = decide(
            &state,
            Input::Session {
                event: SessionEvent::Logoff,
                session_id: 7,
                owner_matches: true,
            },
        );
        assert_eq!(logged_off.restart_due, None);
        assert!(
            logoff_effects
                .iter()
                .any(|effect| matches!(effect, Effect::StopChild))
        );

        state.restart_due = Some(Instant::now());
        let (stopped, stop_effects) = decide(&state, Input::StopManaging);
        assert_eq!(stopped.restart_due, None);
        assert!(
            stop_effects
                .iter()
                .any(|effect| matches!(effect, Effect::StopChild))
        );
    }

    #[test]
    fn port_busy_backs_off_and_gives_up_on_the_fifth() {
        let now = Instant::now();
        let mut state = State::new(managed_camilla(), None);
        let mut effects = Vec::new();
        for second in 0_u32..5 {
            (state, effects) = decide(
                &state,
                Input::PortBusy {
                    reason: "not_camilla",
                    detail: "port 1234 is busy".into(),
                    now: now + Duration::from_secs(u64::from(second)),
                },
            );
            if second == 0 {
                assert_eq!(state.restart_due, Some(now + Duration::from_secs(1)));
                assert!(effects.iter().any(|effect| matches!(
                    effect,
                    Effect::Log(line) if line.event == "port_busy"
                )));
            }
        }

        assert_eq!(state.restart_due, None);
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::SetCamillaState(ManagedProcessState::GivenUp { reason })
                if reason == "five exits within five minutes"
        )));
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
    fn set_managed_accepts_takeover_and_logoff_while_work_is_pending() {
        let state = State::new(
            HostManaged {
                owner_session_id: Some(7),
                owner_user_sid: Some("sid-a".into()),
                ..HostManaged::default()
            },
            None,
        );
        let target = camilla("camilladsp.exe");

        let (pending, effects) = decide(
            &state,
            Input::SetManaged {
                update: HostManaged {
                    camilla: Some(target.clone()),
                    ..HostManaged::default()
                },
                requester_session: 7,
                requester_sid: "sid-a".into(),
                managing_session: Some(7),
            },
        );

        let immediate_response = HostResponse::Ok;
        assert_eq!(immediate_response, HostResponse::Ok);
        assert_eq!(
            pending.pending_takeover,
            Some(PendingTakeover { session: 7, target })
        );
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::LaunchChild { session: 7, .. }))
        );

        let (logged_off, effects) = decide(
            &pending,
            Input::Session {
                event: SessionEvent::Logoff,
                session_id: 7,
                owner_matches: true,
            },
        );
        assert_eq!(logged_off.pending_takeover, None);
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::Log(line) if line.event == "session_logoff"
        )));
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::StopChild))
        );
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
    fn scheduled_launch_is_not_recorded_as_running_before_completion() {
        let mut state = State::new(
            HostManaged {
                camilla: Some(camilla("camilladsp.exe")),
                owner_session_id: Some(7),
                owner_user_sid: Some("sid-a".into()),
                ..HostManaged::default()
            },
            None,
        );
        let mut platform = FakePlatform::default();
        let (next, effects) = decide(
            &state,
            Input::RestartTick {
                owner_session: Some(7),
            },
        );
        state = next;

        drive(&mut state, effects, &mut platform).unwrap();

        assert_eq!(state.running_child_session(), None);
        assert_eq!(
            state.pending_takeover.as_ref().map(|work| work.session),
            Some(7)
        );

        let (next, effects) = decide(&state, Input::TakeoverFinished { session: Some(7) });
        state = next;
        drive(&mut state, effects, &mut platform).unwrap();
        assert_eq!(state.running_child_session(), Some(7));
        assert!(
            platform
                .camilla_states
                .contains(&ManagedProcessState::Running)
        );
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
    fn owner_console_disconnect_stops_the_child_and_marks_no_session() {
        let state = State::new(managed_camilla(), Some(7));

        let (next, effects) = decide(
            &state,
            Input::Console {
                event: ConsoleEvent::Disconnect,
                session_id: 7,
                owner_matches: true,
            },
        );

        assert_eq!(next.running_child_session(), None);
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::StopChild))
        );
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::SetCamillaState(ManagedProcessState::NoSession)
        )));
    }

    #[test]
    fn owner_console_connect_launches_and_repairs() {
        let state = State::new(
            HostManaged {
                camilla: Some(camilla("camilladsp.exe")),
                eqapo: Some(eqapo()),
                owner_session_id: Some(7),
                owner_user_sid: Some("sid-a".into()),
            },
            None,
        );

        let (next, effects) = decide(
            &state,
            Input::Console {
                event: ConsoleEvent::Connect,
                session_id: 7,
                owner_matches: true,
            },
        );

        assert!(!next.other_user);
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::LaunchChild { session: 7, .. }))
        );
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::RepairEqapo { session: 7 }))
        );
    }

    #[test]
    fn other_users_console_connect_marks_other_user() {
        let state = State::new(managed_camilla(), Some(7));

        let (next, effects) = decide(
            &state,
            Input::Console {
                event: ConsoleEvent::Connect,
                session_id: 8,
                owner_matches: false,
            },
        );

        assert!(next.other_user);
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::StopChild))
        );
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::SetCamillaState(ManagedProcessState::OtherUser)
        )));
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
        state.restart_due = Some(Instant::now());
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

        assert_eq!(state.running_child_session(), Some(7));
        assert!(state.restart_due.is_some());
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
        assert_eq!(state.restart_due, None);
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
        state.restart_due = Some(Instant::now());
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

        assert_eq!(state.restart_due, None);
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
    fn watcher_failure_keeps_the_loop_and_later_effects() {
        let mut state = State::new(
            HostManaged {
                camilla: Some(camilla("camilladsp.exe")),
                eqapo: Some(eqapo()),
                ..HostManaged::default()
            },
            None,
        );
        let mut platform = FakePlatform {
            fail_watcher: true,
            ..FakePlatform::default()
        };
        let effects = vec![
            Effect::ReplaceWatcher(state.managed.eqapo.clone()),
            Effect::LaunchChild {
                session: 7,
                target: state.managed.camilla.clone().unwrap(),
            },
        ];

        let result = drive(&mut state, effects, &mut platform);

        assert!(result.is_ok(), "watcher failure ended drive: {result:?}");
        assert_eq!(state.eqapo_error.as_deref(), Some("watcher failed"));
        assert!(
            platform
                .logs
                .iter()
                .any(|line| line.event == "watcher_failed")
        );
        assert_eq!(platform.launches.len(), 1);
    }

    #[test]
    fn watcher_failure_damages_eqapo_and_still_accepts_stop_managing() {
        let state = State::new(
            HostManaged {
                eqapo: Some(eqapo()),
                owner_session_id: Some(7),
                owner_user_sid: Some("sid-a".into()),
                ..HostManaged::default()
            },
            None,
        );

        let (failed, effects) = decide(
            &state,
            Input::WatcherFailed {
                detail: "Equalizer APO machine configuration is not registered".into(),
            },
        );

        assert_eq!(
            failed.eqapo_error.as_deref(),
            Some("Equalizer APO machine configuration is not registered")
        );
        assert_eq!(
            failed
                .eqapo_error
                .as_ref()
                .map(|_| secs_types::ipc::IncludeState::Damaged),
            Some(secs_types::ipc::IncludeState::Damaged)
        );
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::Log(line) if line.event == "watcher_failed"
        )));

        let (stopped, _) = decide(&failed, Input::StopManaging);
        assert_eq!(stopped.managed, HostManaged::default());
        assert_eq!(stopped.eqapo_error, None);
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
        let mut platform = FakePlatform::default();
        let (next, effects) = decide(
            &state,
            Input::ConfigChanged {
                owner_session: Some(7),
            },
        );
        state = next;

        drive(&mut state, effects, &mut platform).unwrap();
        for attempt in 0..3 {
            let error = Err(IpcError::new(ErrorCode::ApplyFailed, "repair failed"));
            let line = finish_repair(&mut state, &error).unwrap();
            platform.log(&line).unwrap();
            if attempt < 2 {
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
        let mut platform = FakePlatform::default();
        let (next, effects) = decide(
            &state,
            Input::ConfigChanged {
                owner_session: Some(7),
            },
        );
        state = next;
        drive(&mut state, effects, &mut platform).unwrap();
        finish_repair(
            &mut state,
            &Err(IpcError::new(ErrorCode::ApplyFailed, "repair failed")),
        );

        let mut deadline = None;
        state.sync_repair_retry_deadline(&mut deadline, std::time::Instant::now());
        assert!(deadline.is_some());

        let (next, _effects) = decide(&state, Input::StopManaging);
        state = next;
        state.sync_repair_retry_deadline(&mut deadline, std::time::Instant::now());

        assert_eq!(deadline, None);
    }

    #[test]
    fn set_managed_accepts_before_repair_finishes_and_schedules_takeover() {
        let mut state = State::new(HostManaged::default(), None);
        let mut platform = FakePlatform::default();
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

        assert_eq!(outcome.repair_error, None);
        assert_eq!(platform.repair_calls, 1);
        assert_eq!(platform.launches.len(), 1);
        assert_eq!(
            state.pending_takeover.as_ref().map(|work| work.session),
            Some(7)
        );
    }
}
