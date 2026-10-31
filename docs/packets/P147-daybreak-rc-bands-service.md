# P147 (Daybreak): extra correction bands in the service, project file and CLI (ADR 0024)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\rc-bands`
(branch `claude/rc-bands`). Do not commit, do not push. ASTRA works on
`crates/secs-engine/**` at the same time (P146); do not touch it.

Read first: `CLAUDE.md`, `docs/adr/0024-rc-extra-bands.md`,
`crates/secs-types/src/{calibration.rs,settings_edit.rs,rules.rs,ipc.rs}`
(`RcBand`, `CalibrationSettings.rc_bands`, `SettingEdit::RcBands`,
`CALIBRATION_LIMITS.rc_lo/rc_band/rc_bands_max`, `RC_LO_READ`,
`ResultCurvesResult.rc_bands_hz` are already defined; do not change
secs-types), then `crates/secs-service/src/{settings_edit.rs,curves.rs,calibration.rs}`,
`crates/secs-project/src/calibration.rs`, `apps/secs2r-cli/src/main.rs`.

## What to build (fixed)

1. **Edit validation** (`crates/secs-service/src/settings_edit.rs`). Reject,
   never clamp, with `ErrorCode::InvalidRequest` and string params, before
   the session lock like the `RcHiTransition` check:
   - `RcLo { hz }` outside `CALIBRATION_LIMITS.rc_lo` (20–200):
     `field=rc_lo, reason=rc_lo_range, min=20, max=200`.
   - `RcLo { hz }` at or above a manual `rc_hi`: `field=rc_lo,
     reason=rc_lo_above_hi, hi_hz=<value>` (needs the stored settings; check
     after reading them, before storing).
   - `RcHi { hz: Some(v) }` outside `CALIBRATION_LIMITS.rc_hi`:
     `field=rc_hi, reason=rc_hi_range, min=100, max=20000`; at or below
     `rc_lo`: `field=rc_hi, reason=rc_hi_below_lo, lo_hz=<value>`.
   - `RcBands { bands }`: more than `rc_bands_max.1` (4) bands:
     `field=rc_bands, reason=rc_band_count, max=4`; a band with an edge
     outside `CALIBRATION_LIMITS.rc_band` (20–20000) or `lo_hz >= hi_hz`
     (or a non-finite value): `field=rc_bands, reason=rc_band_range,
     index=<1-based>, lo_hz=<v>, hi_hz=<v>`. Valid lists are stored sorted
     by `lo_hz`; an empty list clears them.
   Numbers in params use the existing formatting helpers of the service.
2. **Project file** (`crates/secs-project/src/calibration.rs`):
   - Read `rc_lo` with `RC_LO_READ` (10–200), not `CALIBRATION_LIMITS.rc_lo`,
     so a stored 10–19 Hz still loads unchanged.
   - `rc_bands` is written as `[[lo, hi], ...]` only when non-empty, and is
     an optional key in `CALIBRATION_SHAPE` (like `rc_hi_basis`), so a merge
     cannot bring back a removed list. On read, drop bands that break the
     rule (edges in 20–20000, lo < hi, finite), sort, keep the first 4.
   - Round-trip and legacy tests like
     `rc_upper_round_trip_and_legacy_unknown_defaults`.
3. **Graph** (`crates/secs-service/src/curves.rs`, where `rc_band_hz` is
   filled for SPL and GD): fill `rc_bands_hz` with the extra bands of the
   same settings the band comes from (the run's settings when a run exists,
   else the stored settings), sorted; empty otherwise.
4. **Notice.** The engine (P146) adds `Note::RcBands { bands }` (the merged
   band set) and a design field with the same set. Map the note to a notice
   `rc_bands` with param `bands` = `"lo-hi,lo-hi"` (numbers formatted like
   the `rc_limit` params) wherever `rc_limit` is mapped
   (`crates/secs-service/src/calibration.rs`, `scalar_notice`). If P146 has
   not landed when you reach this step, write the mapping against the name
   `Note::RcBands { bands: Vec<(f64, f64)> }` and say so in the report.
5. **Export text and CamillaDSP note.** Where the RC upper limit is written
   (`crates/secs-export/src/lib.rs` ~240-287 and ~398-404,
   `crates/secs-camilla/src/note.rs` ~89-95, the cache label
   `CachedRunLabels` and its fallback in `export_run.rs`), add one line
   listing the extra bands (`RC 추가 구간 15000–20000 Hz` style, follow the
   existing Korean and English wording of the RC line) only when there are
   extra bands. Nothing changes when there are none.
6. **CLI** (`apps/secs2r-cli/src/main.rs` `project set`): `--rc-bands
   15000-20000[,LO-HI...]` sends `SettingEdit::RcBands`; `--rc-bands none`
   sends an empty list. Parse errors are CLI errors; range errors come back
   from the service.
7. Fix every `CalibrationSettings { .. }` literal outside the engine
   (`crates/secs-project/tests/calibration.rs` and others) with
   `rc_bands: Vec::new()`; make the whole workspace except secs-engine
   compile.
8. `features.toml`: `engine.rc_bands` (tests from P146; leave
   `status = "planned"` if those tests do not exist yet and say so),
   `calibration.edit_rc_bands`, `cli.rc_bands`, pointing at real tests.
   `docs/DEVIATIONS.md`: add D48 (format like D46/D47): extra bands
   (`rc_bands`, written only when present, upstream ignores and computes the
   primary band only) and the 20 Hz edit floor on `rc_lo` (upstream accepts
   10–200; files are read with 10–200 unchanged).

## Tests

- Service: every rejection above with its params; a valid list is stored
  sorted; an empty list clears; `engine_key` changes only when bands exist;
  the edit marks results stale.
- Project: round trip with bands, file without the key, invalid stored bands
  dropped, more than 4 trimmed, stored `rc_lo = 15` reads as 15.
- Curves: `rc_bands_hz` filled from run settings and from stored settings.
- CLI: `project set --rc-bands` stores the list; `none` clears it.

## You may edit

`crates/secs-service/**`, `crates/secs-project/**`, `crates/secs-export/**`,
`crates/secs-camilla/**`, `apps/secs2r-cli/**`, `features.toml`,
`docs/DEVIATIONS.md`. Not secs-types, not secs-engine, not the screen.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --exclude secs-engine --all-targets --locked -- -D warnings
cargo test -p secs-service -p secs-project -p secs-export -p secs-camilla -p secs2r-cli --locked
```

(If secs-engine does not compile yet because P146 is mid-way, say so and
run what you can.)

## Report

Files changed, each rejection with its params, the notice and graph fields,
the export lines (both languages), the CLI usage, tests added, and the
commands' last lines.
