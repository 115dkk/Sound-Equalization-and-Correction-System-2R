# P219 (ASTRA): a setting that leaves the mains without alignment delay

Worktree `E:\wt\sub-arrival`, branch `fix/sub-arrival-harmonics`. Build with
`CARGO_TARGET_DIR=C:\cargo-target\sub-arrival` set for every cargo command
(the worktree must not get a `target` folder of its own). Do not commit, do
not push. Run every command in the foreground and end your turn only after
the report. Do not touch the upstream Python tree, `CHANGELOG.md`,
`docs\DEVIATIONS.md`, `docs\adr`, or the markup and styles of
`apps\secs2r\ui\src\screens\calibrate\Settings.tsx` (the main session places
the row on the screen).

## Why

Issue #203: a user gets a few ms of delay on L or R that they did not ask
for and deletes it after every export. There is no setting for it. The
maintainer decided (2026-11-12) that it becomes an advanced setting. The
design is `docs\adr\0061-main-delay-off.md`; read it first, it is binding.

## What to do

Add the calibration setting `main_delay_off` (bool, default false), wired
exactly like `hf_phase` is. `git grep -n hf_phase` lists every place; go
through that list file by file and add the new setting next to it wherever
the place is about declaring, storing, editing, keying, labelling or passing
an engine toggle (skip the places that are about what `hf_phase` computes:
`rc.rs`, `notes.rs`, `Note`, `secs-export`, results labels unless the label
list is generic over engine toggles; say in the report which you skipped and
why).

1. `crates\secs-types\src\calibration.rs`: the field
   (`main_delay_off: bool = false; identity(); flag; project(boolean, true_only);`),
   the edit `["main_delay_off", main_delay_off]` with param `on`,
   `project_order` (after `"dip_limit"`), `engine_key` (appends
   `|main_delay_off` only when on, after the existing toggles in the order
   the code already uses), and a test like
   `hf_phase_defaults_off_and_extends_the_key_only_when_on`. Regenerate the
   bindings with `cargo test -p secs-types` and keep the regenerated files
   under `apps\secs2r\ui\src\ipc\bindings\`.
2. `crates\secs-project`: the key list and a round-trip test (saved only
   when true, a file without the key reads as off).
3. `crates\secs-engine`: `EngineConfig::main_delay_off`
   (`/// Leave the mains without alignment delay between them (ADR 0061).`),
   filled from the settings in `calibration.rs`, defaulted in `testkit.rs`.
   The computation is one place, `prepare_until` in `measurement.rs`:

   ```text
   latest      = latest arrival among mains and subs      (as today)
   latest_main = latest arrival among config.mains
   delay(role) = latest - latest_main      if main_delay_off and role is a main
               = latest - arrival(role)    otherwise                (as today)
   ```

   Keep the float operations of the off path exactly as they are (the
   goldens compare bits). Nothing else in the engine changes: every reader
   takes `Prepared::align_delay_ms`. Check that claim by reading the readers
   (`ba.rs` `base`, `chain.rs`, `design.rs`, `mimo\design.rs`,
   `mimo\run.rs`, `xo.rs`, lf_align in `rc.rs`); if one of them derives a
   main's delay from `arrival_s` or from an onset instead, stop and report
   it instead of patching around it.

   Tests in `crates\secs-engine\src\tests\prep.rs` (or `design.rs` where a
   full design is needed), on a set whose mains arrive at different times
   and that has at least one sub:
   - off: `align_delay_ms` equals today's values;
   - on: every main has the same delay, equal to `latest - latest_main`;
     each sub has the delay it has when off;
   - on, through a full design: the delay of every main in the result
     (`delay_ms` of the main rows) is the same value.
   If the last one does not hold, report what differs and why; do not force
   it.
4. `crates\secs-service`: whatever `hf_phase` has for edit, persistence and
   stale marking (`tests\calibration.rs`, `tests\config.rs`, `cache.rs`
   labels only if they list engine toggles generically). A test that the
   edit persists and marks results stale, modelled on
   `lf_align_and_mode_decay_edits_persist_and_mark_results_stale`.
5. `apps\secs2r-cli`: the same flag surface `hf_phase` has, with its test.
6. UI wiring, no markup: `settingRows.ts` gets the row

   ```ts
   id: "calibrate.mode.main_delay_off", control: "toggle", field: "main_delay_off", kind: "main_delay_off", param: "on",
   label: "calibrate.mode.main_delay_off", help: "calibrate.mode.main_delay_off.help",
   ```

   (no `disabledWhen`, no hint: it works in every mode). Add it to
   `text.ts`, `fixtures.ts`, `mock.ts`, the recorded IPC samples and the
   tests wherever `hf_phase` is listed for the same purpose. The row will
   not be on the screen until the main session adds it to `Settings.tsx`;
   if a test requires every declared row to be rendered, say so in the
   report and leave that test failing.
7. i18n, `ko.json` and `en.json` together (하십시오체, label says the
   effect, no dressed-up wording):
   - `calibrate.mode.main_delay_off`: `메인 딜레이 넣지 않기` /
     `No delay on the mains`
   - `calibrate.mode.main_delay_off.help`: say that the mains get no delay
     to match their arrival times, so a difference in speaker distance
     stays as it is and the image leans toward the nearer speaker; that the
     subs are still aligned, to the main that arrives last; and that it is
     for systems where the player or amplifier already compensates distance
     or where the delay is not wanted. Two or three sentences.
   - `calibrate.mode.advanced` is not needed (the `Advanced` control has its
     own label).
8. `features.toml`: register `calibrate.main_delay_off` as implemented,
   pointing at the tests you added, in the form the `hf_phase` entry uses.

## Checks (run all, in the foreground, and paste the tail of each)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --exclude secs2r --exclude secs2r-host-shell --all-targets --locked -- -D warnings
cargo test -p secs-types -p secs-project -p secs-engine -p secs-service -p secs2r-cli --locked --no-fail-fast
python .github/scripts/check_feature_tests.py
(cd apps/secs2r/ui && npm run typecheck && npm test)
```

`git status --short` must show `bindings\dotnet\Secs2R\NativeMethods.g.cs`
unchanged or changed only in line endings; do not stage anything.

## Report

- Files changed, grouped by step.
- The `hf_phase` places you skipped and why.
- Whether every reader of the alignment delay takes it from
  `Prepared::align_delay_ms` (name any that does not).
- Whether the main delays of a full design are equal when the setting is on.
- The tail of every check, including failures you could not resolve.
