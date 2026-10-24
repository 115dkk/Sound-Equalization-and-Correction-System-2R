# P71 (Daybreak Blue): audit #92 — result staleness and XO recommendation (S1a)

Repository: `/home/user/Sound-Equalization-and-Correction-System-2R`. Work in
the main tree on a local branch: `git switch -c wip/s1a` from the current
`HEAD` before the first change. Do not push, do not switch branches again,
do not spawn sub-agents. Commit once per finding (subjects below); Claude
reviews every commit, re-runs the gates and re-authors the commits.

Read first: `CLAUDE.md`, `CONTEXT.md` (낡음, 잡), `docs/adr/0009-compact-result-ipc-and-job-retention.md`,
`docs/adr/0012-resource-optimizations.md`, `docs/DEVIATIONS.md`,
`TECH_DEBT_AUDIT.md` (rows TD-01, TD-02, TD-10, TD-11, TD-14, TD-27),
`crates/secs-service/src/{calibration.rs,cache.rs,curves.rs,dirac_import.rs,session.rs}`,
`crates/secs-service/tests/{service.rs,dirac.rs,curves.rs}`,
`src/secs/wizard/step_calibration.py:1878-1900` (upstream XO completion).

Method: test first for every finding (`/115dkk-skills:tdd` discipline). Write
the failing test at the seam named below, run it and confirm it fails for the
reason in the finding, then write the minimum code, then run the crate's
tests. Expected values come from the scenario (literals, upstream behaviour),
not from re-running the code under test.

## Findings, in order

### 1. TD-01 — MIMO freshness keyed on the calibration run, not only the settings

Today `MimoResult.calibration_key` and the cache's `mimo_calibration_key`
hold `settings.engine_key()`. Recalibrating after a measurement or target
change keeps the same key, so the old MIMO reads as fresh and ships with the
new calibration (`summaries()` and `export.rs` both trust it; a full run does
not clear `state.mimo`).

Design (fixed):
- Add `pub(crate) fn run_key(signature: &str, target_key: &str, engine_key: &str, config_key: &str) -> String`
  in `calibration.rs` (plain `format!` joined by an unambiguous separator such
  as `\u{1f}`; `config_key` comes from finding 2 — use `""` in this commit and
  thread the real value in finding 2).
- `CalibrationRun` gets `pub run_key: String`, filled at commit from the
  prepared inputs. `MimoResult.calibration_key` stores the calibration run's
  `run_key` (not `engine_key()`).
- Live MIMO staleness compares `state.calibration.run_key` with
  `result.calibration_key` (replacing the `engine_key()` comparison at
  `calibration.rs:701-704`); keep the other three conditions.
- Cache: `CachedResults` gets `#[serde(default)] pub run_key: Option<String>`
  (written for every new snapshot). Cached MIMO is fresh only when
  `cache.mimo_calibration_key == cache.run_key` and both are `Some`. A cache
  written by an older build (`run_key: None`, `mimo_calibration_key` equal to
  the engine key) therefore reads its MIMO as stale once — safe, because the
  old file cannot prove which measurements the MIMO used. Its calibration
  staleness keeps today's rule. Cache `VERSION` stays 5 (additive field).
- Apply the same rule wherever MIMO eligibility is decided: `export.rs`
  (`mimo_eligible` and the cached path) and `curves.rs::fresh_mimo`.
  Do not restructure; the result-book refactor (architecture C1) comes later.

Seam: `Service::call` in `tests/service.rs` with the `room_dba` case
(`write_case`, `open`, `start_calibration`, MIMO via `Request::Mimo`).
Test: calibrate → MIMO → replace one IR with a different waveform through
`secs_project::MeasurementStore` (as `stale_calibration_exports_with_flag_and_cache_survives_reopen`
does) → calibrate again with the same settings → `Results.mimo.stale == true`
and an auto export does not include MIMO. Second test: the same after
closing and reopening the project (cached path). Third: an old-format cache
(write `run_key` absent) reads MIMO stale, calibration not stale.

Commit: `fix(audit): A-1 MIMO 낡음을 보정 실행 지문으로 판정합니다 (#92 TD-01)`

### 2. TD-10 — staleness includes the speaker configuration

The staleness reason ignores the speaker configuration; removing a speaker
after calibration keeps the result fresh and exports the removed channel.

Design (fixed):
- `pub(crate) fn config_key(project: &Project) -> String` in `calibration.rs`:
  a stable string over every project input the engine path reads apart from
  measurements, target and calibration settings. Find them by reading
  `prepare()` and `secs_engine::engine_inputs`' use of the project
  (`project_groups`, roles, outputs, sub bundles, delays, …). Use the existing
  `{:?}`-style key pattern (`target_key_string`) for consistency.
- `stale_reason` gains `config_key` and returns `Some("config")` when it
  differs (checked after measurements, before settings). Add the reason code
  to the UI catalogs only if the UI renders reasons by code: check
  `apps/secs2r/ui/src` for `stale_reason`; if it maps codes to text, add
  `config` to the map and to `ko.json`/`en.json` (ko: "스피커 구성이
  바뀌었습니다", en: "The speaker setup has changed") — keys only in the
  existing reason family.
- `CachedResults` gets `#[serde(default)] pub config_key: Option<String>`;
  `None` (older cache) skips the config comparison.
- `run_key` (finding 1) now receives the real `config_key`.

Seam: `Service::call`: calibrate → `config_remove_speaker` (or the IPC that
removes/changes a speaker; check `ipc.rs`) → `Results.calibration.stale ==
true`, `stale_reason == Some("config")`; after reopen the same.

Commit: `fix(audit): A-2 낡음 판정에 스피커 구성을 넣습니다 (#92 TD-10)`

### 3. TD-02 — Dirac IRs get a content identity

`dirac_import.rs:280` records `uuid: None`, so re-importing a different Dirac
measurement at the same role/position/t0 leaves the measurement signature
unchanged (the signature adds UUIDs precisely to catch this, see
`calibration.rs:837-839`).

Design (fixed): derive a UUID-shaped identifier from the IR content with
`sha2::Sha256` over the little-endian `f32` samples, the sample rate and the
`t0` bits; take the first 16 bytes, set the version nibble to 5 and the RFC
4122 variant bits, format as lowercase `8-4-4-4-12`. Same waveform → same
id (re-importing the same file does not make results stale); different
waveform → different id. Add `sha2` to `secs-service` from the workspace if
it is not already a dependency.

Seam: IPC `MdatScan`/`MdatImport` with a `.liveproject` built by
`secs_dirac::test_support::write` (see `tests/dirac.rs`): import → calibrate →
import a project whose IR differs at one row → `Results.calibration.stale`
with reason `measurements`; importing the identical file again keeps it fresh.

Commit: `fix(audit): A-3 Dirac IR에 내용 지문을 줍니다 (#92 TD-02)`

### 4. TD-11 — a finished calibration does not overwrite settings edited meanwhile

`calibration.rs:129` writes the run's settings back into the project
unconditionally. Settings edited during the run are lost.

Design (fixed): `prepare()` records the project's settings before any
override (`calibration_from_project(&project)` at prepare time). At commit,
store the run's settings only when the project's current settings still
equal that snapshot; otherwise keep the project's settings (the result then
reads stale with reason `settings`). Compare with the existing keys or
derive `PartialEq` on `CalibrationSettings` if it is cheap and every field
supports it (it lives in `secs-types`; adding a derive does not change the
wire format — if you add it, run `cargo test -p secs-types` and commit any
regenerated bindings in the same commit). Apply the same to MIMO only if it
also writes settings at commit (check `start_mimo`).

Seam: `Service::call` with a pausing event sink (see
`project_replacement_rejects_old_compute_commit_and_preserves_completion`):
start calibration, while paused send `CalibrationSettingsEdit` changing one
value, let the job finish → `CalibrationSettingsGet` returns the edited value
and `Results.calibration.stale_reason == Some("settings")`.

Commit: `fix(audit): A-4 계산 중에 바꾼 보정 설정을 덮어쓰지 않습니다 (#92 TD-11)`

### 5. TD-14 — BA table after a subs-only run without a full run

`curves.rs:371` returns empty `result_lines` when `state.calibration` is
`None`, although `state.subs` exists.

Design (fixed): build the BA part from `state.subs.or(state.calibration)` as
the BA tab already does for curves; channel rows still require a full run.

Seam: `Service::call`: fresh project → `Calibrate { subs_only: true }` →
`ResultLines` (check the request name in `ipc.rs`) contains the BA entries
(non-empty, same values as after a full run's BA for the same settings, or
compared against `curves_tests.rs`' expectations).

Commit: `fix(audit): A-14 서브만 계산한 뒤에도 BA 표를 보입니다 (#92 TD-14)`

### 6. TD-27 (service part) — XO recommendation applied as upstream does

Upstream (`step_calibration.py:1883-1890`): when XO recommendation finishes it
sets `settings.xo.update(best)`, `settings.xo_rec = best`, stores the
settings into the project, and recomputes the calibration. The service's
`start_xo` returns the recommendation only.

Design (fixed): at XO job completion, lock the session, check the project
identity captured at start (`require_identity`), call `job.begin_commit()`,
then update the project's calibration settings: `xo` entries for every group
in `best` replaced by the best value, `xo_rec` set to exactly `best`; mark the
project dirty. Return the same `JobResult::Xo`. The recompute is the UI's job
(packet U1) — do not start a calibration from the service.

Seam: `Service::call`: `RecommendXo` on the oracle case used by
`oracle_project_calibrates_recommends_xo_and_exports` → after `Finished`,
`CalibrationSettingsGet` shows `xo` and `xo_rec` equal to the returned
`best` for those groups; other groups unchanged; project info `dirty`.
Check whether the existing oracle test asserts the settings stay unchanged;
if so it was asserting the bug — update it and say so in the report.

Commit: `fix(audit): A-27 XO 추천 결과를 업스트림처럼 설정에 넣습니다 (#92 TD-27)`

## Allowed files

- `crates/secs-service/src/**`, `crates/secs-service/tests/**`,
  `crates/secs-service/Cargo.toml`
- `crates/secs-types/src/calibration.rs` (only a derive, finding 4) and the
  regenerated `apps/secs2r/ui/src/ipc/bindings/**` if that derive changes them
- `apps/secs2r/ui/src/i18n/{ko,en}.json` and the stale-reason mapping file
  (finding 2 only, if the UI maps codes)
- `docs/DEVIATIONS.md`: if an entry describes the staleness keys or the XO
  recommendation behaviour, update its text to match.
- `Cargo.lock` as cargo changes it.

Do not touch the upstream Python tree, other crates, or UI markup/styles.

## Gates (run in the foreground; do not end the turn before they finish)

After each finding, before its commit:

```sh
cargo fmt --all -- --check
cargo clippy -p secs-service --all-targets --locked -- -D warnings
cargo test -p secs-service --locked
```

After the last finding also run `cargo test -p secs-types --locked` and
`python3 .github/scripts/check_feature_tests.py`.

If a pre-existing test fails before you change anything, stop and report it.

## Report (final answer)

For each finding: commit hash, files changed, the test name(s), the red
failure message you saw before the fix, and the green result. Then the full
output tail of the last gate run, anything you could not do, and any test
you changed because it asserted the old behaviour (with the reason).
