# P26 (Daybreak): the methods the screens edit the project with

Worktree: `/home/user/wt/p26` (branch `m5/p26`, from `m5/plan`). Work only
there. Do not commit; Claude reviews the diff and commits. Do not spawn
sub-agents. Build with
`export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p26`
(yours alone). The oracle venv is
`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.
P27 (`crates/secs-service/src/curves*`) and P28 (host, monitor, export
folders) work in parallel; do not create or edit their files. Add your
request variants to `secs_types::ipc` in one contiguous block at the end
of `Request`/`Response` so the three branches merge cleanly.

Read first: `docs/M5.md` (§3 the method table, rows 구성, DSP 프로필, 배치와
측정 옵션, 타겟, 보정 설정, 앱 설정), `docs/M5-inventory.md` §1 steps 2 and 4,
§5 and the target menu, `docs/adr/0003-*` (codes, not sentences),
`docs/DEVIATIONS.md` U5, `crates/secs-types/src/ipc.rs`,
`crates/secs-service/src/{lib.rs,ipc.rs,session.rs,measurement.rs}` (how
methods are dispatched, how the open project is held and saved, how
`position_clear` keeps undo tokens), `crates/secs-project/src/{config.rs,
dsp_profile.rs,plan.rs,calibration.rs}`, `crates/secs-types/src/target.rs`,
and upstream `src/secs/model.py` (`SpeakerConfig`, `DspProfile`, `Project`),
`src/secs/wizard/step2_config.py`, `step4_layout.py`, `timing.py`,
`target_editor.py`, `step_calibration.py` (target editing `:1017-1109`,
`mode_spec` use), `src/secs/calibration.py` (`CalibrationSettings`,
`mode_spec`).

## Goal

Every value the upstream screens let the user change can be read and
changed through `Service::call`, stored where upstream stores it so that
the saved `.secs` bytes stay the same, and validated with codes the screen
turns into sentences.

## Methods (types in `secs_types::ipc`, one `///` line per item)

- **Config.** `config_get` (speakers with role, size, bands with band, out,
  pol; preset; device; groups in upstream order; summary: speaker count,
  outputs, max channel, duplicate outputs, sub count). `config_apply_preset
  { preset, confirm }`, `config_set_sub_count { count, confirm }`,
  `config_set_speaker { index, role?, size?, band_index?, out?, pol? }`,
  `config_add_speaker`, `config_remove_speaker { index }`, `config_renumber`,
  `patch_sheet_csv { path }` (upstream bytes: UTF-8 with BOM, columns
  role,name,size,band,out,pol). The rules upstream's setters follow (a sub
  role forces a SUB band, any edit sets the preset to custom, add uses the
  first unused role and the next free output) stay in `secs-project`;
  port what is missing there.
- **Validation.** `config_validate` returns `Vec<Notice>` (the `Notice`
  type P23a-2 added): a structured twin of `SpeakerConfig::validate` with
  one code per message kind (`role_duplicate`, `out_invalid`,
  `out_duplicate`, `band_unknown`, `pol_invalid`, `bands_empty`,
  `sub_bands`, `main_with_sub_band`, `band_combination`) and the values as
  params, in the same order as the messages. Keep the sentence function for
  its existing oracle test and test that the two agree one to one.
- **Clearing measurements (U5).** Changing the preset, the sub count or
  the arrangement clears measurements upstream. Here such a request
  without `confirm: true` on a project that has measurements returns the
  error `confirm_required { measurements }` and changes nothing; with
  `confirm: true` it clears them and returns an undo token that the
  existing `undo` method restores (reuse P23c's token store).
- **DSP profile.** `dsp_get` (profile name, the five values, the presets
  table, the derived window length and resolution upstream shows),
  `dsp_set { preset }` or `dsp_set { fields }` (a manual edit makes it
  `custom`, as upstream).
- **Arrangement and options.** `layout_get` / `layout_set` for
  `extra['array']` (1, 9, 13, 17), `extra['start_delay']`, the timing
  reference and its reference role or output (`extra['devices']`), plus
  `position_estimate` (per position and total, `secs_project::position_seconds`
  and upstream's `step4_layout` totals). The arrangement geometry the maps
  draw (positions, heights, which is position 1) comes from upstream
  `layout_map.py`; return it as data so the screen does not hard-code it.
- **Target.** `target_get` (points, preset or custom), `target_set_points`
  (clamped to 10–24000 Hz and ±30 dB, at least two points, clears the
  preset), `target_apply_preset { flat | harman | cinema }`,
  `target_save { path }` and `target_load { path }` (upstream's JSON
  format, bounded read).
- **Calibration settings.** `calibration_settings_get` and
  `calibration_settings_set { settings }` over `CalibrationSettings` as
  stored in `extra['calibration']` (`calibration_from_project`,
  `store_calibration`); changing them marks results stale through the
  existing keys. `mode_spec { mode }` returns the values upstream's spec
  line is made from (filters, taps, latency bounds) as numbers, not text.
- **App settings.** `app_settings_get` / `app_settings_set`: language,
  expert display, recent projects (at most 10), last window size. Stored as
  a versioned, data-only JSON file at a path the app injects through
  `Deps` (`app_settings_path: Option<PathBuf>`; `None` keeps them in
  memory, which tests use). Bounded read; an unknown version or a broken
  file means defaults.

Every method that changes the project marks it dirty and leaves saving to
`project_save`, as P23a does.

## Proof

- Oracle family `service_config` (`oracle/export_service_config.py`,
  under 1 MiB): run upstream's model operations (presets with each sub
  count, `set_sub_count` up and down, role and output edits, add and
  remove, renumber, `validate` on a set of broken configs, `DspProfile`
  presets and manual edits, the target presets and edits, the patch sheet
  CSV, `position_seconds` per arrangement and timing reference,
  `CalibrationSettings` round trips) and record the `.secs` text after each
  step and the returned values. The Rust tests replay the same steps
  through `Service::call` and compare the saved `.secs` bytes and the
  values.
- Tests for `confirm_required`, the undo of a cleared preset change, and
  the app settings file (round trip, broken file, oversized file, unknown
  version).

`features.toml`: `ipc.<method>` for every method, `service.config_edit`,
`service.app_settings` (M5) as `implemented` with their tests. Mark U5's
row in `docs/DEVIATIONS.md` to cover the config and arrangement paths.

## Files you may change

- `crates/secs-service/src/**` except `curves*` and the files P28 names
  (`host*`, `monitor*`, `export_folder*`); `crates/secs-service/tests/config*.rs`
- `crates/secs-project/src/**` (model rules that are missing),
  `crates/secs-types/src/ipc.rs`, `apps/secs2r/ui/src/ipc/bindings/**`
  (regenerated with `cargo test -p secs-types`)
- `oracle/export_service_config.py`, `oracle/goldens/service_config/**`
- `features.toml`, `docs/DEVIATIONS.md` (U5), `Cargo.lock` as cargo writes it

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p26
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p26
cargo fmt --all -- --check
cargo clippy -p secs-service -p secs-project -p secs-types --all-targets --locked -- -D warnings
cargo test -p secs-service -p secs-project -p secs-types --locked 2>&1 | grep -E "test result|FAILED|panicked"
cargo test -p secs-policy --locked 2>&1 | grep -E "test result|FAILED"
(cd apps/secs2r/ui && npm ci --no-audit --no-fund && npm run typecheck && npm test)
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-service -p secs-project -p secs-types
python3 .github/scripts/upstream.py check-tree
```

Do not run `check_feature_tests.py` (Claude runs it in the gate). The full
`secs-service` test run takes about two minutes in debug; run it once at
the end, and the new test files alone while you work.

## Report

The methods as built (params, results, errors), the oracle family and its
size, each test, and anything you could not do.
