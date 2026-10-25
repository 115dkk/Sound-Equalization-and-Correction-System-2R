# P92: audit #92 C5, scoped — the REW link owns the license state, the method adapter owns its commit boundary

Claude names the worktree and branch when it hands this over. Do not switch
branches, push, or spawn sub-agents. One commit per item, subjects below;
Claude reviews, re-runs the gates and re-authors the commits.

The user scoped C5 to two things: the REW link (client creation and the
license state in one place) and the commit boundary. Level-monitor sessions
and device availability reports stay where they are.

Read first: `CLAUDE.md`, `docs/adr/0005-native-measurement.md`,
`crates/secs-service/src/{measurement.rs,monitor.rs,lib.rs,watch.rs}`,
`crates/secs-service/src/measure/{mod.rs,rew.rs,native.rs,sample.rs}`,
`crates/secs-rew/src/lib.rs` (`RewClient`, `RewError`),
`crates/secs-service/tests/` (REW scripted-transport tests).

## 1. The REW link

Today `Service.rew_license: Arc<Mutex<Option<bool>>>` is written from four
places: `measure_error` as a side effect when it converts
`RewError::ProRequired` (`measurement.rs:~73`), `rew_status`, the REW
device-set path (`measurement.rs:~396`), and the level paths that pass
`rew.then_some(&rew_license)`. `RewClient::new(Transport((deps.rew_transport)()))`
is built in about six places (`grep -n "RewClient::new" crates/secs-service/src`).

Design (fixed):
```rust
// crates/secs-service/src/measure/rew.rs
/// The service's one link to REW: it builds clients on the configured
/// transport and keeps what REW said about the Pro license.
#[derive(Clone)]
pub(crate) struct RewLink { /* transport factory, Arc<Mutex<Option<bool>>> */ }
impl RewLink {
    pub(crate) fn new(deps: &Deps) -> Self;
    pub(crate) fn client(&self) -> secs_rew::RewClient<…>;
    /// Records what an error says about the license (ProRequired → not Pro).
    pub(crate) fn observe(&self, error: &secs_rew::RewError);
    pub(crate) fn license(&self) -> Option<bool>;
    pub(crate) fn forget_license(&self);
}
```
`measure_error` becomes a pure conversion (drop its `rew_license`
parameter). Every place that converts a REW error first calls
`link.observe(&error)` (the `RewMeasurer` can hold the link and observe its
own errors, which removes the `rew.then_some(..)` plumbing; choose the one
that leaves fewer call sites and say which). `Service.rew_license` is
replaced by a `RewLink` field; all `RewClient::new` sites use
`link.client()`. Behaviour is unchanged: the existing REW status/license
tests pass untouched. Test first: a unit test that `observe` on
`ProRequired` sets the license to `Some(false)` and on other errors leaves
it; and a service-level test that a `ProRequired` during a level check
makes `rew_status`'s `pro` false (find whether one exists; if so, name it
and add only the unit test).
Commit: `refactor(audit): A-76 REW 연결 하나가 클라이언트와 라이선스 상태를 맡습니다 (#92 C5)`

## 2. The commit boundary belongs to the method adapter

`start_measure` decides atomicity from the method string
(`atomic_position = method(&project) != "rew"`). Add to the `Measurer`
trait:
```rust
/// How a position's results are committed: all roles at once (native,
/// mock), or each role as it arrives (REW, which records role by role).
fn commit_boundary(&self) -> CommitBoundary { CommitBoundary::Position }
```
with `pub(crate) enum CommitBoundary { Position, Role }`; `RewMeasurer`
returns `Role`. `start_measure` reads it from the measurer it built; the
string comparison goes. Test: the existing measure tests for native
(atomic, cancel before commit leaves nothing) and REW (completed roles kept
on a later failure) pass; add a unit test of each adapter's
`commit_boundary`.
Commit: `refactor(audit): A-76 측정 방식 adapter가 커밋 경계를 정합니다 (#92 C5)`

## Allowed files

`crates/secs-service/src/**`, `crates/secs-service/tests/**`. Nothing else.

## Gates (foreground; do not end the turn before they finish)

Export `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only`.
After each item: `cargo fmt --all -- --check`;
`cargo clippy -p secs-service -p secs2r-cli -p secs2r --all-targets --locked -- -D warnings`;
`cargo test -p secs-service -p secs2r-cli --locked`, counts before and
after. At the end `python3 .github/scripts/check_feature_tests.py`.

## Report (final answer)

Per item: commit hash, the sites converted (file:function), tests added,
counts before/after, the observe placement you chose and why, tail of the
last gate output.
