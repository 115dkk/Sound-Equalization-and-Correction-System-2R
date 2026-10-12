# P01 (ASTRA): secs-jos and secs-mdat — REW .mdat without REW

Worktree: `/home/user/wt/p01` (branch `m1/p01`). Work only there. Do not
commit; Claude reviews the diff and commits.

## Goal

Port upstream `src/secs/rew_mdat/` (Java Object Serialization reader/writer,
REW schema adapter, builder) and the IR part of `src/secs/mdat_loader.py` into
two crates, with oracle goldens proving byte-identical behaviour.

- `crates/secs-jos`: Java Object Serialization (JOS) stream graph. Parse,
  write back byte-identically, edit. Knows nothing about REW.
- `crates/secs-mdat`: REW `.mdat` on top of secs-jos: `load` (adapt to the REW
  schema like `rew/adapters.py`), `build` (clone the single-measurement
  template like `builder.py`), and the IR extraction of `mdat_loader.py`
  (`load_mdat_project` without `suggest_assignment`; role guessing moves to the
  service in M4 because roles live in secs-types/secs-project).

No clean room: read the upstream Python freely; this is an official port
(GPL-3.0). Keep upstream's behaviour, including its error classes and limits.

## Files you may change

- `crates/secs-jos/**`, `crates/secs-mdat/**` (Cargo.toml, src, tests, benches)
- `oracle/export_mdat.py` (new), `oracle/goldens/mdat/**` (generated)
- `features.toml`: only the entries `format.jos_round_trip`, `format.mdat_read`,
  `format.mdat_build` (set status/tests/note)
- `oracle/README.md`: add rows for the `mdat` family to the Families table

Do not touch any other file. Other workers own `crates/secs-dsp`,
`crates/secs-types`, `crates/secs-project`, `oracle/export_dsp.py`,
`oracle/export_project.py`. Never edit the upstream tree (`src/`, `tests/`,
`tools/`, …): CI checks it is byte-identical to the pinned commit.

Workspace dependencies already available (`<dep>.workspace = true`):
`serde`, `serde_json`, `thiserror`, `secs-dsp` (path dep: add
`secs-dsp = { path = "../secs-dsp" }`), and for tests `secs-oracle = { path =
"../secs-oracle" }`. secs-dsp gives `secs_dsp::fft::{rfft, irfft, Complex64}`
(NumPy `rfft`/`irfft` semantics); do not edit secs-dsp. If you need another
crate, add it to your crate's Cargo.toml with an exact version and say so in
the report.

## Contracts

Read first: `CLAUDE.md`, `docs/ARCHITECTURE.md` §2–6, `docs/adr/0002`,
`docs/adr/0004`, `docs/adr/0007`, `oracle/README.md`.

- Every crate root keeps `#![forbid(unsafe_code)]` and
  `#![deny(unsafe_op_in_unsafe_fn)]`; Cargo.toml keeps `[lints] workspace = true`.
- Inputs are untrusted files. `parse`/`load` must never panic, overflow, or
  allocate beyond `Limits` on any input; they return errors. Recursion depth
  is bounded by `Limits::max_depth`. No `unwrap`/`expect`/indexing that can
  fail on input-derived values in parse paths.
- No `HashMap`/`HashSet` iteration may influence output bytes or order.

### secs-jos

Port `serialization/{constants,nodes,reader,parser,writer,modified_utf8}.py`,
`limits.py`, `errors.py`.

```rust
pub struct Limits { /* the eight ParseLimits fields, same names and defaults */ }
impl Default for Limits { … }

#[derive(Debug, thiserror::Error)]
pub enum JosError {            // errors.py: StreamFormatError family
    Format { message: String, offset: Option<u64> },
    Unsupported { message: String, offset: Option<u64> },
    InvalidHandle { message: String, offset: Option<u64> },
    LimitExceeded { message: String, offset: Option<u64> },
}

pub fn parse(bytes: &[u8], limits: &Limits) -> Result<Stream, JosError>;
pub fn write(stream: &Stream) -> Vec<u8>;   // writer.dumps
pub fn decode_modified_utf8(bytes: &[u8]) -> Result<String, JosError>;
pub fn encode_modified_utf8(s: &str) -> Vec<u8>;
```

`Stream` holds the top-level values, `bytes_consumed`, `handle_count`, and an
arena of nodes (class descriptors, objects, arrays, strings, enums, classes,
block data) addressed by ids, so that shared references survive a round trip
exactly as Python's object identity does (the writer emits `TC_REFERENCE`
for a node it already wrote, and field-signature strings keep their own
identity: see `FieldDescriptor.type_string_node`). Typed primitive arrays keep
their element type (`[B`, `[S`, `[I`, `[J`, `[F`, `[D`, `[Z`, `[C`) and
values. Provide what secs-mdat needs to read and edit: field lookup through
the class hierarchy (`JavaObject.get`, derived first), setting a field value
in its owning class's slot, replacing a primitive array's data, deep-cloning
a subgraph into new nodes (the builder's `copy.deepcopy`). Design the
internals; keep the public surface small and documented.

### secs-mdat

Port `models.py`, `rew/adapters.py`, `builder.py`, `__init__.py` (`load`,
`loads`), and from `mdat_loader.py` the IR extraction (`MeasurementInfo`
fields and the `MdatIRProvider` data: per measurement id the IR as f64 with
`REW_MDAT_IR_SCALE` undone and its sample rate).

```rust
pub const TEMPLATE: &[u8] = include_bytes!("../../../src/secs/rew_mdat/template_1.mdat");
pub const REW_MDAT_IR_SCALE: f64 = 0.01;

pub enum MdatError { Stream(secs_jos::JosError), Schema(String), Input(String) }
pub fn load(bytes: &[u8], limits: &secs_jos::Limits) -> Result<MdatFile, MdatError>;

pub struct MeasurementInput { pub title: String, pub notes: String,
    pub sample_rate_hz: u32, pub samples: Vec<f64>, pub start_time_s: f64,
    pub spl_offset_db: Option<f64> }
/// `uuid` is called once per measurement, in order (upstream: uuid.uuid4().int).
pub fn build(measurements: &[MeasurementInput], template: &[u8],
    spl_offset_db: Option<f64>, uuid: &mut dyn FnMut() -> u128)
    -> Result<Vec<u8>, MdatError>;
```

`MdatFile`/`Measurement`/`FrequencyResponse`/`ImpulseResponse`/
`DistortionDataset` mirror `models.py` (f64 for float arrays; keep `meanings`
and unknown-field names). The builder's float32 cache arrays
(`splValues`, `rawValues`, `phaseValues`, `unwPhaseValues`,
`rawUnwPhaseValues`, `gdValues`, `rawGDValues`, `fdwReal`, `fdwImag`) come
from `_frequency_data` (rfft, `np.interp`, `np.unwrap`, `np.gradient`):
port them with NumPy's exact formulas and order of operations.

## Goldens (`oracle/export_mdat.py`, family `mdat`)

Use `oracle/common.py` (`Family`, `f64`, `blob`, `save`, `finish`). Run with
`<venv>/bin/python oracle/run.py mdat` where `<venv>` is
`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.
The exporter imports `secs.rew_mdat` from `src/` (common.py puts it on the
path). Cases, at least:

1. `template`: the template's adapted `MdatFile` (every scalar, every array
   as sidecars, unknown field names, `handle_count`, `bytes_consumed`) and the
   fact that `dumps(parse(template)) == template`.
2. `build_*`: four or more builds with different counts (1, 3, 14), sample
   rates (44100, 48000, 96000), IR sources (the 14 real IRs in
   `src/secs/rewapi/sample_dolby_714.npz`, plus synthetic decays), start
   times, `spl_offset_db` given/absent, and titles/notes that exercise
   modified UTF-8 (Korean, NUL, characters outside the BMP). Make
   `uuid.uuid4` deterministic by patching it in the exporter with a fixed
   sequence and record the sequence as an input. Outputs: the built bytes as a
   blob and the adapted `MdatFile` of those bytes.
3. `utf8_*`: modified-UTF-8 encode/decode cases.
4. `error_*`: malformed inputs (truncations at several offsets, bad magic,
   bad version, dangling handle, a limit exceeded with small `ParseLimits`):
   the exception class name, message and offset.

## Tests and budgets

- `secs-jos`: `write(parse(template)) == template`; `write(parse(x)) == x`
  for every built golden; modified-UTF-8 cases; error cases give the same
  error kind and offset as the oracle; a deterministic mutation test (a fixed
  LCG flips/truncates/inserts bytes of the template, a few thousand
  variants, each must return `Ok` or `Err` without panicking; mark it
  `#[cfg_attr(miri, ignore)]`). Small unit tests must not touch the file
  system (they run under Miri in M1-4).
- `secs-mdat`: the adapted template equals the golden (integers exactly,
  floats bit-equal: they are stored values); each build equals the golden
  blob byte for byte except the nine float32 cache arrays above, whose
  elements may differ by at most 1 float32 ulp (print `MEASURE` with the
  count of differing elements per array); the IR array and every other byte
  identical. Loading each built golden gives back the inputs (IR × 100 in
  f32 precision, title, notes, rate, start time).
- Register the tests in `features.toml` and mark the three features
  `implemented` only when their tests pass.

## Verification (run in the foreground; finish before you report)

```sh
cd /home/user/wt/p01
<venv>/bin/python oracle/run.py mdat
cargo fmt --all -- --check
cargo clippy -p secs-jos -p secs-mdat --all-targets -- -D warnings
cargo test -p secs-jos -p secs-mdat -p secs-oracle -p secs-policy
python3 .github/scripts/upstream.py check-tree
```

Do not end your turn while a command is still running.

## Report

Files changed; the exact output of the verification commands (test counts and
every MEASURE line); dependencies added; anything you could not match and why;
upstream behaviours you found questionable (do not "fix" them, list them).
