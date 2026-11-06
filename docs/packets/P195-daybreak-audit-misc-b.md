# P195 (Daybreak): audit #186, Category B refactors in secs-audio, secs-jos, the CI scripts, secs-types and the policy gates

Phase 2 of `/115dkk-skills:resolve-audit` for issue #186. This packet runs
AFTER Phase 1 (P183..P187) and the ADR implementation packets (P188..P192)
have been rebased into `fix/audit-186`. Start from the then-current
`fix/audit-186`: Claude creates the worktree `E:\wt\audit-host` on a fresh
branch `audit/186-misc-b` from it. Work only there. Every cargo command runs
with `CARGO_TARGET_DIR=C:\cargo-target\audit-host`. Commit on that branch,
one commit per finding, in the order below. Do not push, do not rebase, do
not spawn sub-agents, do not touch `audit-state.md`. This machine is
Windows 11, so the Windows-only code compiles and runs here.

Allowed files:

- `crates/secs-audio/**` (B-21);
- `crates/secs-jos/**` (B-27);
- `.github/scripts/**` (B-30/X-33);
- `apps/secs2r-host-shell/src/main.rs` and `crates/secs-sys/**` (B-31: the
  `cfg` spelling only; the `unsafe` count of `secs-sys` must not change);
- `crates/secs-types/**` (X-29, X-30; run `cargo test -p secs-types --locked`
  after a change and commit only the binding files whose `git diff` has
  content changes, the rest is line-ending churn);
- `apps/secs2r/src/host.rs` (X-30);
- `crates/secs-policy/**` (X-32);
- `features.toml` only to rename a moved test.

Forbidden: everything else, in particular `crates/secs-service/**`,
`crates/secs-engine/**`, the screens, `docs/**`, `CHANGELOG.md`.

Read first: `CLAUDE.md`, `CONTEXT.md`, `docs/adr/0003-*.md` (the IPC
method rules; search `docs/adr` for "메서드 id"), `docs/adr/0016-public-rust-api-and-local-api.md`
(the access table of the local API), `docs/adr/0018-applying-to-camilladsp-and-equalizer-apo.md`
(the shell and the host), `.github/scripts/README.md` if present, and the
finding texts quoted below. Line numbers are from the audit commit
`97a27818` and have moved; search by name.

## Why

Category B items are pure refactors. The observable behaviour (bytes,
answers, wire shapes, the CI scripts' outputs) must be identical before and
after. The existing suites are the gates: `cargo test` for the crates,
`python3 -m unittest discover -s .github/scripts -p "test_*.py"` for the
scripts. No new tests. If an existing test has to change, undo and report
for reclassification to Category A.

## The protocol for every item

Same as P193: announce; Before counts; the structural change only; After
counts (passed ≥ before, failed 0, same names); stop rule (undo with
`git checkout -- <files you touched>`, never `git checkout .`; report
`RECLASSIFY → A`); gate `cargo fmt --all -- --check`,
`cargo clippy -p <touched crates> --all-targets --locked -- -D warnings`
(for `secs-sys` also the CI's Windows lint
`cargo clippy --workspace --exclude secs2r --exclude secs2r-host-shell --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings`);
commit with `git add <files>` and

```
refactor(audit): B-n <한국어 한 문장> (#186 TD-xxx)

<무엇을 어디로 옮겼는지 한두 문장. 동작은 바뀌지 않았고 시험이 전후 같이 통과했다는 문장.>

Co-Authored-By: Claude Mythos 5.1 <noreply@anthropic.com>
```

(하십시오체, no em-dash, no program-as-subject verbs 묻다/적다/넘기다,
"추가하다" not "더하다"). Architecture deletions: `refactor(audit): X-n <한국어> (#186 A<n>)`.
Before the first item and after every fifth commit: `cargo test --workspace --locked`.

## Items

### B-21 · TD-217 · the measurement estimator recomputes spectra per role and deinterleaves whole buffers

Finding: "채널마다 전체 길이 `Vec<f64>`로 디인터리브하고 역할마다
`estimate_with_noise`가 스윕과 역필터를 다시 FFT하며(`next_power_of_two(stride+L)`의
복소 스펙트럼 셋) `estimate_raw`가 SNR을 위해 다시 컨볼루션합니다. 512 MiB
상한(162행)은 재생 버퍼만 묶고 FFT 작업 메모리(4M 샘플/48 kHz에서 역할당 16M
빈)는 묶지 않습니다. REW 쪽은 역할마다 `/measurements/{id}`를 두 번(`snr_db`,
`spl_offset_db`) 받습니다." Recommendation: compute the sweep and inverse
spectra once per `n` and take `&mic[start..end]` lazily from the interleaved
buffer; fetch the REW measurement object once and read both fields.

- Before: `crates/secs-audio/src/measurement.rs:187` `measure_position` deinterleaves every channel into a full-length `Vec<f64>` (241-247, 255, 272, 296); `crates/secs-audio/src/estimator.rs:117` `estimate_with_noise` and `:103` `estimate_raw` each FFT the test signal and the inverse filter (`:128` `n = (recording.len() + test_signal.len() - 1).next_power_of_two()`, `:174`); `crates/secs-rew/src/client.rs:739-756,1047-1050` fetch `/measurements/{id}` twice per role (the `secs-rew` half is NOT in your files: report it as left for Claude unless Phase 1's A-17/A-41 already changed those lines; check).
- After: `SweepEstimator` caches `(n, sweep_spectrum, inverse_spectrum)` for the last `n` (one entry; roles of one position share `n`) and `estimate_raw` reuses the convolution result `estimate_with_noise` already has (pass it in; keep `estimate_raw` public with its old signature as a wrapper for the published crate); `measure_position` slices `&mic[start..end]` per role from the interleaved buffer and deinterleaves only that range (the sample values fed to the estimator are the same numbers in the same order, so the IR bytes are unchanged). The 512 MiB bound is unchanged (widening it to FFT memory is a behaviour change: report as a Category A follow-up).
- Deleted: the per-role spectra and the full-length copies.

Files: `crates/secs-audio/src/{measurement.rs, estimator.rs}`.
Gate: `cargo test -p secs-audio --locked` (`tests/native.rs`, `mic_calibration.rs`, `level_calibration.rs`, the SimRoom goldens) bit-identical; `cargo test -p secs-service --locked` is not in your tree's scope but must still pass: run it (read-only use of the service crate).
Commit subject: `refactor(audit): B-21 측정 추정기가 스윕 스펙트럼을 한 번만 계산하고 필요한 구간만 디인터리브합니다 (#186 TD-217)`.
Stop rule: a single IR sample difference → reclassify (summation order).

### B-27 · TD-231 · `secs-jos` recomputes the class hierarchy per field access

Finding: "`Stream::get`/`set`이 필드 조회마다 `hierarchy()`(Vec + BTreeSet
할당)를 다시 계산합니다. adapter는 측정마다 약 40번, builder는 수백 번
조회합니다." Recommendation: cache the hierarchy per descriptor id
(invalidated when a `ClassDesc` is pushed) or take a precomputed `&[NodeId]`
on the hot path.

- Before: `crates/secs-jos/src/nodes.rs:96` `pub(crate) fn hierarchy(&self, id) -> Option<Vec<NodeId>>`, called from `get` (111) and `set` (127); `push` (85) adds nodes including `Node::ClassDesc`.
- After: the node arena type in that file keeps `hierarchy_cache: RefCell<HashMap<NodeId, Rc<[NodeId]>>>` (or a `Vec<Option<Rc<[NodeId]>>>` indexed by id if ids are dense) filled on first use and cleared in `push` when the pushed node is a `ClassDesc` (a descriptor push can extend a chain); `get`/`set` read through it. `secs-jos` is a published crate under Miri and fuzz in CI (CLAUDE.md: untrusted-file parsers): no `unsafe`, no interior mutability that Miri flags (`RefCell` is fine), and run `cargo +nightly-2026-10-01 miri test --locked -p secs-jos` yourself if the nightly toolchain is installed here (check with `rustup toolchain list`); if not, say so and CI's Miri job covers it.
- Deleted: nothing (the allocation per call).

Files: `crates/secs-jos/src/nodes.rs`.
Gate: `cargo test -p secs-jos --locked` (the round-trip and `deterministic_3000_mutations` goldens unchanged), Miri as above.
Commit subject: `refactor(audit): B-27 JOS 스트림이 클래스 계층을 descriptor마다 캐시합니다 (#186 TD-231)`.

### B-30 / X-33 · TD-241, A33 · duplicated helpers in the CI scripts

Finding: "`strict_json`, `_header`, 재시도 지연·루프가 두 스크립트에 중복되고
이미 어긋났습니다(`_retry_delay`는 남은 한도가 0이면 `x-ratelimit-reset`까지
기다리고 `_delay`는 안 함). `fetch_file`(내려받기+sha256+재시도 4회+원자 교체)이
세 벌, `_run`/`_git`이 두 벌(`release_gate`는 `-c core.quotepath=off`가 없음).
`install_app`/`prepare_edge`는 네 곳에서 `runpy.run_path(...)['fn']` 한 줄로
불립니다." Recommendation (A33): `.github/scripts/_http.py` (`strict_json`,
`header`, one retry policy, `fetch_file`) and `_git.py` shared through
`sys.path.insert` like `api_dry_run.py:24`; `manage_e2e.py` gets `install`
and `prepare-edge` subcommands.

- Before: `sol_review.py:64,92,104` vs `sol_review_api.py:57,149,153`; `fetch_file` in `blackhole.py:52`, `eqapo_engine.py:66`, `reaper_engine.py:56`; `_run`/`_git` in `upstream.py:161,166` vs `release_gate.py:384,391`; `install_app`/`prepare_edge` in `manage_e2e.py:489,500` called through `runpy.run_path` from four places (find them: `grep -rn "run_path" .github/scripts .github/workflows`).
- After: `.github/scripts/_http.py` with `strict_json`, `header`, `retry_delay(error, attempt, headers)` (ONE policy: the stricter `_retry_delay` behaviour that honours `x-ratelimit-reset`; this is the one place where the two copies differ, and unifying on the stricter one changes `sol_review_api.py`'s waiting behaviour: that is observable only as timing on a rate-limited API and no unit test pins it; keep the stricter one and say so in the commit body), `fetch_file(url, expected_sha256, target, opener=urlopen, attempts=4)` with the three callers' argument shapes reconciled (keep each caller's call working; the unit tests `test_blackhole.py`, `test_eqapo_engine.py`, `test_reaper_engine.py` inject `opener` and must pass unchanged); `.github/scripts/_git.py` with `run(*args, check=True)` and `git(*args)` that always passes `-c core.quotepath=off` (the `release_gate.py` copy lacked it; `quotepath=off` only changes how non-ASCII paths print, and `release_gate` reads `git` output for paths: this could change its parsing of Korean file names, which is a behaviour fix rather than a refactor: keep `release_gate`'s current flag set by passing `quotepath=False` from it, and report the difference as a Category A follow-up); `manage_e2e.py` gains `install <installers>` and `prepare-edge <destination> <artifacts>` subcommands and the four `runpy` call sites (workflows and scripts) call them (workflow YAML lines are `.github/workflows/**`, which is Claude's: list the exact YAML lines that must change in the report and do not edit them; the scripts keep working through `runpy` until Claude switches the workflows, so keep `install_app`/`prepare_edge` importable as functions).
- Deleted: the duplicate function bodies (replaced by `from _http import …` after the `sys.path.insert` idiom at the top of each script).

Files: `.github/scripts/_http.py` (new), `_git.py` (new), `sol_review.py`, `sol_review_api.py`, `blackhole.py`, `eqapo_engine.py`, `reaper_engine.py`, `upstream.py`, `release_gate.py`, `manage_e2e.py`, and their `test_*.py` only where an import path changes (a test that imported a helper from the old module keeps passing through a re-export; do not change assertions).
Gate: `python3 -m unittest discover -s .github/scripts -p "test_*.py"` (count must match before: 362 on this machine at the time of writing; record yours), `python3 -I .github/scripts/upstream.py check-tree`, and `python3 -I .github/scripts/release_gate.py --help` unchanged.
Commit subject: `refactor(audit): X-33 CI 스크립트의 HTTP와 git 도우미를 _http.py와 _git.py로 모읍니다 (#186 A33)`; the body cites TD-241 (B-30).
Stop rule: any unittest change → undo that consolidation.

### B-31 · TD-243 · RFC 3339 by hand and two `cfg` spellings

Finding: "셸은 RFC 3339를 손으로 만들고(`...Z`) 호스트는 chrono(`+00:00`)를 써서
`log()`(`:184-190`)가 둘을 한 목록에 섞어 화면에 보입니다. 같은 크레이트와 같은
파일 안에서 `cfg(windows)`와 `cfg(target_os = "windows")`가 섞여 있습니다."
Recommendation: use the workspace `chrono`'s `to_rfc3339()` and pick one
`cfg` form.

- The timestamp part is NOT a refactor: whichever side changes, its on-screen bytes change (`…Z` vs `…+00:00`), so the unification is Category A. Leave both timestamps as they are, report it (`RECLASSIFY → A`, with the two literal shapes), and do only the `cfg` spelling here.
- `cfg`: `crates/secs-sys/src/lib.rs` uses `#[cfg(target_os = "windows")]` (26, 59, 178, 272, 310) while `crates/secs-sys/src/wasapi.rs` uses `#[cfg(windows)]` (629, 2246, 2304) and `apps/secs2r-host/src/rules.rs` uses `target_os`. Pick `#[cfg(windows)]` (the shorter, equivalent predicate) across `secs-sys` and `secs2r-host-shell`; `apps/secs2r-host` is not in your files: report its three lines. The compiled code is identical (`cfg(windows)` ≡ `cfg(target_os = "windows")` on every target Rust supports); the `unsafe` token count is unchanged (the policy gate counts tokens, so do not reflow lines that contain `unsafe`).
- Deleted: nothing.

Files: `crates/secs-sys/src/{lib.rs, wasapi.rs}` and any other `secs-sys` file with the long form; `apps/secs2r-host-shell/src/main.rs` if it has either form.
Gate: `cargo clippy -p secs-sys --all-targets --locked -- -D warnings`, the Windows-target workspace clippy above, `cargo test -p secs-sys -p secs-policy --locked` (the unsafe budget gate).
Commit subject: `refactor(audit): B-31 secs-sys의 cfg 표기를 cfg(windows) 하나로 맞춥니다 (#186 TD-243)`.

### X-29 · A29 · three method tables become variant properties

Finding: "메서드 94개를 세 표가 따로 세고 각자 시험으로 맞춤. `ui_only`/`mutates`는
variant의 속성." Solution: "strum `EnumProperty`나 `const fn access(&self)`로
variant에 두고 `ACCESS`를 지움." Benefit: "메서드 추가가 아홉 자리에서 일곱
자리로."

- Before: `crates/secs-types/src/local_api.rs:89` `const ACCESS: &[(&str, MethodAccess)]` (the `entry(method, ui_only, mutates)` rows), `:189` the lookup, `:204` the test that `ACCESS` lists exactly `Request::METHODS`; `crates/secs-types/src/ipc.rs:2922` `fn examples() -> Vec<Request>` (the second table, one example per method, kept: it is test data); `features.toml`'s `ipc.*` entries (the third table, kept: the registry is the audit's own tool).
- After: `impl Request { pub const fn access(&self) -> MethodAccess { match self { Request::ProjectOpen(..) => MethodAccess { ui_only: false, mutates: true }, … } } }` generated from the current `ACCESS` rows one to one (write a throwaway script or do it by hand; every variant must appear, which the compiler enforces through the exhaustive match). Where the local API server checks access by wire name BEFORE deserialising (to refuse unknown methods early), keep a `pub fn method_access(name: &str) -> Option<MethodAccess>` that builds its lookup from `Request::METHODS` zipped with `examples().iter().map(Request::access)` at first use (`OnceLock`), so the by-name path keeps working without the hand table. The test at `local_api.rs:204` (`ACCESS` ≡ `METHODS`) is deleted because the compiler now proves it (deleting a test whose property became a compile-time guarantee is the ADR-style deepening step; name it in the report). `ui_only`/`mutates` values must be identical for all 94 methods: before deleting `ACCESS`, add a temporary assertion in the existing test that `ACCESS[i].1 == examples()[i].access()` for every row, run it, then delete both the table and the assertion in the same commit (quote the passing run in the report).
- Deleted: `ACCESS`, `entry()`, the `ACCESS ≡ METHODS` test.

Files: `crates/secs-types/src/{local_api.rs, ipc.rs}`. `crates/secs-local-api/src/lib.rs` is NOT in your files: if it reads `ACCESS` directly, keep a `pub fn method_access(name)` with the same signature it calls today so it needs no change, and report.
Gate: `cargo test -p secs-types -p secs-local-api --locked` (the local API access tests `*ui_only*`/`*mutates*` pass unchanged); bindings regenerate nothing (no `#[ts]` type changed).
Commit subject: `refactor(audit): X-29 메서드 접근 표를 Request variant의 속성으로 옮깁니다 (#186 A29)`.
Stop rule: any access value difference in the temporary assertion → stop, report the method.

### X-30 · A30 · dialog extension lists into `OpenFileKind::extensions()`

Finding: "대화상자 확장자 목록(`secs`, `json`, `txt/cal/frd`, `mdat/liveproject`)이
Tauri adapter에만 있음." Solution: "`OpenFileKind::extensions()`를 `secs-types`에."
Benefit: "`TauriHost`가 순수 pass-through, CLI·시험 공유."

- Before: `apps/secs2r/src/host.rs:34-77,93-103` `add_filter("SECS project", &["secs"])`, `add_filter("Measurements", &["mdat", "liveproject"])`, `add_filter("REW measurement", &["mdat"])`, `add_filter("Dirac Live project", &["liveproject"])`, the json/txt/cal/frd filters, and `camilla_executable_extensions(os)` (190) with its tests (210-212); `crates/secs-types/src/ipc.rs:1487` `pub enum OpenFileKind`.
- After: `impl OpenFileKind { pub fn filters(self, os: &str) -> &'static [(&'static str, &'static [&'static str])] }` (name, extensions) in `secs-types` returning exactly today's lists in today's order (the dialog shows filters in that order; the names are the English strings the dialog shows today and are NOT catalog text, keep them byte-identical); `camilla_executable_extensions` moves to `secs-types` as the data behind the CamillaDSP executable kind with its three unit tests moved along (test names unchanged; the moved tests are the existing ones, no new ones); `TauriHost` loops over `kind.filters(std::env::consts::OS)` and calls `add_filter(name, extensions)`. The ts-rs binding of `OpenFileKind` does not change (a method is not exported); run `cargo test -p secs-types` and confirm only line-ending churn under `bindings/`.
- Deleted: the inline lists in `host.rs`.

Files: `crates/secs-types/src/ipc.rs`, `apps/secs2r/src/host.rs`.
Gate: `cargo test -p secs-types -p secs2r --locked` (the app crate's unit tests; the Tauri app crate builds on this machine).
Commit subject: `refactor(audit): X-30 열기 대화상자의 확장자 목록을 OpenFileKind의 메서드로 옮깁니다 (#186 A30)`.

### X-32 · A32 · delete the hand-written Rust lexer in the policy gates

Finding: "손으로 쓴 러스트 lexer(약 190줄)가 `#[test] fn`을 찾는데, 같은 CI가
Cargo의 `--list`로 더 강한 검사를 이미 함(`ci.yml:177, 251`). lexer는 module에
포함되지 않은 파일을 통과시킴." Solution: "lexer를 지우고
`features_registry_is_well_formed`에 세 가지 형식 검사(`975-995`)만 남김."

- Before: `crates/secs-policy/tests/gates.rs:829-963` (the lexer) and `1422-1464` (its tests), used by `features_registry_is_well_formed` (737) to check that every `tests = [...]` name exists as a `#[test] fn` in the named crate's sources; `.github/scripts/check_feature_tests.py` does the stronger check with `cargo test -- --list` in CI (`ci.yml`).
- After: `features_registry_is_well_formed` keeps only the three format checks the audit names (the registry parses, every entry has `id`/`kind`/`status`, `implemented` entries list at least one test name of the form `<crate>::<test>`), and a one-line comment pointing at `check_feature_tests.py` for existence; the lexer and its unit tests are deleted (the deleted tests are tests of the lexer itself: list their names in the report; they are the shallow tests the stronger CI check already covers). `features.toml` entries that name the deleted lexer tests (if any: `grep -n "secs-policy::" features.toml`) are renamed to the surviving test, which is the one allowed `features.toml` edit.
- Deleted: about 190 + 40 lines.

Files: `crates/secs-policy/tests/gates.rs`, `features.toml` (rename only).
Gate: `cargo test -p secs-policy --locked`; `python3 .github/scripts/check_feature_tests.py` (which on this Windows machine reports two known names it cannot list: `secs-local-api::token_files_are_created_and_tightened_to_owner_only_td109` and `secs-sys::callback_native_sample_conversion`; anything else is yours).
Commit subject: `refactor(audit): X-32 정책 게이트의 손으로 쓴 러스트 lexer를 지우고 형식 검사만 남깁니다 (#186 A32)`.

## Verification (foreground; do not end the turn before they finish)

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --exclude secs2r --exclude secs2r-host-shell --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test --workspace --locked
python3 -m unittest discover -s .github/scripts -p "test_*.py"
python3 .github/scripts/upstream.py check-tree
python3 .github/scripts/check_feature_tests.py
git log --oneline fix/audit-186..audit/186-misc-b
```

## Report

Per item: before/after counts, commit hash, files, what was deleted. Then:
items reclassified (`RECLASSIFY → A` with the test or the literal output
difference: the B-31 timestamp shapes, the `release_gate` quotepath flag,
the 512 MiB bound of B-21), the workflow YAML lines B-30 needs Claude to
switch to the new `manage_e2e.py` subcommands, the `apps/secs2r-host`
`cfg` lines left for Claude, whether Miri ran for B-27, and the final
`git log --oneline` of the branch.
