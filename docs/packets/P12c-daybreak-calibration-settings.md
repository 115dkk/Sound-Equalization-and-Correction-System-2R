# P12c (Daybreak): calibration settings, their `.secs` form, and the engine inputs they resolve to

Worktree: `/home/user/wt/p12c` (branch `m2/p12c`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents.

Read first: `docs/ENGINE.md`, `crates/secs-engine/src/input.rs`
(`EngineConfig`, `SubManual`, `BundleLayout`, `Tuning`), the case set
(`oracle/engine_cases.py`, family `engine_inputs`, loader
`crates/secs-engine/src/tests/cases.rs`), `crates/secs-project` (how
`Project`, `PyJson`, `extra` and byte-identical saving work), and
`crates/secs-types/src/lib.rs` (IPC types derive `ts_rs::TS`; their
TypeScript bindings regenerate when this crate's tests run).

## Goal

Port the settings half of `src/secs/calibration.py` so the service (M4) can
read the user's calibration settings from a `.secs` project, write them back
the way SECS 2 does, and turn them plus the project into the engine's input
values exactly as `calibration.make_engine` does. Nothing here computes a
correction.

Upstream (`src/secs/calibration.py`): constants 35–97; `CalibrationSettings`
108–305 (fields, `from_project`, `store`, `engine_key`, `manual_key`,
`mimo_key` and every other method); `sub_entry` 306–351, `bundle_layout_entry`
353–361, `sub_has_filters` 363–366, `_xo_map` 371–383, `room_dims` 385–396,
`room_volume` 398–402, `ultra_params` 404–412, `mode_params` 414–416,
`mode_spec` 418–425, `mode_label` 427–434, `measured_positions` 436–447,
`measurement_signature` 449–460, `mimo_availability` 462–467, `engine_inputs`
469–498, `target_key` 501–503, `make_engine` 564–581 (only the mapping to
constructor arguments and the attributes it sets after construction; the
engine itself is not yours), `xo_groups` 639–645. Read `correction_engine.py`
`MODES` (lines 43–48) for the mode table.

## Contract

```rust
// crates/secs-types/src/calibration.rs (new; register `pub mod calibration;`)
// IPC-visible: derive Serialize, Deserialize, ts_rs::TS with #[ts(export)].
pub enum Mode { Zero, Mixed, Linear, Ultra }            // "zero" | "mixed" | "linear" | "ultra"
pub struct SubEntry { /* sub_entry()'s normalised fields, upstream names */ }
pub struct BundleLayoutEntry { /* kind: "values" | "dba" | "multi", rear_trim_ms */ }
pub struct CalibrationSettings { /* every upstream field, upstream names and
    defaults; maps (xo, xo_rec, subs_manual, bundle_layouts) as ordered
    Vec<(String, _)> or an ordered map type that serialises as an object */ }
impl Default for CalibrationSettings { /* upstream defaults */ }
impl CalibrationSettings {
    pub fn engine_key(&self) -> String;   // a stable fingerprint string with the same
    pub fn manual_key(&self) -> String;   // inputs and the same exclusions as upstream's
    pub fn mimo_key(&self) -> String;     // tuples (staleness detection, not parity)
}
pub fn mode_params(mode: Mode, ultra_ms: f64) -> ModeParams;  // taps, lift_ms, peq_bands,
                                                              // phase_frac, max_latency_ms
// mode_spec / mode_label: the screen will use keys, so port only what the
// engine or export needs (mode_label feeds the CamillaDSP file name via
// secs-camilla's RunLabels; check calibration.export_camilladsp).

// crates/secs-project: read and write the `.secs` form
pub fn calibration_from_project(project: &Project) -> CalibrationSettings;  // from_project
pub fn store_calibration(project: &mut Project, settings: &CalibrationSettings); // store

// crates/secs-engine/src/calibration.rs (new private module; register it in lib.rs
// with the same dead-code allowance as the other stages)
pub struct ProjectView<'a> {      // what make_engine reads from a project, without secs-project
    pub groups: &'a [(String, Vec<String>)],   // SpeakerConfig.groups(): group -> roles, in order
    pub project_xo: &'a [(String, f64)],
    pub target: &'a TargetCurve,
}
pub struct EngineInputs { pub measurements: MeasurementSet, pub config: EngineConfig,
                          pub notes: Vec<String> /* adapter notes, e.g. skipped biamp roles */ }
pub fn engine_inputs(view: &ProjectView<'_>, irs: &MeasurementSet, settings: &CalibrationSettings)
    -> Result<EngineInputs, EngineError>;   // engine_inputs + make_engine's argument mapping
```

`from_project` clamps, drops non-finite values and wrong types exactly as
upstream; unknown keys in `extra['calibration']` are not carried (upstream's
`asdict` writes only its fields) but the rest of `extra` is untouched.
`store` writes `asdict(self)`: field order, value types (float vs int, null),
nested lists as upstream, so that `Project::save_text` produces the same
bytes SECS 2 does.

## Goldens (`oracle/export_calibration.py`, family `calibration`)

- `from_project` on a table of `extra['calibration']` inputs: defaults
  (missing, `None`, wrong type), every field at, below and above its range,
  non-finite numbers, wrong-typed sub entries and layouts, unknown modes,
  room dimensions (valid, partial, wrong), XO maps with junk keys; record the
  resulting `asdict` and the bytes of a project saved after `store`.
- `engine_key`, `manual_key`, `mimo_key` on a handful of settings, recorded as
  `repr()` so the Rust test can at least check equality classes: two
  settings that upstream keys equal must key equal in Rust, and vice versa
  (record pairs).
- `mode_params` for every mode and a range of `ultra_ms`.
- `engine_inputs` + `make_engine` mapping: for every case in `engine_cases`,
  the settings, the project view and the measurement layout, and the
  constructor arguments (`engine_inputs` already records them; reuse that
  fixture as the expected value instead of re-recording) plus the adapter
  notes. Add two adapter edge cases: a biamp role with `:` in its name, and
  roles measured only at a non-reference position.

## Tests

`crates/secs-project/tests/calibration.rs` (parse, store, saved bytes),
`crates/secs-types` unit tests for `mode_params` and the keys,
`crates/secs-engine/src/tests/calibration.rs` (mapping, using
`tests/cases.rs`; register it in `src/tests/mod.rs`). All exact (no float
arithmetic beyond what upstream does; `ultra_params` is a few products).
Run `cargo test -p secs-types` to regenerate the TypeScript bindings and
include them.

Register `engine.calibration_settings` (kind `"engine"`, milestone `"M2"`,
`upstream = "calibration.py CalibrationSettings, sub_entry, engine_inputs, make_engine"`)
as `implemented` with your tests.

## Files you may change

- `crates/secs-types/src/{lib,calibration}.rs`, `apps/secs2r/ui/src/ipc/bindings/**` (generated)
- `crates/secs-project/src/**` (new module plus its export), `crates/secs-project/tests/calibration.rs`
- `crates/secs-engine/src/calibration.rs`, `crates/secs-engine/src/lib.rs` (one module line),
  `crates/secs-engine/src/tests/{mod,calibration}.rs`
- `oracle/export_calibration.py`, `oracle/goldens/calibration/**`, `oracle/README.md` (family row)
- `features.toml` (the entry above), `Cargo.lock` only as cargo rewrites it

Others hold `crates/secs-engine/src/{fit,rc,ba}.rs` and their tests (P07b,
P09, P10). Do not touch them.

## Verification (foreground; finish before you report)

`<venv>` = `/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

```sh
cd /home/user/wt/p12c
export CARGO_INCREMENTAL=0
<venv>/bin/python oracle/run.py calibration
cargo fmt --all -- --check
cargo clippy -p secs-types -p secs-project -p secs-engine --all-targets --locked -- -D warnings
cargo test -p secs-types -p secs-project -p secs-engine -p secs-policy 2>&1 | grep -E "test result|FAILED|panicked"
git status --short apps/secs2r/ui/src/ipc/bindings
(cd apps/secs2r/ui && npm ci --no-audit --no-fund && npm run typecheck)
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-types -p secs-project -p secs-engine
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

`secs-project` reads untrusted files: also run
`cargo +nightly-2026-10-01 miri test --locked -p secs-project calibration` if
it finishes within ten minutes (report if it does not).

## Report

Files changed; verification output; the clamping and type rules you found
(a short table); anything not matched and why.
