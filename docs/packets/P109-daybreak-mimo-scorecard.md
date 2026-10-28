# P109 (Daybreak): the scorecard after MIMO, and late energy per position

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\mimo-scorecard`
(branch `claude/mimo-scorecard`, based on the API branch). Work only there.
Do not commit, do not push, do not spawn sub-agents. Claude reviews the diff
and commits; the screen comes in a later packet.

Read first: `CLAUDE.md`, `CONTEXT.md`, `crates/secs-engine/src/scorecard.rs`
(`metrics`, `Metrics`), `crates/secs-engine/src/decay.rs` (`late_db_20_150`),
`crates/secs-engine/src/design.rs` (where `design.scores` come from: which
response, which position, which target and normalisation),
`crates/secs-engine/src/mimo/` (the run, `MimoRun`, how support paths are
applied), `crates/secs-export/src/lib.rs` (`before_after_items_with_mimo`:
how the before/after mdat builds the `보정 후 + MIMO` response at the main
position), `crates/secs-service/src/calibration.rs` (`summarize`),
`crates/secs-service/src/results.rs`, `crates/secs-service/src/cache.rs`
(how a MIMO result is cached and restored), `crates/secs-types/src/ipc.rs`
(`CalibrationSummary`, `ChannelScore`, `MetricsSummary`, `MimoSummary`),
`apps/secs2r-cli/src/main.rs` (its JSON output of results),
`docs/perf/analytic-derivatives.md` §5 (the late-energy metric as the Dirac
ART comparison uses it).

## Why

A tester comparing settings found that the scorecard is computed by the
calibration and MIMO runs afterwards, so a MIMO variant shows the same
numbers as the low-latency variant; the effect of MIMO is only visible in the
mdat's `보정 후 + MIMO` items. The comparison with Dirac ART uses
`late_db_20_150` (20–150 Hz late energy, 150–400 ms after arrival against the
first 50 ms) per position and main channel.

## Rules (fixed)

1. **Calibration side.** `CalibrationSummary.late_energy: Vec<LateEnergy>`
   with `LateEnergy { role, position, before_db, after_db }` for every main
   role at every measured position, computed by `decay::late_db_20_150` on
   the same responses the scorecard reads before and after correction (at
   the main position the very responses of `design.scores`; at other
   positions the same construction applied to that position's measurement).
   Order: positions ascending, roles in chain order.
2. **MIMO side.** `MimoSummary.scorecard: Vec<MimoChannelScore>` with
   `MimoChannelScore { role, after_mimo: MetricsSummary }`, the 17 metrics of
   `scorecard::metrics` on the main position's response with the MIMO support
   contributions added, built exactly as the `보정 후 + MIMO` mdat item is
   built (reuse that code; move it to `secs-engine` or `secs-export` where
   both callers can reach it rather than copying), with the same target and
   normalisation the calibration's `after` metrics use. And
   `MimoSummary.late_energy: Vec<MimoLateEnergy { role, position,
   after_mimo_db }>` for every main role and position.
3. **Staleness and cache.** These values belong to the result they come
   from: a stale MIMO result shows its numbers with its stale flag. A MIMO
   result restored from the cache carries them (store them in the cache,
   raising the cache version as the cache code expects, and read old caches
   without them as empty lists, never as an error). Same for the
   calibration's `late_energy`.
4. **Determinism.** Same inputs give the same bits at 1, the default and 7
   threads (ADR 0007); compute in the existing order.
5. **CLI.** The results JSON the CLI prints carries the new fields.
6. **Registry.** `features.toml`: a new entry `results.mimo_scorecard`
   (kind of the neighbouring results entries, status `implemented`, your
   tests, a note that it is new and not upstream).
7. **Bindings.** Regenerate with `cargo test -p secs-types` and keep them.
   The screen will show the new fields later; it must still typecheck now,
   so add the new fields to the screen's fixtures and mock
   (`apps/secs2r/ui/src/ipc/fixtures.ts`, `mock.ts`) with empty lists, and
   nothing else under `apps/secs2r/ui/`.

## Tests (new, all must pass)

- `secs-engine` or `secs-service`: for an oracle project with MIMO, the
  `after_mimo` metrics differ from `after` for at least one role and equal
  the metrics of the mdat's `보정 후 + MIMO` response (same function, same
  bits); `late_energy` has one row per main role and position; with MIMO the
  main position's `after_mimo_db` is lower than `after_db` for the roles
  MIMO supports (it is what MIMO minimises).
- `secs-service`: a cached MIMO result restored after reopening carries the
  same `scorecard` and `late_energy`; an old cache without them restores with
  empty lists.
- Determinism across thread counts for the new fields.

## Files

You may edit: `crates/secs-engine/**`, `crates/secs-export/**`,
`crates/secs-service/**`, `crates/secs-types/src/ipc.rs` and the regenerated
`apps/secs2r/ui/src/ipc/bindings/**`, `apps/secs2r/ui/src/ipc/fixtures.ts`,
`apps/secs2r/ui/src/ipc/mock.ts`, `apps/secs2r-cli/src/**`, `features.toml`,
`Cargo.lock`.

Do not touch: other screen files, the upstream tree, `.github/`, `docs/`,
crate manifests' publish metadata.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python .github/scripts/check_feature_tests.py
cd apps/secs2r/ui && npm ci && npm run typecheck && npm test
```

## Report

Changed files; where the `보정 후 + MIMO` construction lives now; the new
IPC fields; for the oracle MIMO case, a small table of `after` vs
`after_mimo` (`in1_20`, `gd_20`) and `late_db_20_150` before, after and after
MIMO at the main position; the tail of each verification command; anything
left undone.
