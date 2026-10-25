//! Cancellation probes for the stage tests (TD-49).
//!
//! A stage polls `cancelled()` where a job's cancel token would be read. The
//! probe stands in for the token: it reads false for its first `after` polls
//! and true from then on, as a token cancelled in the middle of the stage
//! does, and it counts every poll. Between two polls a stage does at most one
//! unit of its work (an IR cell, a candidate delay, a phase position, a ladder
//! step, a residual evaluation), so the polls that read true before the stage
//! returns bound the work done after cancellation.

use crate::EngineError;
use std::cell::Cell;

pub(crate) struct Probe {
    after: usize,
    polls: Cell<usize>,
}

impl Probe {
    fn after(after: usize) -> Self {
        Self {
            after,
            polls: Cell::new(0),
        }
    }

    pub(crate) fn cancelled(&self) -> bool {
        let k = self.polls.get();
        self.polls.set(k + 1);
        k >= self.after
    }
}

/// Runs `stage` uncancelled and then cancelled after each poll it makes
/// (every flip point up to 64, a geometric sample beyond, and the last one).
/// Each cancelled run must return `Err(Cancelled)` after at most `latency`
/// polls that read true. Returns the uncancelled output and its poll count.
pub(crate) fn assert_cancels<T>(
    name: &str,
    latency: usize,
    stage: impl Fn(&dyn Fn() -> bool) -> Result<T, EngineError>,
) -> (T, usize) {
    let probe = Probe::after(usize::MAX);
    let out = stage(&|| probe.cancelled()).unwrap_or_else(|e| panic!("{name}: {e}"));
    let total = probe.polls.get();
    let mut flips: Vec<_> = (0..total.min(64)).collect();
    let mut n = 64;
    while n < total {
        flips.push(n);
        n = n * 3 / 2;
    }
    flips.push(total - 1);
    flips.dedup();
    let mut worst = 0;
    for n in flips {
        let probe = Probe::after(n);
        let result = stage(&|| probe.cancelled());
        assert!(
            matches!(result, Err(EngineError::Cancelled)),
            "{name}: cancelled after {n} of {total} polls, got {:?}",
            result.err()
        );
        let late = probe.polls.get() - n;
        assert!(
            late <= latency,
            "{name}: {late} polls after cancelling at {n} of {total} (limit {latency})"
        );
        worst = worst.max(late);
    }
    println!("CANCEL {name} polls={total} worst_latency={worst}");
    (out, total)
}
