# P27 (ASTRA): the curves and tables the graphs draw

Worktree: `/home/user/wt/p27` (branch `m5/p27`, from `m5/plan`). Work only
there. Do not commit; Claude reviews the diff and commits. Do not spawn
sub-agents. Build with
`export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p27`
(yours alone). The oracle venv is
`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.
P26 (config and settings methods) and P28 (host, monitor, export folders)
work in parallel in `secs-service`: put your code in new files
`crates/secs-service/src/curves*.rs` and do not edit theirs. Add your
request variants to `secs_types::ipc` in one contiguous block at the end
of `Request`/`Response` so the three branches merge cleanly.

Read first: `docs/M5.md` (§3 rows 측정 곡선, 결과 곡선, 서브(BA)),
`docs/M5-inventory.md` §4 (every graph: curves, ranges, smoothing,
offsets) and §5 (the BA card and its actions), `docs/adr/0003-*`,
`crates/secs-service/src/{lib.rs,ipc.rs,calibration.rs}` (how results and
their staleness are held), `crates/secs-engine/src/{preview.rs,export_view.rs,
design.rs}` (`XoPreview`, `sub_combine`, `subs_only`, the display spectra),
`crates/secs-export/src/lib.rs` (`sub_filter_warnings`, `sub_gain_warnings`,
`sub_values_text`, `rc_pieces`), `crates/secs-types` (`CalibrationSettings`,
`take_sub_values` if ported), and upstream `src/secs/wizard/step5_measure.py`
(`magnitude_db`, `FREQS`, `_octave_smoothing_sigma`, `:501-515`),
`src/secs/wizard/step_calibration.py` (`_draw_*`, `_level_offset`
`:2144-2401`, the metrics and filters lines `:2283-2301`, the BA lines
`:2340-2371`), `src/secs/calibration.py` (`spl_db`, `gd_ms`, `xo_preview`,
`sub_combine`, `take_sub_values`, `sub_filter_warnings`,
`sub_gain_warnings`, `DISPLAY_FRAC`, `SPL_GRID`, `GD_GRID`).

## Goal

The screen draws every upstream graph and table from data the service
returns, and computes nothing itself: curves on their grids, already
smoothed and shifted to the target reference, the RC band lines, the XO
preview for any group and frequency, the BA table and warnings, and the
summary lines as values.

## Methods (types in `secs_types::ipc`, one `///` line per item)

- `measurement_curves { positions?: [i64], roles?: [String] }`: for each
  (role, position) in the project, upstream `magnitude_db` (FFT size,
  log-grid interpolation on `FREQS`, Gaussian smoothing at 1/12 octave
  with `mode='nearest'`, 200–2000 Hz mean set to 0 dB) on the 280-point
  `FREQS` grid. Returns the grid once and one curve per pair, with the
  channel's kind (main, sub, top) so the screen can apply the line rules.
  Large projects: compute in parallel per curve (map only, ADR 0007).
- `result_curves { tab: spl | gd | xo | ba, channel?, group?, xo_hz?, corrected? }`
  from the current calibration result (and the MIMO result when it is
  fresh). Matches upstream:
  - SPL: before, after, target, MIMO applied, all shifted by
    `_level_offset` so the target sits at 0 dB; the RC lower and upper
    frequencies; before any result, the position-1 measurement and the
    target.
  - GD: before and after on `GD_GRID`, with the 1st–99th percentile range
    upstream uses for the axis.
  - XO: `xo_preview(run, group, hz, corrected, role)` curves (target, main,
    sub, sum), for a dragged frequency as well as the configured one.
  - BA: per sub before and after, sub sum before and after, target, from
    `sub_combine(sub_run or run)`.
  - Each response also carries the axis ranges upstream sets for that tab,
    so the screen does not hard-code them.
- `result_lines`: the values behind the metrics line (±1 dB share 20–200 Hz
  before and after, GD spread 20–100 Hz before and after), the filters line
  (PEQ count, FIR taps or none, XO Hz, MIMO support count), total delay,
  and the engine notes as `Notice`s; the BA flatness and seat spread, the
  BA table rows (sub, mode, gain, delay, polarity, all-pass), the fixed
  filters and layouts, and `sub_filter_warnings` and `sub_gain_warnings`
  as `Notice`s. Values, not sentences.
- `calibrate { subs_only: true }`: the subs-only run upstream's [Compute
  subs only] does. It keeps a separate subs-only result that the BA tab
  shows, as upstream, and is cleared when a full calibration finishes
  (`step_calibration.py:1873-1882`). Uses the calculation job group.
- `sub_take_values`: upstream `settings.take_sub_values(run)` (writes the
  result values into the per-sub manual settings and stores them).

## Proof

- Oracle family `service_curves` (`oracle/export_service_curves.py`, under
  4 MiB; store curves as f64 blobs as other families do): upstream
  `magnitude_db` on the `dolby` and `room_dba` measurements; for the
  `dolby` and `room_dba` cases, the SPL, GD, XO (configured and two dragged
  frequencies) and BA curves with `_level_offset`, the metrics and filters
  values, the BA table, the warnings; `take_sub_values` before and after.
  Extract the screen functions without Qt, as P20 did for `step4_layout.py`.
  Tolerances: the ADR 0004 rule for curves (they come from the design,
  which is already statistically gated); exact for counts, codes and table
  values.
- A subs-only run equals the engine's `subs_only` design run directly on
  the same inputs.
- Staleness: curves for a stale result say so (`stale: true`) and are
  still returned.

`features.toml`: `ipc.<method>` for every method, `service.curves` (M5)
as `implemented` with its tests.

## Files you may change

- `crates/secs-service/src/curves*.rs`, the two lines in `lib.rs`/`ipc.rs`
  that declare and dispatch them, `crates/secs-service/tests/curves*.rs`
- `crates/secs-engine/src/**` and `crates/secs-dsp/src/**` only to expose
  what is already computed or to port a missing display function (say
  which)
- `crates/secs-types/src/ipc.rs` (your block), `apps/secs2r/ui/src/ipc/bindings/**`
  (regenerated)
- `oracle/export_service_curves.py`, `oracle/goldens/service_curves/**`
- `features.toml`, `Cargo.lock` as cargo writes it

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p27
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p27
cargo fmt --all -- --check
cargo clippy -p secs-service -p secs-engine -p secs-types --all-targets --locked -- -D warnings
cargo test -p secs-service --test curves --locked 2>&1 | grep -E "test result|FAILED|panicked"
cargo test -p secs-engine -p secs-types --locked 2>&1 | grep -E "test result|FAILED|panicked"
cargo test -p secs-policy --locked 2>&1 | grep -E "test result|FAILED"
(cd apps/secs2r/ui && npm ci --no-audit --no-fund && npm run typecheck && npm test)
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-service -p secs-types
python3 .github/scripts/upstream.py check-tree
```

Name your test target file `crates/secs-service/tests/curves.rs`. Do not
run `check_feature_tests.py`.

## Report

The methods as built, the oracle family (size, cases, tolerances and the
largest differences), each test, and anything you could not do.
