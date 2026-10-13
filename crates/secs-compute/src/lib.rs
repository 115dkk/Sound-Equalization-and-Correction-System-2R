#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Resource rules for heavy computation.
//!
//! One place for what every expensive job must obey:
//!
//! - **Memory budget.** A job estimates its bytes and takes a weighted permit
//!   before it allocates; a single job larger than the budget is refused up
//!   front with the reason, instead of exhausting RAM halfway.
//! - **Caches.** Spectra, responses and stage results live in byte-capped LRU
//!   caches keyed by input fingerprints; a changed measurement evicts its keys.
//! - **Threads.** A rayon pool sized by cores and budget. Parallel work is
//!   `map` only; floating-point accumulation runs in its original order, so
//!   results do not depend on the thread count.
//! - **Cancellation and progress.** A cooperative cancel token checked at
//!   stage, channel, input and iteration boundaries, and a progress sink that
//!   reports stage, done/total and an ETA from the observed rate.
//!
//! The available-memory probe is a seam (`MemoryProbe`): the app injects the
//! OS adapter from `secs-sys`; tests inject a fixed value.

use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use rayon::prelude::*;
use rayon::{ThreadPool, ThreadPoolBuilder};

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Cooperative cancellation shared by a job and everything it starts.
#[derive(Clone, Debug, Default)]
pub struct CancelToken {
    cancelled: Arc<AtomicBool>,
}

impl CancelToken {
    /// Creates a token which has not been cancelled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancels this token and all of its clones.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Returns whether this token has been cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Returns `Err(Cancelled)` once cancelled.
    pub fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }
}

/// A computation was cooperatively cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("cancelled")]
pub struct Cancelled;

/// One progress report.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    /// Stable stage identifier, such as `engine.ba`.
    pub stage: &'static str,
    /// Number of completed items.
    pub done: u64,
    /// Total number of items in this stage.
    pub total: u64,
    /// Estimated time remaining from this stage's observed rate.
    pub eta: Option<Duration>,
}

/// Destination for progress reports.
pub trait ProgressSink: Send + Sync + Debug {
    /// Receives one progress snapshot.
    fn report(&self, progress: &Progress);
}

/// A sink that drops every progress report.
#[derive(Debug, Default)]
pub struct NoProgress;

impl ProgressSink for NoProgress {
    fn report(&self, _progress: &Progress) {}
}

/// Reports the memory currently available to the process.
pub trait MemoryProbe: Send + Sync + Debug {
    /// Returns available bytes.
    fn available_bytes(&self) -> u64;
}

/// A fixed memory reading, useful for tests and deterministic callers.
#[derive(Debug)]
pub struct FixedMemory(pub u64);

impl MemoryProbe for FixedMemory {
    fn available_bytes(&self) -> u64 {
        self.0
    }
}

#[derive(Debug)]
struct BudgetState {
    available: u64,
}

/// A weighted byte budget shared by concurrent work.
#[derive(Debug)]
pub struct MemoryBudget {
    total: u64,
    state: Mutex<BudgetState>,
    released: Condvar,
}

impl MemoryBudget {
    /// Creates a budget with exactly `total_bytes` bytes.
    #[must_use]
    pub fn new(total_bytes: u64) -> Self {
        Self {
            total: total_bytes,
            state: Mutex::new(BudgetState {
                available: total_bytes,
            }),
            released: Condvar::new(),
        }
    }

    /// Uses `fraction` of the probed bytes, with `floor_bytes` as a minimum.
    #[must_use]
    pub fn from_probe(probe: &dyn MemoryProbe, fraction: f64, floor_bytes: u64) -> Self {
        let scaled = (probe.available_bytes() as f64 * fraction).max(floor_bytes as f64);
        let total = if scaled.is_finite() {
            scaled as u64
        } else if scaled.is_sign_positive() {
            u64::MAX
        } else {
            floor_bytes
        };
        Self::new(total.max(floor_bytes))
    }

    /// Returns this budget's byte capacity.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Waits for and reserves `bytes` until the returned permit is dropped.
    pub fn acquire(
        &self,
        bytes: u64,
        cancel: &CancelToken,
    ) -> Result<MemoryPermit<'_>, MemoryError> {
        if bytes > self.total {
            return Err(MemoryError::TooLarge {
                requested: bytes,
                total: self.total,
            });
        }

        let mut state = lock_unpoisoned(&self.state);
        loop {
            cancel.check().map_err(|_| MemoryError::Cancelled)?;
            if state.available >= bytes {
                state.available -= bytes;
                return Ok(MemoryPermit {
                    budget: self,
                    bytes,
                });
            }

            state = match self.released.wait_timeout(state, CANCEL_POLL_INTERVAL) {
                Ok((guard, _)) => guard,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
    }
}

/// Failure to reserve bytes from a [`MemoryBudget`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MemoryError {
    /// One request exceeds the entire budget.
    #[error("request of {requested} bytes exceeds memory budget of {total} bytes")]
    TooLarge {
        /// Bytes requested by the caller.
        requested: u64,
        /// Total bytes in the budget.
        total: u64,
    },
    /// Cancellation occurred while waiting.
    #[error("cancelled")]
    Cancelled,
}

/// A byte reservation which releases itself on drop.
#[derive(Debug)]
pub struct MemoryPermit<'a> {
    budget: &'a MemoryBudget,
    bytes: u64,
}

impl Drop for MemoryPermit<'_> {
    fn drop(&mut self) {
        let mut state = lock_unpoisoned(&self.budget.state);
        state.available = state
            .available
            .checked_add(self.bytes)
            .expect("memory permit release cannot exceed u64");
        debug_assert!(state.available <= self.budget.total);
        drop(state);
        self.budget.released.notify_all();
    }
}

#[derive(Debug)]
enum Executor {
    Inline,
    Pool(ThreadPool),
}

/// Cancellation, progress, threads and memory available to one heavy job.
#[derive(Debug)]
pub struct Context {
    cancel: CancelToken,
    progress: Arc<dyn ProgressSink>,
    executor: Executor,
    budget: MemoryBudget,
}

impl Context {
    /// Creates a context. A zero thread request is treated as one thread.
    #[must_use]
    pub fn new(
        threads: usize,
        budget: MemoryBudget,
        progress: Arc<dyn ProgressSink>,
        cancel: CancelToken,
    ) -> Self {
        let threads = threads.max(1);
        let executor = if threads == 1 {
            Executor::Inline
        } else {
            Executor::Pool(
                ThreadPoolBuilder::new()
                    .num_threads(threads)
                    .build()
                    .expect("building a rayon thread pool should succeed"),
            )
        };
        Self {
            cancel,
            progress,
            executor,
            budget,
        }
    }

    /// Creates a one-thread context with an unlimited budget and no reporting.
    #[must_use]
    pub fn serial() -> Self {
        Self::new(
            1,
            MemoryBudget::new(u64::MAX),
            Arc::new(NoProgress),
            CancelToken::new(),
        )
    }

    /// Returns this job's cancellation token.
    #[must_use]
    pub fn cancel(&self) -> &CancelToken {
        &self.cancel
    }

    /// Returns this job's memory budget.
    #[must_use]
    pub fn budget(&self) -> &MemoryBudget {
        &self.budget
    }

    /// Starts a progress stage and immediately reports its initial state.
    #[must_use]
    pub fn stage(&self, stage: &'static str, total: u64) -> StageProgress<'_> {
        StageProgress::new(stage, total, self.progress.as_ref())
    }

    /// Applies `f` through this context while preserving input order.
    pub fn map<T: Send, U: Send>(
        &self,
        items: Vec<T>,
        f: impl Fn(T) -> U + Sync + Send,
    ) -> Result<Vec<U>, Cancelled> {
        match &self.executor {
            Executor::Inline => items
                .into_iter()
                .map(|item| {
                    self.cancel.check()?;
                    Ok(f(item))
                })
                .collect(),
            Executor::Pool(pool) => pool.install(|| {
                items
                    .into_par_iter()
                    .map(|item| {
                        self.cancel.check()?;
                        Ok(f(item))
                    })
                    .collect()
            }),
        }
    }

    /// Applies fallible `f` while preserving order and reports the lowest
    /// failing item index.
    pub fn try_map<T: Send, U: Send, E: Send>(
        &self,
        items: Vec<T>,
        f: impl Fn(T) -> Result<U, E> + Sync + Send,
    ) -> Result<Vec<U>, MapError<E>> {
        match &self.executor {
            Executor::Inline => {
                let mut output = Vec::with_capacity(items.len());
                for (index, item) in items.into_iter().enumerate() {
                    self.cancel.check().map_err(|_| MapError::Cancelled)?;
                    output.push(f(item).map_err(|error| MapError::Item { index, error })?);
                }
                Ok(output)
            }
            Executor::Pool(pool) => {
                let outcomes: Vec<_> = pool.install(|| {
                    items
                        .into_par_iter()
                        .map(|item| {
                            if self.cancel.is_cancelled() {
                                ItemOutcome::Cancelled
                            } else {
                                ItemOutcome::Value(f(item))
                            }
                        })
                        .collect()
                });
                collect_outcomes(outcomes)
            }
        }
    }
}

#[derive(Debug)]
enum ItemOutcome<U, E> {
    Cancelled,
    Value(Result<U, E>),
}

fn collect_outcomes<U, E>(outcomes: Vec<ItemOutcome<U, E>>) -> Result<Vec<U>, MapError<E>> {
    let mut output = Vec::with_capacity(outcomes.len());
    for (index, outcome) in outcomes.into_iter().enumerate() {
        match outcome {
            ItemOutcome::Cancelled => return Err(MapError::Cancelled),
            ItemOutcome::Value(Ok(value)) => output.push(value),
            ItemOutcome::Value(Err(error)) => return Err(MapError::Item { index, error }),
        }
    }
    Ok(output)
}

#[derive(Debug)]
struct StageState {
    done: u64,
    last_reported: u64,
    last_report_at: Instant,
}

/// Progress tracker for one stage.
#[derive(Debug)]
pub struct StageProgress<'a> {
    stage: &'static str,
    total: u64,
    started_at: Instant,
    sink: &'a dyn ProgressSink,
    state: Mutex<StageState>,
}

impl<'a> StageProgress<'a> {
    fn new(stage: &'static str, total: u64, sink: &'a dyn ProgressSink) -> Self {
        let started_at = Instant::now();
        let tracker = Self {
            stage,
            total,
            started_at,
            sink,
            state: Mutex::new(StageState {
                done: 0,
                last_reported: 0,
                last_report_at: started_at,
            }),
        };
        tracker.sink.report(&Progress {
            stage,
            done: 0,
            total,
            eta: None,
        });
        tracker
    }

    /// Adds up to `n` completed items, saturating at the stage total.
    pub fn advance(&self, n: u64) {
        let mut state = lock_unpoisoned(&self.state);
        let done = state.done.saturating_add(n).min(self.total);
        self.update_locked(&mut state, done);
    }

    /// Moves completion monotonically to `done`, capped at the stage total.
    pub fn set(&self, done: u64) {
        let mut state = lock_unpoisoned(&self.state);
        let done = state.done.max(done.min(self.total));
        self.update_locked(&mut state, done);
    }

    fn update_locked(&self, state: &mut StageState, done: u64) {
        state.done = done;
        if done == state.last_reported {
            return;
        }

        let now = Instant::now();
        let is_last = done == self.total;
        if !is_last && now.duration_since(state.last_report_at) < PROGRESS_INTERVAL {
            return;
        }

        state.last_reported = done;
        state.last_report_at = now;
        self.sink.report(&Progress {
            stage: self.stage,
            done,
            total: self.total,
            eta: eta(self.started_at, now, done, self.total),
        });
    }
}

fn eta(started_at: Instant, now: Instant, done: u64, total: u64) -> Option<Duration> {
    if done == 0 {
        return None;
    }
    let remaining = total.saturating_sub(done) as f64;
    let seconds_per_item = now.duration_since(started_at).as_secs_f64() / done as f64;
    Some(Duration::from_secs_f64(seconds_per_item * remaining))
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Failure from an ordered fallible map.
#[derive(Debug, PartialEq, Eq)]
pub enum MapError<E> {
    /// The context was cancelled before an item started.
    Cancelled,
    /// The function failed for an item.
    Item {
        /// Input index of the failed item.
        index: usize,
        /// Error returned for that item.
        error: E,
    },
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::thread;

    use super::*;

    fn context(threads: usize, cancel: CancelToken) -> Context {
        Context::new(
            threads,
            MemoryBudget::new(1024),
            Arc::new(NoProgress),
            cancel,
        )
    }

    #[test]
    fn parallel_map_is_ordered_and_thread_count_independent() {
        let items: Vec<u64> = (0..48).collect();
        let expected: Vec<_> = items.iter().map(|value| value * value + 3).collect();

        for threads in [1, 2, 8] {
            let caller = thread::current().id();
            let output = context(threads, CancelToken::new())
                .map(items.clone(), |value| {
                    thread::sleep(Duration::from_millis((7 - value % 7) / 2));
                    if threads == 1 {
                        assert_eq!(thread::current().id(), caller);
                    }
                    value * value + 3
                })
                .unwrap();
            assert_eq!(output, expected);
        }
    }

    #[test]
    fn cancellation_stops_map_quickly_and_skips_remaining() {
        let cancel = CancelToken::new();
        let cancel_from_thread = cancel.clone();
        let started = Arc::new(AtomicUsize::new(0));
        let closure_started = Arc::clone(&started);
        let (cancelled_tx, cancelled_rx) = mpsc::channel();

        let canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(25));
            let at = Instant::now();
            cancel_from_thread.cancel();
            cancelled_tx.send(at).unwrap();
        });
        let result = context(2, cancel).map((0..100).collect(), move |_| {
            closure_started.fetch_add(1, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(10));
        });
        let cancelled_at = cancelled_rx.recv().unwrap();
        let response_time = cancelled_at.elapsed();
        canceller.join().unwrap();

        eprintln!("cancellation response: {response_time:?}");
        assert_eq!(result, Err(Cancelled));
        // The remaining items would take about 500 ms on two threads; the
        // bound leaves room for slow CI runners while still proving the skip.
        assert!(
            response_time < Duration::from_millis(250),
            "cancellation took {response_time:?}"
        );
        assert!(started.load(Ordering::SeqCst) < 100);
    }

    #[test]
    fn try_map_reports_lowest_failing_index() {
        let result = context(8, CancelToken::new()).try_map((0..12).collect(), |index| {
            if index == 2 {
                thread::sleep(Duration::from_millis(20));
                Err("low")
            } else if index == 9 {
                Err("high")
            } else {
                Ok(index)
            }
        });

        assert_eq!(
            result,
            Err(MapError::Item {
                index: 2,
                error: "low"
            })
        );
    }

    #[test]
    fn memory_budget_bounds_concurrent_acquirers() {
        let budget = MemoryBudget::new(3);
        let current = AtomicUsize::new(0);
        let maximum = AtomicUsize::new(0);
        thread::scope(|scope| {
            for _ in 0..12 {
                scope.spawn(|| {
                    let _permit = budget.acquire(1, &CancelToken::new()).unwrap();
                    let now = current.fetch_add(1, Ordering::SeqCst) + 1;
                    maximum.fetch_max(now, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(3));
                    current.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });

        assert_eq!(current.load(Ordering::SeqCst), 0);
        assert!(maximum.load(Ordering::SeqCst) <= 3);
    }

    #[test]
    fn memory_budget_rejects_oversized_requests() {
        let budget = MemoryBudget::new(10);
        assert_eq!(
            budget.acquire(11, &CancelToken::new()).unwrap_err(),
            MemoryError::TooLarge {
                requested: 11,
                total: 10
            }
        );
    }

    #[test]
    fn memory_budget_waiter_observes_cancellation() {
        let budget = MemoryBudget::new(1);
        let holder = budget.acquire(1, &CancelToken::new()).unwrap();
        let cancel = CancelToken::new();
        thread::scope(|scope| {
            let waiter_cancel = cancel.clone();
            let budget = &budget;
            let waiter = scope.spawn(move || budget.acquire(1, &waiter_cancel));
            thread::sleep(Duration::from_millis(15));
            cancel.cancel();
            assert_eq!(waiter.join().unwrap().unwrap_err(), MemoryError::Cancelled);
        });
        drop(holder);
    }

    #[test]
    fn memory_permit_releases_after_panic() {
        let budget = Arc::new(MemoryBudget::new(7));
        let panicking_budget = Arc::clone(&budget);
        let panicked = thread::spawn(move || {
            let _permit = panicking_budget.acquire(7, &CancelToken::new()).unwrap();
            panic!("intentional panic while holding permit");
        })
        .join();

        assert!(panicked.is_err());
        let _permit = budget.acquire(7, &CancelToken::new()).unwrap();
    }

    #[derive(Debug, Default)]
    struct RecordingSink {
        reports: Mutex<Vec<(Instant, Progress)>>,
    }

    impl ProgressSink for RecordingSink {
        fn report(&self, progress: &Progress) {
            lock_unpoisoned(&self.reports).push((Instant::now(), progress.clone()));
        }
    }

    #[test]
    fn progress_reports_endpoints_monotonically_and_throttles() {
        let sink = Arc::new(RecordingSink::default());
        let context = Context::new(1, MemoryBudget::new(1), sink.clone(), CancelToken::new());
        let progress = context.stage("engine.test", 100);
        for _ in 0..10 {
            progress.advance(1);
        }
        thread::sleep(Duration::from_millis(110));
        progress.advance(10);
        for done in 21..100 {
            progress.set(done);
        }
        progress.set(100);

        let reports = lock_unpoisoned(&sink.reports);
        let done: Vec<_> = reports.iter().map(|(_, report)| report.done).collect();
        assert_eq!(done.first(), Some(&0));
        assert!(done.contains(&20));
        assert_eq!(done.last(), Some(&100));
        assert!(done.windows(2).all(|window| window[0] <= window[1]));
        assert_eq!(reports.first().unwrap().1.eta, None);
        assert_eq!(reports.last().unwrap().1.eta, Some(Duration::ZERO));
        for reports in reports.windows(2) {
            if reports[1].1.done != 100 {
                assert!(reports[1].0.duration_since(reports[0].0) >= PROGRESS_INTERVAL);
            }
        }
    }

    #[test]
    fn fixed_memory_and_probe_floor_set_budget() {
        assert_eq!(
            MemoryBudget::from_probe(&FixedMemory(1_000), 0.5, 100).total(),
            500
        );
        assert_eq!(
            MemoryBudget::from_probe(&FixedMemory(10), 0.5, 100).total(),
            100
        );
    }
}
