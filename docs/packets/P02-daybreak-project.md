# P02 (Daybreak Blue): role vocabulary, secs-project (.secs and .irs)

Worktree: `/home/user/wt/p02` (branch `m1/p02`). Work only there. Do not
commit; Claude reviews the diff and commits.

## Goal

Port upstream `src/secs/model.py` (roles, speaker configuration, DSP profile,
target curve, the `.secs` project file with its migrations) and the `.irs`
measurement store of `src/secs/rewapi/session.py`, with oracle goldens that
prove SECS 2R reads every `.secs` SECS 2 writes and writes the same bytes.

No clean room: read the upstream Python freely; this is an official port
(GPL-3.0). Keep upstream behaviour exactly.

## Files you may change

- `crates/secs-types/src/role.rs` (new) and one line `pub mod role;` in
  `crates/secs-types/src/lib.rs`. Nothing else in secs-types.
- `crates/secs-project/**`
- `oracle/export_project.py` (new), `oracle/goldens/project/**` (generated)
- `features.toml`: only `format.secs_project`, `format.irs_store`,
  `format.role_vocabulary` (status/tests/note)
- `oracle/README.md`: add rows for the `project` family to the Families table

Do not touch any other file. Other workers own `crates/secs-jos`,
`crates/secs-mdat`, `crates/secs-dsp`, `oracle/export_mdat.py`,
`oracle/export_dsp.py`. Never edit the upstream tree (`src/`, `tests/`,
`tools/`, …): CI checks it is byte-identical to the pinned commit.

Available workspace deps (`x.workspace = true`): `serde`, `serde_json`
(with `preserve_order` and `float_roundtrip`), `thiserror`; path deps
`secs-types = { path = "../secs-types" }`; for tests `secs-oracle = { path =
"../secs-oracle" }`. If you need another crate, put it in your crate's
Cargo.toml with an exact version and say so in the report.

## Contracts

Read first: `CLAUDE.md`, `docs/ARCHITECTURE.md` §2–4, `docs/adr/0002`,
`docs/adr/0004` (the "파일 호환" bullet), `oracle/README.md`.

- Crate roots keep `#![forbid(unsafe_code)]` and
  `#![deny(unsafe_op_in_unsafe_fn)]`; Cargo.toml keeps `[lints] workspace = true`.
- Files are untrusted input: loading never panics; it returns
  `ProjectError` (thiserror) for unreadable JSON, wrong types, bad WAV.
- No `HashMap`/`HashSet` iteration may affect output bytes or order.

### secs-types::role (port the role section of model.py)

Roles stay strings: a project may hold custom roles and bi-amp band roles
like `L:LF`. The module gives the known vocabulary and the pure helpers:

```rust
pub const ROLES: [&str; 27];                  // model.ROLES, same order
pub const SUB_ROLES: [&str; 8];
pub const TOP_GROUP: &str = "탑";             // and HEIGHT_GROUP, LEGACY_TOP_GROUP
pub enum RoleKind { Main, Sub, Top, Height }  // serde snake_case
pub struct RoleInfo { pub label: &'static str, pub kind: RoleKind, pub group: &'static str }
pub fn role_info(role: &str) -> RoleInfo;     // CHINFO, CHINFO_DEFAULT for unknown
pub fn role_label(role: &str) -> &'static str;
pub fn role_kind(role: &str) -> RoleKind;
pub fn role_group(role: &str) -> &'static str;   // "sub" for subwoofers, as upstream
pub fn role_code(role: &str) -> &str;             // ROLE_CODES or the role itself
pub fn role_order(role: &str) -> usize;           // index of the base before ':', else 27
pub fn role_position(role: &str) -> Option<(f64, f64)>;   // ROLE_POSITIONS
pub fn role_from_title(title: &str, roles: Option<&[&str]>) -> Option<&'static str>;
```

Group names are the Korean strings upstream uses: they are `.secs` keys and
must stay byte-identical. `role_from_title` must reproduce Python's
`str.upper()`/`str.isalnum()`/`str.split()`/`str.isdigit()` on the oracle's
title cases (Unicode aware; test Korean and full-width digits). Derive
`Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize` where sensible;
no ts-rs export yet.

### secs-project

Port the rest of `model.py`: `Band`, `Speaker`, `default_bands`,
`SpeakerConfig` (presets, `roles`, `sub_count`, `used_outs`,
`next_free_out`, `groups`, `set_sub_count`, `renumber`, `validate`),
`ordered_speakers`, `DspProfile` (presets, `allocate_taps`, `fir_used`),
`TargetCurve` (presets, `at`, `sample`, `sort`), `snap_focus`,
`rename_keys`, `migrate_group_keys`, `migrate_calibration_keys`, `Project`
(`to_dict`/`from_dict` with `_migrate_legacy`, `bass_path_count`,
`apply_recommended_taps`, `validate`, `from_preset`), and from
`rewapi/session.py` the `.irs` store: path `<project>.irs/<role>_P<pos>.wav`,
the float32 mono WAV writer and the readers (`_write_wav_float32`,
`_read_wav_float32`, `_read_mono_float_wav`; for the soundfile fallback,
read PCM 16/24/32 and float 32/64 WAV including WAVE_FORMAT_EXTENSIBLE, and
document what soundfile does that you do not).

```rust
impl Project {
    pub fn from_value(v: serde_json::Value) -> Result<Project, ProjectError>; // from_dict + migrations
    pub fn to_value(&self) -> serde_json::Value;                              // to_dict, key order as upstream
    pub fn load_text(text: &str) -> Result<Project, ProjectError>;            // Project.load
    pub fn save_text(&self) -> String;                                       // Project.save
}
```

`save_text` must equal Python's
`json.dumps(project.to_dict(), ensure_ascii=False, indent=2) + '\n'` byte for
byte. That needs your own serializer: Python's float `repr` (shortest round
trip, but `1e-05`, `1e+16`, `100000.0`, `-0.0`), ints kept as ints, `NaN`,
`Infinity`, `-Infinity` (Python's default `allow_nan=True`), Python's string
escaping with `ensure_ascii=False`, empty `[]`/`{}`, and `indent=2`
separators. `load_text` must read what `json.loads` reads from such files,
including `NaN`/`Infinity`. Keep unknown top-level keys (`extra`) and
pass-through dicts (`bc`, `mso`, `verify`, `tesira`, `measurements`,
`snapshots`) as ordered JSON values. Out of scope: the pickle cache
`<project>.irs/calibration_cache.pkl` (SECS 2R keeps its own cache later).

## Goldens (`oracle/export_project.py`, family `project`)

Use `oracle/common.py` (`Family`, `f64`, `blob`, `save`, `finish`). Run with
`<venv>/bin/python oracle/run.py project`, `<venv>` =
`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.
Cases, at least:

1. `roles`: every helper over every known role, custom roles, `L:LF`, and
   `role_from_title` over 150+ titles (aliases, codes, numbered, Korean,
   full-width, near misses like `DBA L`, `L EQ`), with and without `roles`.
2. `preset_*`: `Project.from_preset` for every preset with variations
   (sub counts 0–8, bi-amp on, custom role, targets, DSP presets), each with
   `to_dict` and the exact `save` text as a blob.
3. `migrate_*`: hand-built version 1 and 2 dicts that exercise every branch
   of `_migrate_legacy`, `migrate_group_keys`, `migrate_calibration_keys`,
   unknown top-level keys, missing keys: input text blob → `to_dict` and save
   text blob after `load`.
4. `taps_*`: `allocate_taps`/`fir_used`/`apply_recommended_taps` over
   configurations and phases.
5. `target_*`: `TargetCurve.at`/`sample` on a log grid for every preset and
   edited curves.
6. `json_*`: Python `json.dumps(…, ensure_ascii=False, indent=2)` of tricky
   values (floats across repr thresholds, -0.0, NaN, ±inf, big ints, control
   characters, quotes, Korean, emoji) as blobs.
7. `wav_*`: `_write_wav_float32` output blobs for several lengths and rates,
   and the arrays the readers return for those blobs.

## Tests and budgets

Everything here is exact: values equal, bytes equal. Register the tests in
`features.toml`; mark the three features `implemented` only when they pass.

## Verification (run in the foreground; finish before you report)

```sh
cd /home/user/wt/p02
<venv>/bin/python oracle/run.py project
cargo fmt --all -- --check
cargo clippy -p secs-types -p secs-project --all-targets -- -D warnings
cargo test -p secs-types -p secs-project -p secs-oracle -p secs-policy
python3 .github/scripts/upstream.py check-tree
```

Do not end your turn while a command is still running.

## Report

Files changed; the exact output of the verification commands (test counts);
dependencies added; anything you could not match and why; upstream
behaviours you found questionable (do not "fix" them, list them).
