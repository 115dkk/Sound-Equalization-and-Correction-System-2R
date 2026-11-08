# P186 (ASTRA): audit #186, Category A, the engine's entry and the REW client

Tree: `E:\wt\audit-engine` (git worktree of the repository, branch `audit/186-engine`).
Set `CARGO_TARGET_DIR=C:\cargo-target\audit-engine` for every cargo command.
Work only in that tree.

Allowed files: `crates/secs-engine/**` (new tests go in
`crates/secs-engine/src/tests/` or `crates/secs-engine/tests/`),
`crates/secs-rew/**`, `crates/secs-service/src/calibration.rs` (only the
error-mapping lines near `EngineError::Input(_) | EngineError::Dsp(_) => failure`
and the settings clamp in `snapshot`), `crates/secs-service/src/settings_edit.rs`
(only to factor the pure clamp out as a function), `crates/secs-service/src/measure/settings.rs`
and `crates/secs-service/src/measurement.rs` (A-42 only), `features.toml`.
Forbidden: `crates/secs-types/**` (if an item needs a new `ErrorCode` or a
new IPC field, stop that item and report; the main session adds it),
`crates/secs-service/**` beyond the files above, the UI, `oracle/**`, the
upstream Python tree.

Do not push. Do not spawn sub-agents. Commit on `audit/186-engine` only.

Read first: `CLAUDE.md`, `CONTEXT.md`, `docs/adr/0004-upstream-parity.md`
(goldens are the contract; the engine's numeric order never changes here),
`docs/adr/0031-input-highpass.md` (why entry checks exist),
`docs/ENGINE.md`, `crates/secs-engine/src/error.rs`,
`crates/secs-engine/src/design.rs` (`check_taps`, `check_input_hp`),
`crates/secs-engine/src/levels.rs` (the three `room_volume_m3` guards),
`crates/secs-engine/src/memory.rs`, `crates/secs-engine/src/mimo/design.rs`
(`validate_request`, the memory sites), `crates/secs-engine/src/mimo/iir.rs`
(`validate_request`, the joint fit fallback near line 700),
`crates/secs-engine/src/notes.rs` (`MimoNote`), `crates/secs-engine/src/ba.rs`
(`base`), `crates/secs-engine/src/measurement.rs` (`prepare_until`),
`crates/secs-service/src/calibration.rs` (`snapshot`, the `EngineError`
mapping and its test `engine_measurement_errors_preserve_context_and_diagnostic_details`),
`crates/secs-service/src/settings_edit.rs` (`calibration_settings_edit`, the
clamp the screen goes through), `crates/secs-rew/src/client.rs` (the
`sampleRate` read near line 725), `crates/secs-rew/src/types.rs`
(`MeasurementPlan::validate`), `crates/secs-rew/src/launch.rs`,
`crates/secs-audio/src/estimator.rs:38-55` (the sweep checks to model),
`crates/secs-service/src/measure/settings.rs` (`plan` from the project).
The audit's full text for each finding (TD-xxx) is in issue #186; the
"Finding" lines below paraphrase it. The audit was written against main
`97a27818`; line numbers may have drifted.

## Why

The engine trusts its callers for everything but `taps` and `input_hp_hz`,
so a negative room volume or an out-of-range setting from the public API
turns into NaN that no check sees, while an out-of-memory refusal and a
DSP primitive's own defect both reach the user as "invalid input". The REW
client accepts a fractional sample rate and sends NaN sweep settings to
REW. Each item is a behaviour change at a public boundary and is done
test-first at the seam below. The engine's numeric results for every
golden room must stay bit-identical: these items add refusals and names,
never arithmetic.

## Rules for every item

- One finding, one commit. Subject `fix(audit): A-n <한국어 설명> (#186 TD-xxx)`,
  Korean body saying what the Red test proves and what changed, last line
  `Co-Authored-By: Claude Mythos 5.1 <noreply@anthropic.com>`.
- Red test expected values come from literals and the spec (limits from
  `secs_types::rules::CALIBRATION_LIMITS`, the finding, the upstream Python
  line you cite), never from running the code under test. Assert the effect:
  the error variant, its message naming the field, the note recorded.
- Gate per item, foreground: `cargo fmt --all -- --check`,
  `cargo clippy -p <touched crates> --all-targets --locked -- -D warnings`,
  `cargo test -p <touched crates> --locked`. Engine goldens (`cargo test -p secs-engine --locked`)
  bit-identical; a changed golden means the item changed numbers and is
  reported as blocked.
- Every fifth commit and at the end: `cargo test --workspace --locked`.
- Register each new test in `features.toml` beside the matching entry
  (`engine.design`, `engine.calibration_settings`, `engine.mimo_design`,
  `engine.mimo_iir`, `measure.rew_api`, `ipc.rew_launch`, `ipc.calibrate`).
- Tidy touches only lines this cycle wrote.

## Items, in order

### A-11 (TD-163): a DSP primitive's defect is an internal error, not the user's input

Finding: `EngineError::Dsp` (a DSP primitive rejected its arguments, an
internal defect) is mapped to `ErrorCode::EngineInput`, so the screen blames
the user's measurement and the app log files it as an input error.

Seam: the `EngineError → IpcError` mapping in `crates/secs-service/src/calibration.rs`.
Red: a case in `engine_measurement_errors_preserve_context_and_diagnostic_details`:
`EngineError::Dsp(<the variant's payload as the enum defines it>)` maps to
`ErrorCode::Internal` with the `detail` carrying the DSP message and a
`reason = "dsp"` param; the `Input(String)` case stays `EngineInput`.
Green: split the `EngineError::Input(_) | EngineError::Dsp(_) => failure`
arm. `ErrorCode::Internal` exists already (used by `host_error`), so no
`secs-types` change.

### A-26 (TD-249): a memory refusal and a solver failure get their own variants

Finding: `MemoryError::TooLarge` (a budget refusal) and a CPU ridge solver
failure both become `EngineError::Input(String)`, although the comment at
`error.rs:3-4` says the service maps variants to user-facing codes; a
memory refusal therefore shows as "invalid input" and the requested and
total bytes survive only inside the message.

Seams: `EngineError` (new variants `Memory { requested: u64, total: u64 }`
and `Solver(String)`), produced at `memory.rs::memory_error`,
`mimo/design.rs` (the `TooLarge` construction near line 1383 and the two
`memory_error` copies near 1503 and 1617), `normalize.rs:80-87`,
`mimo/acceleration.rs:345-352` (`AccelerationError::Failed` → `Solver`);
consumed by the service mapping (next to A-11's arm).
Red (engine): a test that reserves more than the context budget through the
public entry (`secs_engine::design` with a tiny `Context` budget on a golden
room; find how the existing budget tests build a small budget: grep
`budget` under `crates/secs-engine/src/tests/`) and asserts
`Err(EngineError::Memory { requested, total })` with `total` equal to the
budget literal the test set and `requested > total`. Red (service): a case
in the mapping test. Which code? A memory refusal is a resource limit the
user can act on (close other work, lower taps). Look in
`crates/secs-types/src/ipc.rs` for an existing `ErrorCode` for resource
limits (`Memory`, `Resource`, `Busy`); if none fits, keep `EngineInput` for
now with `reason = "memory_budget"`, `requested` and `total` as params, and
say in the commit body that a dedicated code needs a `secs-types` change
the main session owns. `Solver(detail)` maps like `Dsp` (`Internal`,
`reason = "solver"`).
Green: the variants, the five production sites, the mapping. The old
`Input(other.to_string())` text must not survive at any of the five sites.

### A-25 (TD-248): the engine refuses settings outside the limits; the public API gets the same clamp as the screen

Finding: the engine's entry validates only `taps` and `input_hp_hz`. A
negative `room_volume_m3` passes the guards at `levels.rs:122` (`v == 0.0`)
and `:159` (`!= 0.0`) (only `lenient_until:214` tests `> 0`), so
`sqrt(T60/volume)` at `:189` is NaN and `schroeder_limit:330` returns
`NaN.clamp(..) = NaN`; `setup.rc_hi` and every mask and band after it become
NaN without an error. `rc_range`, `max_boost_db`, `lift_ms`, `peq_bands`,
`sub_manual.hp_hz/lp_hz` and `rc_hi_transition` are likewise assumed in
range. `CalibrateParams.settings` bypasses the `settings_edit` clamp and
`Service::snapshot` only fixes `rc_bands`, so public-API and local-API users
can pass these values straight through (same shape as TD-148).

Seams: (1) `secs_engine::design` entry: `check_config(&EngineConfig)` beside
`check_taps`/`check_input_hp`; (2) `Service::snapshot(override_settings)` in
`crates/secs-service/src/calibration.rs`.
Red 1 (engine): tests in `crates/secs-engine/src/tests/` that call the
public design entry on a golden room with one field out of range at a time
and assert `Err(EngineError::Input(message))` where `message` names the
field: `room_volume_m3 = -10.0` ("room_volume_m3"), `max_boost_db = NaN`,
`lift_ms` outside `CALIBRATION_LIMITS.ultra_ms`, `rc_range` reversed,
`peq_bands` above its limit, `sub_manual` with `hp_hz > lp_hz`,
`rc_hi_transition` out of range even when the basis is not automatic. Take
every bound from `CALIBRATION_LIMITS` (read `crates/secs-types/src/rules.rs`
and `settings_edit.rs` for which field has which limit) and put the literal
you used into the assertion message. A room volume of exactly `0.0` and
`None` keep their meaning (no Schroeder limit) and must still pass.
Red 2 (service): a test in `calibration.rs`'s test module (or
`crates/secs-service/tests/` if the existing calibrate tests live there):
`CalibrateParams { settings: Some(settings with max_boost_db = 1e6) }` →
the snapshot's `settings.max_boost_db` equals the clamp's upper limit, the
same value `calibration_settings_edit` would store. Compare against the
literal limit, not against a second call of the clamp.
Green 1: `check_config` with one predicate per field (finite and inside its
limit), the three volume guards in `levels.rs` folded into one
`fn volume(config) -> Option<f64>` that returns `Some` only for a finite
`v > 0.0` (this must not change any golden: all golden rooms have positive
or absent volumes; if a golden changes, stop and report).
Green 2: `snapshot` runs `override_settings` through the same clamp
function `calibration_settings_edit` uses (factor the pure clamp out of
`settings_edit.rs` if it is not already a function).

### A-49 (TD-258): an unmeasured sub is a structured error; the joint-fit fallback is recorded

Finding: `prepare_until` only needs one main at the reference position, so a
public `design()` request whose `config.subs` lists a sub not measured there
panics at the `unwrap` in `ba::base()` (`engine_inputs` filters only the
calls that go through the service). When both normal-equation solves fail,
the joint fit returns `status: -1` with the seed as `x`, the caller drops
the status, and the IIR support is produced from an unconverged seed with
no note (upstream does the same, `src/secs/mimo_iir.py:326-329`, uncited in
Rust). `validate_request` folds seven and nine field checks into one
"invalid MIMO request" / "invalid MIMO IIR tuning".

Seams: `prepare_until` (public design entry), the MIMO IIR result (`MimoNote`
on the run), `validate_request` in `mimo/design.rs` and `mimo/iir.rs`.
Red 1: a golden room whose config lists a sub role with no measurement at
the reference position → `Err(EngineError::MissingSubMeasurement { position, role })`
(add the variant next to `MissingMainMeasurement`; the service mapping
gets a `reason = "missing_sub_measurement"` case in the same test as A-11,
in this commit). Today the test panics inside `base()`; the Red must show
that panic (run it once, quote the panic line in the commit body).
Red 2: a MIMO IIR fit forced to fail both solves (a tuning with
`mimo_max_nfev = 1` and a degenerate request, or whatever the existing IIR
tests use to hit the fallback; find it with grep `status: -1`) → the result
carries a `MimoNote::IirSeedFallback { support, .. }` (new variant; name it
after what it says) and the run's notes list it. Cite
`src/secs/mimo_iir.py:326-329` in the Rust comment at the fallback.
Red 3: `validate_request` with `fsiso = 300.0` → the message contains
`fsiso`; with `mimo_early_ms = 61.0` → contains `mimo_early_ms`; IIR tuning
with `mimo_q_max = 0.5` → contains `mimo_q_max`.
Green: the variant and the check in `prepare_until` (every configured main
and sub has a reference-position measurement); the note at the fallback;
one `Err` per field in both `validate_request`s (a small helper that takes
the field name). No numeric change.

### A-24 (TD-193): the REW client validates what REW and the project give it

Finding: the `sampleRate` REW returns is only checked for `(0, u32::MAX]`,
so a fraction below 1 becomes 0 through `as u32` and flows into
`MeasurementResult.sample_rate` and the WAV writer; `MeasurementPlan::validate`
does not check `start_frequency`, `end_frequency` and `sweep_level_db` for
finiteness and order, so a NaN from the project file becomes
`startFrequency: 0` through `round_ties_even() as i32` and is sent to REW.
Native measurement rejects the same values in `SweepEstimator::new`.

Seams: `RewClient`'s impulse-response read (`client.rs` near line 725) and
`MeasurementPlan::validate` in `types.rs`.
Red 1: a recorded or fake `/measurements/{id}/impulse-response` answer with
`"sampleRate": 0.5` → `Err(RewError::schema(.., "invalid sample rate"))`
(find how the existing client tests feed a fake transport; `FakeRew` in
`fake.rs` or a recorded transport). `48000.0` still parses to `48000`.
Red 2: `MeasurementPlan { start_frequency: f64::NAN, .. }.validate()` contains
`"rew.plan.frequency"`; `end_frequency <= start_frequency` → the same code;
`sweep_level_db = 1.0` (above 0 dBFS) or NaN → `"rew.plan.level"`. Model the
bounds on `estimator.rs:38-55` (`low > 0`, `high > low`, `high < fs/2`,
level in `-120..=0`).
Green: `rate >= 1.0 && rate.fract() == 0.0` else `RewError::schema`; the two
new checks in `validate`. The service's `measure/settings.rs::plan` reads
these from the project; touch it only if the validation must also be called
there (say so).

### A-42 (TD-221): an optional REW executable path is tried first

Finding: REW is looked for only at four fixed Windows paths or
`/Applications/REW/REW.app`; PATH, the registry and a user-chosen path are
not consulted and Linux is not searched at all, so a user with REW elsewhere
gets `exe_not_found` from `rew_launch`.

Seam: `secs_rew::launch::find_rew_exe_with(configured: Option<&Path>)` (new,
`find_rew_exe` delegates with `None`) and the service's `rew_launch`/`rew_status`
reading the configured path from where the other measurement settings live
(`measure/settings.rs::plan` reads `devices.*` from the project; add
`devices.rew_path` there, read-only in this packet: the screen and the IPC
to set it are not yours; a project file that carries it is enough for the
test).
Red 1 (secs-rew): `find_rew_exe_for_with(Platform::Other, Some(path), exists)`
returns the configured path when `exists(path)` and `None` otherwise; on
`Platform::Windows` the configured path wins over `WINDOWS_PATHS` when both
exist.
Red 2 (service): a project with `devices.rew_path = <temp file>` and the mock
host → `rew_info`'s `executable_path` equals that file; without it, today's
behaviour.
Green: the two functions, the setting read, the two call sites in
`measurement.rs` (around lines 731 and 808). The launch itself goes through
`deps.host.launch_rew()`, whose implementation lives in the app (forbidden
here): if the host needs the path, stop at the status/info part and report
what the app must pass (the `Host` trait in `crates/secs-service/src/lib.rs`
is forbidden to you).

## Report (in your final message, nothing else)

For each item, in order: the commit hash on `audit/186-engine`, the Red test
names and what they assert (with the panic line quoted for A-49's Red 1),
the Green summary in one line, the gate output summary (fmt, clippy, crate
test counts, the workspace run when it ran), and the engine golden status
(unchanged, or which changed and why you stopped). Then anything blocked or
reclassified with why, anything that needs a `secs-types` or `Host` change by
the main session, and `git log --oneline main..audit/186-engine`.
