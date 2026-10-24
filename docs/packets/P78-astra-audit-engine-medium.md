# P78 (ASTRA): audit #92 — engine and DSP Medium findings (E2)

Repository: `/home/user/Sound-Equalization-and-Correction-System-2R`. Work in
the main tree: `git switch -c wip/e2` from the current `HEAD`. Do not push,
do not switch branches again, do not spawn sub-agents. Commit once per
finding (subjects below); Claude reviews, re-runs the gates and re-authors
the commits.

Read first: `CLAUDE.md`, `docs/adr/0004-parity-and-deviations.md`,
`docs/adr/0007-compute-and-gpu.md`, `docs/adr/0012-resource-optimizations.md`,
`docs/ENGINE.md`, `TECH_DEBT_AUDIT.md` (rows TD-48 to TD-52, TD-54, TD-75),
`crates/secs-engine/src/{xo.rs,chain.rs,measurement.rs,ba.rs,rc.rs,fit.rs,design.rs,decay.rs,mimo/*}`,
`crates/secs-engine/src/tests/*`, `crates/secs-dsp/src/iir.rs` and tests,
`crates/secs-compute/src/lib.rs` (cancel token).

Method: test first for A items; B (TD-75) keeps every test unchanged and
reports before/after counts. Results stay bit-identical (ADR 0012): goldens
and `_before` equivalence tests must pass untouched. Cancellation checks must
not change any arithmetic or ordering.

## Findings, in order

1. **TD-48** — `xo.rs:160`: `stage_responses(..).expect("complete measurement position")`
   panics when a non-reference position has mains but not every sub. Before
   scoring, check each scored position has every role `stage_responses`
   needs; return `EngineError::Input` naming the position and missing role.
   Seam: `recommend_xo` on a case where position 2 lacks a sub → `Input`
   error, no panic. Commit:
   `fix(audit): A-46 XO 추천이 빠진 측정을 입력 오류로 알립니다 (#92 TD-48)`
2. **TD-49** — cancellation does not reach: measurement preparation
   (`measurement.rs:199`, validation/onset/FFT loops), the BA bus-delay
   search (`ba.rs:1414`, takes no token), and RC stages outside the PEQ
   fitter (`rc.rs:655`: phase preparation, FIR ladder, wide trims,
   alternative phase candidate; `fit.rs:276` `fit_gd_cuts`). Thread the
   `CancelToken` (or `&Context`) through and check at position/role/candidate/
   iteration boundaries, returning `EngineError::Cancelled`. Seam: a cancel
   token that flips after N checks (a test-only counting wrapper, or cancel
   from another thread after a short delay on the largest test case) →
   `Cancelled` returned from each of the three stages; with no cancel the
   outputs are unchanged (existing goldens). Commit:
   `fix(audit): A-47 측정 준비, BA 버스 지연, RC 단계에도 취소가 닿습니다 (#92 TD-49)`
3. **TD-50** — `rc.rs:213`: `EngineConfig.taps > N_FFT` panics on a slice.
   Validate `taps` at the design entry (`1..=N_FFT`, or the real supported
   range — read where taps come from mode specs) and return
   `EngineError::Input`. Seam: `design` with `taps = 2 * N_FFT` → `Input`.
   Commit: `fix(audit): A-48 설계가 지원 범위 밖의 탭 수를 입력 오류로 거부합니다 (#92 TD-50)`
4. **TD-51** — `secs-dsp/src/iir.rs:144`: `butter(1, &[1.0, 100.0], Bandpass, 600.0)`
   produces two real poles that the `p.im > 0.0` filter drops, then
   `min_by(..).unwrap()` panics. Pair real poles in the band-pass SOS
   construction the way SciPy's `zpk2sos(pairing='nearest')` does. Expected
   coefficients: compute them once with SciPy 1.18.1 in a venv
   (`python3 -m venv /tmp/scipy && /tmp/scipy/bin/pip install scipy==1.18.1 numpy`),
   paste the literal values into the test with the command used in a
   comment; compare with the oracle tolerance used by neighbouring tests.
   Also cover order 3 wide band-pass. Commit:
   `fix(audit): A-49 넓은 대역의 홀수 차수 butter 대역통과가 패닉하지 않습니다 (#92 TD-51)`
5. **TD-52** — `tests/mimo_run.rs:138`, `tests/mimo_iir.rs:91`,
   `tests/mimo.rs:110` fold `(a-b).abs()` with `f64::max`, which ignores NaN.
   Add one test helper `max_abs_diff(a, b) -> f64` that returns `INFINITY`
   when any element pair is non-finite and differs (NaN vs anything), use it
   in all three. Red: feed a NaN array to the old fold (0) vs the helper (∞)
   in a unit test of the helper. Commit:
   `fix(audit): A-50 테스트의 오차 비교가 NaN을 삼키지 않습니다 (#92 TD-52)`
6. **TD-54** — `mimo/acceleration.rs:297`: on a CUDA failure the error's
   `detail` is dropped; only a broad `ComputeReason` survives. Keep the
   fallback behaviour; add a bounded `detail: Option<String>` (≤ 200 chars)
   to the compute report entry for the fallback (find the report type in
   `secs-types` `compute.rs`; adding an optional field with `#[serde(default)]`
   is wire-compatible — regenerate bindings with `cargo test -p secs-types`
   and commit them). Seam: the existing failing `RidgeBackend` fake
   (`FakeRidge` in `secs-service/tests/service.rs` or an engine-level fake)
   returning a detailed error → the report carries the detail. Commit:
   `fix(audit): A-51 GPU 실패의 상세 사유를 계산 보고에 남깁니다 (#92 TD-54)`
7. **TD-75** (B) — duplicated MIMO helpers: low-rate onset (first index above
   10 % of the absolute maximum, else 0) at `mimo/iir.rs:171`,
   `mimo/design.rs:213`, `decay.rs:51`; third-octave band edges at
   `mimo/design.rs:372`, `:435`, `mimo/iir.rs:689`. One `pub(crate)` helper
   each; `measurement::onset_index` (a `>=` rule) stays separate. Bit-identical.
   Commit: `refactor(audit): B-3 MIMO의 onset과 대역 경계 도우미를 하나씩으로 모읍니다 (#92 TD-75)`
8. **TD-96** — `RidgeSweep::new` (`mimo/acceleration.rs:84`) checks the
   cancel token only before and after the Householder factorisation
   (`mimo/solve.rs:36`); requests allow 720 taps, so M2's "cancel stops
   within 1 s" does not hold on large inputs. Check the token once per
   column of the factorisation (arithmetic order unchanged) and return
   `Cancelled`. Test: a token cancelled before the call on a large system
   returns `Cancelled` without finishing; an uncancelled run is
   bit-identical to before (existing goldens). Commit:
   `fix(audit): A-86 Householder 분해 중에도 취소를 확인합니다 (#92 TD-96)`
9. **TD-97** — `secs-dsp/src/fft.rs:18`: the thread-local `realfft`/`rustfft`
   planners keep every planned length forever, and resampling plans FFTs
   at source lengths. Keep a small per-thread cache of the most recently
   used lengths (for example 16 forward and 16 inverse plans, LRU) in front
   of the planners, rebuilding the planner when the cache evicts, so memory
   stays bounded while results are unchanged. Test: plan 100 distinct
   lengths on one thread → the cache holds at most the bound; outputs equal
   the uncached transform bit for bit. Commit:
   `fix(audit): A-87 FFT 계획 캐시에 상한을 둡니다 (#92 TD-97)`
10. **TD-104** — `mimo/iir.rs:953` `validate_request` does not bound
    `mimo_k_bands`/`mimo_a_bands`, and accepts a gain/Q range so narrow
    that `:275` `clamp(lb + 1e-6, ub - 1e-6)` gets `min > max` (a panic in
    `f64::clamp`). Read `bounds()` and where the band counts index, then
    reject in `validate_request` any tuning whose band counts exceed what
    the design supports or whose `bounds()` interval is not wider than
    `2e-6` for some parameter, with `EngineError::Input`. Test: the two
    boundary tunings → `Input`; the defaults pass. Commit:
    `fix(audit): A-92 MIMO IIR 조율 값의 범위를 입구에서 검사합니다 (#92 TD-104)`

## Allowed files

`crates/secs-engine/**`, `crates/secs-dsp/**`, `crates/secs-compute/**`
(only to pass the token), `crates/secs-gpu/**` (call sites), `crates/secs-types/src/compute.rs`
and regenerated bindings (TD-54), `crates/secs-service/**` (call sites and
the TD-54 test), `docs/DEVIATIONS.md` (if a previously panicking input now
has defined behaviour that differs from upstream), `Cargo.lock`.

## Gates (foreground; do not end the turn before they finish)

After each finding: `cargo fmt --all -- --check`,
`cargo clippy -p <touched> --all-targets --locked -- -D warnings`,
`cargo test -p <touched> --locked`. Engine tests take several minutes. At the
end `cargo test --workspace --locked` once.

## Report (final answer)

Per finding: commit hash, files, test names, red failure message, green
result (B: before/after counts). Then the last gate output tail and anything
left undone.
