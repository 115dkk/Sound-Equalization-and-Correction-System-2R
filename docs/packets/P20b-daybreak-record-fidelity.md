# P20b (Daybreak): keep every byte of `.secs` measurement entries

Continue in `/home/user/wt/p20` (branch `m4/p20`, your P20 work is in the tree,
uncommitted). Same rules as P20: do not commit, no sub-agents,
`export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p20`.

## What review found

`MeasurementStore` reads `project.measurements` into `MeasurementRecord`
structs and writes them back from the structs. Upstream
(`src/secs/rewapi/session.py`) edits the entry dicts in place. Three things
are lost on a save:

1. Keys the struct does not know (anything upstream or a later SECS 2 adds)
   are dropped, and known keys are re-ordered.
2. `records_from_project` uses `filter_map`: an entry whose `pos` does not
   parse disappears from the file. Upstream leaves such entries alone.
3. Keys upstream never writes (`snr_db`, and `source` from `record`) appear.

A user who opens a SECS 2 project in SECS 2R and measures one channel must
get back the file SECS 2 would have written.

## Required behaviour

Keep `project.measurements` as the only state (no second copy of the
entries). Read through typed views when convenient, but every change is an
in-place edit of the entry's `PyJson` object, as upstream does:

- `start_position(pos)`: replace the entry with that `pos` by a new entry
  with upstream's `_new_entry` keys in its order, or append one.
- `record(result)`: find the entry by `pos` (an entry whose `pos` does not
  parse is skipped, never removed); if found, apply `_repair_entry`
  (`pos` set to the integer, `label`, `mdat`, `done`, `sample_rate` set only
  if missing, `uuid`/`ir`/`spl_db`/`t0` replaced by `{}` only when not an
  object); else append `_new_entry`. Then set `uuid[role]`, `ir[role]`,
  `spl_db[role]`, `t0[role]` (an existing role key keeps its place; a new
  one goes last), `spl_offset[role]` when an offset is given (the
  `spl_offset` object is appended at the end of the entry if missing, as
  `setdefault` does), `sample_rate`, and `done` exactly as upstream.
- `clear_position` as upstream's step 5 does (read
  `src/secs/wizard/step5_measure.py` `clear_position`).
- Do not persist `snr_db`. Keep SNR on the returned record and in memory for
  the session; `source` is written only by the mdat import path (P23), so
  `record` must not write it.

## Proof (new oracle cases in `service_plan`, bytes compared)

Extend `oracle/export_service_plan.py` with a `session` group: for a few
starting `.secs` files (one from the `project` family, one with an entry
carrying extra keys and odd types (`uuid` as a list, a string `pos`, an
entry with no `pos`), one empty), run a scripted sequence through upstream
`MeasurementSession` with `auto_save=True` and synthetic `MeasurementResult`s
(two positions, start/record/re-measure, one with an SPL offset), and record
the `.secs` text after each step. The Rust test replays the same sequence
through `MeasurementStore` and compares the saved text byte for byte after
every step. Keep the existing tests passing.

## Verification (foreground; finish before you report)

```sh
cd /home/user/wt/p20
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p20
cargo fmt --all -- --check
cargo clippy -p secs-project --all-targets --locked -- -D warnings
cargo test -p secs-project --locked 2>&1 | grep -E "test result|FAILED|panicked"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-project
python3 .github/scripts/upstream.py check-tree
```

## Report

What changed, the new oracle cases and their size, the byte comparison
results, anything you could not do.
