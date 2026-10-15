# P37 (ASTRA): settings edit rules, sub warnings, REW facts, mdat position tag

Worktree: `/home/user/wt/p37` (branch `m5/p37`). Do not commit; Claude
reviews the diff and commits. Another worker runs P36 in parallel
(`/home/user/wt/p36`) on other service gaps; stay inside the items below.

Read `CLAUDE.md`, `docs/M5.md` §3, `docs/adr/0003-stack-and-ipc.md`
(codes, not sentences), `docs/adr/0004-*.md` (oracle first),
`oracle/README*` and an existing oracle family (`oracle/export_service_config.py`
with `crates/secs-service/tests/config.rs`) to see how fixtures are made and
replayed. Upstream is `src/secs/` in the same tree (read-only; GPL-3.0 like
this repository). Use the oracle venv
`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

## 1. Settings edit rules: `calibration_settings_edit`

The 보정 screen (P31b) currently ports upstream's edit rules to TypeScript
(`apps/secs2r/ui/src/screens/calibrate/settingsEdit.ts`): `set_sub` with
group joining and filter sharing, `set_layout`, `set_infra`, room volume and
dimensions, band-type defaults (picking a type sets HP 20 or LP 80), and the
limits. Rules belong in the service. Port them from upstream
`CalibrationSettings` and the screen code that applies them
(`src/secs/calibration.py`, `src/secs/wizard/step_calibration.py:1147-1490`
and the BA card handlers) into Rust:

- `SettingEdit` in `secs-types` mirrors the TypeScript `SettingEdit` union in
  `settingsEdit.ts` exactly (same variants, field names and serde tags), so
  the screen's edit objects serialize as they are.
- `calibration_settings_edit { edit }` applies one edit to the stored
  settings, stores them (dirty, like `calibration_settings_set`) and returns
  the new `CalibrationSettings`.
- Oracle family `service_settings_edit`: drive upstream's own methods for
  every variant and its edge cases (joining a group, leaving it, filter
  sharing across a group, infra on and off, dims to volume, every limit) and
  compare the resulting settings dict; replay in
  `crates/secs-service/tests/settings_edit.rs`.
- Do not delete `settingsEdit.ts`; P33 switches the screen to the IPC and
  removes it.

## 2. `sub_warnings`

Before a run the 보정 screen needs upstream's filter and gain warnings for
the current settings (`cal.sub_filter_warnings`, `cal.sub_gain_warnings`, or
whatever upstream calls on settings alone). `sub_warnings` returns them as
`Notice` codes with params, the same codes `result_lines` uses after a run.
Cover them with the same oracle family.

## 3. REW facts: `rew_info`

The 연결·점검 screen shows what upstream step 1 shows about REW
(`step1_devices.py:179-271,:379-521`) and `RewStatusResult` lacks:
executable path (`find_rew_exe`), version (`rew.py:339`), Pro, blocking mode,
mic calibration file (`input_calibration_file`, `rew.py:708`), input and
output devices with channel counts and input channel names
(`rew.py:409-412`), and REW's own rate, sweep and level ("read settings",
`step1_devices.py:485-521`). Add `rew_info` returning all of them (each
optional when REW does not answer), through `secs-rew` on the injected
transport; extend `secs-rew`'s fake REW so tests cover a running REW, one
without the API, and one without Pro. Check the real REW API shapes against
`secs-rew`'s recorded responses (`crates/secs-rew/tests/`), not from memory.

## 4. mdat position tag

`MdatScanMeasurement.position_tagged`: whether the title named a position
(upstream `split_position`). Untagged measurements go to the current
position in upstream; the assignment dialog needs to know which ones were
untagged. Extend the `service_mdat` oracle if it does not already cover an
untagged title.

## Rules

- IPC types only in `secs-types`; regenerate the bindings with
  `cargo test -p secs-types` run from `/home/user/wt/p37`; keep the UI
  typecheck green (touch only `apps/secs2r/ui/src/ipc/{mock,fixtures}.ts`
  besides the generated bindings).
- Register each method as `ipc.<method>` in `features.toml`, `implemented`
  only with its real test.
- Oracle outputs stay small (the repository holds them); say the size.

## Files you may change

`crates/secs-types/src/**`, `crates/secs-service/**`, `crates/secs-rew/**`,
`oracle/**` (new family and its fixtures), `apps/secs2r/ui/src/ipc/bindings/**`
(generated), `apps/secs2r/ui/src/ipc/{mock,fixtures}.ts`, `features.toml`.

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p37
export CARGO_TARGET_DIR=/home/user/target-p37 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --exclude secs2r --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test --locked -p secs-types -p secs-service -p secs-rew -p secs-policy
python3 .github/scripts/upstream.py check-tree
(cd apps/secs2r/ui && PATH=/opt/nvm/versions/node/v24.22.0/bin:$PATH npm ci && npm run typecheck && npm test)
```

## Report

Per item: types and methods, the oracle family (files, size), the tests,
and any upstream rule you could not reproduce exactly, with the reason.
