# P194 (Daybreak): audit #186, Category B refactors in the writers, secs-dsp and the engine

Phase 2 of `/115dkk-skills:resolve-audit` for issue #186. This packet runs
AFTER Phase 1 (P183..P187) and the ADR implementation packets (P188..P192)
have been rebased into `fix/audit-186`. Start from the then-current
`fix/audit-186`; Claude creates the worktrees and branches before handing
you this packet. Two sections, two trees:

- Section 1 (writers, secs-dsp, CLI): worktree `E:\wt\audit-graph`, branch
  `audit/186-graph-b`, `CARGO_TARGET_DIR=C:\cargo-target\audit-graph`.
- Section 2 (engine): worktree `E:\wt\audit-engine`, branch
  `audit/186-engine-b`, `CARGO_TARGET_DIR=C:\cargo-target\audit-engine`.
  Do section 1 to the end (verification included) before section 2.

Commit on the section's branch, one commit per finding, in the order below.
Do not push, do not rebase, do not spawn sub-agents, do not touch
`audit-state.md`.

Allowed files: `crates/secs-graph/**`, `crates/secs-filters/**`,
`crates/secs-camilla/**`, `crates/secs-export/**`, `crates/secs-dsp/**`,
`crates/secs-engine/**`, `crates/secs-engine-testkit/**`,
`crates/secs-types/src/chain.rs` (the `BandType` method of B-25 only),
`apps/secs2r-cli/**` (B-16 only), `docs/ARCHITECTURE.md` (ADR 0045 step 6
only), the `Cargo.toml` of those crates and `Cargo.lock` (dependency edges
ADR 0045 removes), `features.toml` only to rename a moved test.

Forbidden: `crates/secs-service/**` (if a signature change needs a caller
update there, keep the old function as a thin wrapper and report),
`crates/secs-types/**` beyond the one method, the screens, `docs/**` other
than ARCHITECTURE 3절, `CHANGELOG.md`.

Read first: `CLAUDE.md` (the gates, ADR 0007's rules on parallelism and
accumulation order, the publishing rule that published crates read nothing
outside their directory), `CONTEXT.md`, `docs/adr/0004-goldens.md` (or the
ADR that defines golden parity; search `docs/adr` for "골든"),
`docs/adr/0045-writer-emit-helpers-in-secs-graph.md`,
`docs/adr/0042-engine-note-typed-value.md` (stage 3 already renamed the note
payloads), `docs/adr/0046-narrow-engine-interface.md`,
`docs/adr/0050-one-engine-test-loader.md`, `docs/ENGINE.md`, and the finding
texts quoted below. Line numbers are from the audit commit `97a27818` and
have moved; search by name.

## Why

Category B items are pure refactors. Here the strongest gate is bytes: the
golden tests of `secs-camilla`, `secs-filters`, `secs-export` and the
engine (`oracle/goldens/**` through each crate's `tests/golden.rs` and the
engine's `src/tests`) must pass without a single changed byte. Numeric code
keeps its addition order (ADR 0007). You write no new tests. If any
existing test has to change, undo the item and report it for
reclassification to Category A.

## The protocol for every item

Same as P193: announce; Before counts (`cargo test -p <crates> --locked`);
the structural change only; After counts (passed ≥ before, failed 0, same
test names); stop rule (undo with `git checkout -- <files you touched>`,
never `git checkout .`, report `RECLASSIFY → A` with the failing test and
assertion); gate `cargo fmt --all -- --check` and
`cargo clippy -p <touched crates> --all-targets --locked -- -D warnings`;
commit with `git add <files>` and

```
refactor(audit): B-n <한국어 한 문장> (#186 TD-xxx)

<무엇을 어디로 옮겼는지 한두 문장. 골든이 바이트 그대로 통과했다는 문장.>

Co-Authored-By: Claude Mythos 5.1 <noreply@anthropic.com>
```

(하십시오체, no em-dash, no program-as-subject verbs 묻다/적다/넘기다,
"추가하다" not "더하다"). ADR steps use `refactor(audit): X-4 <한국어> (#186 A4)`.
Before the first item of each section and after every fifth commit:
`cargo test --workspace --locked`.

## Covered elsewhere (verify only)

- B-13 (TD-251 testkit copy) → ADR 0050, P188: verify `secs-engine-testkit` is a facade before B-12.
- B-16's `export.rs` part (TD-195) → ADR 0043, P189.
- B-35's note payloads (TD-257 tuples) → ADR 0042 stage 3, P188: only the role-key lookup helper remains here.
- B-24 (TD-222) is NOT covered elsewhere: ADR 0045 has no packet yet, so its implementation is the first item below and B-24's remainder follows it.
- X-31/B-33 (A31, TD-255) is yours, with the goldens gate.

## Section 1: writers, secs-dsp, CLI (branch `audit/186-graph-b`)

### X-4 · A4 · ADR 0045, writer emit helpers into secs-graph (six steps, seven commits)

Implement `docs/adr/0045-writer-emit-helpers-in-secs-graph.md` `## 이행`
steps 1 to 6 exactly, one commit per step (step 3 is two commits, as the
ADR says). The ADR's `## 시험` says which tests move; moving a test module
is allowed here because it is the ADR's deepening step (the tests at the
new interface are the moved ones plus the `nan`/`inf`/negative-zero and
line-break cases the ADR names; those expected strings are CPython
literals, write them from the ADR, not from running the code).

- Step 1: `crates/secs-graph/src/emit/mod.rs` + `emit/pyfmt.rs` holding `round_ndigits`, `repr_f64`, `format_g`, `format_fixed`, `format_i3`, `format_s30` moved verbatim from `crates/secs-camilla/src/pyfmt.rs` (byte-identical function bodies; `nan`/`inf` early return as TD-267/A-52 already added in Phase 1, keep it). `secs_camilla::pyfmt` becomes `#[deprecated(since = "<next minor>", note = "use secs_graph::emit::pyfmt (ADR 0045)")] pub use secs_graph::emit::pyfmt::*;`. Commit `refactor(audit): X-4 pyfmt를 secs_graph::emit으로 옮깁니다 (#186 A4)`.
- Step 2: `secs-filters` and `secs-export` import from `secs_graph::emit::pyfmt` and drop `secs-camilla` from their `Cargo.toml` (`cargo deny --all-features check` and `cargo publish --workspace --dry-run --locked --allow-dirty` per CLAUDE.md's procedure must pass; run the dry run in a throwaway worktree as CLAUDE.md describes, with a fresh `CARGO_HOME`). Commit `refactor(audit): X-4 secs-filters와 secs-export가 secs-camilla 의존을 뺍니다 (#186 A4)`.
- Step 3a: `emit::note` (`mode_name`, `camilla_note`, `eqapo_note`, `eqapo_note_with_bands`) moved from the writers; 3b: `emit::guard` (`finite`, `identifier`, `distinct_outputs`, `no_line_breaks`) and `emit::grouped`, deleting the copies in the three writers and in `crates/secs-engine/src/notes.rs` (engine file allowed for this deletion only). Two commits.
- Step 4: `emit::fir_files(plan, prefix, rate) -> Result<Vec<(String, Vec<u8>)>, GraphError>` and the deletion of `secs-camilla`'s direct `MimoFilter` reading. The test compares the bytes with the existing golden WAVs at 48 k and 96 k (the ADR names the fixture). Commit.
- Step 5: the four atomic-replace copies (`secs_camilla::atomic_write` → deprecated re-export of `secs_types::fs::write_atomic_bytes`; `secs-export::atomic_replace` and `secs-project::record::atomic_write` are P193's crate boundary: `secs-export`'s is yours, `secs-project`'s is not), the four 48,000 constants → `secs_dsp::rate::DESIGN_RATE` with the doc fix, `headroom_db_half_db`, `f64::round_ties_even`, `ExportGraph::support_groups_rounded`. Two commits (constants; rounding and grouping).
- Step 6: `docs/ARCHITECTURE.md` 3절 diagram: remove the `secs-camilla → secs-filters/secs-export` arrows. Commit.

Gate after every step: `cargo test -p secs-graph -p secs-camilla -p secs-filters -p secs-export --locked` with every golden byte-identical.
Stop rule: a single golden byte or a moved function body that is not verbatim → undo the step.

### B-24 · TD-222 · the writer copies ADR 0045 did not name

Finding: "작성기 사이의 작은 복사본. 반 dB 헤드룸 올림 셋, `Fir → fir_to_rate`/
`FirLowRate → lowrate_to_rate` match 셋(한편 `FirView::materialize`는 호출자
없음), `round_ndigits(low_hz, 3)` 서포트 묶기 둘, ties-to-even 구현 둘, 48,000
상수 넷 …, `n_in`의 LFE 규칙 재계산, `finite`·`find_ops`·`comma`·"."/".." 가드
사본." After X-4, what remains: the `Fir → fir_to_rate` / `FirLowRate →
lowrate_to_rate` match triplet (→ `FirView::materialize_at(rate)` in
`secs-graph`, and `FirView::materialize` (48 k only, no caller) deprecated
one version), the `n_in` LFE recomputation (→ `Config` keeps `n_in`), the
`find_ops`/`comma`/`"."`/`".."` guard copies (→ `emit::guard`).

- Before/after per copy as above; files `crates/secs-graph/src/plan.rs` (or wherever `FirView` lives), `crates/secs-camilla/src/lib.rs`, `crates/secs-filters/src/{lib.rs, playback.rs, render.rs}`, `crates/secs-export/src/lib.rs`.
- Deleted: the three `match` copies and the guard copies.

Gate: the four crates' goldens byte-identical.
Commit subject: `refactor(audit): B-24 작성기 셋의 남은 복사본을 secs-graph의 도우미로 모읍니다 (#186 TD-222)`.

### B-7 · TD-178 · render buffers for unreachable input/output pairs

Finding: "`collect_routes`가 입력×출력 모든 짝에 `PairRoutes`를 만들고, 렌더
루프는 닿지 않는 짝에도 `candidate` 길이의 `Vec<f64>`를 만들어
보관합니다(`rendered.push(response)`, 503). 13입력×16출력 계획에서
`max_len=taps_at(1<<20, rate)`이면 반복마다 208×8 MiB(48 k)에서 208×32 MiB(192 k)가
대부분 0으로 상주합니다." Recommendation: drop pairs whose `routes` is empty
from `pairs` (or `continue` before the `vec![0.0; candidate]` at 491) and
discard all-zero responses before the next doubling iteration.

- Before: `crates/secs-filters/src/render.rs:345` `fn collect_routes(..)` builds a `PairRoutes` per (input, output); the loop at `:491` allocates `vec![0.0; candidate]` and `rendered.push(response)` for every pair.
- After: `collect_routes` skips pairs with no route (the output type carries only reachable pairs; the public `RenderedPaths` shape, which the convolution bundle and the goldens read, stays the same: an unreachable pair is still reported the way it is today, i.e. look at how the bundle writer treats an all-zero response and keep that observable result, for instance by reporting the pair as absent only where it was already absent in the written bundle). If the written bundle today contains a zero WAV for an unreachable pair, the WAV must still be written byte-identically; then the change is only to allocate lazily (`Option<Vec<f64>>` per pair, materialised as zeros at write time).
- Deleted: the eager zero buffers.

Files: `crates/secs-filters/src/render.rs` (and `lib.rs` where the bundle reads `RenderedPaths`).
Gate: `cargo test -p secs-filters --locked` (the convolution goldens, including the 13×16 plan fixture if one exists, byte-identical).
Commit subject: `refactor(audit): B-7 닿지 않는 입출력 짝의 렌더 버퍼를 만들지 않습니다 (#186 TD-178)`.
Stop rule: any convolution bundle byte change → reclassify.

### B-8 · TD-179 · the DTFT runs twice per path and FIRs are resampled per route

Finding: "경로마다 정확한 DTFT(O(F·N), 봉우리용 약 480 주파수 + CSV용 약 540)가
`path.response`와 `written`에 한 번씩 두 번 돕니다. `written`은
`attenuation`×f32 양자화한 `response`뿐이라 둘째 통과는 2^20~2^22 샘플을 약 1,000번
더 훑습니다. 또 `fir_to_rate`/`lowrate_to_rate`가 경로(route)마다 불려 입력 FIR이
메인 경로와 서브 전송 경로마다 다시 리샘플되고(≠48 k에서는 2^21점 FFT 세 번의
chirp-z), 48 k에서는 경로마다 `to_vec` 복제입니다." Recommendation: one DTFT
on the CSV grid from which the peak (`peak_gain_db`, with `headroom_db`,
20 Hz to 20 kHz) is read; memoise the expanded taps per `Op::Fir.name`
(or `(ptr, len, rate)`) inside `collect_routes` and share them as
`Arc<[f64]>`.

- Before: `crates/secs-filters/src/lib.rs` around the convolution bundle (`:1841-1862` at the audit commit: `path.response` then `written`), `render.rs:184-197` (`fir_to_rate`/`lowrate_to_rate` per route).
- After: the CSV grid DTFT is computed once per path and the peak search reads the same magnitudes (the peak frequency set today is a subset of the CSV grid? verify: if the 480-point peak grid is not a subset of the 540-point CSV grid, the peak value can differ by a bit, which changes `headroom_db` in the guide text: then this half is NOT a pure refactor; keep the two DTFTs and do only the memoisation half, and report). The resampled taps are memoised per FIR name in `collect_routes` as `Arc<[f64]>` and each route holds the `Arc`.
- Deleted: the second DTFT pass (if provably identical) and the per-route resampling.

Files: `crates/secs-filters/src/{lib.rs, render.rs}`.
Gate: `cargo test -p secs-filters --locked` byte-identical (the CSV and the guide text with `headroom_db` are in the goldens).
Commit subject: `refactor(audit): B-8 경로의 DTFT를 한 번만 돌리고 리샘플한 FIR 탭을 경로끼리 공유합니다 (#186 TD-179)`.
Stop rule: a CSV or guide byte change → undo the DTFT half.

### B-25 · TD-223 · RBJ peaking and TDF2 twice; BandType tables three times

Finding: "RBJ 피킹과 TDF2 처리가 같은 크레이트에 두 번 있고(렌더는 `SECTION_Q`에서
대역 섹션을 손으로, 재생은 피킹을 손으로 만들고 대역은 `secs_dsp::iir::butter`),
BandType→(계열, 차수/복사/Q) 매핑이 세 표에 있습니다." Recommendation: digital
RBJ peaking/all-pass/band biquad constructors (with `rate`) into `secs-dsp`,
the three tables derived from one `BandType` method in `secs-types`,
`SECTION_Q` kept as the five-digit APO view.

- Before: `crates/secs-filters/src/render.rs:70` `fn peaking(frequency, q, gain_db, rate)` and the TDF2 section code near it; `crates/secs-filters/src/playback.rs:85-96` `fn band(.., kind: BandType, high)` with the `(order, copies)` match and its own peaking; `crates/secs-camilla/src/lib.rs:500-508` `fn band_name(kind, highpass) -> (&'static str, i64)`; `crates/secs-filters/src/lib.rs:48-56` `SECTION_Q`.
- After: `crates/secs-dsp/src/iir.rs` gains `pub fn rbj_peaking(rate, hz, q, gain_db) -> Biquad`, `rbj_allpass(..)`, and a `pub struct Tdf2` (if the two TDF2 loops are the same arithmetic in the same order; if the render path accumulates in a different order from playback, keep both and report: ADR 0007 forbids changing accumulation order); `crates/secs-types/src/chain.rs` `impl BandType { pub const fn family(self) -> BandFamily /* LinkwitzRiley | Butterworth */; pub const fn order(self) -> u8; pub const fn cascades(self) -> u8 }` and the three tables read it (`band_name` in camilla formats `family` + `order`; playback's `(order, copies)` = `(order / 2, cascades)` or whatever reproduces the current numbers exactly: write the equivalence out in the report with the six variants); `SECTION_Q` stays as the APO five-digit table.
- Deleted: the duplicated peaking/TDF2 and two of the three tables.

Files: `crates/secs-dsp/src/iir.rs`, `crates/secs-types/src/chain.rs`, `crates/secs-filters/src/{render.rs, playback.rs, lib.rs}`, `crates/secs-camilla/src/lib.rs`.
Gate: `cargo test -p secs-dsp -p secs-filters -p secs-camilla -p secs-types --locked`; the playback goldens (`secs-filters` playback tests) and the CamillaDSP YAML goldens byte-identical; `cargo test -p secs-types` regenerates nothing (a `const fn` on an enum does not change the ts-rs output; check `git status apps/secs2r/ui/src/ipc/bindings` shows only line-ending churn).
Commit subject: `refactor(audit): B-25 RBJ 피킹과 TDF2를 secs-dsp에 하나씩 두고 BandType 표 셋을 메서드 하나로 끌어냅니다 (#186 TD-223)`.
Stop rule: any playback sample or YAML byte change → reclassify (floating-point order).

### B-16 (remainder) · TD-195 · the 600-line functions outside `export.rs`

Finding: "`make_config_observed`(777줄), `eqapo_filters`(375줄, 지역 struct 셋과
`move_fixed`/`share_delay`가 인라인), CLI `main.rs`(clap 모델·runner·`deps`·`run`
280줄 match·시험이 한 파일)." Recommendation: `make_config_observed` keeps
its single body (ADR 0045's 결정) but the pure builders
(`support_groups_for_camilla`, `head_comment`, `title_and_description`,
`serialize`) are extracted; `eqapo_filters` splits into `send_groups`,
`bass_buses`, `support_groups_apo`; the CLI splits into `args.rs`,
`runner.rs`, `commands.rs` without changing a flag.

- `crates/secs-camilla/src/lib.rs::make_config_observed` (713): extract the four pure builders as private functions called in the same order; the YAML bytes are the gate.
- `crates/secs-filters/src/lib.rs::eqapo_filters` (412): the three local structs become module-level private types; `move_fixed`/`share_delay` become private functions; the body becomes `send_groups` + `bass_buses` + `support_groups_apo` called in the current order.
- `apps/secs2r-cli/src/main.rs` (1,245 lines): `args.rs` (the clap `Cli`, `Action`, `ApplyAction`, `HostAction`, `ProjectAction`, the `*Choice` enums, `InputHpArg`, `RcBandsArg`), `runner.rs` (`Events`, `Runner`, `lock`, `response_job`, `deps`), `commands.rs` (`run`, `check`, `full_run`, `untagged_rows_use_position`, `export_kind_name`, `print_json`), `main.rs` keeps `main` and `mod` lines; the `#[cfg(test)] mod tests` moves with the code it tests (split into the three files' test modules by subject). The CLI's `--help` output and every flag must be identical: capture `cargo run -p secs2r-cli -- --help` (and each subcommand's `--help`) before and after into files in `C:\cargo-target\audit-graph\` and diff them; the diff must be empty.
- Deleted: nothing.

Three commits: `refactor(audit): B-16 make_config_observed의 순수 빌더를 함수로 뽑습니다 (#186 TD-195)`, `refactor(audit): B-16 eqapo_filters를 송출, 저역 버스, 서포트 묶음으로 나눕니다 (#186 TD-195)`, `refactor(audit): B-16 CLI를 인자, 실행기, 명령 파일로 나눕니다 (#186 TD-195)`.
Gate: camilla and filters goldens byte-identical; `cargo test -p secs2r-cli --locked` and the `--help` diffs empty.

## Section 1 verification (foreground)

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo deny --all-features check
python3 .github/scripts/check_feature_tests.py
git log --oneline fix/audit-186..audit/186-graph-b
```

## Section 2: engine (branch `audit/186-engine-b`, tree `E:\wt\audit-engine`)

Run `cargo test -p secs-engine --locked` first (long: the goldens) and record
the counts. Every item below ends with the same command and every golden
must be bit-identical. Numeric helpers move verbatim; addition order never
changes (ADR 0007).

### B-32 · TD-254 · dead code behind module-level allows

Finding: "dead_code를 강제로 켜고 라이브러리를 빌드하면 module 단위
`#[allow(dead_code)]` 13개 뒤에 쓰이지 않는 항목 21개가 나옵니다.
`mimo::iir::solve_problem`은 어디에서도 참조되지 않고, `fit_peq`/`fit_peq_until`,
`WINDOW_MS`/`windowed`, `correction`/`correction_traced`, `design_prepared`,
`design_input(_observed)`, `outcome`, `worst_band_db`, `BandGuard::evaluate`,
`solve_joint`, `design_mimo_iir_prepared`, `RcOutput.design_db`,
`PostResult.{level_cut,violations,gain_steps}`, `SeedResult.{cost,nfev,status}`는
시험 전용 도우미가 생산 코드에 컴파일됩니다. `secs-dsp/src/fft.rs:137`의
`rfft_owned`에 붙은 `#[allow(dead_code)]`는 `fir.rs:70`이 쓰므로 미래의 죽은
항목만 가립니다. `mimo/run.rs:156-212`는 `notes.rs:1`의 규칙과 달리 한국어 UI
문장(`availability`, `cap_text`, `fir_budget_text`, …)을 만듭니다." Decision
(open question 9): the Korean strings in `mimo/run.rs` stay; cite that in a
comment at `run.rs` above `availability`: `// UI text by design: the
service shows these sentences unchanged (audit #186 TD-254, maintainer
decision 2026-11-06).`

- Before: the thirteen `#[allow(dead_code)]` in `crates/secs-engine/src/lib.rs:39-79` and the ones in `mimo/iir.rs`, `fit.rs`, `measurement.rs`, `secs-dsp/src/fft.rs:137`.
- After: reproduce the list by building with the allows removed (`cargo build -p secs-engine` after deleting them; collect the warnings); each unused item is either deleted (`solve_problem`, no reference anywhere including tests) or moved behind `#[cfg(any(test, feature = "test-support"))]` when tests use it (`fit_peq`, `fit_peq_until`, `WINDOW_MS`, `windowed`, `correction`, `correction_traced`, `design_prepared`, `design_input(_observed)`, `outcome`, `worst_band_db`, `BandGuard::evaluate`, `solve_joint`, `design_mimo_iir_prepared` and the struct fields: a field cannot be cfg'd without changing the struct's layout for tests, so fields that only tests read get `#[cfg_attr(not(any(test, feature = "test-support")), allow(dead_code))]` on the field, which is the narrowest allow); the thirteen module-level allows are deleted; `rfft_owned`'s allow is deleted. After ADR 0046 (P188) the surface is narrower, so re-derive the list on the current tree rather than from the audit.
- Deleted: the module-level allows and `solve_problem` (and anything else with zero references).

Files: `crates/secs-engine/src/**`, `crates/secs-dsp/src/fft.rs`.
Gate: `cargo test -p secs-engine --locked` (goldens), `cargo clippy -p secs-engine --all-targets --locked -- -D warnings`, and a build with `RUSTFLAGS="-W dead_code"` shows no warning in `secs-engine`.
Commit subject: `refactor(audit): B-32 엔진의 module 단위 dead_code 허용을 걷어내고 쓰이지 않는 항목을 정리합니다 (#186 TD-254)`.

### X-31 / B-33 · A31, TD-255 · `geomspace`, `median`, `interp`, `unwrap`/`gradient` copies

Finding: "`median` 네 벌이 가장자리에서 다릅니다(levels/decay는 빈 입력·NaN에
NaN, ba/fit은 `x[m-1]`을 색인해 빈 입력에서 패닉). `fit::interp`는
`secs_dsp::interp`의 NaN·클램프 규칙이 없습니다. `geomspace`는 비트 동일한 정의
다섯에 200점 고정 변형 하나이고 …, 위상 `unwrap`/`gradient`는 세 벌로 NaN 처리가
다르며 …, 취소 검사 도우미가 세 모양으로 열 곳, `MemoryError → EngineError`
매핑이 네 곳입니다. 통과대역 최댓값 규칙이 두 벌인데 `low_protect`만
`indices.is_empty()`를 막아 `support_ranges`는 … `indices[first_max(..)]`(:173)에서
패닉합니다." A31: "`secs_dsp::grid::geomspace`, `secs_dsp::stats::median`(NumPy와
같은 NaN 전파) 하나씩. 덧셈 순서가 같아 골든 불변이 기대되지만 확인이 필요."
The cancel helpers are ADR 0047 (P188) and the `MemoryError` mapping is A-26
(P186): not yours. The `support_ranges` empty-indices panic is a behaviour
question (Category A): report it, do not change it.

- `geomspace`: six definitions (`design.rs:302` `pub(crate)`, `rc.rs:356`, `fit.rs:403`, `scorecard.rs:36`, `preview.rs:241`, `mimo/design.rs:91` with fixed 200 points). Diff all six bodies first; those that are byte-identical move to `secs_dsp::grid::geomspace(lo, hi, n)`; a variant whose formula differs (for instance `exp(ln lo + i·(ln hi − ln lo)/(n−1))` vs `lo·(hi/lo)^(i/(n−1))`) is NOT the same function bit for bit: keep it where it is and report which one differs. The MIMO 200-point variant becomes `secs_dsp::grid::geomspace(start, end, 200)` only if its body is the same formula.
- `median`: four definitions (`levels.rs:45`, `decay.rs:201`, `ba.rs:89`, `fit.rs:423`). Keep `decay::median` semantics (NaN on empty, NaN propagation) as `secs_dsp::stats::median(&[f64]) -> f64`; `levels.rs` uses it (same semantics); `ba.rs` and `fit.rs` panic on empty today: their callers must keep panicking semantics or the goldens may hide a changed path, so at those two call sites add an explicit `assert!(!values.is_empty(), "…")` before calling the shared `median` (a panic stays a panic; the message changes, which no test asserts: verify by grep for the old panic text in tests).
- `fit::interp` (`fit.rs:413`) vs `secs_dsp::numeric::interp` (`numeric.rs:50`): the NaN/clamp rules differ. The engine golden decides: replace and run the goldens; if any byte changes, keep `fit::interp` and report.
- `unwrap`/`gradient`: `rc.rs:458,485` and `scorecard.rs:47,64` (and the third copy the audit counts, find it with `grep -n "fn unwrap\|fn gradient" crates/secs-engine/src -r`). Only `rc.rs` cleans non-finite values. Consolidation changes NaN handling for the other two, so this is Category A unless the goldens prove the inputs never carry NaN: move only if the goldens pass byte-identically, otherwise report.
- Deleted: the moved copies.

Files: `crates/secs-dsp/src/{grid.rs, stats.rs (new), numeric.rs, lib.rs}`, `crates/secs-engine/src/{design.rs, rc.rs, fit.rs, scorecard.rs, preview.rs, levels.rs, decay.rs, ba.rs, mimo/design.rs}`.
Gate: `cargo test -p secs-dsp -p secs-engine --locked`, every golden bit-identical; `secs-dsp` is published: new `pub` items are additive and must have a doc line.
Commit subject: `refactor(audit): X-31 geomspace와 median을 secs-dsp 하나씩으로 모읍니다 (#186 A31)`; a second commit `refactor(audit): B-33 보간과 위상 unwrap의 사본을 정리합니다 (#186 TD-255)` only for the parts the goldens allowed.
Stop rule: any golden bit → keep the copy, report which.

### B-34 · TD-256 · `design_mimo_before` is a 150-line copy of the accelerated path

Finding: "'독립 비트 oracle'이 `design_mimo_prepared_accelerated`의 150줄 사본이고
`validate_request`의 인라인 사본(:1540-1550)까지 들어 있어 `mimo_early_ms`와
`seat_guard`를 넣을 때 이미 같이 고쳐야 했습니다." Recommendation: call
`validate_request`, share the range/usable/note prelude, keep only the
per-input preparation that differs.

- Before: `crates/secs-engine/src/mimo/design.rs:1522` `pub(crate) fn design_mimo_before` (`cfg(test)`) duplicating `design_mimo_prepared_accelerated` (1292) and `validate_request` (1488).
- After: `design_mimo_before` calls `validate_request(req)?` and a shared `fn prepare_prelude(..) -> Prelude` (ranges, usable set, note prelude) that both functions use; it keeps its own per-input loop (the point of the oracle is a second, bit-independent solve of the normal equations; that part must stay separate). The oracle tests that compare the two paths (`tests/mimo.rs`, search `design_mimo_before`) pass unchanged.
- Deleted: the duplicated prelude and the inline validation.

Files: `crates/secs-engine/src/mimo/design.rs`.
Gate: `cargo test -p secs-engine --locked`.
Commit subject: `refactor(audit): B-34 MIMO 비트 oracle이 검증과 전주를 생산 경로와 공유합니다 (#186 TD-256)`.

### B-35 (remainder) · TD-257 · role-key lookups

Finding (remaining part): "역할 키 데이터는 `Vec<(String, T)>`를
`.iter().find(..).unwrap()`으로 찾는 자리가 약 30곳이고(삽입 순서가 이유,
`input.rs:63-65`) 자리마다 '역할이 있다'는 불변식을 다시 유도합니다."
Recommendation: a `get(&self, role)` helper or an order-preserving `RoleMap`
newtype that turns the unwraps into one `missing role` error.

- Before: `grep -c "\.iter()\.find(" crates/secs-engine/src -r` gives the sites (ba.rs 7, design.rs 5, xo.rs 2, measurement.rs 2, chain.rs 1, fit.rs 1, input.rs 1, mimo/run.rs 1 at the audit commit).
- After: `crates/secs-engine/src/input.rs` gains `pub(crate) struct RoleMap<T>(Vec<(String, T)>)` with `get(&self, role) -> Option<&T>`, `get_mut`, `iter()` in insertion order, `push`, `FromIterator`; the sites use `.get(role).expect("role present: …")` where the invariant holds today (the panic stays a panic, same as the current `unwrap`; turning it into an error is Category A and is A-49's business), so this item only removes the duplicated lookup code. Keep the `Vec<(String, T)>` as the inner type so the serialized/ordered behaviour is unchanged.
- Deleted: the thirty inline `find` closures.

Files: `crates/secs-engine/src/{input.rs, ba.rs, design.rs, xo.rs, measurement.rs, chain.rs, fit.rs, mimo/run.rs}`.
Gate: `cargo test -p secs-engine --locked`.
Commit subject: `refactor(audit): B-35 역할 키 조회를 RoleMap 하나로 모읍니다 (#186 TD-257)`.

### B-36 · TD-259 · pair correlation and pair-phase recompute windows and stages

Finding: "`pair_correlation`이 모든 짝의 두 메인에 대해 위치마다
`direct_window`(IR 전체의 `onset_index` 스캔 + 4096점 FFT)를 다시 계산해 셀마다
(M−1)번 스캔하고 `polarity::check`는 취소 토큰을 보지 않습니다. 채널 사이 위상
검사는 메인마다 전체 `stage_responses`(…)를 부르고 약 440빈의 `top_xo`만 쓰는데
… `low_protect::rolloff`(…)는 `low_protect`나 `lf_align`이 켜지면 채널마다 세 번
돕니다." Recommendation: compute the (position, main) windowed spectra once
before the pair loop and check the token per position; a `stage_top` (or a
stop flag after `top_xo`) with the temporary memory either reserved or
documented as deliberately unreserved; compute `rolloff` once in
`prepare_correction` and pass it in `FirPreparation`.

The cancel-token check in `polarity::check` is a behaviour change (Category
A, cancellation becomes observable): leave it out and report it. The three
memoisations are refactors with a bit-identical gate.

- `crates/secs-engine/src/polarity.rs:92` `pair_correlation(a, b, rate)` and `:25` `direct_window`: hoist the windowed spectrum per (position, main) into a `Vec` computed once in `check` (`:200`) and pass slices to `pair_correlation` (keep a thin wrapper with the old signature for the unit tests that call it directly).
- `crates/secs-engine/src/design.rs:557,1113-1134` pair-phase: add `chain::stage_top(..)` that computes only the bins `top_xo` needs by calling the same functions on the same inputs in the same order up to that point (if `stage_responses` cannot be cut without changing the order of floating-point operations that feed `top_xo`, keep the full call and only add the reservation comment the finding asks for; report which).
- `crates/secs-engine/src/low_protect.rs:16` `rolloff`: computed once per channel in `prepare_correction` and carried in `FirPreparation`.
- Deleted: the repeated computations.

Files: `crates/secs-engine/src/{polarity.rs, design.rs, chain.rs, low_protect.rs}`.
Gate: `cargo test -p secs-engine --locked` bit-identical.
Commit subject: `refactor(audit): B-36 극성과 채널 사이 위상 검사가 창 스펙트럼과 롤오프를 한 번만 계산합니다 (#186 TD-259)`.
Stop rule: any golden bit → undo that memoisation.

### B-37 · TD-260 · `design_pass` is 615 lines

Finding: "`design_pass`는 615줄 함수로 극성, Schroeder 상한, 레벨, BA, 상단 PEQ,
버스 지연, RC 맵, 서브, 성적, 채널 사이 위상을 섞고, RC의 두 반쪽은 343줄과
257줄입니다." Recommendation: split at the existing `observed(..)` boundaries
into `ba_stage`/`rc_stage`/`score_stage` passing the small structs that
already exist (`ChannelBytes`, `RcPass`); the numeric order does not change.

- Before: `crates/secs-engine/src/design.rs:530` `fn design_pass(..)`; `rc.rs:868-1211` and `1213-1470` the two RC halves.
- After: `design_pass` calls `polarity_stage`, `levels_stage`, `ba_stage`, `rc_stage`, `sub_stage`, `score_stage`, `pair_phase_stage` in the current order, each a private function whose body is the moved lines verbatim and whose inputs/outputs are the existing structs (`ChannelBytes`, `RcPass`, …) or a new private struct holding exactly the locals that cross the boundary; the `observed(..)` calls stay at the same points. The two RC halves get the same treatment only if a boundary already exists as a comment or an `observed` call; otherwise leave `rc.rs` and report.
- Deleted: nothing.

Files: `crates/secs-engine/src/{design.rs, rc.rs}`.
Gate: `cargo test -p secs-engine --locked` bit-identical; `cargo clippy` with `-D warnings` (too_many_arguments: use the structs, not an allow).
Commit subject: `refactor(audit): B-37 design_pass를 observed 경계에서 단계 함수로 나눕니다 (#186 TD-260)`.

### B-14 · TD-253 · `applied_response` recomputes per (position, input, support)

Finding: "`applied_response`가 (위치, 입력, 서포트)마다 `support_path`(지연 램프 +
`hp2` + 곱: 전체 격자 `Vec<C>` 셋, 빈마다 `from_polar` 둘)와
`support.response(f)`/`fir_response`를 다시 계산하지만, 경로는 (위치, 서포트)에만,
필터는 (입력, 서포트)에만 의존합니다. P·M·S번의 전체 격자 평가가 직렬로, 어떤
permit 밖에서 돕니다." Recommendation: precompute the (position, support)
path spectra and the (input, support) filter responses once in
`assemble_prepared`, put them in the held reservation, and multiply per
position.

- Before: `crates/secs-engine/src/mimo/run.rs:420` `fn applied_response`, `:467` `assemble_prepared`, `support_path` (search).
- After: `assemble_prepared` computes `path_spectra[(position, support)]` and `filter_responses[(input, support)]` once (same functions, same inputs, same order) and reserves their memory through the existing `memory::reserve` call (the amount: P·S + M·S grid vectors of `C`); `applied_response` multiplies the precomputed values in the same order the inline code did. Floating-point: the product `path × filter` per bin must be formed from the same two operands in the same order as today; if the inline code accumulates several supports in a loop, the loop order stays.
- Deleted: the recomputation.

Files: `crates/secs-engine/src/mimo/run.rs`, `crates/secs-engine/src/memory.rs` (reservation accounting only).
Gate: `cargo test -p secs-engine --locked` (the `engine_mimo_run` goldens) bit-identical.
Commit subject: `refactor(audit): B-14 MIMO 적용 응답의 경로 스펙트럼과 필터 응답을 미리 한 번 계산합니다 (#186 TD-253)`.
Stop rule: a single bit → undo, report (the audit marked it speculative).

### B-12 · TD-250 · `engine_perf` re-includes 28 source files

Finding: "성능 예제가 엔진 소스 파일 28개를 `#[path]`로 다시 포함하고
(`#![allow(dead_code)]`와 함께) 크레이트의 두 번째 private 사본을 컴파일해
`design_observed`·`mimo::design::design_input`·`mimo::solve`를 잽니다."
Recommendation: expose the timing hooks through the `test-support`
feature's `testkit.rs` and make the example an ordinary consumer of
`secs_engine` + `secs_engine_testkit`.

- Before: `crates/secs-engine/examples/engine_perf.rs` with 28 `#[path = "../src/…"] mod …;` lines.
- After: `crates/secs-engine/src/testkit.rs` (ADR 0050 put the loader there) gains `pub mod perf { pub fn design_observed(..), pub fn mimo_design_input(..), pub fn mimo_solve(..) }` behind `test-support`, each a one-line re-export/wrapper of the private function; the example's `Cargo.toml` entry gets `required-features = ["test-support"]`; the example imports `secs_engine::testkit::perf` and `secs_engine_testkit` fixtures; the `#[path]` block and the `#![allow(dead_code)]` go. `cargo build -p secs-engine --example engine_perf --features test-support --release` must compile and `cargo run … -- --help` (or whatever its argument surface is) prints the same text.
- Deleted: the 28 `#[path]` lines.

Files: `crates/secs-engine/examples/engine_perf.rs`, `crates/secs-engine/src/testkit.rs`, `crates/secs-engine/Cargo.toml`.
Gate: `cargo build -p secs-engine --examples --features test-support --release`; `cargo test -p secs-engine --locked`.
Commit subject: `refactor(audit): B-12 성능 예제가 소스를 다시 포함하지 않고 testkit의 훅을 씁니다 (#186 TD-250)`.

## Section 2 verification (foreground)

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 .github/scripts/check_feature_tests.py
git log --oneline fix/audit-186..audit/186-engine-b
```

## Report

Per item: before/after counts, commit hash, files, what was deleted, and
for every numeric item the statement "goldens bit-identical" or the name
of the first golden that differed (then the item is undone). Then: items
reclassified or partially done with the reason (which copy differed, which
DTFT grid is not a subset, which `unwrap` carries NaN), what is left for
Claude (the `support_ranges` panic, the cancel token in `polarity::check`,
the junction of the service), and the two branches' `git log --oneline`.
