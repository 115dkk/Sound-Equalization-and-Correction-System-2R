# P11 (Daybreak): CamillaDSP v4 writer over the signal-chain value

Worktree: `/home/user/wt/p11` (branch `m2/p11`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents.

Read first: `crates/secs-types/src/chain.rs` (the signal-chain value: your
only input besides options), `crates/secs-camilla/src/lib.rs` (crate brief),
`docs/ENGINE.md`, `oracle/README.md`, the case set P05 landed
(`oracle/engine_cases.py`).

## Goal

Port `src/secs/export_camilladsp.py` (whole file except the MIMO paths,
which M3 adds) and the wrapper `calibration.export_camilladsp`
(`src/secs/calibration.py:1366–1382`) so SECS 2R writes the same CamillaDSP
configuration SECS 2 writes, **byte for byte**, from a `SignalChain` instead
of engine internals.

Upstream facts you must reproduce (read the source for the rest):

- The YAML is hand-assembled lines whose values are `json.dumps(v,
  ensure_ascii=True)` with Python's default separators (`", "`, `": "`),
  Python float `repr` (shortest round trip, `80.0`, `1e-05`, `1e+16`), ints
  as ints, `true`/`false`, dict insertion order.
- Numbers pass through Python `round(x, 4)` / `round(x, 5)` / `round(low, 3)`
  (correctly rounded on the exact binary value, ties to even: CPython's
  `float.__round__`), `float(...)` where it is not rounded, and delays
  `int(round(ms * 48000 / 1000))` (ties to even).
- Head comments use Python format specs: `{:+g}`, `{:g}`, `{:.1f}`,
  `{:.3f}`, `{:+.2f}`, `{:+.3f}`, `{:3d}`, `{:30s}`; `note` drops non-ASCII
  characters; the file name keeps only ASCII alphanumerics and `-_.`.
- FIR WAVs: IEEE float32 mono, the exact header `write_wav` writes; the
  zero-FIR mode's `[1.0]` sentinel writes no Conv filter and no file.
- Relative paths from the config folder (`os.path.relpath` semantics, `/`
  separators); the file-device placeholders hold absolute paths of the
  output folder.
- Every check (ASCII, finite numbers, a sub with no send dropped from the bus
  mapping) runs before the first file is written; a failure writes nothing.

## Contract (`crates/secs-camilla/src/lib.rs` and modules you add)

```rust
/// Export options (upstream `make_config` / `write_config` keyword arguments).
pub struct Options {
    pub capture: Option<Device>, pub playback: Option<Device>,  // None = file placeholders
    pub chunksize: u32,                 // 1024
    pub note: String,
    pub fsiso: f64,                     // 150, used by MIMO paths (M3)
    pub lfe_gain_db: Option<f64>,       // Some(10.0); None = no LFE input
    pub lfe_lp_hz: f64,                 // 120
    pub latency_ms: Option<f64>,
    pub mode: String,
}
impl Default for Options { /* upstream defaults */ }
/// A device map as CamillaDSP takes it, keys in insertion order.
pub struct Device(pub Vec<(String, DeviceValue)>);
pub enum DeviceValue { Str(String), Int(i64), Float(f64), Bool(bool) }

/// The configuration text and the coefficient files, nothing written yet.
pub struct Config { pub yaml: String, pub coeffs: Vec<(PathBuf, Vec<u8>)> }
/// `make_config`: pure. `out` is the export folder, `config_dir` the folder
/// the YAML will live in, `fir_prefix` the coefficient file prefix.
pub fn make_config(chain: &SignalChain, out: &Path, config_dir: &Path, fir_prefix: &str,
                   options: &Options) -> Result<Config, CamillaError>;
/// `write_config`: writes `out/configs/<name>.yml` and `out/coeffs/<name>_*.wav`
/// (and `out/in.raw` for the file placeholder), returns the written paths
/// with the config first, as upstream.
pub fn write_config(chain: &SignalChain, out: &Path, name: &str, options: &Options)
    -> Result<Vec<PathBuf>, CamillaError>;
/// `calibration.export_camilladsp` without MIMO: the note and file name from
/// the run's mode, ultra budget, latency and RC upper bound.
pub fn export(chain: &SignalChain, folder: &Path, labels: &RunLabels) -> Result<Vec<PathBuf>, CamillaError>;
pub struct RunLabels { pub mode: String, pub ultra_ms: f64, pub latency_ms: f64, pub rc_hi_hz: f64 }
#[derive(Debug, thiserror::Error)] pub enum CamillaError { /* NonAscii, NonFinite, Io, … */ }
```

Python formatting: write one small module (`pyfmt.rs`) with `repr_f64`,
`round_ndigits` (CPython's algorithm: exact decimal of the binary value,
half-even), and the format specs above, each tested against the oracle on
edge values (ties, 0.5 ulp around ties, subnormals, 1e16 boundary, negative
zero). The `ryu` crate (already a workspace user via `secs-project`, pinned
`=1.0.20`) gives the shortest digits; Python's layout rules are yours to
apply. Do not depend on `secs-project` or `secs-engine`.

## Goldens (`oracle/export_camilla.py`, family `camilla`)

- Run `calibration.compute` on the cases `dolby` (mixed), `dolby_zero`
  (no FIR), `dolby_ultra` (long FIR), `dolby_infra` (sub fixed LP),
  `room_manual` (fixed and bundle labels), `room_dba` (DBA labels),
  `room_filters` (HP/LP/BW types, fixed PEQ), then
  `calibration.export_camilladsp` into a fixed temporary folder. Record the
  chain value extracted from the run in the `SignalChain` field names
  (`result.channels` → `mains` with `engine.top_peq` and
  `engine.align_delay_ms`; `result.subs` → `subs`; `engine.sub_bus_peq`;
  `engine.main_extra_ms`; `result.lift_ms`; `run.latency_ms`), the labels,
  the YAML text with the output folder replaced by a placeholder, and every
  WAV as a blob. FIR taps as f64 sidecars.
- Also call `make_config` directly on hand-built chains (duck-typed `eng` /
  `res` objects) for: no subs (no LFE, no bus), subs without any XO (empty
  bus sources dropped), `lfe_gain_db=None`, `lfe_gain_db=0`, custom
  capture/playback dicts, a non-ASCII note, a name with non-ASCII
  characters, negative-zero and tie-rounding values in PEQs and delays.
- `pyfmt`: `repr`, `round(x, n)` for n in 3, 4, 5 and the format specs on a
  list of edge values.

Budget `camilla` at most 6 MB.

## Tests (`crates/secs-camilla/tests/golden.rs`)

YAML text equal byte for byte (after substituting the placeholder with the
test's own folder); WAV bytes equal; written file list equal; `pyfmt` exact.
Error cases (non-finite gain, non-ASCII role) return the error and write no
file (check the folder stays empty).

Register `export.camilladsp` and `format.camilladsp_v4` as `implemented`
with your tests (they exist in `features.toml`; keep their ids).

## Files you may change

- `crates/secs-camilla/**`
- `oracle/export_camilla.py` (new), `oracle/goldens/camilla/**`,
  `oracle/README.md` (family row)
- `features.toml` (the two entries above)
- `Cargo.lock`, only as cargo rewrites it

Others hold `crates/secs-engine/**` and `crates/secs-types/**` (read-only for
you; if `SignalChain` lacks a field the export needs, report it instead of
changing it).

## Verification (foreground; finish before you report)

`<venv>` = `/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

```sh
cd /home/user/wt/p11
export CARGO_INCREMENTAL=0
<venv>/bin/python oracle/run.py camilla
du -sh oracle/goldens/camilla
cargo fmt --all -- --check
cargo clippy -p secs-camilla --all-targets --locked -- -D warnings
cargo clippy -p secs-camilla --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test -p secs-camilla -p secs-policy 2>&1 | grep -E "test result|FAILED|panicked"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-camilla
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## Report

Files changed; golden size; verification output; the Python formatting rules
you implemented and the edge values that needed care; anything not matched
and why.
