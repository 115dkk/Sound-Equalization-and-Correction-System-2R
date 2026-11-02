# P164 (ASTRA): audit #155, group W4: the Schroeder analysis (A2), engine tests, oracle cache

Follow the **common rules** in `docs/packets/P161-daybreak-audit155-deploy.md`.

Worktree `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\audit-155-w4`,
branch `claude/audit-155-w4`. Files: `crates/secs-engine/**`, `oracle/**`
(not `oracle/goldens/**` data), `docs/ENGINE.md`. Read ADR 0004, 0007, 0021,
0022, `docs/ENGINE.md`, `crates/secs-engine/src/memory.rs`, and how
`design.rs` reserves memory (near lines 247 to 285 and 332 to 346).

Floating-point order stays exactly as it is: parallel code uses map only and
accumulations keep their original order (ADR 0007). Existing goldens must stay
bit-identical.

## Design (fixed): the Schroeder analysis owns its working memory and cancel (A2)

The strict and lenient estimates in `levels.rs` each filter full IRs and build
four IR-length `f64` arrays, outside any reservation and without cancel checks.
Give the analysis one place that, before allocating, reserves the working
memory from the actual longest IR it will process (four arrays of that length
per concurrent task, through the existing memory permit type), and that checks
the cancel token between bands and measurements and every so many samples in
the long loops. Both estimates use it; band choices and aggregation rules stay.
The design's own budget no longer guesses this memory.

## Items (in this order)

| Item | Finding | Cat | Seam and what is fixed |
|---|---|---|---|
| A-13a | TD-107 | A | The analysis call with a small memory budget and a long synthetic IR: refused with the existing budget error before the large allocation; the same layout with a short IR passes. Commit as `A-13`, finding TD-107. |
| A-13b | TD-49 | A | The analysis call with a cancel token set after it starts (use a token that trips after N checks, like the existing cancel tests in `tests/cancel.rs`): returns `Cancelled` without finishing the loops. Commit as `A-13`, finding TD-49. |
| A-36 | TD-64 | A | `oracle/export_engine_xo.py` cache key: include the hashes of `oracle/engine_cases.py` and `oracle/export_engine_e2e.py` (or a digest of the generated inputs). Test in a new `oracle/test_export_engine_xo.py` (`unittest`, no network, no upstream run): changing only the input generator's bytes changes the key. |
| C-1 | TD-52 | C | `src/tests/hf_phase.rs` compares errors through the shared helper in `src/tests/mod.rs` (near lines 32 to 65), so a NaN fails. |
| C-8 | TD-142 | C | `docs/ENGINE.md` near line 45: describe `test_support` as `cfg(test)` only and point downstream crates and perf tools at `secs-engine-testkit`. |
| C-15 | Q8 | C | Check, without committing the check, that every recorded case's low-end protection RC filters have a positive DC sum; then at `src/rc.rs` near lines 457 to 463 write why the signed `sum(h) - 1` is enough (cite `audit #155 Q8` and ADR 0022). If any case has a non-positive sum, do not write the comment: report the case. |

Crates to gate: `secs-engine` (`cargo test -p secs-engine --locked` takes a few
minutes); `python -m unittest oracle/test_export_engine_xo.py` for A-36.
