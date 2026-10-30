# P139 (ASTRA): RC 상한 기준(엄격한·너그러운 슈뢰더, 위치 일치도)과 전이 구간 배수

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\rc-upper`
(branch `claude/rc-upper`, from main at 0.35.2). Do not commit, do not push.
Do not edit `CHANGELOG.md` (Claude writes it at PR time) or anything under
`apps/secs2r/ui/` except the generated bindings.

Read first: `CLAUDE.md`, `CONTEXT.md`, **`docs/adr/0021-rc-upper-limit-basis.md`
(the design; follow it exactly)**, `docs/DEVIATIONS.md` (format; the engine
table), then:
- `crates/secs-engine/src/levels.rs` (`schroeder_hz`, `weights`, `band_level`,
  `median`), `crates/secs-engine/src/design.rs` (around `RC_AUTO_MAX_HZ` and
  the `rc_auto` block, `Note::RcLimit`), `crates/secs-engine/src/notes.rs`,
  `crates/secs-engine/src/setup.rs` (`rc_hi_manual`), `crates/secs-engine/src/input.rs`
  (`EngineConfig`, `room_volume_m3`), `crates/secs-engine/src/calibration.rs`,
  `crates/secs-engine/src/rc.rs` (how RC uses `rc_lo`/`rc_hi`, the smoothed
  responses and the target).
- `crates/secs-types/src/calibration.rs` (`CalibrationSettings`, `engine_key`),
  `crates/secs-types/src/settings_edit.rs` (`SettingEdit::RcHi`, `RoomVolume`),
  `crates/secs-types/src/rules.rs`.
- `crates/secs-project/src/calibration.rs` (read/write of `rc_hi`,
  `room_volume_m3`) and its tests.
- How `secs-service` turns `CalibrationSettings` into `EngineConfig` and
  applies `SettingEdit`s (search `room_volume_m3` and `RcHi`).
- How the `rc_limit` note reaches the screen: `apps/secs2r/ui/src/screens/calibrate/text.ts`
  (read only; P140 changes the screen).

## Design (fixed by ADR 0021)

1. **Types (`secs-types`).**
   - `pub enum RcHiBasis { Strict, Lenient, Agreement }` (serde snake_case,
     default `Strict`, ts export).
   - `CalibrationSettings` gains `rc_hi_basis: RcHiBasis` and
     `rc_hi_transition: u8` (1, 2 or 4; `#[serde(default)]` giving 1),
     defaults `Strict` and 1, both in `engine_key` so a change makes results
     stale. Manual mode stays `rc_hi: Some(hz)`; the basis applies only when
     `rc_hi` is `None`.
   - `SettingEdit` gains `RcHiBasis { basis }` and `RcHiTransition { factor }`
     (reject factors other than 1, 2, 4 with the existing invalid-edit
     error). Keep `RcHi` and `RoomVolume` as they are.
   - Regenerate the bindings with `cargo test -p secs-types` and keep them.
2. **Project (`secs-project`).** Read and write `rc_hi_basis` (string) and
   `rc_hi_transition` (number) in the calibration settings object; missing
   or unknown values read as `strict` and 1. Tests: round trip; a file
   without the keys (upstream) reads as strict ×1; unknown values read as
   strict ×1.
3. **Engine (`secs-engine`).**
   - `EngineConfig` gains the basis and the transition factor (defaults
     strict, 1) wherever `room_volume_m3` travels.
   - `levels::schroeder_hz` stays as it is (strict, upstream golden).
   - New `levels::lenient_schroeder_hz(set, config) -> Option<(f64, f64, f64)>`
     returning (chosen band centre Hz, that band's median T60 s, g(f)): 1/3-
     octave centres from 100 Hz to 500 Hz (base-10 nominal centres 100, 125,
     160, 200, 250, 315, 400, 500), each band a 4th-order Butterworth band-pass
     between `fc / 2^(1/6)` and `fc × 2^(1/6)`, T60 from the backward
     integral as `schroeder_hz` does, median over **every position and every
     main**; pick the highest band with `g(f) >= f`. `None` without a volume
     or without any band meeting the condition.
   - Strict value `S` = today's `schroeder_hz`. Lenient value
     `L = max(S, lenient band centre)` (when one of them is `None`, the other;
     both `None` gives `None`).
   - Transition factor `m`: the automatic upper limit for the two Schroeder
     bases is `clamp(value × m, 100, 500 × m)`; without a volume it is 500 Hz
     and `m` is not applied (as today).
   - **Agreement:** only with at least three measured positions (otherwise an
     input error the screen already prevents; return the existing kind of
     `EngineError` with a clear message). Pass 1: run the design with
     `rc_hi = 2000`. For each main, for each 1/3-octave band from 100 Hz to
     2 kHz, per position: the band-mean error before (smoothed response minus
     target, dB) and after adding the pass-1 RC correction at that band;
     gain = |before| − |after|; the band's gain is the position-weighted mean
     (`levels::weights`). The main's limit is the upper edge (`fc × 2^(1/6)`)
     of the last band in the run of positive-gain bands starting at 100 Hz
     (if the 100 Hz band is not positive, 100 Hz). The upper limit is the
     median over mains, clamped to [100, 2000]; pass 2 runs with it. Reuse the
     engine's existing smoothing, target and correction-response code rather
     than writing new versions; say which functions you used.
   - `Note::RcLimit` carries the basis, the factor and the evidence: strict
     `schroeder_hz`; lenient band centre and band T60 (and the strict value);
     agreement the number of positions. Keep the existing params so old
     screens still read `hz`, `auto`, `schroeder_hz`; add new params with new
     names (`basis`, `factor`, `band_hz`, `band_t60_s`, `positions`).
   - Defaults must leave every golden unchanged: run the golden tests.
4. **Service (`secs-service`).** Carry the new settings into the engine
   config; apply the new `SettingEdit`s; when the project has fewer than
   three positions and the basis is `agreement`, the calibration request
   fails with a typed input error whose params name the reason (the screen
   shows it). Tests for the edits and the error.
5. **Docs and registry.** `docs/DEVIATIONS.md` engine table: a new row (next
   free D-number) describing the three bases, the factor and the file
   compatibility (upstream ignores the keys and computes strict ×1), status
   `적용`, source `correction_engine.py schroeder_hz` and the upstream
   calibration screen's RC range. `features.toml`: register the bases, the
   factor and the new `SettingEdit`s pointing at real tests.
6. **Tests (engine).** Synthetic IRs with known decay per band (build
   exponentially decaying noise or a sum of decaying sinusoids per band) so
   that: an untreated room (flat T60) gives lenient ≈ strict; a treated room
   (long low-band T60, short mid-band T60) gives lenient > strict with the
   expected band; a room whose low-band T60 is shorter keeps lenient = strict;
   factors 2 and 4 multiply and clamp at 1000 and 2000; no volume gives 500
   without the factor; agreement picks the band where position errors stop
   improving on a constructed case, and refuses fewer than three positions.

## Files

You may edit: `crates/secs-types/src/{calibration.rs,settings_edit.rs,lib.rs,ipc.rs}`
(only what the types need), `crates/secs-project/src/**`,
`crates/secs-project/tests/**`, `crates/secs-engine/src/**`,
`crates/secs-engine/tests/**`, `crates/secs-service/src/**`,
`crates/secs-service/tests/**`, `apps/secs2r/ui/src/ipc/bindings/**`
(generated), `docs/DEVIATIONS.md`, `features.toml`.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-types -p secs-project -p secs-engine -p secs-service --locked
python .github/scripts/check_feature_tests.py
```

(`check_feature_tests.py` on Windows also reports `secs-sys::callback_native_sample_conversion`,
a non-Windows test; that line is expected. Any other line is yours to fix.)

## Report

The rules as built with the functions you reused, the note params, the
DEVIATIONS row, tests, files changed, and the commands' last lines.
