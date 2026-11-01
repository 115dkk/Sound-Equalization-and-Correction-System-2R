# P160 (Daybreak): the common input high-pass in the service, project file, export text and CLI (ADR 0031)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\lf-align`
(branch `claude/input-hp`). Do not commit, do not push. ASTRA works on the
engine, graph, filter and CamillaDSP crates at the same time (P159) and Claude
on `apps/secs2r/ui/**`; do not touch them.

Read first: `CLAUDE.md`, `docs/adr/0031-input-high-pass.md`, then the existing
numeric opt-in settings this packet copies: `rc_bands` and `mimo_low` end to
end, and the boolean `hf_phase` (P158, `docs/packets/P158-daybreak-hf-phase-service.md`):
`crates/secs-service/src/{settings_edit.rs,calibration.rs,curves.rs,results.rs,export_run.rs}`,
`crates/secs-project/src/calibration.rs`, `crates/secs-export/src/lib.rs`
(`lf_correction_pieces`), `apps/secs2r-cli/src/main.rs`, and their tests.

Already done by Claude (do not change): `CalibrationSettings.input_hp_hz:
Option<f64>`, `SettingEdit::InputHp { hz: Option<f64> }` (apply arm in
`settings_edit.rs` sets it without checks), `CalibrationLimits.input_hp`
(10–50 Hz), the `|input_hp=` engine-key suffix, `SignalChain.input_hp_hz`,
`BandType::Bw48`, `Note::InputHp { hz }` and its notice
`{"code":"input_hp","params":{"hz","order":"8"}}`.

## What to build (fixed)

1. **Edit.** `SettingEdit::InputHp { hz: Some(v) }` with `v` not finite or
   outside `LIMITS.input_hp` is rejected the way `rc_lo` is (same error code,
   `field`, `reason`, `min`, `max` params); `None` turns it off. The edit
   makes the calibration stale through the engine key (check it does, the way
   `hf_phase` is tested).
2. **Project file.** `input_hp_hz` in the calibration section: written only
   when `Some` (a float), read when present and finite and clamped to
   `LIMITS.input_hp`, absent or `null` reads as `None`. Register the key
   wherever `hf_phase` is registered (`CALIBRATION_SHAPE` or the like) so a
   legacy file and an upstream SECS 2 file read as off. Round-trip test and
   legacy-default test, like `hf_phase_round_trip_and_legacy_default`.
3. **Result curves.** When the result's chain has `input_hp_hz`, the
   calibration result graphs show the cut: add `20·log10|H|` of
   `secs_dsp::response::band_filter(grid, hz, "HP", "BW48")` to the `after`
   and `target` SPL series (and `sum_after`, per-role `after`), and the cut's
   group delay to the `after` GD series. `before` stays as measured. The
   verdict numbers (`within_1db_pct`, `gd_spread_ms`, `VerdictSummary`) stay
   exactly as now (ADR 0031 says why). Use the chain the result already holds
   (the run cache keeps `calibration_chain`); off keeps every curve
   bit-identical.
4. **Export text.** `lf_correction_pieces` (or the closest place that lists
   active low-frequency options in exported text) adds `입력 저역 컷 {hz} Hz`
   when it is on; check that the exported README/notes path that uses it gets
   it. Mirror the `hf_phase` piece and its golden.
5. **CLI.** `secs2r-cli project set --input-hp <Hz|off>` maps to
   `SettingEdit::InputHp`; the flag's help says it is the common input
   high-pass of ADR 0031. Add it to the CLI test that covers `--hf-phase`.
6. **Deviation.** `docs/DEVIATIONS.md` gets `D53` after `D52`, in the same
   table format: upstream has no common input high-pass; SECS 2R adds the
   opt-in `input_hp_hz` (ADR 0031), stored only when on, ignored by upstream;
   name the project and service tests that verify it.
7. **features.toml.** One entry for the service, project and CLI side with
   your test names. Only your entry; ASTRA adds the engine one.

## Tests

- `settings_edit`: 27 accepted; 9.9, 50.1, NaN rejected with the params; `None`
  accepted; the stored settings carry the value.
- Service: after a mock calibration with the setting on, the result curves'
  `after` and `target` differ from off by the high-pass (check two
  frequencies: at 27 Hz about −3 dB, at 200 Hz within 0.01 dB) and the
  verdict summary equals off.
- Project and CLI as above.

## Allowed files

`crates/secs-service/**`, `crates/secs-project/**`, `crates/secs-export/**`,
`apps/secs2r-cli/**`, `docs/DEVIATIONS.md`, `features.toml` (your entry only).
If a service test regenerates a recorded IPC fixture under
`apps/secs2r/ui/src/ipc/recorded/`, keep the regenerated file and name it in
the report; do not edit other files under `apps/secs2r/ui/`.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-service -p secs-project -p secs-export -p secs2r-cli --locked
python .github/scripts/check_feature_tests.py
```

`check_feature_tests.py` may fail on ASTRA's entry if P159 is not finished,
and always on `secs-sys::callback_native_sample_conversion` on Windows (the
module is not built here); say so if it does. If the engine crates are mid-way
in P159 and do not compile, say so and run what you can. Never revert files
you did not change. Before a large build, check that drive E: has at least
20 GB free; if not, delete only `target/debug/incremental` in this worktree.

## Report

Files changed, the edit rejection params, the project key handling, the curve
change (which series, how), the export text piece, the CLI usage, tests added,
and the commands' last lines.
