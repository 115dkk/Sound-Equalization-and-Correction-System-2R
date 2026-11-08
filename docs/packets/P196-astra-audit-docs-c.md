# P196 (ASTRA): audit #186, Category C documentation findings

Phase 3 of `/115dkk-skills:resolve-audit` for issue #186. It runs after
every code packet has landed on `fix/audit-186`, so the documents describe
the final code. Claude created the worktree `E:\wt\audit-api` on a fresh
branch `audit/186-docs-c` from the current `fix/audit-186` (`git status`
shows only an untracked `bindings/python/secs2r/_secs2r.pyd`; leave it).
Work only there. Commit on that branch, one commit per finding, in the
order below, with the trailer
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Do not push, do
not rebase, do not spawn sub-agents, do not touch `audit-state.md`. Cargo
runs only for `cargo doc` and the feature check, with
`CARGO_TARGET_DIR=C:\cargo-target\audit-api`; never `cargo clean`, never
delete anything under `C:\cargo-target`.

Category C means: no runtime behaviour change. These items change words
only: ADR text, repository documents, doc comments (`///` and `//!`), and
workflow header comments. The gate is the Korean writing hook on Write/Edit
(it refuses em-dashes, program-as-subject verbs, "더하다" for "추가하다",
English idioms translated word for word, and reader instructions written
in -ㅂ니다 instead of -십시오) plus `cargo doc` where a doc comment
changes, plus your own `git diff` review that no code token moved.

Allowed files: `docs/**` (not `docs/packets/**`, not `docs/upstream/**`),
`README.md`, `CONTRIBUTING.md`, `SECURITY.md`, doc comments in
`crates/*/src/**/*.rs` and `bindings/python/secs2r/__init__.pyi`,
`bindings/python/README.md`, `bindings/dotnet/README.md`,
`.github/workflows/release.yml` header comments only (lines before `on:`),
`features.toml` only the `release.updater` entry (C-29). Forbidden: every
code line, `CHANGELOG.md`, the screens, the other workflows.

Read first: `CLAUDE.md`, `CONTEXT.md` (terms), `docs/adr/0038-advanced-promotion.md`
(house style of an ADR), the cited files below at the cited lines (line
numbers are from main 97a27818; the branch is newer, so search by the quoted
text when a number is off), and the finding texts quoted per item.

Writing rules for the Korean you add: 하십시오체 narrative, sentences under
about 25 words, no em-dash, no colon-then-list inside a sentence, design
terms module/interface/seam/adapter stay in English as CONTEXT.md does,
instructions to the reader end in -십시오, and no sentence whose subject is
a program doing a human act (묻다, 적다, 넘기다, 그리다, 채우다, 쓰다 as
"write"): say 표시합니다, 기록합니다, 전달합니다, 만듭니다, 저장합니다.

## The protocol for every item

1. Announce the item in the report (`── C-n: … ──`).
2. Make the change with Edit (the hook checks it).
3. Gate: `git diff --stat` shows only the files named; for a doc comment,
   `cargo doc -p <crate> --no-deps --locked` with
   `CARGO_TARGET_DIR=C:\cargo-target\audit-api` (warnings are errors in
   CI's rustdoc job: no broken intra-doc links).
4. Commit with `git add <the files you touched>` and this message shape:

   ```
   chore(audit): C-n <한국어 한 문장> (#186 TD-xxx)

   <무엇을 어떻게 고쳤는지 한두 문장. 코드는 바뀌지 않는다는 문장.>

   Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
   ```

## Items

### C-3 · TD-166 · cache keys are Debug strings; the policy is nowhere

Finding: "`config_key`는 `format!("{:?}:{:?}", project.config, project.xo)`이고
`signature_key`도 측정 서명의 `{:?}`를 넣습니다. 이 Debug 문자열이 실행
캐시(`run_key`, `config_key`)와 프로젝트 파일(`xo_rec_key`)에 저장되므로
`SpeakerConfig`·`Speaker`에 필드가 하나 늘면 판을 올릴 때마다 모든 캐시와
추천이 조용히 '낡음'이 되고, 키 문자열을 고정하는 시험이 없으며 …"
Maintainer decision (2026-11-06, open question 4): this is the accepted
upgrade policy; document it, do not rewrite the keys.

Sites: `crates/secs-service/src/cache.rs` (the file's `//!` doc or the
`CachedResults` doc; `const VERSION: u32 = 6` at line 10 is the cache file
version, there is no `CACHE_KEY_VERSION`), `crates/secs-service/src/calibration.rs:1167-1170`
(`config_key`), `CONTRIBUTING.md` (the checks section).

Change: a doc paragraph above `config_key` and one in `cache.rs`'s module
doc saying, in Korean, that `config_key`, `run_key` and `xo_rec_key` embed
the `Debug` form of `SpeakerConfig` and `XoSettings`, so a field added to
either marks every saved result and crossover recommendation as 낡음 after
the upgrade, that this is the accepted policy (감사 #186 TD-166, 유지보수자
결정 2026-11-06), and that a change which must NOT invalidate caches has to
leave those structs' `Debug` output unchanged. In `CONTRIBUTING.md`, one
instruction under the checks section, in -십시오: a PR that adds a field
to those structs must say so in `CHANGELOG.md` because saved results go
stale.

Commit subject: `chore(audit): C-3 캐시 키가 Debug 문자열인 정책을 코드와 기여 안내에 기록합니다 (#186 TD-166)`.

### C-4 · TD-168 · `Engine::service()` exposes internal crates through the public one

Finding: "`Engine::service()`가 `&secs_service::Service`를 돌려주고
`EngineBuilder`가 `secs_rew::Transport`·`secs_audio::AudioBackend`를
받으므로, ADR 0016이 '판 호환을 약속하는 것은 공개 크레이트 하나뿐'이라고
했는데도 내부 크레이트의 항목이 공개 크레이트의 API가 됩니다."
Maintainer decision (open question 3): it is an unstable escape hatch;
mark it in rustdoc.

Sites: `crates/secs-api/src/lib.rs:450-454` (`/// Returns the underlying
service for operations not wrapped by this crate.` above `pub fn service`),
`:172-179` (the `AudioFactory`/`RewFactory` seams the builder takes),
`docs/adr/0016-public-rust-api-and-local-api.md` line 16 (the
compatibility sentence). ADR 0054 (A13) later decides the typed surface;
this item only labels what is unstable today.

Change: rustdoc on `service()` and on the builder methods that accept
`secs_rew::Transport` / `secs_audio::AudioBackend`: a first line "Unstable:
…" (English, like the crate's other docs) saying these expose internal
crates whose items change without a compatibility promise (ADR 0016) and
that typed `Secs` calls are the stable surface. In ADR 0016 add one
sentence after the compatibility sentence: `Engine::service()`와 seam
타입을 받는 빌더 메서드는 호환 약속 밖이며 rustdoc에 Unstable로 표시합니다
(감사 #186 TD-168).

Gate: `cargo doc -p sound-equalization-and-correction-system-rust-api --no-deps --locked`.

Commit subject: `chore(audit): C-4 Engine::service()와 seam 빌더를 호환 약속 밖으로 표시합니다 (#186 TD-168)`.

### C-6 · TD-177 · four timers poll against ADR 0003

Finding: "ADR 0003은 '화면은 폴링하지 않는다'고 정했지만 타이머 넷이
서비스를 폴링합니다(적용 상태 3초, 업데이트 상태 60초/1초, 호스트 상태 2초,
바깥 잡이 돌 때 `jobs` 1초). 적힌 이유가 있는 것은 `jobs`뿐이고 ADR은 어느
것도 기록하지 않습니다." Maintainer decision (open question 6): record the
four exceptions with reasons in ADR 0003; no `status_watch` push this round.

Sites: `docs/adr/0003-stack-and-ipc.md` line 15 ("화면은 폴링하지 않습니다");
the pollers: `apps/secs2r/ui/src/screens/export/useExportScreen.ts:48-68`
(apply status every 3 s while an apply target is managed and the window is
visible), `apps/secs2r/ui/src/state/update.ts:40` (update status every
60 s, 1 s while checking or downloading), `apps/secs2r/ui/src/host/useHostShell.ts:15,70-92`
(`POLL_MS = 2000` chained reads while visible), `apps/secs2r/ui/src/state/session.ts:650-654`
(`jobs` every 1 s only while external jobs run, because the shell's jobs
have no UI Channel).

Change: in ADR 0003, after the sentence at line 15, a short subsection
`### 폴링의 예외` with a four-row table (어디 | 주기 | 이유): apply status
(CamillaDSP and Equalizer APO are other processes whose state the service
cannot push without a watcher; read only while the apply target is managed
and the window is visible), update status (the updater runs in the Tauri
host, outside the service's job events), host shell (a separate Tauri app
without the service's Channel), external jobs (no UI Channel exists for a
job the shell started). End with one instruction in -십시오 that a new
poller gets a row in this table with its reason (감사 #186 TD-177). Do not
change the TS files.

Commit subject: `chore(audit): C-6 ADR 0003에 폴링 예외 넷과 이유를 기록합니다 (#186 TD-177)`.

### C-7 · TD-181 · the Equalizer APO document says the FIR is 48 kHz

Finding: "\"FIR WAV는 48 kHz입니다. 장치 샘플레이트는 이 값과 맞춰야 하며\"가
ADR 0017·F22와 어긋납니다. 작성기는 출력 레이트로 씁니다(`eqapo_config_at`가
`{output_rate} Hz … kHz`를 찍음). miniDSP 프리셋(96 k) 사용자가 문서대로
48 kHz로 맞추면 APO가 FIR을 버립니다."

Site: `docs/EQUALIZER_APO_INTEROP.md:103` (section `## 유지하는 사용 조건`).

Change: replace the first sentence with one saying that the FIR WAV is at
the output rate of the export (default 48 kHz, the DSP profile's value),
that the value is shown in the `config.txt` header and the guide file, and
that the device sample rate must match it (ADR 0017, 편차 F22). Keep the
rest of the paragraph.

Commit subject: `chore(audit): C-7 Equalizer APO 문서의 FIR 샘플레이트 문장을 출력 레이트로 고칩니다 (#186 TD-181)`.

### C-11 · TD-194 · deviation F6 is still 계획

Finding: "F6이 아직 `계획`인데 코드는 첫 파일을 쓰기 전에 모든 검사를
하고(`files.rs:71-248`의 staging 설치) 시험 넷이 증명합니다."

Site: `docs/DEVIATIONS.md:69` (row F6); the tests
`crates/secs-camilla/tests/golden.rs` `invalid_values_write_nothing` (1261),
`fir_taps_that_overflow_f32_write_nothing_td45` (1307),
`duplicate_or_reserved_roles_write_nothing_td46` (1347),
`negative_delays_write_nothing_td94` (1384).

Change: set the F6 row's status to `적용` and extend its third column:
all checks run before the first file is written (`secs-graph/src/files.rs`
staging install), and name the four tests as
`secs-camilla::{invalid_values_write_nothing,fir_taps_that_overflow_f32_write_nothing_td45,duplicate_or_reserved_roles_write_nothing_td46,negative_delays_write_nothing_td94}`
in the ledger's usual form. If Phase 1's A-19 (TD-182 range rules) added a
case to `every_rule_rejects_its_input_and_names_the_field`, mention that
test too.

Commit subject: `chore(audit): C-11 편차 F6을 적용으로 바꾸고 지키는 시험 넷을 기록합니다 (#186 TD-194)`.

### C-15 · TD-203 · ADR 0018's sentence about the first generation

Finding: "ADR은 0.46.0 평면 배치의 파일을 '첫 세대 적용이 성공한 뒤' 지운다고
적지만 `retire_after_success`는 이전 세대를 모르면 일찍 돌아가므로 첫
적용에서는 남고 둘째 적용에서 지워지며, 시험이 그 동작을 고정합니다."

Site: `docs/adr/0018-applying-to-camilladsp-and-equalizer-apo.md`, the
`- **이전 배치.**` bullet (near line 56).

Change: rewrite the bullet so it says the flat-layout files of 0.46.0 and
earlier are removed at the second successful apply, that the first apply
knows no previous generation and therefore makes no retirement decision,
and that this conservative judgement matches the paragraph's own rule of
not deleting what it cannot identify (`crates/secs-service/tests/apply.rs`
pins it, 감사 #186 TD-203).

Commit subject: `chore(audit): C-15 ADR 0018의 이전 배치 정리 시점을 코드와 맞춥니다 (#186 TD-203)`.

### C-16 · TD-209 · `wait_job` timeout semantics and README gaps

Finding: "`wait_job`이 시한 초과(그리고 알 수 없거나 버려진 잡 ID)를
`Error::Protocol`로 보고합니다. .NET README는 1024 이벤트 큐·`DroppedEvents`·
한 번만 열거를 적지만 파이썬 README는 `dropped_events()`도 `events()`가
막힌다는 것도 적지 않습니다. ADR 0016에는 WebSocket 큐 크기(256,
`EVENT_CAPACITY`)와 1013 닫기가 없습니다." Phase 1 (P184 A-1, A-9) and
ADR 0054 may already have added `Error::Timeout`, `Lagged` and the Python
`close()`/`timeout`; document what the code on the branch does, not what
the audit saw.

Sites: `crates/secs-api/src/lib.rs` (`wait_job` doc comment, near line
430), `bindings/python/README.md:23` and `bindings/python/secs2r/__init__.pyi`
(the `_EngineEvents`/`_ClientEvents` docstrings), `bindings/dotnet/README.md:38`
(the queue paragraph; keep it as the model), `docs/adr/0016-public-rust-api-and-local-api.md`
(the local API section near line 26).

Change: `wait_job`'s rustdoc states what a timeout and an unknown job id
return (read the current code: `Error::Protocol` or the new
`Error::Timeout { job }`) in one sentence each. The Python README gets a
paragraph mirroring the .NET one: the 1024-event queue, `dropped_events()`,
that `next()` blocks until an event or the `timeout`, and `close()`
(check `__init__.pyi` for the real names after P184) and the same text in
the `.pyi` docstrings. ADR 0016 gets one bullet: the local API event
socket queues `EVENT_CAPACITY` (read the constant in
`crates/secs-local-api/src/lib.rs`) events per subscriber and closes a
slower reader with code 1013 after a `lagged` event that carries the
missed count (if P184's A-9 landed; else without the lagged event).

Gate: `cargo doc -p sound-equalization-and-correction-system-rust-api --no-deps --locked`.

Commit subject: `chore(audit): C-16 wait_job의 시한과 이벤트 큐의 의미를 문서에 기록합니다 (#186 TD-209)`.

### C-29 · TD-246 · the documentation mismatch bundle

Finding (eleven places): "release.yml 머리말과 ADR 0006은 수동 실행이
없다고 하지만 `workflow_dispatch`(pypi_only/nuget_only)가 있음(main으로
안전하게 한정). ADR 0006의 자산 목록에 CLI 아카이브 셋과 `.sig`가 없음.
ARCHITECTURE 3절 그림이 `secs-service → secs-graph`와 `secs-sys →
secs-types` 화살표를 빠뜨리면서 모든 `Cargo.toml`과 같다고 적고, M6 행은
이미 나온 업데이터(ADR 0020)와 GPU adapter(ADR 0008)를 아직 남은 것으로
적음. SECURITY.md는 격리 크레이트를 `secs-sys` 하나로, 로컬 채널을 REW
콜백 포트 하나로 적음(로컬 API와 SYSTEM 파이프 빠짐). CONTRIBUTING의 검사
목록에 Windows 대상 clippy, `cargo deny`, `check-tree`,
`check_feature_tests.py`, `npm run build`가 없고 AGPL 목록에 NuGet이 없음.
ADR 0002가 유일한 프로세스 실행기를 `crates/secs-rew/src/launch.rs`로
적는데 게이트는 `secs-sys/src/rew.rs`·`host.rs`를 허용. UNSAFE_BOUNDARIES.md
제목과 도입이 GPU 경계만 말하고, ADR 0010만 상태 줄 형식이 다름.
`release.updater`가 `planned`인데 `release.yml:383-427`과
`updater_manifest.py`가 구현함."

Sites and changes (one commit; nine files):

1. `.github/workflows/release.yml` header comment ("Push only. A manual
   dispatch would let any ref's code run…", lines 58-59 area): say that
   the two `workflow_dispatch` inputs (`pypi_only`, `nuget_only`) rebuild
   main's current version only and run no gate, as the inputs'
   descriptions say; header comment only, no YAML key changes.
2. `docs/adr/0006-releases.md`: the asset list (lines 20-23) gains the CLI
   archives (read the real names from `.github/scripts/collect_bundles.py:80-94`
   and the release workflow) and the updater `.sig` files (ADR 0020); the
   sentence "손으로 실행하는 트리거도 두지 않습니다" (line 24) becomes a
   sentence saying that two dispatches exist which re-publish only PyPI or
   NuGet for main's current version, pass no gate, and take no other
   branch's code.
3. `docs/ARCHITECTURE.md` section 3 graph: add `secs-service ─> secs-graph`
   (check `crates/secs-service/Cargo.toml` really lists it) and
   `secs-sys ─> secs-types`; keep the sentence that the graph equals the
   manifests true (verify every arrow you touch against the `Cargo.toml`).
   Line 175 (M6 row): move the updater (ADR 0020) and the GPU adapter
   (ADR 0008) to what is done, note that signing is now the self-signed
   certificate of ADR 0041, and leave what is still open (실장비 검증, 출시
   전 성능 감사).
4. `SECURITY.md` lines 9 and 12: the isolation crates are three
   (`secs-sys`, `secs-cuda-sys`, `secs-dotnet-sys`, ADR 0002/0008/0032);
   local channels are three (the REW callback port, the local API on
   127.0.0.1 with a token, ADR 0016, and the managed service's SYSTEM pipe,
   ADR 0018).
5. `CONTRIBUTING.md` lines 26-32: replace the hand-written list with a
   pointer to `CLAUDE.md`'s 검사 section and the full list copied from it
   (Windows-target clippy, `cargo deny`, `upstream.py check-tree`,
   `check_feature_tests.py`, `npm run build`); line 38: the AGPL list adds
   the NuGet package `Secs2R`.
6. `docs/adr/0002-unsafe-isolation-and-policy-gates.md` line 23: the
   process launchers are `crates/secs-sys/src/rew.rs` and
   `crates/secs-sys/src/host.rs` (the gate's `SUBPROCESS_ALLOWLIST` in
   `crates/secs-policy/tests/gates.rs:1233`), since ADR 0018 moved the REW
   launcher.
7. `docs/UNSAFE_BOUNDARIES.md` lines 1-5: title `# unsafe 경계` and an
   intro that names the three budgeted crates and their ADRs (0002 for
   `secs-sys`, 0008 for `secs-cuda-sys`, 0032 for `secs-dotnet-sys`),
   keeping the GPU section below as is.
8. `docs/adr/0010-skin-sets.md` lines 1-4: the status line in the house
   format (`상태: 채택, 2026-10-22` as a plain line, title `# 0010. 스킨
   셋 (기본 · Hardware Rack · Signal Matrix)`); keep the 관련 line.
9. `features.toml` `release.updater` (line 1385): `status = "partial"`
   with a `note` naming `release.yml`'s updater steps and
   `.github/scripts/updater_manifest.py`, and `tests = []` kept (the
   registry's `partial` needs no test), like `release.installers` does.

Gate: `git diff --stat` lists exactly those nine files; `python3
.github/scripts/check_feature_tests.py` still accepts the registry.

Commit subject: `chore(audit): C-29 문서 열한 곳을 코드와 워크플로에 맞춥니다 (#186 TD-246)`.

### C-31 · TD-261 · engine documentation out of date

Finding: "`guard_cause`의 doc 주석이 `seat_change`/`SEAT_VIOLATION_DB`를
끼워 넣으며 갈라져 … MIMO.md는 λ마다 Cholesky에 LU 대체로 푼다고 하지만
코드는 시스템마다 Householder 삼중대각화 한 번 뒤 이동마다 삼중대각
형태로 풀며 … 크레이트 doc은 module 아홉, ENGINE.md는 파일 열셋을 적어
소스 module 29개 가운데 16개가 지도에 없습니다. `run.rs:34-35`는 … 같은
생성자가 `fsiso`·`mimo_low`·`mimo_max_supports`는 조용히 클램프합니다."

Sites: `crates/secs-engine/src/mimo/design.rs:800-818` (the split doc
comment: the line "The last rejected guard step as a note's cause; `None`
when every step" sits above `seat_change`, and "passed (no reduction)."
above `guard_cause`), `docs/MIMO.md` section 3 second bullet (line 37,
"λ마다의 풀이는 촐레스키"), `crates/secs-engine/src/mimo/solve.rs:10-65`
(what the code does: read it and describe `RidgeSweep`), `crates/secs-engine/src/lib.rs:12-30`
(module list), `docs/ENGINE.md` section 1 table, `crates/secs-engine/src/mimo/run.rs:34-35`.

Change (one commit):

1. `design.rs`: move the two doc lines back together above `guard_cause`
   (`/// The last rejected guard step as a note's cause; `None` when every
   step passed (no reduction).`) and give `seat_change` its own first
   line; nothing else moves.
2. `docs/MIMO.md` bullet: describe the real solver: one Householder
   tridiagonalisation per system, then each λ shift solved in tridiagonal
   form, with Cholesky/LU only as the fallback when that fails (read
   `solve.rs:10-65` and name the functions); make the later bullet in the
   same section agree.
3. `crates/secs-engine/src/lib.rs` module doc: one line per source module
   that is missing (`levels`, `low_protect`, `polarity`, `pair_phase`,
   `normalize`, `replay`, `replay_mimo`, `export_view`, `preview`,
   `display`, `memory`, `calibration`, and the rest you find with `ls
   crates/secs-engine/src`), each with its ADR where one exists (0033
   polarity, 0034 dip limit in rc, 0035 pair_phase, 0036/0037 mimo).
4. `docs/ENGINE.md` section 1 table: a row group `뒤에 생긴 module`
   listing the same modules with the ADR number in the 업스트림 column
   (`새로; ADR 00NN`) and the packet in the 작업서 column where
   `docs/packets/` names one.
5. `run.rs:34-35` doc comment: say that `fsiso`, `mimo_low` and
   `mimo_max_supports` are clamped to the project bounds because they
   predate validation and saved projects may hold out-of-range values,
   while `mimo_early_ms` is kept as given so the engine rejects an invalid
   direct setting (ADR 0037), and cite `audit #186 TD-261`. Doc comment
   only; the code stays.

Gate: `cargo doc -p secs-engine --no-deps --locked`.

Commit subject: `chore(audit): C-31 엔진 문서와 MIMO 풀이 설명을 코드와 맞춥니다 (#186 TD-261)`.

## Covered elsewhere (do not touch)

- C-5 (TD-174 constants): ADR 0052 / P190 and P192.
- C-13, C-21, C-22, C-23, C-24 (UI comments, tsconfig, specimens, typing,
  CSP reasons): P192.
- Tests and fuzz (C-1, C-2, C-8, C-9, C-10, C-12, C-14, C-18, C-19, C-20,
  C-32): P197. CI and configuration (C-17, C-25, C-26, C-27, C-28, C-30): P198.

### Catalog · X-10 and A-42 · the provisional sentences P204 added

P204 (Opus, screens) added these keys with provisional sentences to
`apps/secs2r/ui/src/i18n/ko.json` and `en.json` (this item may edit those
two files, nothing else under `apps/`):
`frame.status.settings_notice.title`, `frame.status.settings_notice.close`,
`notice.app_settings_unreadable` (`{path}`), `notice.app_settings_newer`
(`{version}`), `connect.rew_executable`, `connect.rew_executable.placeholder`,
`connect.rew_executable.help`, `connect.rew_executable.error.relative`.
Read the screens that show them (`src/app/ProjectStatus.tsx`,
`src/screens/connect/ConnectScreen.tsx`) and the neighbouring keys of the
same screens, then rewrite any Korean or English sentence that reads as
translated, decorated, or unlike its neighbours (하십시오체, labels as noun
phrases, the effect a control has, no em-dash). Keep the keys and the
placeholder sets identical in both files. Gate:
`(cd apps/secs2r/ui && npx vitest run src/i18n)`. If every sentence already
reads naturally, make no commit and say so.

Commit: `chore(audit): X-10 설정 알림과 REW 실행 파일 입력란의 문구를 다듬습니다 (#186 A10 TD-221)`

## Verification (foreground)

```
git diff --stat fix/audit-186..audit/186-docs-c
cargo doc -p secs-engine -p secs-service -p sound-equalization-and-correction-system-rust-api --no-deps --locked
python3 .github/scripts/check_feature_tests.py
git log --oneline fix/audit-186..audit/186-docs-c
```

## Report

Per item: the commit hash, the files changed, one line on what the text
now says. Then: anything you found already fixed by Phase 1 or 2 (say which
commit), anything the hook refused and how you rephrased it, and the final
`git log --oneline` of the branch.
