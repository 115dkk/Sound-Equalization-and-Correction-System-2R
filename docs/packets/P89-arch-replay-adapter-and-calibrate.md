# P89: audit #92 C9 (one oracle replay adapter, TD-53) and C7 (calibration as one call)

Repository worktree: `/home/user/wt-rust2`. Claude names the branch when it
hands this over; do not switch branches, push, or spawn sub-agents. Commit
once per item with the subject given; Claude reviews, re-runs the gates and
re-authors the commits.

Read first: `CLAUDE.md`, `CONTEXT.md`, `docs/ARCHITECTURE.md` sections 4–5,
`docs/ENGINE.md`, `docs/adr/0004-parity-and-deviations.md`,
`docs/adr/0012-resource-optimizations.md` (RC shared preparation, bit
identity), `TECH_DEBT_AUDIT.md` row TD-53, then
`crates/secs-engine/src/{test_support.rs,lib.rs,fit.rs,design.rs,calibration.rs,normalize.rs,xo.rs}`,
`crates/secs-engine/src/tests/{cases.rs,design.rs,mod.rs}`,
`crates/secs-engine/examples/engine_perf.rs`, `crates/secs-engine/Cargo.toml`,
`crates/secs-service/src/{calibration.rs,export.rs}` (the calibration job,
`prepare`, the XO job, and the cached-mdat redesign near `export.rs:272-311`).

Both items are deepenings (resolve-audit category A, "deepening a module"):
write the tests at the deepened module's interface first, move the behaviour
behind it until they pass, then delete the old tests on the shallow modules
that the new ones cover. Results stay bit-identical: every golden, every
`_before` equivalence test and every downstream oracle test (secs-export,
secs-service, secs-gpu) must pass unchanged in its assertions.

## 1. C9 — `test_support` is the only replay adapter and case loader (TD-53)

Today there are two case loaders (`src/tests/cases.rs`, `test_support.rs`)
and two replay fitter factories (`src/tests/design.rs:~150-252`, with request
hash checks, ADR 0012's RC shared-preparation de-duplication and per-stage
call counts; `test_support::replay_case`, without them, which injects with
an infinite tolerance off the mixed mode). Downstream crates get the weaker
one.

Design (fixed):
- `crates/secs-engine/src/lib.rs`: `#[cfg(any(test, feature = "test-support"))] pub mod test_support;`
  (dev-dependencies already carry `secs-oracle` and `serde_json`).
- `test_support` owns: the case type (measurements, config, tuning, settings
  JSON) with `load(name)`, `load_fixture(&Fixture)`, `names()` and the JSON
  helpers; and one replay factory, `pub struct Replay` built by
  `Replay::for_case(name: &str) -> Replay`, exposing
  `fn factory(&self) -> Box<FitterFactory<'_>>` (or an equivalent that lets
  `design` borrow it) and `fn finish(self)` which asserts every stage's call
  count. It carries everything the engine test's replayer does today:
  `verify_request_hash`, the RC shared-preparation de-duplication with its
  assertions, the tolerance policy (`PLATFORM_RTOL` on the oracle libm,
  infinity with the `MEASURE … report-only` lines only where the engine test
  uses infinity today: the `room` case and foreign libm), and the counts.
- `replay_case(name)` and `production_case(name, threads)` keep their
  signatures and use the above; downstream tests therefore get the checks.
- `src/tests/cases.rs` and the replay part of `src/tests/design.rs` are
  deleted; engine tests call `test_support`. `engine_perf` takes its case
  loading from `test_support` (add `required-features = ["test-support"]`
  to its `[[example]]` if needed; if it still needs private engine modules
  through `#[path]`, keep those and say so).
- Red first: a test at `test_support`'s interface that replays a non-mixed
  case (pick a linear or ultra case from `oracle/` fixtures; list the case
  names you found) through `replay_case` and asserts the design equals the
  one from the engine's own replayer path (call counts finished). It must
  fail on today's `test_support` for the reason TD-53 states (count or
  tolerance), and pass after. Report the failure text.
- Commit: `refactor(audit): A-71 오라클 재생 adapter와 케이스 로더를 test_support 하나로 모읍니다 (#92 C9, TD-53)`

## 2. C7 — calibration is one engine call

Today the service assembles four steps itself, twice (calibration job and
cached-mdat export redesign): `engine_inputs` → sample-rate
`normalize_measurements` → `production_fitters(job.cancel())` → `design`,
counting resampled IRs in the service and passing adapter notes by hand; the
export path drops the resample notice.

Design (fixed; names may be adjusted only to match existing conventions):
```rust
// crates/secs-engine/src/calibration.rs (re-exported from lib.rs)
pub struct CalibrationRequest<'a> {
    pub view: &'a ProjectView<'a>,
    pub irs: &'a MeasurementSet,
    pub settings: &'a CalibrationSettings,
    pub subs_only: bool,
}
pub struct Calibration {
    pub design: Design,
    /// The normalised measurements and config the design was made from.
    pub measurements: MeasurementSet,
    pub config: EngineConfig,
    /// IRs resampled to 48 kHz by the sample-rate policy.
    pub resampled: usize,
}
/// Resolve, normalise and design with production fitters (`ctx.cancel()`).
pub fn calibrate(req: &CalibrationRequest<'_>, ctx: &Context) -> Result<Calibration, EngineError>;
/// The same with the caller's fitters: the test seam (oracle replay).
pub fn calibrate_with(req: &CalibrationRequest<'_>, ctx: &Context, fitters: &FitterFactory<'_>) -> Result<Calibration, EngineError>;
```
- `engine_inputs`' notes go into the design's `adapter_notes` inside
  `calibrate_with`, as the service passes them today.
- The service's calibration job and the export redesign both call
  `calibrate`; the service no longer counts resampled IRs or calls
  `normalize_measurements`/`production_fitters`/`design` directly. Where the
  service still needs `engine_inputs` before the job starts (validation that
  must fail at request time, the run signature), keep that call and say why.
  The summary's `measurement_resampled` notice keeps its code, params and
  position (it is appended after the summary's other notes today). The
  export redesign now reports the same notice where that path reports notes
  (find it; if the export path has no notes channel, say so and leave it).
- The XO job builds its inputs with the same resolution and normalisation
  (a shared private helper inside the engine, e.g. `prepare_inputs`), so the
  two cannot drift; `recommend_xo`'s public signature changes only if needed.
- Tests at the new interface: `calibrate_with` with `test_support`'s replay
  factory on a shared case equals today's assembled path (bit for bit,
  compare the `Design` fields the existing goldens compare); a case with a
  44.1 kHz IR reports `resampled > 0`; cancellation before the call returns
  `Cancelled`. Then delete service tests that only re-checked the assembly
  order, if any (name them).
- Commit: `refactor(audit): A-68 보정을 엔진 호출 하나로 모읍니다 (#92 C7)`

## Allowed files

`crates/secs-engine/**`, `crates/secs-service/src/{calibration.rs,export.rs}`
and `crates/secs-service/tests/**` (call sites and moved tests),
`crates/secs-export/tests/**`, `crates/secs-gpu/tests/**` (only if a
`test_support` signature change requires it), `docs/ENGINE.md` (the entry
point description), `Cargo.lock`. Other agents work in
`/home/user/Sound-Equalization-and-Correction-System-2R`, `/home/user/wt-l3`
and `/home/user/wt-ui`; never touch those trees.

## Gates (foreground; do not end the turn before they finish)

Export `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only`. After
each item: `cargo fmt --all -- --check`;
`cargo clippy -p secs-engine -p secs-service -p secs-export -p secs-gpu --all-targets --locked -- -D warnings`;
`cargo test -p secs-engine -p secs-export -p secs-service -p secs-gpu --locked`.
Record the test counts before item 1 and after each item. At the end
`cargo test --workspace --locked` once and
`python3 .github/scripts/check_feature_tests.py`. Check `df -h /home/user`
before the workspace test; if under 5 GB is free, delete older duplicate
executables in `/home/user/wt-rust2/target/debug/deps` (keep the newest per
name and source) and nothing else.

## Report (final answer)

Per item: commit hash, files, test names added and deleted, the red failure
text, test counts before/after, and every place you kept an older call with
the reason. Then the tail of the last gate output and anything left undone.
