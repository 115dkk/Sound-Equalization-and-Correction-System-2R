# P188 (Daybreak): audit #186, the engine ADRs 0042, 0046, 0050 and 0047

Runs AFTER the Phase 1 packets P183..P187 have been rebased into
`fix/audit-186`. Claude re-creates the worktree branch from the then-current
`fix/audit-186`; do not start before Claude says the branch exists.

Tree: `E:\wt\audit-engine`, a git worktree on branch `audit/186-engine-adr`.
Work only there. Every cargo command runs with
`CARGO_TARGET_DIR=C:\cargo-target\audit-engine`. Commit on that branch, one
commit per ADR step, in the order below. Do not push, do not rebase, do not
spawn sub-agents, do not touch `audit-state.md`, do not edit `docs/**`
except the one `lib.rs` doc comment named under ADR 0050.

Allowed files:

- `crates/secs-engine/**`, `crates/secs-engine-testkit/**`;
- `crates/secs-compute/**` (ADR 0047 step 1 only);
- `crates/secs-filters/**` (ADR 0047 step 3 only);
- `crates/secs-export/src/lib.rs`: only the eight `run.design.*` reads and
  the two free-function calls ADR 0046 step 2 names;
- `crates/secs-service/src/calibration.rs`: only the removal of the
  `notices`/`guard_cause_params`/`scalar_notice` match (ADR 0042 step 1) and
  the two `design.replay.*` reads at lines 325-326 (ADR 0046 step 3);
- `crates/secs-service/src/export.rs`: only the call sites of the
  `secs_filters` render/prepare functions whose signature ADR 0047 changes;
- `crates/secs-service/tests/service.rs`: append one wire test (ADR 0042
  step 2) at the end of the file, nothing else;
- `crates/secs-types/src/notes.rs` (new), the one `pub mod notes;` line in
  `crates/secs-types/src/lib.rs`, and the regenerated
  `apps/secs2r/ui/src/ipc/bindings/*.ts` (ADR 0042 step 3 only);
- `crates/secs-policy/tests/gates.rs`: one new test (ADR 0042 step 1);
- `Cargo.toml` of `secs-engine`, `secs-engine-testkit`, `secs-compute`,
  `secs-filters` and the workspace `Cargo.lock` (ADR 0050 and 0047 only);
- `features.toml`: register every new test name in the `[[feature]]` entry
  that covers the behaviour (`engine.*`, `ipc.*`, `format.*`) or add one in
  the same style; `python3 .github/scripts/check_feature_tests.py` must
  accept it.

Forbidden: `apps/secs2r/ui/src/**` other than the regenerated bindings,
`crates/secs-service/**` beyond the lines above, `docs/**`, `CHANGELOG.md`.

Read first: `CLAUDE.md`, `CONTEXT.md` (the design-term table), the four
ADRs in full (`docs/adr/0042-engine-note-typed-value.md`,
`0046-narrow-engine-interface.md`, `0050-one-engine-test-loader.md`,
`0047-cancel-progress-context.md`), `docs/adr/0004` (goldens are the
contract), `docs/ENGINE.md`, `crates/secs-engine/src/lib.rs`,
`crates/secs-engine/src/notes.rs`, `crates/secs-engine/src/design.rs:60-100`,
`crates/secs-service/src/calibration.rs:677-1095` and its tests at
`1340-1420` and `1668-1900`, `crates/secs-engine/src/test_support.rs`,
`crates/secs-engine/src/testkit.rs`, `crates/secs-engine-testkit/src/lib.rs`,
`crates/secs-compute/src/lib.rs:60-150, 337-470`,
`crates/secs-engine/src/tests/cancel.rs`, `crates/secs-filters/src/render.rs`
and `lib.rs:1580-1840`.

## Why

The monthly audit's architecture review (issue #186) found four leaks in
the engine's interface. Each has an ADR now. This packet implements them in
the resolve-audit skill's 'deepening' shape: the tests at the deepened
interface are written first, behaviour moves behind it, and the shallow
tests the new ones cover are deleted. The engine's numeric code and its
goldens (ADR 0004) do not change; `cargo test -p secs-engine --locked` must
pass bit-identically after every step.

## The protocol for every step

1. Announce the step in the report (`── X-n step k: … ──`).
2. **Tests first.** Write the tests named below; run them; they fail on the
   assertion (stub the new item so they compile). Expected values are
   literals copied from the existing tests or fixtures the step names, never
   computed by the code under test.
3. **Move.** The smallest code move that makes them pass.
4. **Delete.** The shallow tests or functions the step lists.
5. Gate: `cargo fmt --all -- --check`,
   `cargo clippy -p <touched crates> --all-targets --locked -- -D warnings`,
   `cargo test -p <touched crates> --locked` (always including
   `-p secs-engine`). After the last step of each ADR and at the end:
   `cargo test --workspace --locked` in the foreground.
6. Commit with `git add <the files you touched>` (never `git add -A`), in
   this shape, Korean, subject under 72 characters where possible:

   ```
   refactor(audit): X-n <한국어 한 문장> (#186 A<n>)

   ADR 00NN의 <k>단계입니다. <무엇을 옮겼고 무엇이 그대로인지 한두 문장.>

   Co-Authored-By: Claude Mythos 5.1 <noreply@anthropic.com>
   ```

   `feat(audit):` instead of `refactor(audit):` where the step changes
   observable behaviour (marked below). Korean sentences: 하십시오체, no
   em-dash, no program-as-subject verbs 묻다/적다/넘기다 (use
   표시합니다/기록합니다/전달합니다), "추가하다" not "더하다".
7. If a step needs a file you may not edit, stop that step, keep its tests
   `#[ignore]`d with a comment naming the dependency, commit nothing for it,
   and report it.

## ADR 0042 (X-1): the engine owns the note's record convention

### Step 1 · `Note::record()` in the engine (structure only)

Tests first, in `crates/secs-engine/src/notes.rs` `#[cfg(test)]`:

- `record_of_polarity_reversed_matches_the_service_literal`,
  `record_of_pair_phase_matches_the_service_literal`,
  `record_of_rc_limit_and_rc_bands_match_the_service_literals`,
  `record_of_sub_filters_mode_decay_and_mimo_guard_match_the_service_literals`,
  `record_of_flat_notes_preserves_scalar_values_and_order`: each builds the
  `Note` the corresponding service test builds today
  (`calibration.rs:1340` `polarity_notes_map_to_notices_with_role_confidence_and_groups`,
  `:1364`, `:1380`, `:1417`, `:1668`, `:1700`, `:1724`, `:1752`, `:1784`,
  `:1811`) and asserts `note.record()` equals the same `(code, params)`
  literals those tests assert (copy the literals; do not call the service).
  At this step the three mini-formats (`pair_phase` `bands`, `dip_limit`
  `dips`, `rc_bands` `bands`) are still single records with the joined
  string, exactly as today.
- `every_note_variant_records_at_least_one_code`: the engine twin of
  `calibration.rs:1899 every_engine_note_maps_to_a_notice_code_and_params`.
- In `crates/secs-policy/tests/gates.rs`: `engine_note_codes_are_catalog_keys`
  reads `apps/secs2r/ui/src/i18n/ko.json` and asserts that every code
  `Note::record()` can produce (expose the code list as
  `secs_engine::notes::RECORD_CODES: &[&str]`) has a `notice.<code>` key
  (the policy crate already reads repository files; the engine crate must
  not, ADR 0016).

Interface (fixed, from the ADR):

```rust
pub struct NoteRecord { pub code: &'static str, pub params: Vec<(&'static str, String)> }
impl Note { pub fn record(&self) -> Vec<NoteRecord>; }
impl MimoNote { pub fn record(&self) -> Vec<NoteRecord>; }   // if the service maps MimoNote separately today
```

Move: the bodies of `notices`, `guard_cause_params` and `scalar_notice`
(`calibration.rs:677-1095`) into `record()`; the service keeps
`impl From<NoteRecord> for Notice` and the call sites become
`note.record().into_iter().map(Notice::from)`. Delete: the three service
functions and the ten service tests listed above (their literals now live
in the engine). `scalar_notice` is `pub(crate)`: find its other callers
(`grep -n scalar_notice crates/secs-service/src`) and route them through
`record()`.

Gate: `cargo test -p secs-engine -p secs-service -p secs-policy --locked`.
Commit: `refactor(audit): X-1 엔진이 Note의 기록 규약을 소유합니다 (#186 A1)`.

### Step 2 · one record per item for the three mini-formats (behaviour)

Tests first (engine `notes.rs`): `pair_phase_records_one_record_per_band_with_index`
(a note with two bands → two records, codes both `pair_phase`, params
`index` = `"0"`/`"1"`, `lo_hz`, `hi_hz`, `phase_deg`, `loss_db` as the
literal strings the old joined format carried, plus the role params),
`dip_limit_records_one_record_per_dip` (`index`, `hz`, `depth_db`,
`fill_db`), `rc_bands_records_one_record_per_band` (`index`, `lo_hz`,
`hi_hz`); and the wire test appended to `crates/secs-service/tests/service.rs`:
`calibration_summary_notes_are_scalar_records_td162` (after a mock
calibration that produces at least one `dip_limit` and one `rc_bands`
note, no `params` value in `CalibrationSummary.notes`/`rc` contains `,`
or `:`).

Move: the three `record()` arms. Keep the codes. This changes what the
screen receives, so run `cd apps/secs2r/ui && npm run typecheck && npm test`
and list every failing UI test in the report without editing the UI: Claude
dispatches the screen change (`text.ts` drops its `split` parsing) to the
UI worker before the PR merges.

Commit: `feat(audit): X-1 미니 형식 알림을 항목마다 한 record로 폅니다 (#186 A1 TD-162)`.

### Step 3 · `Note` as named structs in `secs-types` (structure)

Tests first, in `crates/secs-types/src/notes.rs` `#[cfg(test)]`:
`each_note_variant_round_trips_through_json_with_its_snake_case_code` (for
every variant: a JSON literal with `"code": "<snake_case>"` deserialises,
re-serialises to the same bytes; include `PairBand`, `Dip`, `Mode`, `Band`,
`SubFilterNote`, `Peq`, `RoleNote<T>` samples) and
`note_json_rejects_non_finite_numbers` (serialising a note holding `NaN`
fails; this pins the invariant the ADR names).

Move: `Note`, `MimoNote`, `GuardCause` and the carried types to
`crates/secs-types/src/notes.rs` with the ADR's derives
(`#[serde(tag = "code", rename_all = "snake_case")]`, `TS`, `#[ts(export)]`);
tuples become the named structs of the ADR; `secs-engine` re-exports
(`pub use secs_types::notes::{GuardCause, MimoNote, Note};`) and its
producers (`polarity.rs`, `pair_phase.rs`, `rc.rs`, `mimo/*`) fill the
structs. `record()` stays in the engine as an extension (an inherent
`impl` on a foreign type is not possible; use a
`pub trait NoteRecords { fn record(&self) -> Vec<NoteRecord>; }` implemented
in the engine, and keep `Note::record` callable through the trait). Run
`cargo test -p secs-types --locked` so `Note.ts` and friends regenerate;
commit only the binding files whose `git diff` has content changes. The
wire fields (`CalibrationSummary.notes` etc.) stay `Vec<Notice>` in this
packet; ADR 0042 steps 4-5 are a later packet.

Gate: `cargo test -p secs-types -p secs-engine -p secs-service --locked`,
`cd apps/secs2r/ui && npm run typecheck`.
Commit: `refactor(audit): X-1 Note를 secs-types의 이름 있는 struct로 옮깁니다 (#186 A1 TD-257)`.

## ADR 0046 (X-5): narrow the engine interface

### Step 1 · `Design` methods and the surface test

Tests first:

- `crates/secs-engine/tests/surface.rs::design_methods_reproduce_the_values_secs_export_reads`:
  add `secs-engine-testkit` as a dev-dependency of `secs-engine` (the
  feature unification ADR 0050 describes makes `secs_engine::testkit`
  available to integration tests; it is NOT usable from `src/tests`), replay
  the `dolby` case with `secs_engine_testkit::replay_case("dolby")`, and
  assert `design.config().main_position`, `design.rc_hi()`,
  `design.latency_ms()`, `design.main_roles()` and the digest of
  `design.fir("L")` against the literals the `secs-export` golden fixture
  for that case records (take them from the oracle fixture JSON that
  `crates/secs-export/tests/golden.rs` reads, not from the engine).
- `crates/secs-engine/tests/surface.rs::public_surface_is_the_adr_0046_list`:
  parses `crates/secs-engine/src/lib.rs` (`env!("CARGO_MANIFEST_DIR")`,
  the crate's own file, allowed) for `pub use` and `pub mod` lines and
  compares the item set with the literal list in ADR 0046 '남는 공개 표면'.
  This test fails until step 4; mark it `#[ignore = "ADR 0046 step 4"]` in
  this commit and un-ignore it in step 4.

Move: add the ten methods of the ADR to `impl Design` (`design.rs`) over the
existing fields; `stage()` wraps today's free `stage_responses`,
`export_ir()` wraps `replay.export_ir`; fields stay `pub`.
Commit: `refactor(audit): X-5 Design에 바깥이 읽는 값의 메서드를 둡니다 (#186 A5)`.

### Step 2 · `secs-export` reads through the methods

Change the reads in `crates/secs-export/src/lib.rs` (`run.design.replay.config()`
at 108-119, 417, 612; `run.design.subs` 213; `run.design.chain.subs` 242;
`run.design.rc_hi` 249, 481; `run.design.chain.mains` 428;
`run.design.chain.latency_ms` 480; `run.design.replay.export_view` 416, 611;
free `stage_responses` 434, 630) to `Design` methods; edit nothing else
there. Goldens bit-identical: `cargo test -p secs-export --locked`.
Commit: `refactor(audit): X-5 secs-export가 Design을 메서드로 읽습니다 (#186 A5)`.

### Step 3 · the service's two `replay` reads

The ADR expected a cached `DesignReplay`; the current service does not keep
one (`grep -rn DesignReplay crates/secs-service/src` is empty). The only
reads are `crates/secs-service/src/calibration.rs:325-326`
(`design.replay.measurements()`, `design.replay.config()`). Replace them with
`design.config()` and a new `Design::measurements(&self) -> &MeasurementSet`
(add it to the method list and to the surface test's literal list). Skip
`replay_for_cache`/`from_replay`; say so in the report.
Commit: `refactor(audit): X-5 서비스가 Design의 replay 필드를 직접 읽지 않습니다 (#186 A5)`.

### Step 4 · remove the 36 re-exports and five `pub mod`, lower `Design`'s fields

Delete the re-exports listed in ADR 0046 '배경' and make `grid`,
`export_view`, `pair_phase`, `polarity`, `display` private (`pub(crate)`);
`FitterFactory`, `DesignRequest`, `design`, `FitStage` go behind
`#[cfg(any(test, feature = "test-support"))]`; `replay`, `spectra`, `mains`,
`subs`, `chain` and `MainDesign::{before, combined, after}` become
`pub(crate)`. Un-ignore `public_surface_is_the_adr_0046_list`. The compiler
names every remaining outside use; fix them with the methods, never by
re-adding a re-export. `cargo test --workspace --locked` must pass
(`secs-service`, `secs-export`, `secs-gpu`, `secs-engine-testkit` compile
unchanged). Claude runs the crates.io publish dry-run after the rebase
(CLAUDE.md's throwaway-copy procedure); you run
`cargo build -p secs-engine --locked` (default features) as the local proxy.
Commit: `refactor(audit): X-5 엔진의 공개 항목을 ADR 0046의 목록으로 줄입니다 (#186 A5)`.

### Step 5 · item-level `#[allow(dead_code)]`

Replace the 14 module-level `#[allow(dead_code)]` with item-level ones and
delete items that are really dead (TD-254's engine list: `mimo::iir::solve_problem`
and the test-only helpers that compile into production; move those behind
`cfg(any(test, feature = "test-support"))` rather than deleting when a test
uses them). `cargo build -p secs-engine` with `RUSTFLAGS=-Dwarnings` and
without the allows must be clean. No test change.
Commit: `refactor(audit): X-5 dead_code 허용을 항목 단위로 바꾸고 죽은 항목을 삭제합니다 (#186 A5 TD-254)`.

ADR 0046 step 6 (`docs/ARCHITECTURE.md`) is Claude's.

## ADR 0050 (X-9): one engine test loader

### Step 1 · the loader lives in `secs_engine::testkit`

Tests first: the ADR names no new test; `crates/secs-engine/tests/replay.rs:8-20`
(every case replays to its recorded design) is the loader's test. Add one
assertion test in `crates/secs-engine/src/testkit.rs` `#[cfg(test)]`:
`names_match_the_oracle_case_set` (compare `names()` with the literal list
of `oracle/goldens/engine_inputs` case names as `secs_oracle` exposes them).

Move: the whole of `src/test_support.rs` into `src/testkit.rs` (which today
holds `MimoAcousticMetrics`, `mimo_acoustic_metrics` and the `RecordedFit`
re-export); delete `test_support.rs`; in `lib.rs` replace the two
`#[cfg(test)] pub mod test_support;` / `#[cfg(feature = "test-support")] pub mod testkit;`
lines with `#[cfg(any(test, feature = "test-support"))] pub mod testkit;`;
change `src/tests/mod.rs:8` to `pub(crate) use crate::testkit as cases;`.
`read_recorded_fit`, `same_recorded_fit`, `replay`, `replay_with_context`
stay `pub(crate)` (the last two may move to `tests/mod.rs` as the ADR
allows). In `Cargo.toml`: `secs-oracle` and `serde_json` become
`optional = true` normal dependencies enabled by
`test-support = ["dep:secs-oracle", "dep:serde_json"]`, and stay as
dev-dependencies. Gate adds `cargo tree -p secs-engine -e normal | grep -c secs-oracle`
(must print 0) and `cargo build -p secs-engine --locked` (default features).
Commit: `refactor(audit): X-9 엔진 시험 로더를 testkit 하나로 합칩니다 (#186 A9 TD-251)`.

### Step 2 · the testkit crate is a facade

`crates/secs-engine-testkit/src/lib.rs` becomes the crate doc plus
`pub use secs_engine::testkit::*;`; its `Cargo.toml` keeps
`secs-engine = { …, features = ["test-support"] }` and drops dependencies
the facade no longer uses (`secs-oracle`, `serde_json` if unused). Gate:
`cargo test -p secs-engine -p secs-engine-testkit -p secs-export -p secs-service --locked`
(the two outside consumers `crates/secs-export/tests/golden.rs:10` and
`crates/secs-service/src/curves_tests.rs:13,48` must pass unchanged).
Also fix the `lib.rs:82-86` doc comment that described two loaders
(`docs/ENGINE.md:45` is Claude's).
Commit: `refactor(audit): X-9 secs-engine-testkit을 재수출 facade로 줄입니다 (#186 A9)`.

## ADR 0047 (X-6): cancel and progress through `Context`

### Step 1 · `secs-compute`: `CancelToken::probe` behind `test-support`

Tests first (`crates/secs-compute/src/lib.rs` `#[cfg(test)]`):
`probe_token_cancels_from_the_nth_check_and_counts_calls` (`probe(3)`:
`is_cancelled()` is false twice, true from the third call, the counter reads
`3` after three calls) and `map_with_a_probe_token_stops_at_the_recorded_point`
(a `Context::map` over 100 items with `probe(10)` returns `Err(Cancelled)`
and the counter is at least 10 and below 100). Add `test-support = []` to
`secs-compute`'s features and `#[cfg(any(test, feature = "test-support"))] pub fn probe(after: usize) -> (CancelToken, Arc<AtomicUsize>)`.
`impl From<Cancelled> for EngineError` and `for FiltersError` are added in
the steps that need them.
Commit: `refactor(audit): X-6 secs-compute에 취소 지점을 세는 probe 토큰을 둡니다 (#186 A6)`.

### Step 2 · the engine's stage functions take `&Context`

Tests first: move `crates/secs-engine/src/tests/cancel.rs::assert_cancels`
to take `impl Fn(&Context) -> Result<T, EngineError>` and build its contexts
with `CancelToken::probe` (enable `secs-compute`'s `test-support` feature
from `secs-engine`'s `test-support` feature and dev-dependencies). The case
list and every `latency` literal stay as they are; the test names stay.

Move: replace the 25 `cancelled: &dyn Fn() -> bool` parameters in
`measurement.rs` (5), `levels.rs` (6), `rc.rs` (5), `fit.rs` (5), `ba.rs`
(2), `xo.rs` (1), `export_view.rs` (1), `replay.rs` (1) with
`ctx: &secs_compute::Context`; every `check(cancelled)?` becomes
`ctx.cancel().check()?` through `impl From<secs_compute::Cancelled> for EngineError`;
delete the `check` helpers at `measurement.rs:190`, `rc.rs:815`, `levels.rs:249`
and `design.rs:192` and the bridge closures at `design.rs:359-361, 757, 847`.
The number of cancellation checks must not change: in the report quote
`git diff --stat` and the count of removed `check(` versus added
`.check()?`. Goldens bit-identical.
Commit: `refactor(audit): X-6 엔진의 단계 함수가 Context로 취소를 받습니다 (#186 A6)`.

### Step 3 · `secs-filters` takes `&Context` and reports through `ctx.stage`

Tests first (`crates/secs-filters/tests/golden.rs`):
`convolution_prepare_reports_progress_until_done_equals_total` (a `Context`
with a recording `ProgressSink`; after `prepare` the last `Progress` for
stage `"filters.convolution"` has `done == total` and `total` equals the
literal path count of the fixture) and move the existing cancel case at
`golden.rs:1174` to `convolution_prepare_cancelled_midway_writes_nothing`
using `CancelToken::probe` (asserts `Err(FiltersError::Cancelled)` and an
empty destination folder).

Move: `secs-filters` gains `secs-compute` as a dependency (workspace
version); the ten `cancelled: &dyn Fn() -> bool` parameters in `render.rs`
(115, 162, 267, 300, 349, 460) and `lib.rs` (1585, 1607, 1795) become
`ctx: &Context`; `progress: &dyn Fn(u64, u64)` at `lib.rs:1796` becomes
`ctx.stage("filters.convolution", total)` with `advance`/`set`;
`impl From<Cancelled> for FiltersError`. Adjust the callers in
`crates/secs-service/src/export.rs` (the closures that bridged
`job.cancel()`), nothing else there. Register the stage id
`filters.convolution` in `features.toml`'s note of the convolution export
entry (it reaches the screen as `JobProgress.stage`).
Commit: `refactor(audit): X-6 작성기가 Context로 취소와 진행을 다룹니다 (#186 A6)`.

ADR 0047 step 4 (`measure::ProgressSink` deletion, `role_progress`) is in
P189.

## Verification (foreground; do not end the turn before they finish)

After the last commit:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build -p secs-engine --locked
cargo tree -p secs-engine -e normal | grep -c secs-oracle     # 0
cargo test -p secs-types --locked && git status --short apps/secs2r/ui/src/ipc/bindings
(cd apps/secs2r/ui && npm run typecheck && npm test)          # list failures from 0042 step 2
python3 .github/scripts/check_feature_tests.py
git log --oneline fix/audit-186..audit/186-engine-adr
```

The bindings folder shows every file as modified after regeneration because
the generator writes LF: commit only the files whose `git diff` has content
changes. `check_feature_tests.py` on Windows reports two names it cannot
list here (`secs-local-api::token_files_are_created_and_tightened_to_owner_only_td109`,
`secs-sys::callback_native_sample_conversion`); anything else is yours.

## Report

Per step: the commit hash, the tests written first and the assertion they
failed on, what moved, what was deleted, the gate tails. Then: the UI tests
that fail after 0042 step 2 (names only), the check-count diff of 0047
step 2, steps blocked or narrowed and why, and the final
`git log --oneline` of the branch.
