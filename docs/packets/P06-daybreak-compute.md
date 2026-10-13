# P06 (Daybreak): `secs-compute` — cancellation, progress, ordered parallel map, memory permits

Worktree: `/home/user/wt/p06` (branch `m2/p06`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents; do the
work yourself.

Read first: `crates/secs-compute/src/lib.rs` (the crate's doc comment is the
brief), `docs/adr/0007-*.md` (deterministic parallelism, memory budget),
`docs/ARCHITECTURE.md` §4 (the `MemoryProbe` seam) and `docs/ENGINE.md` §5.

## Goal

The engine (and later the service) runs heavy work through one context that
gives it cancellation, progress, a bounded thread pool and memory permits.
This packet builds that context; nothing calls it yet except its own tests.
No upstream code is ported here (upstream has none of this: it ran the engine
on one thread and killed process pools to cancel).

## Contract

Keep these names and semantics; private helpers and extra methods are yours.

```rust
/// Cooperative cancellation shared by a job and everything it starts.
#[derive(Clone, Debug, Default)]
pub struct CancelToken { /* Arc<AtomicBool> */ }
impl CancelToken {
    pub fn new() -> Self;
    pub fn cancel(&self);
    pub fn is_cancelled(&self) -> bool;
    /// `Err(Cancelled)` once cancelled; call at stage, item and iteration boundaries.
    pub fn check(&self) -> Result<(), Cancelled>;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("cancelled")]
pub struct Cancelled;

/// One progress report.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub stage: &'static str,   // stable id, e.g. "engine.ba", "engine.rc"
    pub done: u64,
    pub total: u64,
    pub eta: Option<std::time::Duration>,  // from the observed rate within the stage
}
/// Where progress goes: the service's job events, or a test recorder.
pub trait ProgressSink: Send + Sync + std::fmt::Debug {
    fn report(&self, progress: &Progress);
}
/// A sink that drops everything.
#[derive(Debug, Default)] pub struct NoProgress;

/// Available memory: the OS adapter lives in `secs-sys`; tests use `FixedMemory`.
pub trait MemoryProbe: Send + Sync + std::fmt::Debug {
    fn available_bytes(&self) -> u64;
}
#[derive(Debug)] pub struct FixedMemory(pub u64);

/// A weighted byte budget. `acquire` blocks until the bytes are free; a request
/// larger than the whole budget fails at once with `MemoryError::TooLarge`
/// carrying both numbers; waiting stops with `MemoryError::Cancelled` when the
/// token is cancelled. The permit releases its bytes on drop.
pub struct MemoryBudget { /* … */ }
impl MemoryBudget {
    pub fn new(total_bytes: u64) -> Self;
    /// `fraction` of what the probe reports, at least `floor_bytes`.
    pub fn from_probe(probe: &dyn MemoryProbe, fraction: f64, floor_bytes: u64) -> Self;
    pub fn total(&self) -> u64;
    pub fn acquire(&self, bytes: u64, cancel: &CancelToken) -> Result<MemoryPermit<'_>, MemoryError>;
}
pub struct MemoryPermit<'a> { /* … */ }

/// Everything a heavy job gets.
pub struct Context { /* cancel, progress sink, rayon pool, memory budget */ }
impl Context {
    pub fn new(threads: usize, budget: MemoryBudget, progress: Arc<dyn ProgressSink>, cancel: CancelToken) -> Self;
    /// One thread, unlimited budget, no progress, never cancelled (tests, CLI defaults).
    pub fn serial() -> Self;
    pub fn cancel(&self) -> &CancelToken;
    pub fn budget(&self) -> &MemoryBudget;
    /// Starts a progress stage of `total` items; the returned tracker reports
    /// done/total and an ETA, throttled to at most one report per 100 ms
    /// except the first and the last.
    pub fn stage(&self, stage: &'static str, total: u64) -> StageProgress<'_>;
    /// Ordered parallel map on the context's pool: output `i` is `f(items[i])`,
    /// whatever the thread count. Checks cancellation before each item; once
    /// cancelled, remaining items are skipped and the call returns
    /// `Err(Cancelled)`. `f` must not accumulate floats across items.
    pub fn map<T: Send, U: Send>(&self, items: Vec<T>, f: impl Fn(T) -> U + Sync + Send) -> Result<Vec<U>, Cancelled>;
    /// Same, for a fallible `f`; the first error by item index wins.
    pub fn try_map<T: Send, U: Send, E: Send>(&self, items: Vec<T>,
        f: impl Fn(T) -> Result<U, E> + Sync + Send) -> Result<Vec<U>, MapError<E>>;
}
pub struct StageProgress<'a> { /* … */ }
impl StageProgress<'_> { pub fn advance(&self, n: u64); pub fn set(&self, done: u64); }
#[derive(Debug)] pub enum MapError<E> { Cancelled, Item { index: usize, error: E } }
```

Rules:

- `rayon` (workspace dependency) with a dedicated `ThreadPool` per
  `Context`, not the global pool. `map` uses `install` + indexed parallel
  iteration (`into_par_iter().map(..).collect()` keeps order). A single-thread
  context runs inline on the calling thread.
- No `unsafe`, no OS dependencies (the crate root already forbids unsafe;
  `[lints] workspace = true`). Time comes from `std::time::Instant` only for
  the ETA; it never influences results.
- Memory permits use `Mutex` + `Condvar`; waiting wakes on release and on a
  short timeout to observe cancellation. A dropped permit never leaks bytes,
  also when the holder panics.
- No global state.

## Tests (in the crate)

- `map` returns identical outputs for 1, 2 and 8 threads on a workload whose
  items take uneven time; the output order is the input order.
- Cancelling from another thread mid-`map` returns `Err(Cancelled)` within
  50 ms of the cancel for items that each take 10 ms, and skips the rest.
- `try_map` reports the lowest failing index even when a higher one fails
  first in time.
- `MemoryBudget`: concurrent acquirers never exceed the total; an oversized
  request fails at once with both numbers; a waiter returns `Cancelled` after
  cancel; a permit dropped by a panicking thread frees its bytes.
- `StageProgress`: the first and last reports always arrive, the sink sees
  `done` monotone, throttling holds (use a recording sink).

Register `compute.cancellation`, `compute.parallel_map`,
`compute.progress` (kind `"compute"`, milestone `"M2"`, status
`implemented`, with your tests) in `features.toml`, and set
`compute.memory_budget` to `implemented` with its tests (its milestone moves
to M2).

## Files you may change

- `crates/secs-compute/**`
- `features.toml` (the entries above only)
- `Cargo.lock`, only as cargo rewrites it

Others hold `crates/secs-engine/**`, `oracle/**` (P05). Do not touch them.

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p06
export CARGO_INCREMENTAL=0
cargo fmt --all -- --check
cargo clippy -p secs-compute --all-targets --locked -- -D warnings
cargo clippy -p secs-compute --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test -p secs-compute -p secs-policy 2>&1 | grep -E "test result|FAILED|panicked"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-compute
python3 .github/scripts/check_feature_tests.py
```

## Report

Files changed; verification output; the timing numbers your cancellation test
observed; anything you could not meet and why.
