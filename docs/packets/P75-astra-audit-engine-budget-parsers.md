# P75 (ASTRA): audit #92 — engine memory budget, mdat and REW callback bounds (E1 + L2)

Repository: `/home/user/Sound-Equalization-and-Correction-System-2R`. Work in
the main tree: `git switch -c wip/e1` from the current `HEAD`. Do not push,
do not switch branches again, do not spawn sub-agents. Commit once per
finding (subjects below); Claude reviews, re-runs the gates and re-authors
the commits.

Read first: `CLAUDE.md`, `docs/adr/0007-compute-and-gpu.md` (determinism:
parallel map only, floating-point accumulation in the original order),
`docs/adr/0008-native-gpu-acceleration.md`, `docs/adr/0012-resource-optimizations.md`,
`docs/MIMO.md` (section 6, 1 GB), `TECH_DEBT_AUDIT.md` (rows TD-18, TD-21,
TD-23, TD-24), `crates/secs-compute/src/lib.rs`, `crates/secs-engine/src/{design.rs,xo.rs,mimo/design.rs,mimo/prepared.rs,mimo/run.rs}`,
`crates/secs-engine/src/tests/{mimo.rs,mimo_run.rs}`, `crates/secs-mdat/src/{lib.rs,source_info.rs}`
and `tests/`, `crates/secs-rew/src/callback.rs` and its tests.

Method: test first. Red at the named seam (confirm the failure is the
finding's), minimum green, crate tests. Results must stay bit-identical
(ADR 0012): every existing golden and `_before` equivalence test must pass
unchanged.

## Findings, in order

### 1. TD-23 — no blocking on the memory budget inside a Rayon task

`mimo/design.rs:1203-1220` acquires a `MemoryBudget` permit inside
`ctx.try_map` over the main inputs, and the body runs a nested `ctx.map`
(`:349-354`) on the same pool. While a task waits for its nested map, Rayon
may run another input task on the same stack; that task blocks in
`MemoryBudget::acquire`'s condvar and the permit below it can never be
released (until the user cancels).

Design (fixed): never wait on the budget inside a pool task. Before the
`try_map`, on the calling thread, split the inputs into consecutive batches
whose summed `input_bytes(..)` fit `ctx.budget().total()`; for each batch,
acquire one permit for the batch's bytes on the calling thread, then run the
batch's inputs with `ctx.try_map` (the body does no acquire); results are
concatenated in input order, so outputs are byte-identical. A single input
larger than the whole budget keeps today's `TooLarge` error. Apply the same
rule to any other `acquire` you find inside `ctx.map`/`try_map` bodies in the
engine (grep `acquire(`).

Seam: `mimo::compute` (or `design_mimo` at crate level) on the smallest MIMO
case in `src/tests/mimo_run.rs` with `Context::new(4, MemoryBudget::new(b), …)`
where `b` fits exactly one input → completes within a timeout (run it on a
thread and `recv_timeout`; red = timeout) and equals the result with an
unbounded budget bit for bit.

Commit: `fix(audit): A-23 MIMO 입력 예산을 병렬 작업 안에서 기다리지 않습니다 (#92 TD-23)`

### 2. TD-24 — design and MIMO preparation reserve memory; XO shares the job's budget

- `mimo/prepared.rs:18` rebuilds 500 ms and then 2 s spectra for every IR
  outside any permit (27 roles × 17 positions ≈ 1.4 GiB at peak).
- `design.rs:419` reserves nothing.
- `xo.rs:266-271` builds a new full-size `MemoryBudget` per XO candidate.

Design (fixed):
- `secs-compute`: make `Context` hold its budget behind an `Arc` and add
  `pub fn child_serial(&self) -> Context` — one thread, the same budget and
  cancel token, no progress. Existing constructors keep their signatures.
- `recommend_xo` uses `ctx.child_serial()` instead of the hand-built context.
- Before allocating the prepared spectra in MIMO preparation, acquire a
  permit for their bytes (count = IR cells × bins × 16 bytes for each spectra
  set alive at the same time) on the calling thread and hold it until the
  preparation's buffers are dropped; with the batch rule of finding 1 the
  per-input permits come out of what remains.
- `design`: acquire a permit for the shared preparation and the per-channel
  RC working set before they are allocated (estimate from the arrays the code
  actually allocates; write the formula next to the acquire with the array
  names). Too small a budget returns the existing `TooLarge`/input error
  early instead of allocating.

Seams: (a) `Context` with a budget below the MIMO preparation's need → error
before allocation (check with the error variant, and that no spectra were
built — e.g. via the existing stage observer or a counting probe); (b)
`recommend_xo` with a `Context` whose budget fits one design → completes and
the budget's available bytes return to the total afterwards; (c) all
existing engine goldens unchanged.

Commit: `fix(audit): A-24 설계와 MIMO 준비가 메모리를 먼저 예약하고 XO는 잡 예산을 나눠 씁니다 (#92 TD-24)`

### 3. TD-18 — mdat source info stays inside the parse budget

`secs-mdat/src/source_info.rs:125`: after `load_mdat_project`, IR and
source-info extraction no longer charge the JOS/MDAT allocation budget; when
many measurements reference the same large `meterCal` array, each extraction
clones it (8 MiB × 200 references → 1.6 GiB from a small file).

Design (fixed): thread the remaining budget (the same `Limits`/cost counter
the adapter uses) into `extract_ir` and `source_info::extract`; charge every
array or string copy before making it; exceeding the budget returns the
existing limit error. Seam: a small synthetic graph (built with the crate's
test helpers or `secs-jos` writer) where 200 measurements reference one
8 MiB double array → `load_mdat_project` returns the budget error quickly; the
oracle mdat goldens still load byte-identically.

Commit: `fix(audit): A-18 mdat 출처 정보 추출도 할당 예산을 씁니다 (#92 TD-18)`

### 4. TD-21 — REW callback server stops while a client keeps sending

`secs-rew/src/callback.rs:247` drains large bodies with `io::copy`, and the
body read at `:258` does not check cancellation; a local client that keeps
sending faster than the 1 s socket timeout keeps `stop()` waiting on the
worker join. The path token is checked only after the body is read.

Design (fixed): check the path token right after the request line and
headers (reject before reading the body); read and drain bodies in chunks of
at most 64 KiB, checking the cancel flag and a per-request deadline (e.g.
5 s) between chunks; on cancel or deadline close the connection. Normal
callbacks (REW's real POSTs, the existing tests) behave the same.

Seam: test that connects a `TcpStream`, sends headers with a large
`Content-Length` and keeps writing a byte every 100 ms, then calls `stop()`
→ returns within ~2 s (red: it hangs; use a thread + timeout); a request
with a wrong token gets the rejection without the body being read.

Commit: `fix(audit): A-21 REW 콜백 서버가 계속 보내는 연결에도 멈춥니다 (#92 TD-21)`

## Allowed files

`crates/secs-compute/**`, `crates/secs-engine/**`, `crates/secs-gpu/**` (only
if a `Context` signature change needs a call-site update), `crates/secs-mdat/**`,
`crates/secs-jos/**` (only if the budget type lives there), `crates/secs-rew/**`,
call sites in `crates/secs-service/src/**` and `apps/secs2r-cli/src/**` that
construct a `Context` (signature-preserving updates only), `docs/MIMO.md`
(memory section) and `docs/DEVIATIONS.md` (if a bound is new to upstream),
`Cargo.lock` as cargo changes it.

## Gates (foreground; do not end the turn before they finish)

After each finding:

```sh
cargo fmt --all -- --check
cargo clippy -p <touched crates> --all-targets --locked -- -D warnings
cargo test -p <touched crates> --locked
```

`secs-engine` tests run optimized (`[profile.test] opt-level = 2`) and take
several minutes; run them in the foreground. After findings 3 and 4, also
the untrusted-input checks if the nightly toolchain is installed
(`rustup toolchain list | grep nightly-2026-10-01`):
`cargo +nightly-2026-10-01 miri test --locked -p secs-mdat`; otherwise say so.

## Report (final answer)

Per finding: commit hash, files, test names, red failure message, green
result, and for finding 2 the byte formulas you used. Then the last gate
output tail, anything left undone.
