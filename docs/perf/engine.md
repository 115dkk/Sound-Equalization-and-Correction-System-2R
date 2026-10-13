# Engine performance audit (P13)

Status: **partial — M2 performance exit criterion is not established**. Timing is not a CI gate.

## Method and machine

- Linux 6.18.44-fc-v114, x86-64 KVM, Intel Xeon @ 2.10 GHz. `lscpu`: four cores/four logical CPUs, AVX2 and AVX-512; L1d 192 KiB, L1i 128 KiB, L2 8 MiB, L3 260 MiB. `free -g`: 15 GiB RAM, 14 GiB available, no swap.
- Upstream `60ac53ad3a3d`, unchanged. Python 3.13.16, NumPy 2.5.3, SciPy 1.18.1 in the supplied oracle venv; Rust release, `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p13`. No dependencies added.
- Shipped: default four-thread Rust Context; NumPy SIMD dispatch enabled, OpenBLAS default threads. Python recommendation uses upstream's own two-process pool and its own single-BLAS-thread worker initialization. One thread: Context(1), `OPENBLAS_NUM_THREADS=1`, `recommend_xo(workers=1)`. Neither configuration disables NumPy SIMD.
- A fresh process per operation/configuration, one warm-up, median of five timed calls. Whole-design mode variants and recommendation use three timed calls under the continuation's >10 s exception (five if the warm-up is at most 10 s). Stage sweeps retain five calls to measure their short stages. Fixture loading/imports/builds and initial designs for preview/export are outside the timer. Benchmarks run sequentially, not concurrently with each other or verification.
- Python uses `time.perf_counter` around the upstream call. Stage instrumentation inserts boundary timers into an in-memory source copy, never the protected upstream tree. Rust uses `Instant` in release examples. `ba` includes alignment, bus delay and fold, excluding top PEQ. `top` covers all roles; scores includes metrics and display curves.
- **Shipped per-role RC times overlap other channel tasks and are not compared.** The shipped table compares `rc_all` and `design`; only the one-thread table contains `rc:<role>` rows. Stage medians need not sum to the median total.
- Fit rows use identical recorded inputs: first/second RC calls and first bus/XO call from engine_fit (engine_design references when needed). GD reads the exact same recorded complex128 input. Neither golden values nor tolerances change.
- Preview: corrected front group at 80 Hz after cache creation. Cache row times construction alone. `sub_combine` is the uncached calculation; the Python result cache is cleared before, not inside, the timer.
- Exports include response construction, serialization and filesystem writes in disposable scratch directories, with warm filesystem caches and no added fsync. Both implementations' export setup uses production design (outside the timer). The original replay setup failed on room's known BA divergence; it was replaced, not relaxed, and every export table cell was remeasured. Shapes match, but coefficients can differ within the parity envelope: these are end-user export timings, not identical-byte serialization microbenchmarks.
- Seconds throughout; ratio = Rust/Python (<1 is faster). `not measured` is not zero and never a pass. `/usr/bin/time -v` captures process high-water RSS including setup/warm-up. Its process-pool figure is not the sum of simultaneously resident workers, so aggregate recommendation memory is not certified.
- `perf` is unavailable. Release harness profile operations time hot loops. The final binary selects three/five samples from the warm-up duration for design/recommend; every measured recommendation call exceeded 10 s. `uptime` is logged before each comparison. Rows within 20% require a quiet repeat; unresolved close rows are not certified. The continuation started at 15:21 UTC with a 16:51 UTC measurement-start cutoff. The last ad-hoc GD batch omitted that cutoff check: Rust one-thread GD started at 16:51:10 and Python at 16:52:18. The batch was stopped at 16:53; those late/incomplete samples are excluded. No measurements ran with the final verification.
- Core sweeps had no competing benchmark/build/test processes; one-minute load was about 1.0 in the one-thread sweep. Repeats were also sequential, with logged load 1.1–2.3 (including preceding work's moving average), not an independently enforced idle-host condition. Close repeats are conservatively left uncertified. Explicit paired repeats: room shipped cache/preview/bus, Dolby shipped cache/preview, Dolby one-thread Camilla, room one-thread Camilla/bus, room_dba shipped Camilla. Serial score stages, Dolby serial sub-combine and room_dba serial cache still need quiet repeats.

## Reproduction

```sh
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p13
cargo run -p secs-engine --release --example perf -- stages dolby
SECS_PERF_THREADS=1 cargo run -p secs-engine --release --example perf -- recommend room
<oracle-venv>/bin/python oracle/bench_engine.py stages dolby --config shipped
<oracle-venv>/bin/python oracle/bench_engine.py stages dolby --config one
SECS_PERF_OUTPUT=<scratch>/exports cargo run -p secs-export --release --example perf -- camilla dolby
# Same export example: mdat_staged, mdat_before_after, profile_camilla.
# Engine operations: design, stages, prepare, recommend, fit_rc1, fit_rc2,
# fit_bus, fit_xo, cache, preview, sub_combine, profile_fit, profile_sub.
# profile_fit takes optional kind (bus, rc, xo) after case; profile_scores times one role.
SECS_PERF_GD_INPUT=<scratch>/gd.bin <oracle-venv>/bin/python oracle/bench_engine.py record_gd dolby --config one
# With the same SECS_PERF_GD_INPUT, run gd in both implementations.
```

## shipped

| Operation | Case | Python median (s) | Rust median (s) | Rust/Python | Peak RSS Python/Rust (KiB) |
|---|---|---:|---:|---:|---:|
| design | dolby | 29.895521 | 4.129216 | 0.138 | 246632 / 210412 |
| prepare | dolby | 0.053609 | 0.037133 | 0.693 | — |
| ba | dolby | 8.383014 | 0.980556 | 0.117 | — |
| top | dolby | 2.235343 | 0.401433 | 0.180 | — |
| rc_all | dolby | 18.164491 | 2.497380 | 0.137 | — |
| scores | dolby | 0.666304 | 0.153749 | 0.231 | — |
| fit_rc1 | dolby | 1.011643 | 0.461180 | 0.456 | — |
| fit_rc2 | dolby | 0.216758 | 0.094441 | 0.436 | — |
| fit_bus | dolby | 0.092282 | 0.039475 | 0.428 | — |
| fit_xo | dolby | 0.080285 | 0.048252 | 0.601 | — |
| gd | dolby | 20.142817 | 11.297085 | 0.561 | — |
| cache | dolby | 0.035860 | 0.030322 | 0.846 | — |
| preview | dolby | 0.009860 | 0.008803 | 0.893 | — |
| sub_combine | dolby | 0.038105 | 0.029411 | 0.772 | — |
| camilla | dolby | 0.008564 | 0.004564 | 0.533 | — |
| mdat_staged | dolby | 1.216330 | 0.404708 | 0.333 | — |
| mdat_before_after | dolby | 0.642767 | 0.247633 | 0.385 | — |
| recommend | dolby | 58.512082 | 11.681475 | 0.200 | 252696 / 590108 |
| design | room | 26.266598 | 3.643901 | 0.139 | — |
| prepare | room | 0.077722 | 0.051770 | 0.666 | — |
| ba | room | 17.939877 | 1.927585 | 0.107 | — |
| top | room | 0.818526 | 0.245669 | 0.300 | — |
| rc_all | room | 6.891191 | 1.345708 | 0.195 | — |
| scores | room | 0.164065 | 0.058274 | 0.355 | — |
| fit_rc1 | room | 1.077930 | 0.491087 | 0.456 | — |
| fit_rc2 | room | 1.016932 | 0.541360 | 0.532 | — |
| fit_bus | room | 0.187937 | 0.199959 | 1.064 | — |
| fit_xo | room | 0.076792 | 0.035553 | 0.463 | — |
| gd | room | not measured | not measured | — | — |
| cache | room | 0.010918 | 0.009632 | 0.882 | — |
| preview | room | 0.008201 | 0.005247 | 0.640 | — |
| sub_combine | room | 0.061659 | 0.038833 | 0.630 | — |
| camilla | room | 0.003753 | 0.001528 | 0.407 | — |
| mdat_staged | room | 0.379243 | 0.155039 | 0.409 | — |
| mdat_before_after | room | 0.217508 | 0.131558 | 0.605 | — |
| recommend | room | not measured | not measured | — | — |
| design | room_dba | 10.209242 | 1.846075 | 0.181 | — |
| prepare | room_dba | 0.090257 | 0.055545 | 0.615 | — |
| ba | room_dba | 0.927096 | 0.241174 | 0.260 | — |
| top | room_dba | 0.777635 | 0.237540 | 0.305 | — |
| rc_all | room_dba | 7.872470 | 1.235612 | 0.157 | — |
| scores | room_dba | 0.188397 | 0.062490 | 0.332 | — |
| fit_rc1 | room_dba | 0.973885 | 0.523986 | 0.538 | — |
| fit_rc2 | room_dba | 1.139117 | 0.480372 | 0.422 | — |
| fit_bus | room_dba | 0.199957 | 0.080724 | 0.404 | — |
| fit_xo | room_dba | 0.079747 | 0.034336 | 0.431 | — |
| gd | room_dba | not measured | not measured | — | — |
| cache | room_dba | 0.017217 | 0.012572 | 0.730 | — |
| preview | room_dba | 0.010938 | 0.006577 | 0.601 | — |
| sub_combine | room_dba | 0.114583 | 0.079740 | 0.696 | — |
| camilla | room_dba | 0.002333 | 0.002696 | 1.155 | — |
| mdat_staged | room_dba | 0.453385 | 0.225683 | 0.498 | — |
| mdat_before_after | room_dba | 0.378186 | 0.249500 | 0.660 | — |
| design | dolby_zero | not measured | not measured | — | — |
| design | dolby_linear | not measured | not measured | — | — |
| design | dolby_ultra | not measured | not measured | — | — |
| .secs load | demo | not measured | not measured | — | — |
| .secs save | demo | not measured | not measured | — | — |

## one

| Operation | Case | Python median (s) | Rust median (s) | Rust/Python | Peak RSS Python/Rust (KiB) |
|---|---|---:|---:|---:|---:|
| design | dolby | 27.412777 | 10.359237 | 0.378 | 246040 / 123108 |
| prepare | dolby | 0.058729 | 0.026008 | 0.443 | — |
| ba | dolby | 8.888068 | 0.854276 | 0.096 | — |
| top | dolby | 1.942313 | 0.342953 | 0.177 | — |
| rc:L | dolby | 1.487257 | 1.031034 | 0.693 | — |
| rc:R | dolby | 1.569876 | 0.807085 | 0.514 | — |
| rc:C | dolby | 0.840015 | 0.388100 | 0.462 | — |
| rc:Ls | dolby | 1.333982 | 0.675272 | 0.506 | — |
| rc:Rs | dolby | 0.742298 | 0.342642 | 0.462 | — |
| rc:Lb | dolby | 1.972786 | 1.102659 | 0.559 | — |
| rc:Rb | dolby | 1.828025 | 1.051224 | 0.575 | — |
| rc:Tfl | dolby | 0.881007 | 0.437952 | 0.497 | — |
| rc:Tfr | dolby | 1.010530 | 0.527244 | 0.522 | — |
| rc:Tbl | dolby | 1.649963 | 0.930514 | 0.564 | — |
| rc:Tbr | dolby | 1.808236 | 0.988284 | 0.547 | — |
| rc_all | dolby | 15.147295 | 8.483867 | 0.560 | — |
| scores | dolby | 0.588181 | 0.571532 | 0.972 | — |
| fit_rc1 | dolby | 0.899156 | 0.439966 | 0.489 | — |
| fit_rc2 | dolby | 0.212720 | 0.083263 | 0.391 | — |
| fit_bus | dolby | 0.082652 | 0.040464 | 0.490 | — |
| fit_xo | dolby | 0.069626 | 0.032819 | 0.471 | — |
| gd | dolby | not measured | not measured | — | — |
| cache | dolby | 0.049859 | 0.029813 | 0.598 | — |
| preview | dolby | 0.010925 | 0.005449 | 0.499 | — |
| sub_combine | dolby | 0.030074 | 0.025893 | 0.861 | — |
| camilla | dolby | 0.006127 | 0.005241 | 0.855 | — |
| mdat_staged | dolby | 1.276160 | 0.427649 | 0.335 | — |
| mdat_before_after | dolby | 0.629122 | 0.224385 | 0.357 | — |
| recommend | dolby | not measured | not measured | — | not measured / not measured |
| design | room | 25.142993 | 5.454065 | 0.217 | — |
| prepare | room | 0.063775 | 0.037664 | 0.591 | — |
| ba | room | 17.932263 | 1.906149 | 0.106 | — |
| top | room | 0.747042 | 0.232238 | 0.311 | — |
| rc:L | room | 2.115531 | 1.041288 | 0.492 | — |
| rc:R | room | 1.981809 | 1.096555 | 0.553 | — |
| rc:C | room | 2.134696 | 1.112333 | 0.521 | — |
| rc_all | room | 6.363577 | 3.339318 | 0.525 | — |
| scores | room | 0.163806 | 0.129128 | 0.788 | — |
| fit_rc1 | room | 0.881151 | 0.462226 | 0.525 | — |
| fit_rc2 | room | 0.785805 | 0.469134 | 0.597 | — |
| fit_bus | room | 0.180711 | 0.189938 | 1.051 | — |
| fit_xo | room | 0.082340 | 0.037249 | 0.452 | — |
| gd | room | not measured | not measured | — | — |
| cache | room | 0.010458 | 0.007226 | 0.691 | — |
| preview | room | 0.007560 | 0.004642 | 0.614 | — |
| sub_combine | room | 0.065830 | 0.043315 | 0.658 | — |
| camilla | room | 0.003094 | 0.001367 | 0.442 | — |
| mdat_staged | room | 0.413465 | 0.170665 | 0.413 | — |
| mdat_before_after | room | 0.194560 | 0.140973 | 0.725 | — |
| recommend | room | not measured | not measured | — | — |
| design | room_dba | 8.036892 | 3.810529 | 0.474 | — |
| prepare | room_dba | 0.105519 | 0.050721 | 0.481 | — |
| ba | room_dba | 0.828716 | 0.216242 | 0.261 | — |
| top | room_dba | 0.780120 | 0.220156 | 0.282 | — |
| rc:L | room_dba | 1.855209 | 1.033838 | 0.557 | — |
| rc:R | room_dba | 2.000375 | 1.038939 | 0.519 | — |
| rc:C | room_dba | 2.001886 | 1.093713 | 0.546 | — |
| rc_all | room_dba | 5.949483 | 3.159819 | 0.531 | — |
| scores | room_dba | 0.161787 | 0.132552 | 0.819 | — |
| fit_rc1 | room_dba | 0.821030 | 0.523517 | 0.638 | — |
| fit_rc2 | room_dba | 0.901348 | 0.495870 | 0.550 | — |
| fit_bus | room_dba | 0.155084 | 0.089238 | 0.575 | — |
| fit_xo | room_dba | 0.111062 | 0.055372 | 0.499 | — |
| gd | room_dba | not measured | not measured | — | — |
| cache | room_dba | 0.017123 | 0.014007 | 0.818 | — |
| preview | room_dba | 0.008441 | 0.004660 | 0.552 | — |
| sub_combine | room_dba | 0.151673 | 0.058327 | 0.385 | — |
| camilla | room_dba | 0.003486 | 0.001802 | 0.517 | — |
| mdat_staged | room_dba | 0.460959 | 0.262825 | 0.570 | — |
| mdat_before_after | room_dba | 0.332801 | 0.196865 | 0.592 | — |
| design | dolby_zero | not measured | not measured | — | — |
| design | dolby_linear | not measured | not measured | — | — |
| design | dolby_ultra | not measured | not measured | — | — |
| .secs load | demo | not measured | not measured | — | — |
| .secs save | demo | not measured | not measured | — | — |

## Memory

- Dolby design, shipped: Python 246632 KiB; Rust 210412 KiB.
- Dolby recommend, shipped: Python 252696 KiB; Rust 590108 KiB.
- Dolby design, one: Python 246040 KiB; Rust 123108 KiB.
- Dolby recommend, one: Python not measured KiB; Rust not measured KiB.

Process high-water RSS includes fixture creation, imports and warm-up. **The requested GNU-time comparison fails for shipped recommendation:** Rust 590,108 KiB exceeds Python's reported 252,696 KiB (2.335×). This is a blocker, not a pass. Rust runs four full candidate designs concurrently (`xo.rs` creates a serial inner Context per candidate); Python ships two worker processes. Its GNU-time maximum is not aggregate simultaneous RSS, so a separate process-tree measurement is also needed before comparing total application memory. Candidate concurrency/memory budgeting was not changed or verified in this timebox. Both measured design configurations use less Rust RSS.

## Unresolved measured rows

- shipped / dolby / cache: ratio 0.846; within 20% repeat band.
- shipped / dolby / preview: ratio 0.893; within 20% repeat band.
- shipped / room / fit_bus: ratio 1.064; slower.
- shipped / room / cache: ratio 0.882; within 20% repeat band.
- shipped / room_dba / camilla: ratio 1.155; slower.
- one / dolby / scores: ratio 0.972; within 20% repeat band.
- one / dolby / sub_combine: ratio 0.861; within 20% repeat band.
- one / dolby / camilla: ratio 0.855; within 20% repeat band.
- one / room / fit_bus: ratio 1.051; slower.
- one / room_dba / scores: ratio 0.819; within 20% repeat band.
- one / room_dba / cache: ratio 0.818; within 20% repeat band.

These are not certified performance passes. Missing rows remain visible in both tables.

## Fixes and before/after

The first continuation baseline already contains the first run's scalar smoothing hoist and parallel scores, plus the pending grid-phase hoist (validated at the start). The following paired medians are from this continuation, five samples after warm-up; no optimization changes an accumulation order.

| Change / operation | Configuration | Rust before (s) | Rust after (s) | Python (s) |
|---|---|---:|---:|---:|
| fit_bus, dolby | shipped | 0.065489 | 0.040458 | 0.082617 |
| fit_bus, dolby | one | 0.062116 | 0.040857 | 0.071882 |
| sub_combine, dolby | shipped | 0.033833 | 0.024178 | 0.030455 |
| sub_combine, dolby | one | 0.032710 | 0.024368 | 0.030916 |
| fit_rc1, dolby | shipped | 0.902990 | 0.455885 | 0.975729 |
| fit_rc1, dolby | one | 0.875783 | 0.438676 | 1.047284 |

1. **Fit band-response cache.** Finite-difference probes change one band, but formerly recomputed every band response. Cache each band's response keyed by the exact three parameter bit patterns; reuse the fixed grid phase. Every residual still multiplies the full cascade in band order and performs the same magnitude/log/weight operations. Cache is local to one fit, with at most one response per band. Bus profile: total 0.069534 → 0.045244 s; residual 0.045700 → 0.020912 s (medians). Both have 968 residual calls, nfev=38, njev=31. A new recorded-input test checks bit equality through every single-column perturbation and return to the base point.
2. **Sub-preview smoothing bounds.** The same full design grid's floor/ceil bounds were recalculated for every sub/seat/state curve. Build two bound arrays (1/12 and 1/6 octave) per call, then reuse them. Sequential prefix sums, subtraction, division, hypot/log and interpolation retain their arithmetic. Hot-loop medians before: one term 0.001100 s, one smoothing pass 0.001465 s, one log pass 0.000505 s. About 2 MiB of temporary bound storage replaces repeated work; it is freed at return. Cached-vs-direct smoothing bit/error tests and existing preview goldens cover this path.
3. **XO preview smoothing cache.** Reuse a 1/12-octave bound array in XoPreview for its three curves; no prefix sum or arithmetic is reused across different spectra. Room shipped preview 0.011899 → 0.005247 s (separate paired runs), Python repeat 0.008201 s. Cache construction is included in the cache row, and retains about 1 MiB extra per XoPreview; room repeated cache 0.009632 s Rust vs 0.010918 s Python remains close.
4. **First-run improvements retained.** Scalar smoothing width hoist: early Dolby preview 0.014327 → 0.009031 s. Independent channel score/curve map: early shipped scores 0.861577 → 0.156449 s. These historical observations are not paired continuation measurements; current table rows govern the Python comparison. Ordered-map bit checks compare all metrics and curves against serial replay.
5. **CamillaDSP investigated, not modified.** The initial continuation remeasurement (replay export setup) gave shipped 0.004848 s Rust vs 0.005913 s Python; one-thread 0.005427 vs 0.010939 s. The previous 1.13 ratio did not reproduce in that pair; production-setup rows in the tables supersede those values and some are still close/slower. Diagnostic construction (default options, not the exact export label wrapper) median 0.002855 s, file writes 0.001597 s. Filesystem timings are noisy. Camilla implementation is outside the allowed files, so no writer change is claimed.

## Remaining scope and prohibited shortcuts

- `.secs` load/save: not measured. Neither permitted bench crate depends on secs-project, and adding a dependency or changing secs-project's examples is outside the packet's allowed files. This remains a scope blocker, not a performance pass.
- Timebox omissions: all three Dolby mode variants; room recommendation in both configurations; Dolby one-thread recommendation; room/room_dba GD; Dolby one-thread GD. Those cells are explicitly `not measured`. Dolby shipped GD used five timed runs despite exceeding 10 s, as the three-run exception was restricted to design/recommend in the harness. Recorded GD input: first main (L), captured with GD cuts enabled, shared as one little-endian complex128 file by both implementations.
- Serial score profile (Dolby L after, five samples): metrics 0.012903 s; inverse FFT 0.000578 s; GD core with a six-point diagnostic output grid 0.005242 s. This identifies non-FFT phase/smoothing work but does not substitute for the complete score-stage row.
- **Room bus fitting remains slower.** Cached Rust uses 4,449 residual calls, nfev=159, njev=143; Python SIMD-on uses 1,559 calls, nfev=59, njev=50 on the identical recorded input. Rust hot-loop profile median total 0.180504 s, residual 0.085263 s; about 0.095 s remains in solver/Jacobian/SVD work. Python one-thread profile total 0.125319 s, residual 0.070192 s. These profiles include timer overhead and are not the table timing. Matching Python's convergence path by changing libm/arithmetic/stopping is disallowed. A four-entry-per-band cache experiment retained nfev but gave 0.179058 s vs 0.176349 s for the one-entry cache, so it was removed (not a demonstrated improvement).
- No analytic Jacobian, solver replacement, changed SVD, TRF evaluation/stopping limit, DE variable removal, reassociated reduction, SIMD transcendental change, unsafe or faer parallel mode was used. No speed-up requiring different arithmetic was applied.
- `perf.engine` remains partial while any row is missing, slower, an unresolved close comparison, or aggregate memory remains unverified. The 90-minute measurement cutoff takes precedence over filling a cell. Single-thread score/curve work uses scalar hypot/log and sequential FFT/phase/smoothing paths; it receives no benefit from the channel map. A SIMD math substitution could alter bits and was not made. Camilla ratios are filesystem-noise sensitive and its implementation remains out of scope.

## Verification

Final gates ran sequentially from 16:53 to 17:05 UTC, after benchmarks stopped, and all exited 0. An additional normal-libm release engine run also passed afterwards:

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| clippy: engine/DSP/compute/export, all targets, locked, `-D warnings` | pass |
| normal engine/DSP/compute/export suite | pass; engine 40/40, all other suites pass |
| `SECS_ORACLE_FOREIGN_LIBM=1` engine/export suite | pass; engine 40/40, export suites 4/4 and 5/5 |
| forced-foreign release engine suite | pass; 40/40 |
| normal-libm release engine suite (additional check) | pass; 40/40 |
| rustdoc, four crates, `RUSTDOCFLAGS="-D warnings"` | pass |
| feature test registry | pass; all 110 implemented-feature references compiled/listed |
| upstream check-tree | pass; 12 protected paths match `60ac53ad3a3d` |

Cached residual tests include every recorded initial single-column probe, return to the base point, and recorded final solution. Smoothing bit/error checks and existing preview, design and e2e gates pass; no golden or tolerance was edited. The initial pending grid-phase change also passed all three engine paths before further work. No commit was made. The packet document's pre-existing continuation edit was left untouched.
