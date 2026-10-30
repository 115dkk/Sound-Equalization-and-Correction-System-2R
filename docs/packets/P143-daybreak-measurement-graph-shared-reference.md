# P143 (Daybreak): 측정 그래프의 모든 곡선을 한 기준에 맞춥니다 (#127)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\rc-upper`
(branch `claude/rc-upper`). Do not commit, do not push. Do not edit
`CHANGELOG.md`, the upstream tree (`src/`, `tests/`, `tools/`), or anything
under `apps/secs2r/ui/` except generated bindings and `ipc/recorded/`.

## The defect (#127)

Users see the subwoofer curve 30 to 35 dB above the mains in the measurement
graph, in SECS 2 and in SECS 2R, although they matched levels. Cause (found
2026-10-30, verified on upstream code with synthetic IRs):
`display::magnitude_db` (`crates/secs-engine/src/display.rs`, port of
`src/secs/wizard/step5_measure.py:501-515`) subtracts each curve's **own**
mean over 200–2000 Hz, and `measurement_curves`
(`crates/secs-service/src/curves.rs`) calls it per measurement. A sub is far
down in 200–2000 Hz, so subtracting its own midrange lifts its bass by
`20·n·log10(632/fc)` dB (second-order 100 Hz low-pass: about 32 dB). A scalar
gain on any channel also cancels, so the graph cannot show level
differences. The correction uses the IRs, not these arrays, so results are
not affected.

## Design

1. `secs-engine::display`: keep `magnitude_db` as it is (the upstream golden
   tests use it). Split it so the smoothed dB curve without the midrange
   subtraction is available (`smoothed_db(samples, rate) -> Vec<f64>`) and the
   reference is a separate function (`midrange_reference(curve) -> f64`, the
   same 200–2000 Hz mean). `magnitude_db` becomes `smoothed_db` minus its own
   `midrange_reference`, bit for bit as today.
2. `secs-service` `measurement_curves`: compute every curve with
   `smoothed_db`, then subtract **one** reference from all of them:
   - the 200–2000 Hz mean of the first main role in configuration order
     (`project_groups` order, not a sub) measured at the main position
     (position 1); if that is not measured, the same role at the lowest
     measured position; if no main is measured anywhere (a sub-only
     project), the mean over 40–120 Hz of the first sub at the lowest
     measured position.
   - choose the reference from the project's stored measurements **before**
     the request's `positions`/`roles` filters, so filtering or toggling a
     curve never moves the others.
   - the reference role and position go into the result so the screen can
     say what 0 dB means: add `reference: Option<CurveReference>` to
     `MeasurementCurvesResult` in `secs-types` (`role`, `position`,
     `band: "mid" | "sub"`); regenerate bindings with `cargo test -p secs-types`.
3. Keep the axes as they are unless a test shows the shared reference puts
   typical curves outside `[-35, 20]`; if so, widen the lower bound only and
   say why.
4. `docs/DEVIATIONS.md` 화면 table: next free U-number, status `적용`, source
   `wizard/step5_measure.py magnitude_db` and its callers (1155-1159,
   1508-1518): upstream normalizes each curve to its own midrange, which
   lifts subwoofers by tens of dB and hides level differences; here every
   curve shares the reference above. `features.toml`: register it with the
   tests below.

## Tests

- `secs-engine`: `magnitude_db` still equals the upstream golden (existing
  tests pass unchanged); `smoothed_db - midrange_reference == magnitude_db`.
- `secs-service` `tests/curves.rs`: a flat main and a unity-gain
  second-order 100 Hz low-passed sub (synthetic IRs) show the sub's passband
  near 0 dB, not near +32 dB; a −10 dB scaled copy of the main shows −10 dB;
  filtering the request to the sub alone returns the same sub values;
  a sub-only project uses the 40–120 Hz reference; the result names the
  reference. The existing `service_curves` goldens that encode the upstream
  per-curve normalization stay as upstream references for `magnitude_db`;
  where a service-level golden now differs, change that test to compare
  against the upstream function explicitly and cite the deviation, rather
  than regenerating the oracle files.
- Recorded IPC copies: regenerate with
  `SECS_RECORD=1 cargo test -p secs-service --test recorded` if they differ.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-types -p secs-engine -p secs-service --locked
python .github/scripts/check_feature_tests.py
```

(`check_feature_tests.py` on Windows also reports `secs-sys::callback_native_sample_conversion`;
that line is expected.)

## Report

The functions added, the reference rule as built, the new result field, the
tests and goldens touched (and why), the DEVIATIONS row, files changed, and
the commands' last lines. List what the screen should show for the new
`reference` field; Claude routes that to the screen.
