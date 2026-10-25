# P90: audit #92 C4 — jobs commit only to the project they started from

Claude names the worktree and branch when it hands this over. Do not switch
branches, push, or spawn sub-agents. One commit, subject below; Claude
reviews, re-runs the gates and re-authors it.

Read first: `CLAUDE.md`, `CONTEXT.md`, `docs/adr/0009-compact-result-ipc-and-job-retention.md`
(late results of an earlier project must not land in a newer one),
`crates/secs-service/src/{session.rs,jobs.rs,calibration.rs,measurement.rs,export.rs,monitor.rs,lib.rs}`.

## Problem

Every job checks "is the open project still the one I started from" by hand:
it copies `open.identity` when the handler starts, later locks the session,
compares, and calls `job.begin_commit()` (which fails with `Cancelled` and
makes the job non-cancellable). The order differs between jobs (calibration
checks identity first; measurement calls `begin_commit` first), the error
wording is repeated, and `calibration.rs` has a private `require_identity`.
Find every site: `grep -n "identity\|begin_commit" crates/secs-service/src/*.rs`.

## Design (fixed)

In `session.rs`:

```rust
/// The open project a request started from (ADR 0009). A job takes one when
/// its handler starts and changes the project only through `commit`/`apply`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProjectTicket(u64);

impl Session {
    /// The open project's ticket; `NoProject` when none is open.
    pub(crate) fn ticket(&self) -> Result<ProjectTicket, IpcError>;
}

/// Commits a finishing job's change (ADR 0009). Under the session lock: a
/// different open project fails with `StaleInputs` ("project changed while
/// computing"), then `job.begin_commit()` fails with `Cancelled` if the job
/// was cancelled; only then `change` runs. `change` returns its value and
/// whether it changed the project; `commit` sets `dirty` when it did.
pub(crate) fn commit<T>(
    session: &Mutex<Session>,
    ticket: ProjectTicket,
    job: &JobContext,
    change: impl FnOnce(&mut OpenProject) -> Result<(T, bool), IpcError>,
) -> Result<T, IpcError>;

/// The same check for a job that keeps running and writes as it goes (the
/// level monitor): the ticket check only, no `begin_commit`.
pub(crate) fn apply<T>(
    session: &Mutex<Session>,
    ticket: ProjectTicket,
    change: impl FnOnce(&mut OpenProject) -> Result<(T, bool), IpcError>,
) -> Result<T, IpcError>;
```

The exact placement (free functions or methods on a `Session` handle) may
follow how the session mutex is held today; the contract above is fixed.
Every job commit point (calibration, MIMO, XO recommendation, measurement,
sample load, mdat/Dirac import, cached-mdat export redesign, level monitor,
and any other you find) takes a ticket in its handler and commits through
`commit` or `apply`. `require_identity` and the hand-written comparisons
disappear. Lock order inside `change` stays as today (session, then
results; see the comment near `calibration.rs:734`). Synchronous request
handlers (`project_edit.rs`) are not part of this packet.

The measurement job records role by role through `measure_recording`'s
callback, and only an atomic method (native, mock; today
`atomic_position = method != "rew"` in `start_measure`) calls
`begin_commit` on its first record. Keep that: the first record of an
atomic method goes through `commit`, every other record through `apply`.
(P92 later moves the atomic/streaming choice into the method adapter; do not
do that here.)

The order is ticket first, then `begin_commit`. If a test expected
`Cancelled` where the project had also changed (measurement's old order),
say which test and what it expects now.

## Tests

Deepening (resolve-audit category A): tests at the new interface first. A
unit test module beside `commit` that runs real jobs through the job
registry without the engine (a closure job that calls `commit`): a stale
ticket → `StaleInputs` and `change` did not run; a cancelled job →
`Cancelled` and `change` did not run; a valid ticket → `change` ran and
`dirty` follows its boolean; `apply` with a stale ticket → `StaleInputs`.
Then remove any service test that only re-checked one job's hand-written
comparison and is now covered (name each); keep end-to-end tests that
exercise a real job's stale or cancelled path.

Commit: `refactor(audit): A-69 잡이 시작한 프로젝트에만 커밋하는 규약을 Session이 맡습니다 (#92 C4)`

## Allowed files

`crates/secs-service/src/**`, `crates/secs-service/tests/**`. Nothing else.

## Gates (foreground; do not end the turn before they finish)

Export `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only`.
`cargo fmt --all -- --check`;
`cargo clippy -p secs-service -p secs2r-cli -p secs2r --all-targets --locked -- -D warnings`;
`cargo test -p secs-service -p secs2r-cli --locked` before (record counts)
and after. `python3 .github/scripts/check_feature_tests.py`.

## Report (final answer)

Commit hash, every commit point converted (file:function), tests added and
removed, counts before/after, any changed expectation, tail of the last gate
output.
