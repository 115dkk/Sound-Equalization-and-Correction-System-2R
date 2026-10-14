# P17b (Daybreak): MIMO in the CamillaDSP and mdat exports

Worktree: `/home/user/wt/p17b` (branch `m3/p17b`). Work only there. Do not
commit; Claude reviews the diff and commits. Do not spawn sub-agents. Build
with `export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p17b`
(yours alone). The oracle venv is
`/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/oracle-venv`.

Read first: `docs/MIMO.md`, the CamillaDSP writer `crates/secs-camilla/src/lib.rs`
(P11) and its golden tests, the mdat export `crates/secs-export/src/lib.rs`
(P12d; note the `MimoExport` / `MimoResponses` placeholder and
`ExportError::MimoNotImplemented`), the MIMO path types in
`crates/secs-types/src/chain.rs` (`MimoPaths`, `MimoPath`, `MimoFilter`,
already there; do not change them), and upstream
`src/secs/export_camilladsp.py` (`mimo_paths`, every `mimo`/`paths` branch of `make_config`: `mimo_fork`,
the split and bus pass-through, the path filters, `mimo_merge`),
`src/secs/calibration.py` (`export_camilladsp`'s MIMO note, `stage_items`
stage 4, `before_after_items` "_3 보정 후 + MIMO"), and the tests
`tests/test_calibration_mimo_fir_1012.py`,
`tests/test_export_camilladsp_mimo_delay.py`,
`tests/test_calibration_r27_sub_manual.py` (its CamillaDSP MIMO branch).

P17a (ASTRA) ports the engine side (`mimo::run::compute`, `fir_to_48k`, the
applied responses) in parallel. You do not call it: your inputs are
`MimoPaths` and per-role applied responses, which your tests read from
your own oracle fixtures. FIR paths arrive with their 48 kHz taps
(`MimoFilter::Fir { taps_48k }`, already `fir_to_48k` of the low-rate
support), so the writer stores them as they are and never resamples.

## Interface (fixed)

- `secs-camilla`: `make_config`, `write_config` and `export` take an extra
  `mimo: Option<&MimoPaths>`. `None` must produce today's bytes exactly
  (the existing goldens stay unchanged). With paths: the `mimo_fork` mixer
  (one extra channel per path copying the raw input before input
  correction), the path channels passed through `split` and `bus`, the path
  filters (IIR: gain with inversion for negative polarity, integer-sample
  delay of path delay plus `MimoPath.delay_ms`, PEQs, all-passes, LR4 HP at
  `low_hz`, LR4 LP at `fsiso`, then the support speaker's fixed filters;
  FIR: delay, LR4 HP, `Conv` of the wav named as upstream's
  `fir_file(f'mimo_{i}_{j}')` with `taps_48k`, then the fixed filters), and
  `mimo_merge` adding each path to its support speaker's output. An empty
  path list writes what `None` writes (upstream: no paths, no mixers). FIR wav files are written like the other FIR
  files (same writer, temp-then-rename). Keep upstream's names, order,
  rounding and number formatting exactly; the configuration text is
  compared byte for byte.
- `secs-export`: replace the placeholder with
  `pub struct MimoResponses { pub kind: MimoKind /* Iir | Fir */, pub fsiso: f64,
  pub cap_text: String, pub paths: MimoPaths, pub applied: Vec<(String, Vec<C>)> }`
  (applied: per chain role, upstream `MimoRun.applied`, the design-grid
  response at the main position; only roles with paths get MIMO items).
  `stage_items_with_mimo` and `before_after_items_with_mimo` add upstream's
  stage-4 and "_3 보정 후 + MIMO(kind)" items with upstream's titles and
  notes; remove `MimoNotImplemented` once nothing returns it.
- The CamillaDSP head-comment note "MIMO <kind> support paths <n>" as
  upstream's `calibration.export_camilladsp` writes it.

## Oracle family `export_mimo`

`oracle/export_export_mimo.py`: on `room_dba` and `room_filters`, IIR and
FIR modes, run upstream `compute_mimo` from the recorded design (replay
`engine_design`'s fits, as `oracle/export_engine_mimo_iir.py` does) on the
reference settings only (the IIR fit runs once per case), then record: the `MimoPaths` values (as JSON your test can read
into the Rust type; FIR taps as `fir_to_48k(s, fs_low)`), the applied responses, the full CamillaDSP YAML and
every FIR wav's float32 samples (digest plus a sample set, as the existing
camilla family does), and the mdat stage and before/after items (titles,
notes, IR digests and samples). Under 8 MiB.

## Tests

- `crates/secs-camilla/tests/`: YAML byte-equal to upstream for the four
  case/mode combinations; FIR wav samples equal; `None` unchanged (existing
  goldens); branch delay = `round((3 + align_SW1 + main_extra)·48)`
  (`test_export_camilladsp_mimo_delay.py`); a filtered sub's branch ends with
  that sub's fixed filters; FIR mode has no `_peq0` filters and its wavs have
  8000 frames.
- `crates/secs-export/tests/`: the MIMO items equal upstream (titles and
  notes exactly; IR samples to the existing tolerance), on both libm paths
  (off the oracle libm report what follows BA, as the existing export tests
  do).

`features.toml`: set `export.camilla_mimo` (add it if absent, M3) and
`export.mimo_mdat` to `implemented` with your tests.

## Files you may change

- `crates/secs-camilla/src/**`, `crates/secs-camilla/tests/**`
- `crates/secs-export/src/**`, `crates/secs-export/tests/**`,
  `crates/secs-export/examples/export_perf.rs` (keep it compiling)
- `oracle/export_export_mimo.py` (new), `oracle/goldens/export_mimo/**`
- `features.toml` (the entries above), `Cargo.lock` only as cargo rewrites it

Do not edit `crates/secs-types` or `crates/secs-engine` (P17a owns them).

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p17b
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p17b
cargo fmt --all -- --check
cargo clippy -p secs-camilla -p secs-export --all-targets --locked -- -D warnings
cargo test -p secs-camilla -p secs-export --locked 2>&1 | grep -E "test result|FAILED|panicked"
SECS_ORACLE_FOREIGN_LIBM=1 cargo test -p secs-export --locked 2>&1 | grep -E "test result|FAILED"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-camilla -p secs-export
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## Report

What you changed and where, the oracle family (cases, size), each test's
result, anything reported rather than asserted and why, anything you could
not do.
