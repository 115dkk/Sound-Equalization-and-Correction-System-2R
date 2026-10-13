# P12d (Daybreak): `secs-export`, the staged and before/after mdat exports

Worktree: `/home/user/wt/p12d` (branch `m2/p12d`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents. Build
with `export CARGO_TARGET_DIR=/home/user/wt/target-p12d` (yours alone).
Another worker (P12b, XO in `secs-engine`) runs at the same time: do not
touch `crates/secs-engine/src/{xo,preview,design,chain}.rs`.

Read first: `docs/ARCHITECTURE.md` (crate boundaries), `docs/ENGINE.md`,
`docs/adr/0004-parity-and-deviations.md`, `oracle/README.md`,
`crates/secs-mdat` (its `builder::MeasurementInput` and `build`, which write
REW mdat files byte-identically to upstream's builder), `crates/secs-camilla`
(`pyfmt`: Python's `format`, `:g`, `:.Nf`, `repr`), `crates/secs-engine`
(`design::Design`, `chain::stage_responses`, `measurement::long_spectra`),
`crates/secs-types::calibration`.

## Goal

The two REW exports a user makes after a calibration, from a `Design` and
the measurements: the staged mdat (`SECS 보정 설계 <모드>.mdat`, the
response at every chain stage) and the before/after mdat (`SECS 보정 전후
<모드>.mdat`). SECS 2's file names and notes are file content; port them
exactly (Korean included) so a REW user sees the same file. MIMO items come
in M3: implement the `mimo_run is None` path and leave a typed hook.

Upstream `src/secs/calibration.py`: `mode_label` 427–434 and
`CalibrationSettings.label` 298–300 (if not yet in `secs-types`; P12c ported
`mode_params` but not these), `rc_pieces` 613–628, `_chain_text` 998–1003,
`LAYOUT_PLACE_TEXT` 1006, `sub_mode_text` 1009–1019, `band_type_text` 1022–1024,
`sub_filter_text` 1048–1053, `sub_value_text` 1055–1061, `sub_values_text`
1063–1066, `layout_lines` 1069–1079, `stage_items` 1082–1133 (without the
MIMO branch), `project_spl_offset` 1292–1296, `export_mdat` 1299–1305,
`before_after_items` 1308–1355 (without MIMO), `export_before_after_mdat`
1358–1364. Also the two warnings `sub_filter_warnings` 1031–1039 and
`sub_gain_warnings` 1042–1045 (`SUB_GAIN_WARN_DB` 1028), which the screen
shows: return them as typed values (role and numbers), not text.

## Contract

```rust
// crates/secs-types/src/calibration.rs (additions)
pub fn mode_label(mode: Mode, ultra_ms: Option<f64>) -> String;
impl CalibrationSettings { pub fn label(&self) -> String; }

// crates/secs-export (new crate: #![forbid(unsafe_code)], [lints] workspace = true,
// depends on secs-engine, secs-mdat, secs-camilla (pyfmt), secs-types)
pub struct ExportRun<'a> {       // what upstream's CalibrationRun gave these functions
    pub design: &'a Design,
    pub measurements: &'a MeasurementSet,
    pub config: &'a EngineConfig,
    pub settings: &'a CalibrationSettings,
}
pub fn rc_pieces(run: &ExportRun<'_>) -> Vec<String>;
pub fn layout_lines(run: &ExportRun<'_>) -> Vec<String>;
pub fn sub_values_text(run: &ExportRun<'_>) -> String;
pub fn stage_items(run: &ExportRun<'_>, label: Option<&str>) -> Vec<MeasurementInput>;
pub fn before_after_items(run: &ExportRun<'_>) -> Vec<MeasurementInput>;
pub fn project_spl_offset(measurement_records: &[…]) -> Option<f64>; // from the .secs project
pub fn export_mdat(run: &ExportRun<'_>, folder: &Path, spl_offset_db: Option<f64>) -> Result<PathBuf, ExportError>;
pub fn export_before_after_mdat(run: &ExportRun<'_>, folder: &Path, spl_offset_db: Option<f64>) -> Result<PathBuf, ExportError>;
pub enum SubWarning { NarrowBand { role: String, hp_hz: f64, lp_hz: f64 },
                      HighGain { role: String, gain_db: f64 } }
pub fn sub_filter_warnings(settings: &CalibrationSettings, roles: &[String]) -> Vec<SubWarning>;
pub fn sub_gain_warnings(design: &Design) -> Vec<SubWarning>;
```

If `secs-engine` does not expose something these need (the arrival times
`engine.arrival`, `latency_ms`, the layout info, the settings label), add
the smallest public accessor in `crates/secs-engine/src/lib.rs` or a new
`crates/secs-engine/src/export_view.rs` (yours), not in the files P12b
holds, and list it. The IR of a stage is `irfft(H, N_FFT)` cast to float32,
as upstream; keep that order (float64 irfft, then cast).

Write the files atomically (temporary file in the same folder, then rename)
and check everything before writing, as `secs-camilla` does.

## Goldens (`oracle/export_engine_export.py`, family `export_mdat`, budget 4 MB)

For `dolby`, `room_dba`, `room_manual`, `room_filters` (replayed designs
from `engine_design`, referenced by case name): the item list of
`stage_items` and `before_after_items` (names, notes, sample rates, t0
exactly; IRs sampled: the first 4096 samples, every 97th after, and a
SHA-256 of the float32 bytes), `rc_pieces`, `layout_lines`,
`sub_values_text`, the two warnings, `mode_label` for every mode and a few
`ultra_ms`, and the bytes of both mdat files built with
`spl_offset_db=None` and with an offset (store the SHA-256 and size; the
writer itself is already gated in `format.mdat_build`).

## Tests (`crates/secs-export/tests/golden.rs`)

On the replayed designs (build them the way `secs-engine`'s design test
does; if that needs the test-only replay fitter, add a small
`#[doc(hidden)] pub` test-support function behind a `test-support` cargo
feature of `secs-engine` and enable it only in `secs-export`'s
dev-dependencies):

- exact: item names, notes, sample rates, t0, every text helper, warnings,
  `mode_label`;
- IRs within the measured tolerance (`MEASURE`) of the sampled values; the
  float32 SHA-256 equal on the oracle libm where the IR's float64 values
  match within 1 ulp of float32 rounding (report the others);
- the mdat bytes: equal SHA-256 where every IR digest matched, else parse
  the file back with `secs-mdat` and compare structure and names.

Register `export.staged_mdat` and `export.before_after_mdat` as
`implemented` with these tests.

## Files you may change

- `crates/secs-export/**` (new), `Cargo.toml` (workspace member line),
  `Cargo.lock` as cargo rewrites it
- `crates/secs-types/src/calibration.rs` (`mode_label`, `label`) and its
  generated bindings if the types change (they should not)
- `crates/secs-engine/src/lib.rs` (pub accessor lines only),
  `crates/secs-engine/src/export_view.rs` (new), `crates/secs-engine/Cargo.toml`
  (the `test-support` feature)
- `oracle/export_engine_export.py`, `oracle/goldens/export_mdat/**`,
  `oracle/README.md` (family row), `features.toml` (the two entries),
  `docs/THIRD_PARTY.md` only if you bring code from elsewhere

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p12d
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p12d
<venv>/bin/python oracle/run.py export_mdat
du -sh --apparent-size oracle/goldens/export_mdat
cargo fmt --all -- --check
cargo clippy -p secs-export -p secs-engine -p secs-types --all-targets --locked -- -D warnings
cargo test -p secs-export -p secs-types -p secs-policy 2>&1 | grep -E "MEASURE|test result|FAILED|panicked"
SECS_ORACLE_FOREIGN_LIBM=1 cargo test -p secs-export 2>&1 | grep -E "test result|FAILED"
cargo deny --all-features check
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-export
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

`<venv>` = `/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

## Report

Files changed; golden size; verification output; the public accessors you
added to `secs-engine`; anything not matched and why.
