# MIMO performance audit (P18)

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
