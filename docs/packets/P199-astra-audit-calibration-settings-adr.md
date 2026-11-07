# P199 (ASTRA): ADR 0055, calibration settings from one declaration, and the TD-248 entry checks

Tree: `E:\wt\audit-api`, a git worktree on branch `audit/186-settings-adr`
(Claude created it from the current `fix/audit-186`; `git status` shows only
an untracked `bindings/python/secs2r/_secs2r.pyd`, leave it). Every cargo
command runs with `CARGO_TARGET_DIR=E:\cargo-target\audit-engine` in the
environment: export it once per shell and check it with
`echo $CARGO_TARGET_DIR` before the first build (a build into the
worktree's own `target/` fills the drive and stops the whole session). That
target is shared with Claude's verification runs ("Blocking waiting for
file lock" means wait). Never run `cargo clean`, never delete anything under
`E:\cargo-target`. No workspace-wide `cargo test`; per-package (`-p`) only,
plus one `cargo clippy --workspace` at the end. Commit on that branch, one
commit per step below, in order. Do not push, do not rebase, do not spawn
sub-agents, do not touch `audit-state.md`. Another worker (P193, service and
project Category B) runs at the same time in `E:\wt\audit-svc`: stay inside
the allowed files so the branches merge.

## Since this packet was written (2026-11-08)

P202 (ADR 0052 step 1) already added to `crates/secs-types/src/rules.rs` a
hand-written `pub const SCALAR_EDITS: &[ScalarEditRule]` (`ScalarEditRule
{ kind, field, limit, policy }`, all `&'static str`), `SUB_LIMITS` with
`filter_hz: (10.0, 300.0)`, `JOB_GROUPS` and the other tables, exported to
`bindings/rules.ts`; `crates/secs-service/src/settings_edit.rs` and
`jobs.rs` read them, and the screen's mock (packet P204, running now) looks
`SCALAR_EDITS` up by `kind`. Consequences for this packet:

- Step 1's macro generates `SCALAR_EDITS` and replaces the hand table. Keep
  the Rust type name `ScalarEditRule`, its four fields and the exported
  `rules.ts` shape; the rows may follow declaration order, but every `kind`
  the hand table has must still be there with the same `field`, `limit` and
  `policy` unless the current `validate_edit`/`apply` code proves the hand
  table wrong (then say which row and why).
- Decision 3's `sub_filter_hz` is `SUB_LIMITS.filter_hz` (one source, as the
  decision already allows): do not add a second pair to `CalibrationLimits`.
  Only `peq_bands: (0.0, 64.0)` is added there. Step 1's
  `limits_table_has_the_two_new_entries` asserts
  `CALIBRATION_LIMITS.peq_bands == (0.0, 64.0)` and
  `SUB_LIMITS.filter_hz == (10.0, 300.0)`.
- P189, P203 and P202 moved code in `secs-service` (settings store,
  `commit(.., Finality)`, `ProjectSnapshot`), `secs-engine` (narrowed
  public surface, ADR 0046) and the CLI (it now uses the public API and
  `secs_sys::native_deps`). Line numbers below are stale; search by names.
- `npm run typecheck` in `apps/secs2r/ui` must stay green: the bindings you
  regenerate only add fields.

Allowed files:

- `crates/secs-types/**` (the macro, the limits, `SettingEdit`; run
  `cargo test -p secs-types --locked` after every change there so
  `apps/secs2r/ui/src/ipc/bindings/*.ts` and `bindings/rules.ts` are
  regenerated, and commit the regenerated files whose `git diff` has content
  changes, not the ones that differ only in line endings);
- `crates/secs-service/src/settings_edit.rs` and `crates/secs-service/src/calibration.rs`
  (only the scalar arms of `validate_edit`/`apply`, and `snapshot`);
- `crates/secs-engine/src/design.rs`, `crates/secs-engine/src/levels.rs`,
  `crates/secs-engine/src/tests/**`;
- `apps/secs2r-cli/src/main.rs`, `apps/secs2r-cli/tests/cli.rs`;
- `features.toml`: register every new test name in the `[[feature]]` entry
  that covers the behaviour (`ipc.calibration_settings_edit`, `ipc.calibrate`,
  `engine.input_hp` for the engine entry check, or a new
  `calibration.settings_declaration` entry in the same style);
  `python3 .github/scripts/check_feature_tests.py` must accept it.

- `crates/secs-project/src/calibration.rs` and its tests (step 5 only).

Forbidden: the rest of `crates/secs-project/**` (P193 changes `record.rs`,
`irs.rs`, `pyjson.rs` and `lib.rs` at the same time), the screens
(`apps/secs2r/ui/**` beyond the regenerated bindings), `crates/secs-engine`
outside the three paths above, `CHANGELOG.md`, `docs/**`.

Read first: `CLAUDE.md`, `CONTEXT.md`, `docs/adr/0055-one-place-for-calibration-settings.md`
(the specification; this packet narrows it where the files above require),
`docs/adr/0031-input-highpass.md` (or the ADR that added `check_input_hp`;
search `docs/adr` for "input_hp"), `docs/adr/0024` (RC bands and `RC_LO_READ`),
`crates/secs-types/src/calibration.rs` (`CalibrationSettings`, `Default`,
`engine_key`/`manual_key`/`mimo_key`, `mode_params`, the key tests at
734-930), `crates/secs-types/src/settings_edit.rs` (`SettingEdit`),
`crates/secs-types/src/rules.rs` (`CalibrationLimits`, `CALIBRATION_LIMITS`,
`RC_LO_READ`, the `typescript` module that writes `rules.ts`),
`crates/secs-service/src/settings_edit.rs` (`validate_edit`, `apply`),
`crates/secs-service/src/calibration.rs:270-289` (`snapshot`) and its test
module from 1288, `crates/secs-engine/src/design.rs:191-220`
(`check_taps`, `check_input_hp`; their callers at 346 and 542),
`crates/secs-engine/src/input.rs:156-220` (`EngineConfig`),
`crates/secs-engine/src/setup.rs:138` (`peq_bands` default 16),
`crates/secs-engine/src/levels.rs:122, 159, 189, 214` (the three volume
guards), `crates/secs-engine/src/tests/design.rs:411-480` (the TD-130 budget
test: how a `Context` with a tiny `MemoryBudget` is built) and `:608-640`
(the ADR 0031 entry test to model), `apps/secs2r-cli/src/main.rs:195-260,
672-760` (`ProjectAction::Set`), `apps/secs2r-cli/tests/cli.rs:83-290`
(the `project set` tests).

## Why

ADR 0055 (architecture review A14 of audit #186) puts the scalar and boolean
calibration settings into one declaration so that a field, its default, its
JSON key, its `SettingEdit` variant, its limit and its policy (clamp or
refuse) are written once, and so that every entrance (screen edit, public
API `CalibrateParams.settings`, local API, CLI) goes through the same check.
TD-248 is the behaviour half of it: today `snapshot`
(`crates/secs-service/src/calibration.rs:270-289`) fixes only `rc_bands`, so
a public-API caller can hand the engine `boost_max_db = 1e6` or
`room_volume_m3 = -1`, and the engine checks only `taps` and `input_hp_hz`,
so a negative volume turns into NaN inside `levels.rs` and poisons every mask
(TD-248). The P186 worker reproduced both halves as Red tests but could not
make them Green because the limits table lacked two entries that live in
`secs-types`, which that packet forbade. This packet owns `secs-types`.

## Decisions this packet fixes (do not reopen them)

1. **`ultra_ms` is a latency budget, not a lift bound.** `mode_params`
   (`secs-types/src/calibration.rs:566-600`) derives the ultra lift as
   `ultra_ms - 2` (28..198 ms) and the other modes use fixed lifts 0, 8 and
   28 ms. The engine's `check_config` therefore accepts
   `0.0 <= lift_ms <= CALIBRATION_LIMITS.ultra_ms.1` (200). A lift of 201 ms
   is refused; 0, 8, 28 and 198 stay valid. `ultra_ms` itself is checked by
   `checked` with the `ultra_ms` limit (30..200, clamp policy as `apply` does).
2. **RC low edge: reads keep `RC_LO_READ`, requests use the edit limit.** A
   project file stored with 10..19 Hz keeps computing (ADR 0024, D48), so the
   engine's `check_config` accepts `RC_LO_READ.0 <= rc_range.0 <= RC_LO_READ.1`
   (10..200). `checked` on a request override (public API, local API, CLI) uses
   `CALIBRATION_LIMITS.rc_lo` (20..200) with refuse policy, exactly like a
   screen edit, because an override is a request, not a stored file.
3. **Two limits are added to `CalibrationLimits`** (and therefore to
   `bindings/rules.ts`):
   - `peq_bands: (0.0, 64.0)`: the engine's input PEQ band count
     (`EngineConfig.peq_bands`, default 16 at `setup.rs:138`, 32 in zero mode
     at `mode_params`). No screen control sets it; 64 is the largest IIR band
     count any DSP profile offers (`crates/secs-project/src/dsp_profile.rs`
     presets: CamillaDSP 64), so nothing a profile can run is refused and
     `usize::MAX` is.
   - `sub_filter_hz: (10.0, 300.0)`: the manual sub high-pass and low-pass
     corners. The value is the screen's `SUB_LIMITS.filterHz = [10, 300]`
     (`apps/secs2r/ui/src/screens/calibrate/settingsView.ts:28`, the hand copy
     TD-174 names); a corner of 0 means "off" and is not range-checked.
   If P190 (ADR 0052) has already moved `SUB_LIMITS` into `rules.rs` when you
   start, reuse its `filterHz` pair for `sub_filter_hz` (one source) and say so
   in the report; do not keep two copies.
4. **Policy per field** follows the service as it is today: a field that
   `validate_edit` refuses keeps refuse policy (`rc_lo`, `rc_hi`, `input_hp_hz`,
   `mimo_early_ms`, `rc_hi_transition` as the 1|2|4 rule); a field that
   `apply` clamps keeps clamp policy (`ultra_ms`, `boost_max_db`,
   `infra_boost_max_db`, `xo_peq_lo`, `xo_peq_hi`, `sub_main_delay_ms`,
   `room_volume_m3` as "at or below the low end clears it, above the high end
   clips"). Read the remaining scalar arms of `apply` (`fsiso`, `mimo_low`,
   `mimo_max_supports`) and give each the policy the arm implements today;
   list the resulting table in the report. Booleans and enums have no limit
   (`-`). `checked` applies the same policies to a whole settings value.
5. **Structured settings stay hand-written** (`subs_manual`, `bundle_layouts`,
   `xo`, `rc_bands`, `room_dims_m`), as the ADR says. `checked` still covers
   them with the existing hand rules: `rc_bands` through the existing count
   and range checks, each manual sub's nonzero `hp_hz`/`lp_hz` inside
   `sub_filter_hz` and `hp_hz < lp_hz` when both are nonzero (refuse, field
   `subs_manual.<role>.hp_hz`), `xo` per group inside `xo` (clamp, rounded as
   the edit does).
6. **Engine `check_config` names the field in the message**, in the shape the
   ADR 0031 test pins (`message.starts_with("<field> ")`): `"room_volume_m3 -1
   m³ must be greater than 0"`, `"max_boost_db NaN outside 0..=10 dB"`,
   `"lift_ms 201 ms outside 0..=200 ms"`, `"rc_range 500..=100 Hz must be
   ascending"`, `"peq_bands 18446744073709551615 outside 0..=64"`,
   `"sub_manual.SW1.hp_hz 100 Hz must be below lp_hz 50 Hz"`,
   `"rc_hi_transition 3 must be 1, 2 or 4"`. It runs where `check_taps` and
   `check_input_hp` run (`design.rs:346` and `:542`), before any allocation.
   The three volume guards (`levels.rs:122, 159, 214`) become one
   `pub(crate) fn room_volume(config: &EngineConfig) -> Option<f64>` that
   returns the volume only when it is finite and greater than 0 (0 stays "no
   volume", as today); the goldens must not change, so the predicate keeps
   returning `None` for `Some(0.0)`.

## The protocol for every step

Identical to P183: announce the step; Red (the named test fails on its
assertion, not on compilation; stub first if needed); Green (the smallest
change); Tidy (only this step's lines); gate; commit with
`git add <files>` and this message shape:

```
<prefix>(audit): <id> <한국어 한 문장> (#186 <audit id>)

<Red 시험이 무엇을 증명하는지 한두 문장. 바뀐 동작이나 구조 한 문장. ADR 0055 n단계.>

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
```

(하십시오체, no em-dash, no program-as-subject verbs 묻다/적다/넘기다, "추가하다"
not "더하다"). Gate per step: `cargo fmt --all -- --check`,
`cargo clippy -p <touched crates> --all-targets --locked -- -D warnings`,
`cargo test -p <touched crates> --locked`; after step 3 and at the end
the per-package list of the Verification section in the foreground (the
engine goldens take about ten minutes; wait for them). The `engine_key`, `manual_key` and
`mimo_key` bytes must not change: the key tests in
`secs-types/src/calibration.rs:734-930` and the service cache tests pin them.

## Steps

### Step 1 · X-14 · the `calibration_settings!` declaration (ADR 0055 step 1, structure)

Red tests first, in `crates/secs-types/src/calibration.rs` tests:

- `scalar_settings_table_round_trips_defaults_and_keeps_engine_keys`: iterates
  `rules::SCALAR_EDITS` and asserts, for every row, that the `SettingEdit` kind
  names a variant, that the field's default survives a JSON round trip
  (`serde_json` of `CalibrationSettings::default()`), that the limit key (when
  not `-`) is a field of `CalibrationLimits`, and that
  `CalibrationSettings::default().engine_key()` equals the literal string the
  existing key tests use today (copy the literal from
  `rc_upper_defaults_and_each_setting_change_engine_key`; never compute it).
- `apply_scalar_clamps_and_refuses_by_policy`: `BoostMax { db: 1e9 }` yields
  `Applied` with `boost_max_db == 10.0`; `RcLo { hz: 5.0 }` yields
  `Err(SettingsRefusal { field: "rc_lo", reason: "rc_lo_range", min: 20.0,
  max: 200.0 })`; `InputHp { hz: Some(f64::NAN) }` is refused with field
  `input_hp`; `DipLimit { on: true }` sets the flag (no limit).
- `checked_applies_every_policy_to_a_whole_value`: a settings value with
  `boost_max_db = 1e9`, `ultra_ms = 1.0`, `rc_lo = 5.0` → `Err` naming
  `rc_lo` (the first refusal wins; say in the doc comment that refusals are
  reported in declaration order); the same value with `rc_lo = 20.0` →
  `Ok` with `boost_max_db == 10.0` and `ultra_ms == 30.0`.
- `limits_table_has_the_two_new_entries`: `CALIBRATION_LIMITS.peq_bands ==
  (0.0, 64.0)` and `CALIBRATION_LIMITS.sub_filter_hz == (10.0, 300.0)`
  (literals), and the regenerated `rules.ts` text contains both keys (the
  `typescript` module's own test already compares the file; extend its
  expectation).

Green: the declarative macro exactly as ADR 0055's listing (field, type,
default, JSON key, edit variant with its payload field name, limit key,
policy, engine_key participation `flag` | `number` | `"prefix="` | `-`),
generating:

- the `CalibrationSettings` fields and `Default` for the scalars and booleans
  (the structured fields stay hand-written beside the macro call, in the same
  struct: write the macro so it takes the hand-written fields as a trailing
  block, or generate a `ScalarSettings` part that `CalibrationSettings`
  embeds with `#[serde(flatten)]`; choose the one that keeps the JSON byte
  layout and the ts-rs output identical, and say which in the report);
- the engine_key contributions in today's order and bytes (the key is
  assembled by hand today; the macro only has to produce the per-field
  snippets `|dip_limit` (flag), `number(..)` (number), `|input_hp=..`
  (prefix) that the hand-written `engine_key` concatenates in its present
  order; do not reorder the key);
- the scalar `SettingEdit` variants (the enum keeps its hand-written
  structured variants);
- `apply_scalar`, `checked`, `SettingsRefusal { field, reason, min, max }`
  (`From<SettingsRefusal> for IpcError` lives in the service: code
  `InvalidRequest`, params `field`, `reason`, `min`, `max`, the same shape
  `validate_edit` produces today so the screen's rejection texts keep working);
- `rules::SCALAR_EDITS: &[ScalarEdit { kind, field, limit, policy }]` and
  `KNOWN_KEYS` (project-file key order; exported to `rules.ts` as
  `SCALAR_EDITS` and `KNOWN_KEYS`), and the two new `CalibrationLimits`
  fields.

Then `crates/secs-service/src/settings_edit.rs`: the scalar arms of
`validate_edit` (`RcLo`, `RcHi`, `InputHp`, `MimoEarly`) and of `apply`
(`UltraMs`, `BoostMax`, `InfraBoost`, `XoPeq` lo/hi, `RoomVolume`'s clamp,
`MainDelay`, the MIMO scalars) call `apply_scalar`; the cross-field rules
(`validate_against_settings`, `RoomVolume` clearing `room_dims_m`, `Xo`
turning `xo_rec_include` off, the sub and bundle arms) stay. The existing
service tests (`settings_edits_and_sub_warnings_replay_upstream_oracle`) must
pass unchanged: that is the proof the behaviour is the same.

Deletions: the duplicated range checks in `validate_edit` and the clamp calls
in `apply` that the macro now owns.

Gate: `cargo test -p secs-types -p secs-service --locked`, bindings
regenerated and committed (`CalibrationLimits.ts`, `rules.ts`, `SettingEdit.ts`
if its order changed: it must not).

Commit subject: `refactor(audit): X-14 보정 설정 선언 하나가 필드와 편집 variant와 한도를 만듭니다 (#186 A14)`.

### Step 2 · A-25 · `snapshot` checks the override like an edit (TD-248, service half)

Red tests in `crates/secs-service/src/calibration.rs` tests:

- `calibration_snapshot_clamps_override_boost_like_settings_edit`: a real
  project (`Project::default()` with `store_calibration` of a default
  settings value, opened in a `Service` as the tests at 1465-1530 do), a
  `CalibrateParams.settings` override with `boost_max_db = 1e6`; `snapshot`
  returns settings whose `boost_max_db == 10.0` (literal). The P186 worker
  saw this fail with `left 1000000.0, right 10.0`.
- `calibrate_params_settings_are_checked_like_edits` (the ADR's name): the
  override with `rc_lo = 5.0` makes `snapshot` answer an `IpcError` with
  code `InvalidRequest`, `params.field == "rc_lo"`, `params.reason ==
  "rc_lo_range"`; the override with `room_volume_m3 = -1.0` answers
  `InvalidRequest` with `params.field == "room_volume_m3"` and no engine call
  is made (assert through the error alone: the engine is never reached
  because `snapshot` runs before the job starts).

Green: `snapshot` calls `settings.checked()?` on the override (after the
`rc_bands` fix-up that stays), mapping `SettingsRefusal` to `IpcError` with
the `From` of step 1. The project's own settings (no override) are not
re-checked: a stored 10..19 Hz `rc_lo` must still compute (decision 2).

Gate: `cargo test -p secs-service --locked`.

Commit subject: `fix(audit): A-25 공개 API의 보정 설정이 화면 편집과 같은 검사를 거칩니다 (#186 TD-248)`.

### Step 3 · A-25 · engine `check_config` and one volume predicate (TD-248, engine half)

Red test in `crates/secs-engine/src/tests/design.rs`, modelled on
`design_rejects_an_input_cut_outside_its_limits` (`:608-640`) and the budget
test (`:411-480`):

- `design_rejects_invalid_settings_before_allocation`: load `room_manual`
  (it has subs with corners); for each of the seven cases set the field on
  `c.config` and call `design::design` with a `Context` whose
  `MemoryBudget::new(1024)` cannot hold a design (so a check that did not
  fire shows up as a memory error, never as a long run):
  1. `room_volume_m3 = Some(-1.0)` → `Input` starting with `"room_volume_m3 "`;
  2. `max_boost_db = f64::NAN` → `"max_boost_db "`;
  3. `lift_ms = 201.0` → `"lift_ms "`;
  4. `rc_range = (500.0, Some(100.0))` → `"rc_range "`;
  5. `peq_bands = Some(usize::MAX)` → `"peq_bands "`;
  6. the first manual sub with `hp_hz = 100.0`, `lp_hz = 50.0` →
     `"sub_manual.<role>.hp_hz "`;
  7. `rc_hi_transition = 3` with `rc_range.1 = Some(300.0)` →
     `"rc_hi_transition "`.
  Each assertion is
  `matches!(&result, Err(EngineError::Input(m)) if m.starts_with(<literal>))`
  and the message is printed on failure. The P186 worker saw all seven reach
  memory reservation. Then the valid cases `lift_ms` in `[0.0, 8.0, 28.0,
  198.0]`, `rc_range.0 = 10.0` (decision 2), `peq_bands` in `[None, Some(0),
  Some(16), Some(64)]`, `room_volume_m3` in `[None, Some(0.0), Some(120.0)]`
  pass `design::check_config` (the function is `pub(crate)` like
  `check_input_hp`, so the test calls it directly for the positive cases).
- `room_volume_predicate_ignores_zero_and_nonfinite` in the same file:
  `design::room_volume` (or wherever you put it; `levels.rs` is allowed)
  answers `None` for `None`, `Some(0.0)`, `Some(f64::NAN)`, and `Some(120.0)`
  for `Some(120.0)`.

Green: `check_config` beside `check_taps`/`check_input_hp`, called at both
entry points; the predicate replaces the three guards in `levels.rs` (the
Schroeder limit and the lenient basis keep their arithmetic: only the guard
expression changes). Run `cargo test -p secs-engine --locked` in full: the
golden rooms (`dolby`, `room`, `room_dba`, `room_manual`) must stay
bit-identical, which the recorded-stage tests prove; if any golden differs,
stop and report (that would mean a guard changed a result, which this step
must not do).

Gate: `cargo test -p secs-engine -p secs-service --locked`, then the
per-package list of the Verification section.

Commit subject: `fix(audit): A-25 엔진 입구가 범위 밖 설정을 필드 이름과 함께 거부합니다 (#186 TD-248)`.

### Step 4 · X-14 · CLI `project set` as a table (ADR 0055 step 3)

Red test in `apps/secs2r-cli/tests/cli.rs` next to the `project set` tests:

- `every_scalar_flag_makes_its_edit`: a table-driven test that runs
  `project set <path> --<flag> <value>` for every row of the CLI's
  `SCALAR_FLAGS` (expose the table as `pub` or print it through a hidden
  `project set --list-flags` subcommand output; choose the one that keeps
  the clap help text unchanged and say which) and asserts, after
  `calibration_settings_get`, the literal value each flag sets: for example
  `--mimo-early 20` → `mimo_early_ms == 20.0`, `--dip-limit on` →
  `dip_limit == true`, `--input-hp 25` → `input_hp_hz == Some(25.0)`,
  `--input-hp off` → `None`. The existing `project_set_persists_*` tests stay
  and must pass unchanged (they are the behaviour contract).

Green: `SCALAR_FLAGS: [(flag, SettingEdit kind)]` plus
`SettingEdit::scalar(kind, value)` (a constructor the step-1 macro can
generate, taking a `ScalarValue::{Bool, Number, OptionalNumber}`), replacing
the eleven `if let Some(..) { edits.push(..) }` blocks. The clap struct, its
doc comments and the `rc_bands` / `analytic derivatives` structured arms stay.

Gate: `cargo test -p secs2r-cli --locked`, `python3 .github/scripts/check_feature_tests.py`.

Commit subject: `refactor(audit): X-14 CLI의 project set이 표 한 줄로 편집을 만듭니다 (#186 A14)`.

### Step 5 · X-14 · the project file's scalar round trip through `KNOWN_KEYS` (ADR 0055 step 2)

This is a structural step inside the ADR: the upstream JSON bytes of a
`.secs` file must not change, and the existing round-trip goldens of
`secs-project` are the gate (no new behaviour, so no Red test; if a golden
or an existing test has to change, undo the step and report it).

- Before: `crates/secs-project/src/calibration.rs` `calibration_from_project`
  reads every scalar and boolean by hand (`num(.., low, high)`,
  `as_float`, the flag reads) and `calibration_json` writes them back in a
  fixed key order.
- After: the scalar and boolean reads and writes iterate `KNOWN_KEYS` (the
  project-file key order step 1 generated) with the accessors the step-1
  macro generates (one getter and one setter per scalar field over
  `CalibrationSettings`, plus the JSON key and the read clamp the hand code
  uses today); the structured fields (`subs_manual`, `bundle_layouts`, `xo`,
  `rc_bands`, `room_dims_m`) keep their hand-written functions. The read
  clamps stay the ones the hand code applies on load (for example
  `RC_LO_READ` for the RC low edge), not the edit limits.
- Deleted: the per-field scalar read and write lines the loop replaces.

Gate: `cargo test -p secs-project -p secs-types -p secs-service --locked`
(the oracle round-trip goldens in `crates/secs-project/tests` must pass
byte-identical).

Commit subject: `refactor(audit): X-14 KNOWN_KEYS 한 표로 프로젝트 파일의 스칼라 설정을 읽고 저장합니다 (#186 A14)`.

## Hand-off to the UI packet (not yours)

After step 1 the screen can read, from the regenerated bindings:

- `bindings/CalibrationLimits.ts` with the new key `peq_bands` (the sub
  filter corners are `SUB_LIMITS.filter_hz`, already generated by P202);
- `bindings/rules.ts` `SCALAR_EDITS` in the same shape as today, and
  `KNOWN_KEYS`;
- `bindings/SettingEdit.ts` unchanged in shape.

The screen step of ADR 0055 (`screens/calibrate/settingRows.ts` with about
40 rows `{ id, field, kind, control, limits, unit, step, digits, shownWhen,
disabledWhen, hint }` and one `SettingRow` component; the range-bands editor,
the sub editor and the XO card stay hand-written; a test iterating the rows
checks the 54 controls' ids, limits and edit kinds, and the `REACH_IDS` of
`CalibrateScreen.test.tsx` for those rows come from the table) is a later
Opus packet. The mock service looks up `SCALAR_EDITS` to clamp or refuse
(ADR 0052). List in your report every `SettingEdit` kind and the limit key
it carries so Claude can paste the table into that packet.

## Left out of this packet

- The `CHANGELOG.md` line for the public-API behaviour change (out-of-range
  settings are now clamped or refused): Claude writes it.

## Verification (foreground; do not end the turn before they finish)

```
echo $CARGO_TARGET_DIR                       # must print E:\cargo-target\audit-engine
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-types -p secs-service -p secs-engine -p secs-project -p secs2r-cli -p secs-local-api --locked --no-fail-fast
cargo test -p sound-equalization-and-correction-system-rust-api --all-features --locked
cargo test -p secs-types --locked            # bindings and rules.ts regenerated and committed?
git -c core.safecrlf=false diff --ignore-cr-at-eol --stat -- apps/secs2r/ui/src/ipc/bindings  # commit content changes only
(cd apps/secs2r/ui && npm run typecheck)     # must be green
python3 .github/scripts/check_feature_tests.py
git log --oneline fix/audit-186..audit/186-settings-adr
```

`check_feature_tests.py` on Windows reports two known names it cannot list
here (`secs-local-api::token_files_are_created_and_tightened_to_owner_only_td109`,
`secs-sys::callback_native_sample_conversion`); anything else is yours.

## Report

Per step: the commit hash, the Red test names and the assertion each failed
on, the Green summary, the gate outputs (tail). Then: the policy table you
derived for every scalar field (kind, field, limit key, policy), the choice
made for the macro's struct shape and for the CLI table exposure, the
`npm run typecheck` error list for P192, anything you narrowed, and the
final `git log --oneline` of the branch.
