# MIMO performance audit (P18 / P18b / P18c)

## Verdict (Claude, 2026-10-14)

M3's performance and memory conditions, as amended in `docs/MIMO.md` §6, are
met. `perf.mimo` is `implemented`. Big case (six positions, 7.1.4 + two subs),
same machine, SECS 2 / Rust:

| Operation | One thread | Shipped |
|---|---:|---:|
| whole MIMO, IIR (default) | 3,168.8 / 277.0 s = **11.4×** | > 5,280 / 79.6 s = **> 66×** |
| `mimo_iir_one` (input L) | 275.9 / 23.25 s = **11.9×** | — |
| whole MIMO, FIR mode | 14.36 / 4.21 s = 3.4× | 13.47 / 1.95 s = **6.9×** |
| `mimo_design_one` (input L, 180 taps) | 5.18 / 2.22 s = 2.3× | 3.13 / 2.03 s = 1.5× |
| peak PSS, whole IIR | ≥ 1,152,868 / 483,028 KiB | ≥ 1,901,543 / 808,083 KiB |

- **MIMO run ≥ 5×**: met in both configurations (11.4× and over 66×). The
  shipped SECS 2 run did not finish in 88 minutes, so that ratio is a lower
  bound; its two pool workers each run multi-threaded BLAS on four cores.
- **Memory**: Rust's peak is 0.79 GiB shipped and 0.46 GiB one thread, under
  1 GB, and below SECS 2's peaks observed within the first 25 and 10 minutes
  (lower bounds for SECS 2).
- **Known shortfall, recorded rather than hidden**: the ideal FIR design step
  alone (2.3× / 1.5×) and FIR mode with one thread (3.4×). The step is dense
  factorization. SECS 2 runs it in multi-threaded LAPACK; this port keeps it
  sequential so results do not depend on the thread count (ADR 0007). P18b
  replaced 19 Cholesky factorizations with one tridiagonal reduction (2.4×
  on the sweep). P18c computed the shared preparation and support paths once
  (bit-identical). The blocked reduction (P18c Part B) and the GPU experiment
  (P19, moved to M6) are the follow-ups.
- **Not measured**: the MIMO exports (CamillaDSP and mdat items) and the
  `room_dba` whole runs. Neither is in the M3 conditions.


## P18c: request-local preparation and shared low-rate paths

### Supplied quiet-machine observations (2026-10-14)

These are the observations supplied with P18c, reproduced without replacing
incomplete runs with estimates. Ratios in this table are SECS 2 / Rust.

| Operation | Config | SECS 2 s | Rust s | Ratio |
|---|---|---:|---:|---:|
| whole MIMO, IIR | shipped | > 5,280 (not finished in 88 min) | 81.8 | > 64× |
| whole MIMO, IIR | one | 3,168.8 | 286.5 | 11.1× |
| whole MIMO, FIR mode | shipped | 13.47 | 2.98 | 4.5× |
| whole MIMO, FIR mode | one | 14.36 | 5.94 | 2.4× |
| `mimo_design_one` (L, 180 taps) | shipped | 3.13 | 2.22 | 1.4× |
| `mimo_design_one` (L, 180 taps) | one | 5.18 | 2.46 | 2.1× |
| `mimo_iir_one` (L) | one | 275.9 | 23.25 | 11.9× |
| peak PSS, whole IIR | shipped | ≥ 1,901,543 KiB (25 min cap) | 808,083 KiB | |
| peak PSS, whole IIR | one | ≥ 1,152,868 KiB (10 min cap) | 483,028 KiB | |

Source method: the originating session's `measure_{a,b,c,d}.sh` scripts.
Rust used `engine_perf` defaults, except `SECS_PERF_RUNS=3` for whole IIR.
Python whole IIR used `--time-budget 1` (one sample); FIR used `--runs 3`.
PSS used `oracle/peak_memory.py`; the timed-out Python observations are lower
bounds. Python was **not rerun** in P18c. These rows have mixed sampling
protocols, not a claim of paired equal-sample medians.

### Shared work and proofs

`mimo::prepared::Prepared` holds long spectra and alignment for one request.
It calls measurement preparation once, then the same `long_spectra` operation
and sub-filter selection as `export_view`. `compute` lends this value to the
ideal design, IIR fit and assembly; standalone design/fit prepare their own.
The measurement preparation's large `h`/`h0` buffers are dropped after long
spectra are built. Public signatures and arithmetic order are unchanged.

The ideal design maps each needed `(position, candidate)` once to its 720-sample
protected/aligned low-rate path. Inputs select candidate paths in their old
order. The table has one retained budget permit; temporary full-rate path
buffers have per-task permits and input-local copies remain in the existing
input budget. The combined shared-plus-largest-input reservation is checked
before acquiring the retained permit, preventing a self-wait under a small
budget. This is allocation accounting, **not** a new peak-PSS observation.

- `mimo_shared_preparation_bit_identical` compares every stage-boundary case
  (eight computational cases plus the two-position note-only case) against a
  test-only copy of the pre-P18c per-input preparation. It uses Context(1) and
  Context(4), structural equality and explicit `f64::to_bits` vectors for all
  design and trace numbers. No result golden was added.
- `mimo_compute_shared_preparation_bit_identical` compares whole room_dba IIR
  and FIR runs with the old separately prepared design, fit and assembly,
  including every ideal tap, exported filter, final/applied response and SPL
  value, with structural equality and explicit bits.
- `mimo_shared_paths_reject_combined_budget` uses a budget that fits each old
  input permit alone but not that permit plus the shared table; it must return
  an error, not wait indefinitely.
- Existing MIMO tests and tolerances, including both P18b ridge-sweep proofs,
  are unchanged. No solver or reduction arithmetic changed.

### Before/after profiles

Release, big case, one warm-up then five samples per operation/configuration,
`engine_perf` default sampling. Context(1) and Context(4); the latter is this
machine's shipped configuration. Calibration/reconstruction precede timing.
Measurements ran sequentially with no concurrent build, test or benchmark.
`uptime` was recorded at each start. Baseline batches started 09:30–09:35 UTC;
after batches 09:41–09:46 UTC. First batches followed release builds (one-minute
loads 1.10 and 1.35 respectively); loads include the preceding sequential job.

The complete-operation timers include the section callbacks on both sides.
Section values are medians across samples. Input sections are **summed elapsed
worker seconds**, not additive wall-time partitions in shipped mode. The
`inputs_wall` interval encloses the entire input map. In whole FIR, before
`design_wall` includes preparation; after it excludes the now-shared preparation.
`assemble` before includes its repeated preparation and after does not.

| Operation | Config | Before s | After s | Before/after |
|---|---|---:|---:|---:|
| `mimo_design_one` | one | 2.252487 | 2.222060 | 1.014× |
| `mimo_design_one` | shipped | 2.170595 | 2.030304 | 1.069× |
| `mimo_design_all` | one | 21.031620 | 20.017563 | 1.051× |
| `mimo_design_all` | shipped | 6.185466 | 6.372843 | 0.971× |
| `mimo_fir` | one | 5.677796 | 4.207428 | 1.349× |
| `mimo_fir` | shipped | 2.868605 | 1.952964 | 1.469× |

#### `mimo_design_one` section medians (seconds)

| Section | Before one | After one | Before shipped | After shipped |
|---|---:|---:|---:|---:|
| `preparation` | 0.488748 | 0.366484 | 0.539406 | 0.372024 |
| `ranges` | 0.069552 | 0.068904 | 0.071777 | 0.074277 |
| `shared_paths` | 0.000000 | 0.111672 | 0.000000 | 0.032387 |
| `input_preparation` | 0.144615 | 0.030026 | 0.157909 | 0.045972 |
| `normal_equations` | 0.382698 | 0.369589 | 0.124355 | 0.131544 |
| `sweep` | 1.153490 | 1.151714 | 1.222616 | 1.296123 |
| `path_building` | 0.001211 | 0.001105 | 0.002003 | 0.002223 |
| `ranking` | 0.000000 | 0.000000 | 0.000000 | 0.000000 |
| `guard_steps` | 0.017619 | 0.017747 | 0.018052 | 0.026866 |
| `inputs_wall` | 1.698041 | 1.622636 | 1.531409 | 1.508446 |

#### `mimo_design_all` section medians (seconds)

| Section | Before one | After one | Before shipped | After shipped |
|---|---:|---:|---:|---:|
| `preparation` | 0.546804 | 0.408792 | 0.523108 | 0.386790 |
| `ranges` | 0.073500 | 0.069470 | 0.066833 | 0.067852 |
| `shared_paths` | 0.000000 | 0.116394 | 0.000000 | 0.033552 |
| `input_preparation` | 1.680791 | 0.379079 | 1.789229 | 0.396395 |
| `normal_equations` | 4.275043 | 4.308505 | 4.086276 | 5.124021 |
| `sweep` | 14.038525 | 14.391486 | 13.983411 | 14.688784 |
| `path_building` | 0.014456 | 0.015196 | 0.013811 | 0.014953 |
| `ranking` | 0.000004 | 0.000003 | 0.000003 | 0.000003 |
| `guard_steps` | 0.356790 | 0.378126 | 0.359243 | 0.401822 |
| `inputs_wall` | 20.365590 | 19.403759 | 5.591588 | 5.864693 |

#### `mimo_fir` section medians (seconds)

| Section | Before one | After one | Before shipped | After shipped |
|---|---:|---:|---:|---:|
| `preparation` | 0.456071 | 0.385848 | 0.510846 | 0.371744 |
| `ranges` | 0.068720 | 0.070860 | 0.070014 | 0.066852 |
| `shared_paths` | 0.000000 | 0.116409 | 0.000000 | 0.031592 |
| `input_preparation` | 1.634598 | 0.381137 | 1.796387 | 0.371744 |
| `normal_equations` | 1.543076 | 1.552145 | 1.456021 | 1.478298 |
| `sweep` | 0.843441 | 0.933917 | 0.949336 | 0.900400 |
| `path_building` | 0.014188 | 0.014679 | 0.015877 | 0.014700 |
| `ranking` | 0.000004 | 0.000003 | 0.000003 | 0.000003 |
| `guard_steps` | 0.136786 | 0.149581 | 0.157292 | 0.145514 |
| `inputs_wall` | 4.172819 | 3.036623 | 1.212036 | 0.774956 |
| `design_wall` | 4.682119 | 3.226707 | 1.815266 | 0.872960 |
| `realization` | 0.000039 | 0.000052 | 0.000063 | 0.000067 |
| `assemble` | 0.995587 | 0.599842 | 1.099254 | 0.619917 |

Remaining time: the 180-tap per-input λ sweep still takes about 1.15–1.30 s;
normal equations take about 0.37 s one-thread / 0.13 s shipped for L. The shared
preparation itself takes about 0.37–0.41 s. The shipped all-input design did not
improve in this batch (6.185 → 6.373 s); its summed normal-equation and sweep
intervals increased despite reduced input preparation. In FIR mode the solve
is smaller (70 taps), so removing repeated preparation is a larger share:
5.678 → 4.207 s one-thread and 2.869 → 1.953 s shipped. Afterward the FIR
assembly is about 0.60–0.62 s, and the one-thread normal equations about 1.55 s
summed across inputs. These are section observations, not a target verdict.

### P18c Rust-only rows

SECS 2 values below are the supplied table above, not new measurements.
Rust `mimo_iir` uses one warm-up and exactly three samples; other rows use five.
No Python all-input ideal-design time was supplied, so that ratio is left blank.
The shipped IIR comparison uses the incomplete 5,280 s lower bound.

| Operation | Config | SECS 2 s | Rust median s | Rust/SECS 2 | SECS 2/Rust |
|---|---|---:|---:|---:|---:|
| `mimo_design_one` | one | 5.18 | 2.222060 | 0.4290 | 2.331× |
| `mimo_design_one` | shipped | 3.13 | 2.030304 | 0.6487 | 1.542× |
| `mimo_design_all` | one | — | 20.017563 | — | — |
| `mimo_design_all` | shipped | — | 6.372843 | — | — |
| `mimo_fir` | one | 14.36 | 4.207428 | 0.2930 | 3.413× |
| `mimo_fir` | shipped | 13.47 | 1.952964 | 0.1450 | 6.897× |
| `mimo_iir` | one | 3168.8 | 277.020135 | 0.0874 | 11.439× |
| `mimo_iir` | shipped | > 5280 | 79.638093 | < 0.0151 | > 66.300× |

### Part B

Not attempted: the time box was spent on A/C, independent parity proofs and
foreground gates. faer's sequential `tridiag_in_place` and `RidgeSweep` are
unchanged. There is **no blocked-reduction benchmark**, no claimed 1.5× result,
and no new reduction accepted. The P18b candidate measurements below remain
historical measurements, not P18c blocked-reduction evidence.

### Verification

All requested gates finished in the foreground after benchmarking stopped:

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy -p secs-engine --all-targets --locked -- -D warnings` | pass |
| `cargo test -p secs-engine --locked` | 69 passed; 0 failed; doc tests pass |
| `SECS_ORACLE_FOREIGN_LIBM=1 cargo test -p secs-engine --release --locked` | 69 passed; 0 failed; doc tests pass |
| `python3 .github/scripts/check_feature_tests.py` | all 148 referenced tests compiled/listed |
| `python3 .github/scripts/upstream.py check-tree` | 12 protected paths match `60ac53ad3a3d` |
| `git diff --check` | pass |

Raw samples and gate logs are in the P18c session scratchpad (`before_*`,
`after_*`, `gate-*`). Early instrumentation build errors were corrected before
the baseline measurement; the final gates above passed. The post-measurement
cleanup only factors validation, preserves empty-fit progress, adjusts table
metadata accounting and strengthens tests; it does not change numerical work.
No new memory measurement or export timing was run. No commit or push.

---

## P18b: one reduction per input

P18b keeps the first-pass observations below as **before**. This pass reports
measurements, not a verdict on the 5× target. The big whole run, new memory
measurements and exports are **not measured**; they are reserved for a later pass.

### Candidate benchmark and choice

Release, actual big/L matrix (2160 × 2160), `Par::Seq`, one thread, one warm-up
then five samples. All 19 λ values are taken from the actual design trajectory,
not a repeated representative λ. Timings include decomposition, RHS projection
and all shifted solves. `uptime` was recorded before each batch; no other
benchmark, build or test ran concurrently.

| Candidate | Initial median s | Final recheck s |
|---|---:|---:|
| Fresh Cholesky for every λ | 3.092588 | 3.053423 |
| Self-adjoint spectral decomposition | 2.310761 | 2.345390 |
| Householder tridiagonal reduction | 1.095407 | 1.250375¹ |
| Cholesky-preconditioned CG reuse | 7.620776 | 7.174154 |

¹ The selected production version includes one fixed residual-refinement step
per λ, reusing the tridiagonal reduction. Its final whole-sweep speed-up over
fresh Cholesky is **2.442×**. Spectral does not clear 2×. CG attempted reuse for
12 shifts, exhausted 30 iterations each (360 total), and factored all 19 shifts:
the true relative residual did not reach 1e-14. The non-iterative tridiagonal
choice is also faster than reuse, so the 20% preference rule is unambiguous.

`RidgeSweep` in `solve.rs` retains M and b, reduces M once, projects b once,
solves each shifted tridiagonal system by LDLᵀ, and applies the Householder
sequence. Non-positive/non-finite pivots or non-finite output route that λ to
the original Cholesky/LU solver. One original-coordinate residual correction
is necessary: the unrefined candidate passed normwise proofs but one unchanged
end-to-end ideal-FIR check measured 3.670419e-9 against its 3e-9 limit. Refinement
passed that test without changing its tolerance. Bisection midpoints, comparisons,
accepted-result handling and all guard calculations are unchanged.

### Proofs

- `mimo_ridge_sweep_recorded_systems_and_trajectories` rebuilds all 24 systems
  from the eight stage-boundary cases (including room/L from the old
  Cholesky/LU proof, capped supports and FIR taps). At every reference-recorded
  λ and every locally computed midpoint it checks both solutions' infinity-norm
  backward error ≤16·N·ε and sweep/Cholesky relative difference ≤16·N·κ·ε, using
  sequential eigenvalues for κ as in the existing test.
- The same test computes both complete trajectories independently, requires
  exact midpoint/accepted-flag/final-log equality, and compares the production
  trace. It also checks the mapped M and b **bit for bit** against the old
  sequential builder with Context(1) and Context(4) on every system.
- `mimo_ridge_sweep_falls_back` exercises zero/non-positive pivots, a successful
  LU fallback and non-finite input/shift errors. No pivot is clamped.
- Existing MIMO tests and tolerances are unchanged, including
  `mimo_deterministic_threads`.

### Input profile

Boundary timers on actual big/L low-rate `design_input`, median of five after
warm-up. “Path building” is its common low-pass convolution; shared range,
full-rate preparation and low-rate projection precede this boundary and are
included only in the complete `mimo_design_one` timing rows. Ranking is not
invoked for uncapped big/L. “Other” is measured total minus timed sections per
sample. Component medians need not add to the median total.

| Section | Part A one s | Part A shipped s | Final one s | Final shipped s |
|---|---:|---:|---:|---:|
| Normal equations | 0.415920 | 0.411489 | 0.355669 | 0.118982 |
| Sweep incl. evaluation | 1.074408 | 1.026861 | 1.268833 | 1.230700 |
| Ranking | 0.000000261 | 0.000000225 | 0.000000359 | 0.000000238 |
| Path building | 0.001168 | 0.001074 | 0.001209 | 0.001070 |
| Guard steps | 0.017414 | 0.017470 | 0.017548 | 0.016544 |
| Other | 0.003912 | 0.003602 | 0.003962 | 0.003659 |
| Total | 1.505186 | 1.469199 | 1.643164 | 1.385211 |

The initial Part A profile (before the fixed refinement) attributed 27.6% /
28.0% to normal equations. Therefore P18b maps independent block pairs through
`ctx`; each block accumulates positions and times in exactly the old order,
then is placed without a reduction. No faer parallelism or parallel reduction
was introduced. The input memory reservation now covers mapped block storage,
the retained reduction, and a possible concurrent fallback factor; it is not a
new process-memory observation.

### New timing rows

Measurements in progress; completed rows and verification are added below.

---

## Before: P18 first pass

Status: **partial / target missed**. M3 is not closed. The large-case ideal
FIR design is only 1.271× faster with one thread and is slower than SECS 2
with shipped settings. No tolerance, result golden, solver path, parallel
reduction, faer parallelism, or memory budget was changed to obtain a pass.

This is a stopped audit, not a completed performance sweep. Missing cells
below are not zero and are not passes. The blocking FIR result and its profile
are retained rather than weakening the five-times requirement.

## Method and machine

- Measured 2026-10-14, Linux 6.18.44-fc-v114, x86-64 KVM, Intel Xeon
  @ 2.10 GHz, four cores/four logical CPUs. AVX2/AVX-512 available;
  L1d 192 KiB, L1i 128 KiB, L2 8 MiB, L3 260 MiB. RAM 16,093 MiB,
  no swap.
- Unchanged upstream `60ac53ad3a3d`. Supplied oracle venv: Python 3.13.16,
  NumPy 2.5.3, SciPy 1.18.1. Rust release, `CARGO_INCREMENTAL=0`,
  `CARGO_TARGET_DIR=/home/user/wt/target-p16`. No dependencies added.
- Shipped: Rust default four-thread Context; NumPy SIMD enabled,
  `OPENBLAS_NUM_THREADS` and `OPENBLAS_CORETYPE` unset. Upstream
  `calibration.MIMO_WORKERS` is two. Its worker BLAS initialization is left
  unchanged. One thread: Context(1), `OPENBLAS_NUM_THREADS=1`, `workers=1`.
- Calibration design is computed before the MIMO timing. Separate native
  calibration designs are used, as in P13 production-export measurements;
  this is not a byte-identical intermediate-input microbenchmark.
- Every completed timing is the median of five calls after one warm-up,
  in a fresh process per operation/configuration. The MIMO harness selects
  three calls only when warm-up exceeds 60 seconds. `perf_counter` and
  Rust `Instant` surround the upstream/production operation, not imports,
  fixture generation or calibration. The sampler's one-shot durations are
  **not** timing medians.
- `mimo_design_one` selects input **L**, retaining all support candidates
  and the full calibration design. It includes the function's shared
  range/preparation work on both sides. `mimo_iir_one` retains only L in
  the ideal design, then includes problem preparation, seeds, joint fit
  and postprocessing. `*_all` selects all inputs; it is not a multiplication
  of the single-input result.
- Comparisons were sequential, not concurrent with each other or verification.
  `uptime` was checked before every pair. Large one-thread FIR pair started
  at 02:10:40 UTC (one-minute load 0.84); shipped pair at 02:16:58 (0.73).
  Profile started after a build at 02:15:21 (2.00, build residue); therefore
  its absolute times are diagnostic, not a separately quiet-certified gate.
  Reference room pairs started at 02:45:12 (2.47) and 02:45:38 (1.97), after
  the four-thread memory run; they are reference observations, not independent
  quiet-host certifications. No completed Python/Rust ratio is within 20%
  of equality. The large one-thread ratio 0.787 is close to the edge of that
  band but nowhere near the required 0.200.

### Large input and storage

`engine_cases.big_case()` is separate from `cases()`. Existing exporters
therefore keep their existing inputs. The new case has six positions,
eleven mains and two subs from the upstream 7.1.4 preset:
`L R C Ls Rs Lb Rb Tfl Tfr Tbl Tbr SW1 SW2`.

It uses the same 48 kHz, one-second modal-room recipe as `_room()`, upstream
`synth.main_ir`/`sub_ir`, seeds `synth.SEED + role_index`, and offsets
`(position - 1)*0.00013 + role_index*0.00107 - 0.0002`. The first five
receivers are upstream `POSITIONS`; the sixth is `(2.6, 2.4, 1.15)`.
Source coordinates, modal parameters and ordering are explicit in the
function and recorded recipe.

Full float32 IR storage would be 14,976,000 bytes, above 8 MiB. Instead,
`oracle/goldens/perf_mimo/` contains only calibration configuration, modal
recipe and upstream f64 excitation (including its deterministic random
noise), **4,999,926 bytes total**. Rust rebuilds the modal responses before
any timer. This avoids inventing a different RNG or truncating the IRs.
There are no performance output goldens.

A direct comparison of all 3,744,000 regenerated float32 samples found
194 differing bit patterns, maximum absolute difference
`1.4551915228366852e-11`. FFT/libm reconstruction is not bit-identical to
NumPy. These are the same recipe and dimensions, not certified identical
measurement bytes. A future strict byte-identical audit can store sparse
reconstruction corrections or obtain approval for full input storage;
this audit does not silently call the reconstructed inputs identical.

## Timing results

Seconds; ratio is **Rust/Python** (five-times requires ≤0.200 for the
large per-input ideal FIR and IIR rows). “—” means not measured to the
required completed-sample protocol.

### Per-input design and fitting

| Configuration | Case | Operation | Python s | Rust s | Rust/Python | Python/Rust |
|---|---|---|---:|---:|---:|---:|
| shipped | big | ideal FIR, L | 3.129367 | 4.042733 | 1.292 | 0.774× |
| one | big | ideal FIR, L | 5.178047 | 4.074154 | 0.787 | 1.271× |
| shipped | room_dba | ideal FIR, L | 0.968258 | 0.720669 | 0.744 | 1.344× |
| one | room_dba | ideal FIR, L | 1.124146 | 0.687860 | 0.612 | 1.634× |
| shipped | big | IIR, L | — | — | — | — |
| one | big | IIR, L | — | 23.252138 | — | — |
| shipped | room_dba | IIR, L | — | — | — | — |
| one | room_dba | IIR, L | — | — | — | — |

Rust one-thread IIR samples were 23.632419, 22.425203, 23.744653,
22.781513 and 23.252138 seconds. The corresponding Python process was
stopped before finishing its required three timed samples after warm-up.
It printed two completed postprocessing messages, but the old harness
printed its samples only after the whole batch; **no Python median or
speed ratio is inferred**. The harness now flushes each MIMO sample to
make interrupted future runs diagnosable. The Python process was stopped
before builds/tests and memory observations resumed.

### All inputs, whole runs and exports

Each cell is Python / Rust seconds. No single-input or memory observation
is substituted for these medians.

| Operation | big shipped | big one | room_dba shipped | room_dba one |
|---|---|---|---|---|
| ideal FIR, all inputs | — / — | — / — | — / — | — / — |
| IIR fits, all inputs | — / — | — / — | — / — | — / — |
| whole MIMO, IIR | — / — | — / — | — / — | — / — |
| whole MIMO, FIR | — / — | — / — | — / — | — / — |
| CamillaDSP with paths | — / — | — / — | — / — | — / — |
| staged mdat with MIMO | — / — | — / — | — / — | — / — |
| before/after mdat with MIMO | — / — | — / — | — / — | — / — |

The audit stopped on the measured FIR blocker instead of running the
remaining expensive sweep without an admissible demonstrated fix.
The Python harness supports these operations. The engine example supports
design, fit and whole-run operations, **not Rust exports**: secs-engine has
no secs-camilla/secs-export dependency, and the allowlist permits no new
dependency or change to `secs-export/examples/export_perf.rs`. No source
copy of the exporter was introduced to disguise that scope constraint.
Completing export timing needs an approved harness location/dependency.

## Whole-run process-tree memory

`oracle/peak_memory.py` walks every thread's children recursively and samples
the simultaneous PSS/RSS sum every 10 ms. The sampler is outside the counted
tree. Each observation is one fresh process, no warm-up, one whole
IIR `mimo::compute`, including fixture reconstruction and preceding calibration
setup in the peak. Thus the measurement is a conservative whole-process
scope, not MIMO-live-allocation attribution. Sampling peaks remain lower
bounds, not kernel high-water proofs. Rust kept the existing benchmark's
unlimited MemoryBudget; concurrency was not capped to hide a peak.

| Case | Configuration | Python PSS KiB | Rust PSS KiB | Rust RSS KiB | Processes | Samples |
|---|---|---:|---:|---:|---:|---:|
| big | shipped | — | 802289 | 804284 | 1 | 8663 |
| big | one | — | 481643 | 483508 | 1 | 29610 |
| room_dba | shipped | — | — | — | — | — |
| room_dba | one | — | — | — | — | — |

Rust observed peaks are 821,543,936 and 493,202,432 bytes, both below
1,000,000,000 bytes. The **below-SECS-2 requirement is unverified**: neither
Python whole-run observation was completed, including its pool children.
These observations cannot close the M3 memory gate. Start times/load:
02:38:00/0.96 (one), 02:43:12/0.94 (shipped); elapsed including setup and
sampler was 300.108 and 98.305 seconds respectively, not timing-table rows.

## Profile and fix decision

`perf` was not executable (`perf: command not found`), contrary to the
packet's environment expectation. `mimo_profile_design` uses benchmark-only
boundary timers on production functions, with five samples and one warm-up.
The first input has twelve candidates, 180 taps each: dimension **2160**.

| Diagnostic, big / one thread | Median s |
|---|---:|
| Toeplitz normal-equation construction | 0.405047 |
| 19 sequential Cholesky solves at representative λ | 2.953175 |
| full low-rate `design_input`, actual bisection and guards | 3.541550 |

The 19-solve row uses a fixed λ equal to the mean diagonal, not the actual
bisection trajectory; it isolates repeated factorization cost. It is not an
additive trace of the full row. The dominant cost is repeated dense
factorization, not construction of the avoided tall design matrix.

The shipped target is ≤0.625873 seconds, the single-thread target
≤1.035609 seconds. The measured factorization component alone exceeds both.
Removing all range/preparation/guard work would not meet either target with
this measured factorization implementation. Workspace reuse alone cannot
be claimed to supply the necessary reduction. No allowed, validated
optimization sufficient to close that gap was established in this audit;
this is **not a proof that all possible legal optimizations are impossible**.
A one-eigendecomposition shifted-system solver would be a different solve
path; faer parallelism and reductions across inputs would violate the
packet. Neither was attempted.

**No production speed/memory fix was landed**, so there is no fabricated
before/after improvement table. The only production-source change extracts
an internal input selector so the example can measure exactly one input
without deleting other mains from support-range calculation. `None` is the
unchanged production all-input path. New test
`mimo_input_selection_preserves_all_input_result` checks each input against
the all-input result, including trace, ranges, levels and support tap bits.
The existing shared case loader now accepts a fixture explicitly; its
normal `engine_inputs` path is unchanged.

## Reproduction

```sh
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/user/wt/target-p16
# Record the separate recipe/excitations, not existing engine families:
<oracle-venv>/bin/python oracle/bench_engine.py record_mimo_case big --config shipped
cargo build -p secs-engine --release --example engine_perf --locked
uptime
<oracle-venv>/bin/python oracle/bench_engine.py mimo_design_one big --config one
SECS_PERF_THREADS=1 cargo run -p secs-engine --release --example engine_perf -- mimo_design_one big
# Also supported: mimo_design_all, mimo_iir_one, mimo_iir_all, mimo_iir, mimo_fir.
SECS_PERF_THREADS=1 cargo run -p secs-engine --release --example engine_perf -- mimo_profile_design big
# Omit SECS_PERF_THREADS / use --config shipped for shipped settings.
# Do not include cargo in a sampled process tree:
SECS_PERF_MEMORY_ONCE=1 python3 oracle/peak_memory.py -- \
  /home/user/wt/target-p16/release/examples/engine_perf mimo_iir big
python3 oracle/peak_memory.py -- <oracle-venv>/bin/python \
  oracle/bench_engine.py mimo_iir big --config shipped --memory-once
# Optional IR diagnostic dump (outside measured operations):
SECS_PERF_IR_DUMP=<scratch>/irs <engine-perf-binary> mimo_case big
```

## Verification and verdict

All requested gates finished in the foreground, after measurements stopped:

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| strict clippy, engine/DSP/Camilla/export, all targets, locked | pass |
| four-crate locked test suite | pass; engine 64/64, all other suites pass |
| forced-foreign release engine tests | pass; 64/64 |
| feature test registry | pass; all 148 referenced tests compiled/listed |
| upstream protected tree | pass; 12 paths match `60ac53ad3a3d` |
| Python syntax / diff whitespace | pass |

The initial strict-clippy run caught `chunks_exact_to_as_chunks` in the
new excitation loader. It was corrected and clippy rerun successfully.
No result goldens or tolerance gates were modified. No commit or push was
made. Raw sample JSONL and gate logs are in the P18 session scratchpad.

**Verdict against MIMO.md §6:** ideal FIR per-input speed fails both large
case configurations; IIR ratio is incomplete; Rust sampled memory is below
1 GB but comparison with Python is incomplete; local goldens pass, not a
claim about three-OS CI. `perf.mimo` remains **partial**. M3 must not be
marked complete from this audit.
