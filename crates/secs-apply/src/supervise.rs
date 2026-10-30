use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestartPolicy {
    pub delays: Vec<Duration>,
    pub window: Duration,
    pub max_failures: usize,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            delays: [1, 2, 4, 8].map(Duration::from_secs).to_vec(),
            window: Duration::from_secs(300),
            max_failures: 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    RestartAfter(Duration),
    GiveUp,
}

#[derive(Debug)]
pub struct Supervisor {
    policy: RestartPolicy,
    failures: Vec<Instant>,
}

impl Supervisor {
    pub fn new(policy: RestartPolicy) -> Self {
        Self {
            policy,
            failures: Vec::new(),
        }
    }

    /// Records an unexpected child exit and returns the next action.
    pub fn on_exit(&mut self, now: Instant) -> Decision {
        self.failures.retain(|failure| {
            now.checked_duration_since(*failure)
                .is_none_or(|age| age < self.policy.window)
        });
        self.failures.push(now);

        if self.failures.len() >= self.policy.max_failures {
            return Decision::GiveUp;
        }

        let delay_index = self.failures.len().saturating_sub(1);
        let delay = self
            .policy
            .delays
            .get(delay_index)
            .or_else(|| self.policy.delays.last())
            .copied()
            .unwrap_or(Duration::ZERO);
        Decision::RestartAfter(delay)
    }

    /// Forgets failures after the child has run for the policy's stable window.
    pub fn on_stable(&mut self) {
        self.failures.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supervise() {
        let start = Instant::now();
        let mut supervisor = Supervisor::new(RestartPolicy::default());
        for (seconds, expected) in [(0, 1), (10, 2), (20, 4), (30, 8)] {
            assert_eq!(
                supervisor.on_exit(start + Duration::from_secs(seconds)),
                Decision::RestartAfter(Duration::from_secs(expected))
            );
        }
        assert_eq!(
            supervisor.on_exit(start + Duration::from_secs(40)),
            Decision::GiveUp
        );
    }

    #[test]
    fn failures_at_or_beyond_the_window_are_forgotten() {
        let start = Instant::now();
        let mut supervisor = Supervisor::new(RestartPolicy::default());
        assert_eq!(
            supervisor.on_exit(start),
            Decision::RestartAfter(Duration::from_secs(1))
        );
        assert_eq!(
            supervisor.on_exit(start + Duration::from_secs(300)),
            Decision::RestartAfter(Duration::from_secs(1))
        );
    }

    #[test]
    fn stable_run_resets_failures_and_backoff() {
        let start = Instant::now();
        let mut supervisor = Supervisor::new(RestartPolicy::default());
        let _ = supervisor.on_exit(start);
        let _ = supervisor.on_exit(start + Duration::from_secs(1));
        supervisor.on_stable();
        assert_eq!(
            supervisor.on_exit(start + Duration::from_secs(302)),
            Decision::RestartAfter(Duration::from_secs(1))
        );
    }

    #[test]
    fn final_delay_repeats_when_the_failure_limit_allows_it() {
        let start = Instant::now();
        let policy = RestartPolicy {
            max_failures: 7,
            ..RestartPolicy::default()
        };
        let mut supervisor = Supervisor::new(policy);
        let expected = [1, 2, 4, 8, 8, 8];
        for (index, delay) in expected.into_iter().enumerate() {
            assert_eq!(
                supervisor.on_exit(start + Duration::from_secs(index as u64)),
                Decision::RestartAfter(Duration::from_secs(delay))
            );
        }
    }
}
