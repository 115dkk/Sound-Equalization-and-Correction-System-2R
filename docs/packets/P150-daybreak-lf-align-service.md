# P150 (Daybreak): LF arrival and room-ringing switches in the service, project file, export text and CLI (ADR 0025)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\lf-align`
(branch `claude/lf-align`). Do not commit, do not push. ASTRA works on
`crates/secs-engine/**` at the same time (P151) and Opus on
`apps/secs2r/ui/**` (P152); do not touch either.

Read first: `CLAUDE.md`, `docs/adr/0025-lf-arrival-and-mode-decay.md`, then
the existing `low_protect` switch end to end, which this packet copies:
`crates/secs-types/src/{calibration.rs,settings_edit.rs}` (already done:
`CalibrationSettings.lf_align`, `.mode_decay`, `SettingEdit::LfAlign { on }`,
`SettingEdit::ModeDecay { on }`, `engine_key` suffixes; do not change
secs-types), `crates/secs-engine/src/notes.rs` (already done:
`Note::LfAlign { role, strength, arrival_ms: Option<f64>, limited }` and
`Note::ModeDecay { role, target_t60_s, modes: Vec<(hz, t60_s, depth_db)> }`;
do not change secs-engine), then `crates/secs-service/src/{settings_edit.rs,calibration.rs}`,
`crates/secs-project/src/calibration.rs`, `crates/secs-export/src/lib.rs`,
`apps/secs2r-cli/src/main.rs`.

The workspace does not compile at the start: `secs-service` has two
non-exhaustive matches (the new `SettingEdit` variants and the new `Note`
variants). Making it compile is part of this packet.

## What to build (fixed)

1. **Edit** (`crates/secs-service/src/settings_edit.rs`): `LfAlign { on }`
   sets `lf_align`, `ModeDecay { on }` sets `mode_decay`, exactly like
   `LowProtect`. `lf_align` is accepted in every mode (the engine ignores it
   in Zero mode, and the screen explains that); do not reject it. Both edits
   mark results stale like `LowProtect`.
2. **Project file** (`crates/secs-project/src/calibration.rs`): both keys are
   optional in `CALIBRATION_SHAPE` like `low_protect`; read with
   `is_truthy`; write `lf_align: true` / `mode_decay: true` only when on
   (D49: upstream SECS 2 ignores the keys). Round-trip and legacy tests like
   the `low_protect` ones.
3. **Notices** (`crates/secs-service/src/calibration.rs`):
   - `Note::LfAlign` is a scalar notice `lf_align` (in `scalar_notice`) with
     params `role`, `strength`, `limited` (`"true"`/`"false"`), and
     `arrival_ms` only when `Some`. Numbers use the existing `to_string()`
     style of that function.
   - `Note::ModeDecay` is a collection note (expand it where
     `Note::LongModeBoost` expands, and add it to the `unreachable!` list in
     `scalar_notice`). Each mode becomes a notice `mode_decay` with params
     `role`, `hz`, `t60_s`, `target_s` (= `target_t60_s`), `depth_db`. An
     empty `modes` becomes one notice `mode_decay_none` with params `role`,
     `target_s`.
   - Unit tests for both mappings next to the existing `RcBands` and
     `LowProtect` notice tests.
4. **Export text** (`crates/secs-export/src/lib.rs`, next to
   `rc_bands_piece` and its two call sites): when `lf_align` is on add the
   piece `저음 도착 맞춤` / `bass arrival aligned`; when `mode_decay` is on add
   `방 울림 줄임` / `room ringing shortened`. Follow the language and the
   separator the RC line already uses (look at how the English variant of
   that line is produced; if there is only Korean text there, write only
   Korean). Nothing changes when both are off; extend the existing export
   text test with one case where both are on.
5. **CLI** (`apps/secs2r-cli/src/main.rs` `project set`): `--lf-align on|off`
   and `--mode-decay on|off`, the same `Switch` type and handling as
   `--low-protect`. Test like the `--low-protect` test.
6. Fix every `CalibrationSettings { .. }` literal outside secs-types and
   secs-engine that the new fields break; make the whole workspace compile.
7. `features.toml`: `calibration.edit_lf_align`, `calibration.edit_mode_decay`
   and `cli.lf_align` (one entry may cover both CLI flags) pointing at the
   real tests you add. Do not add the engine entries (P151 does).
   `docs/DEVIATIONS.md`: add D49 in the format of D48: the two switches,
   off by default, written only when on, ignored by upstream SECS 2, and
   what each changes (common LF arrival below 200 Hz with a pull limit;
   pole-replacing cuts for long modes). Point at ADR 0025.

## You may edit

`crates/secs-service/**`, `crates/secs-project/**`, `crates/secs-export/**`,
`crates/secs-camilla/**`, `apps/secs2r-cli/**`, `features.toml`,
`docs/DEVIATIONS.md`. Not secs-types, not secs-engine, not the screen.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --exclude secs-engine --all-targets --locked -- -D warnings
cargo test -p secs-service -p secs-project -p secs-export -p secs-camilla -p secs2r-cli --locked
python .github/scripts/check_feature_tests.py
```

`check_feature_tests.py` may fail only on entries P151 has not added yet;
say so if it does. If secs-engine is mid-way in P151 and does not compile,
say so and run what you can. Never revert files you did not change.

## Report

Files changed, the notice codes and params, the export pieces, the CLI
usage, tests added, and the commands' last lines.
