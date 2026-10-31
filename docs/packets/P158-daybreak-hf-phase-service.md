# P158 (Daybreak): the HF phase switch in the service, project file, export text and CLI (ADR 0030)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\lf-align`
(branch `claude/hf-phase-unwind`). Do not commit, do not push. ASTRA works on
`crates/secs-engine/**` at the same time (P157) and Claude on
`apps/secs2r/ui/**`; do not touch either.

Read first: `CLAUDE.md`, `docs/adr/0030-hf-phase.md`, then the existing
`lf_align` switch end to end, which this packet copies (P150 did it; read
`docs/packets/P150-daybreak-lf-align-service.md`):
`crates/secs-service/src/{settings_edit.rs,calibration.rs,cache.rs,export_run.rs,results.rs}`,
`crates/secs-project/src/calibration.rs`, `crates/secs-export/src/lib.rs`
(`lf_correction_pieces`), `apps/secs2r-cli/src/main.rs`, and their tests.

Already done by Claude (do not change): `CalibrationSettings.hf_phase`,
`SettingEdit::HfPhase { on }` and the `|hf_phase` engine-key suffix
(secs-types), and `Note::HfPhase { role, rotation_deg: Option<f64>, limited }`
in `crates/secs-engine/src/notes.rs`.

The workspace does not compile at the start: `secs-service` has
non-exhaustive matches on `SettingEdit` and `Note`. Making it compile is part
of this packet.

## What to build (fixed)

1. **Edit** (`settings_edit.rs`): `HfPhase { on }` sets `hf_phase`, exactly
   like `LfAlign`, accepted in every mode, marks results stale the same way.
2. **Project file** (`secs-project/src/calibration.rs`): optional key
   `hf_phase` in `CALIBRATION_SHAPE`, read with `is_truthy`, written as
   `hf_phase: true` only when on (D52). Round-trip and legacy tests like the
   `lf_align` ones.
3. **Notices** (`secs-service/src/calibration.rs`): `Note::HfPhase` is a
   scalar notice. With `rotation_deg: Some(v)` the code is `hf_phase` with
   params `role`, `rotation_deg` (same number style as `lf_align`'s
   `arrival_ms`) and `limited` (`"true"`/`"false"`). With `None` the code is
   `hf_phase_none` with param `role`. Unit tests next to the `lf_align` notice
   tests, and add the codes wherever the `lf_align` code is listed (around
   line 1797).
4. **Labels of a run** (`cache.rs` `CachedRunLabels`, `export_run.rs`,
   `results.rs`): carry `hf_phase` exactly like `lf_align` (serde default,
   skipped when false, so existing caches load and new ones without it are
   unchanged). Extend the tests that cover `lf_align` there.
5. **Export text** (`secs-export`, `lf_correction_pieces`): when `hf_phase` is
   on and the mode is not Zero add the piece `고역 위상 맞춤`, after the two
   existing pieces. Extend the golden test case that turns the ADR 0025
   switches on.
6. **CLI** (`project set`): `--hf-phase on|off`, the same `Switch` handling as
   `--lf-align`. Test like the `--lf-align` test.
7. Fix every `CalibrationSettings { .. }` literal or exhaustive match outside
   secs-types and secs-engine that the new field breaks.
8. `features.toml`: `calibration.edit_hf_phase` and `cli.hf_phase`, pointing
   at the real tests you add. `docs/DEVIATIONS.md`: add D52 in the format of
   D49: the switch, off by default, written only when on, ignored by upstream
   SECS 2, and what it changes (unwinds the wrapped constant phase the RC FIR
   leaves above the band over the two octaves above the last fade edge). Point
   at ADR 0030.

## You may edit

`crates/secs-service/**`, `crates/secs-project/**`, `crates/secs-export/**`,
`crates/secs-camilla/**`, `apps/secs2r-cli/**`, `features.toml` (only your
entries), `docs/DEVIATIONS.md`. Not secs-types, not secs-engine, not the
screen.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --exclude secs-engine --all-targets --locked -- -D warnings
cargo test -p secs-service -p secs-project -p secs-export -p secs-camilla -p secs2r-cli --locked
python .github/scripts/check_feature_tests.py
```

`check_feature_tests.py` may fail only on the engine entry P157 has not added
yet (and on `secs-sys::callback_native_sample_conversion`, which is skipped on
Windows); say so if it does. If secs-engine is mid-way in P157 and does not
compile, say so and run what you can. Never revert files you did not change.

## Report

Files changed, the notice codes and params, the export piece, the CLI usage,
tests added, and the commands' last lines.
