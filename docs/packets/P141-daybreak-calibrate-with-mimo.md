# P141 (Daybreak): 보정 계산 한 번으로 MIMO까지 계산합니다

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\rc-upper`
(branch `claude/rc-upper`). Do not commit, do not push. Do not edit
`CHANGELOG.md` or anything under `apps/secs2r/ui/` except the generated
bindings. Start only after P139 (RC upper basis) is in the tree; it touches
the same `secs-types` and `secs-service` files.

## Why

A user report (2026-10-30): after pressing 보정 계산 they had to find and press
'MIMO 계산' separately, and expected many people to miss it. Today (as
upstream `wizard/step_calibration.py` `_mimo_card`/`compute_mimo`) MIMO is the
fifth card at the bottom of the settings: a 'MIMO 사용' toggle, then a
separate 'MIMO 계산' button that needs a calibration result first. The
maintainer wants MIMO chosen up front and computed by the same 보정 계산.

Read first: `CLAUDE.md`, `CONTEXT.md`, `docs/DEVIATIONS.md` (the 화면 table),
`crates/secs-types/src/ipc.rs` (`CalibrateParams`, `MimoParams`, `JobResult`,
`JobKind`), `crates/secs-service/src/calibration.rs` (`start_calibration`,
`start_mimo`: the MIMO preconditions, `compute_with_acceleration`, the commit
and `persist_cache` calls), `crates/secs-service/src/jobs.rs` (stages,
cancellation, groups), `crates/secs-api/src/lib.rs`, `apps/secs2r-cli/src/main.rs`
(the `calibrate` command).

## Design

1. **Types.** `CalibrateParams` gains `#[serde(default)] pub mimo: bool`
   ("after the correction, compute MIMO in the same job"). Default false, so
   API and CLI callers keep today's behaviour. Regenerate bindings with
   `cargo test -p secs-types`.
2. **Service.** In `start_calibration`, when `params.mimo` is true and
   `subs_only` is false:
   - Refuse up front (before the job starts) with the same typed errors
     `start_mimo` uses when MIMO cannot run: fewer than three positions,
     `mimo_on` false in the effective settings, a GPU preference whose GPU is
     unavailable. Reuse the checks; do not write new copies.
   - In the job: run and **commit** the calibration exactly as today, then run
     MIMO on the committed design with the same code `start_mimo` uses (move
     the shared body into one function both call; keep `start_mimo` working
     for the separate 'MIMO 계산' button, which stays for MIMO-only changes),
     then commit the MIMO result and persist its cache.
   - Progress: the MIMO stages follow the calibration stages in the same job.
   - Cancel during MIMO: the calibration result stays committed, no MIMO
     result is committed, the job ends cancelled.
   - MIMO failure after the calibration commit: the calibration stays; the
     job fails with the MIMO error, and the error params carry
     `stage: "mimo"` so the screen can show it in the MIMO card.
   - `JobResult` for this job stays `Calibrate` (add a field only if the
     screen cannot learn that MIMO ran from the results summary; say which).
3. **CLI and API.** `secs2r-cli calibrate` gains `--mimo`; `secs-api` passes
   the field through. No other public API change.
4. **Docs and registry.** `docs/DEVIATIONS.md` 화면 table: a new row (next
   free D-number): upstream needs a separate 'MIMO 계산' after the
   correction; here 'MIMO 사용' chosen before 보정 계산 computes both in one
   job, the separate button stays for MIMO-only changes; status `적용`; source
   `wizard/step_calibration.py _mimo_card, compute_mimo`. `features.toml`:
   register `ipc.calibrate` mimo and the CLI flag pointing at real tests.

## Tests (`secs-service`)

- `calibrate` with `mimo: true` on a three-position project commits both
  results; the MIMO result is not stale and matches a separate
  `calibrate` + `mimo` run on the same inputs (same summary numbers).
- Fewer than three positions or `mimo_on` false: typed refusal, no job.
- Cancel during the MIMO stage keeps the new calibration and no MIMO result.
- A MIMO failure keeps the calibration and reports `stage: "mimo"`.
- `mimo: false` and a missing field behave as today (an existing test
  passing unchanged is enough for the second).

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-types -p secs-service -p secs-api -p secs2r-cli --locked
python .github/scripts/check_feature_tests.py
```

## Report

The shared function and what moved, the error codes and params, tests, the
DEVIATIONS row, files changed, and the commands' last lines.
