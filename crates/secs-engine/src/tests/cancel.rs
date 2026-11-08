//! Cancellation probes for the stage tests (TD-49).
//!
//! A stage receives the same compute context as production. Its probe token
//! reads false before the requested check, true from that check onward, and
//! records every poll. Between polls a stage does at most one unit of work.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::EngineError;
use secs_compute::{CancelToken, Context, MemoryBudget, NoProgress};

fn context(after: usize) -> (Context, Arc<AtomicUsize>) {
    let (cancel, polls) = CancelToken::probe(after);
    (
        Context::new(1, MemoryBudget::new(u64::MAX), Arc::new(NoProgress), cancel),
        polls,
    )
}

/// Runs `stage` uncancelled and then cancelled after each poll it makes
/// (every flip point up to 64, a geometric sample beyond, and the last one).
/// Each cancelled run must return `Err(Cancelled)` after at most `latency`
/// polls that read true. Returns the uncancelled output and its poll count.
pub(crate) fn assert_cancels<T>(
    name: &str,
    latency: usize,
    stage: impl Fn(&Context) -> Result<T, EngineError>,
) -> (T, usize) {
    let (ctx, polls) = context(usize::MAX);
    let out = stage(&ctx).unwrap_or_else(|error| panic!("{name}: {error}"));
    let total = polls.load(Ordering::SeqCst);
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
        let (ctx, polls) = context(n + 1);
        let result = stage(&ctx);
        assert!(
            matches!(result, Err(EngineError::Cancelled)),
            "{name}: cancelled after {n} of {total} polls, got {:?}",
            result.err()
        );
        let late = polls.load(Ordering::SeqCst) - n;
        assert!(
            late <= latency,
            "{name}: {late} polls after cancelling at {n} of {total} (limit {latency})"
        );
        worst = worst.max(late);
    }
    println!("CANCEL {name} polls={total} worst_latency={worst}");
    (out, total)
}
