# P144 (ASTRA): 극저역 보호 엔진

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\rc-upper`
(branch `claude/rc-upper`). Do not commit, do not push. Do not edit
`CHANGELOG.md`, the upstream tree (`src/`, `tests/`, `tools/`), or anything
under `apps/secs2r/ui/` except generated bindings and `ipc/recorded/`.

Read first: `CLAUDE.md`, `CONTEXT.md`, **`docs/adr/0022-low-end-protection.md`
(the design; follow it exactly)**, `docs/DEVIATIONS.md` (engine table
format). Facts already surveyed (file:line in this worktree):

- RC: `crates/secs-engine/src/rc.rs` `prepare_correction` (371-433): `corr`
  clamp at 402-414, `rc_lo` ramp at 429-431, the existing no-boost mask
  (164-181, applied at 423-425, enforced on the residual at 641 and on PEQ
  gains at 656-661), `smoothed_db` (69-78). `RcInput` is built per role in
  `design.rs:772-781`; `comb` (746-770) is what RC sees.
- BA: `crates/secs-engine/src/ba.rs` bus PEQ `bus_fit` (785-805, lo = 20 Hz at
  777), bundle PEQ (1036-1045), sub level reference (734-757), sub sum by
  position (1307-1338). Sub fixed filters are multiplied into the measured
  sub spectra in `measurement.rs` (107-131, 262-266, 325); exported through
  `SubChannel.hp_hz`/`lp_hz` (`design.rs:886-892`).
- MIMO's rolloff finder: `crates/secs-engine/src/mimo/design.rs`
  `support_ranges` (94-188), reference rules and −10 dB search; its HP is
  `hp2²` (190-216). MIMO results must not change: if you share code with it,
  move only pure helpers and prove MIMO goldens unchanged.
- Settings: `crates/secs-types/src/calibration.rs` (`CalibrationSettings`
  118-163, `Default` 165-197, `engine_key` 205-247 appends suffixes only for
  non-default values, model test 616-646); `settings_edit.rs` (`SettingEdit`
  95-203); `rules.rs`; project `crates/secs-project/src/calibration.rs`
  (`calibration_json` 134-227 writes keys only when non-default;
  `CALIBRATION_SHAPE.optional` 21-34 must list such keys); service apply
  `crates/secs-service/src/settings_edit.rs` (268-403); engine plumbing
  `crates/secs-engine/src/calibration.rs` `engine_inputs` (110-239) →
  `EngineConfig` (`input.rs:154-204`) → `Setup`; notes `notes.rs` mapped by
  `secs-service/src/calibration.rs` `notices()` (617+), guarded by
  `every_engine_note_maps_to_a_notice_code_and_params`; CLI `project set`
  (`apps/secs2r-cli/src/main.rs:548-565`). Precedent for a default-off
  setting: `engine.analytic_derivatives` in `features.toml`.

## Build

1. `low_protect: bool` (serde default false) in `CalibrationSettings`, in
   `engine_key` only when true; `SettingEdit::LowProtect { on }`; project key
   `low_protect` written only when true (and listed as optional in the
   shape); CLI `project set --low-protect on|off`; bindings regenerated.
2. Rolloff point per ADR 0022: position-weighted power, 1/3-octave smoothed,
   reference median (subs 40–80 Hz, mains 200–1000 Hz; a sub with user
   filters uses its passband maximum, as MIMO does), search down from the
   reference band's low edge for the first frequency 10 dB below; none
   within the measured band means no protection for that channel.
3. When on: RC never boosts below the channel's rolloff point (same
   mechanism as the no-boost mask, enforced on residual and PEQ gains); bus
   and bundle PEQ fit from the sub sum's rolloff point instead of 20 Hz;
   each sub without a user high-pass gets a 24 dB/oct high-pass (two
   second-order Butterworth sections) at rolloff × 2^(−1/2) in its exported
   `SubChannel` (not multiplied into the measured response used for the
   design); mains get no high-pass.
4. `Note::LowProtect { role, rolloff_hz, hp_hz: Option<f64> }` for each
   protected channel, mapped to notice `low_protect` with params `role`,
   `rolloff_hz`, `hp_hz` (absent when none).
5. Off: every golden unchanged (run them).
6. `docs/DEVIATIONS.md` engine table: next free D-number, status `적용`,
   source `correction_engine.py` RC (1134-1155) and bus PEQ (777-783):
   upstream boosts down to 20 Hz regardless of the speaker's rolloff;
   `low_protect` (default off) stops it. `features.toml`: register the
   setting, the edit and the CLI flag with real tests. Immediately before
   you edit these two files read them again: another agent (P143) may be
   adding rows at the same time; keep theirs.

## Tests (synthetic IRs)

- A sub with a 4th-order 30 Hz high-pass: rolloff found near the expected
  −10 dB point (state the expected value and tolerance); RC correction ≤ 0
  below it; bus PEQ has no band boosting below it; the exported sub has the
  24 dB/oct high-pass at rolloff/√2.
- A sub with a user `hp_hz`: the user filter is kept, no second high-pass,
  and no boost below the rolloff the user filter creates.
- A channel flat to 20 Hz: no rolloff, nothing changes.
- A main without subs that rolls off at 40 Hz: no boost below its rolloff,
  no high-pass.
- Off: results identical to before (goldens), and `engine_key` unchanged.
- Project round trip, upstream file without the key, edit and CLI flag,
  results become stale when toggled, the notice params.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-types -p secs-project -p secs-engine -p secs-service -p secs2r-cli --locked
python .github/scripts/check_feature_tests.py
```

(On Windows `check_feature_tests.py` also reports `secs-sys::callback_native_sample_conversion`;
that line is expected.)

## Report

The rules as built with the functions reused, the rolloff values in the
tests, the note params, the DEVIATIONS row, files changed, and the
commands' last lines. List what the screen must wire (edit kind, notice
code and params) for P145.
