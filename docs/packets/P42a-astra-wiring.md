# P42a: M5 감사 후속 — 연결·상태 (ASTRA)

`docs/packets/P42-m5-followups.md`의 ASTRA 절을 구현합니다. 배경은 `docs/M5-review.md`의 ASTRA 감사 표와 "확신하지 못한 것" 표입니다.

## 작업 위치

- **워크트리 `/home/user/wt/p42-astra`(브랜치 `p42-astra`)에서만 일합니다.** 다른 경로(특히 `/home/user/Sound-Equalization-and-Correction-System-2R`)의 파일은 읽기만 합니다. 같은 시각에 Opus가 `/home/user/wt/p42-opus`에서 화면 쪽(P42b)을 고칩니다.
- 커밋하지 않습니다. 커밋은 Claude가 합니다. git으로 인덱스나 작업 트리를 바꾸지 않습니다(add, stash, checkout, reset 금지).
- 이 워크트리에는 `CalibrateView.curvesLoading: boolean`(`screens/calibrate/types.ts`)이 이미 있고, `useCalibrateScreen.ts`가 지금은 `false`를 넣습니다. 그 값을 실제로 채우는 것이 1번입니다. 필드의 뜻은 types.ts의 주석이 정본입니다.

## 할 일

### 1. 결과 곡선의 선택값 (`state/session.ts`, `screens/calibrate/useCalibrateScreen.ts`)

- `refreshCurves`(그리고 `resultCurves` 리소스)가 요청한 `tab`·`channel`·`group`을 결과와 함께 둡니다. `ResultCurvesResult`에는 채널이 없으므로 화면 쪽 상태가 기억해야 합니다.
- `view.curves`는 마지막으로 받은 곡선, `view.curvesLoading`은 "현재 `host.tab`·`host.channel`·`host.xoGroup`에 맞는 곡선을 기다리는 중"입니다. 응답이 오면 거짓, 요청이 실패하면 `curves: null`이고 거짓입니다(실패는 지금처럼 `session.errors`로 알림).
- 앞선 요청의 응답이 나중 요청보다 늦게 오면 버립니다(일반 리소스 읽기의 세대 검사와 같은 방식).
- 테스트: `useCalibrateScreen.test.tsx` 또는 `state/session.test.ts`에 채널을 바꾸는 동안 `curvesLoading`이 참이고 이전 곡선이 남아 있으며, 응답 뒤 거짓이 되고, 늦게 온 이전 응답이 새 곡선을 덮지 않는 것.

### 2. 미터 `stalled` (`state/session.ts`)

- 업스트림 `src/secs/wizard/mic_meter.py`의 `_poll`: 같은 읽기가 `STALE_POLLS`번(`POLL_S` 간격) 이어지거나 읽기가 없으면 REW 레벨 모니터를 다시 시작하고 "다시 시작 중"을 보입니다. 상수의 값을 그 파일(또는 그것이 가져오는 곳)에서 확인해 같은 시간 기준을 씁니다.
- 우리 쪽: 레벨 모니터 잡(`level_monitor`)의 Meter 이벤트가 그 시간 동안 오지 않으면 `host.monitor = "stalled"`로 두고, 그 문구(`connect.mic.stalled`: "레벨 멈춤 · 다시 시작 중")가 말하는 재시작을 실제로 합니다: `level_monitor_stop` 뒤 `level_monitor_start`. 새 읽기가 오면 `receiving`. 모니터를 끈 뒤나 프로젝트를 바꾼 뒤에는 타이머가 남지 않아야 합니다.
- 먼저 서비스(`crates/secs-service/src/`의 레벨 모니터 구현)가 이미 멈춤을 다루는지 읽고, 다룬다면 화면 쪽은 서비스가 보내는 신호를 따르게 합니다. 어느 쪽이든 보고서에 근거를 적습니다.
- 테스트: 가짜 시계(vitest의 fake timers)로 멈춤 → `stalled` → stop/start 호출 → 새 읽기 → `receiving`, 그리고 끈 뒤 타이머 없음.

### 3. `CalibrationSummary.warnings` (`crates/secs-service/src/calibration.rs:348`)

- 업스트림이 보정 결과 요약에 `notes`와 별개인 경고를 내는지 업스트림 코드(`src/secs/`)에서 확인합니다.
- 낸다면: 서비스가 같은 경고를 `Notice`(코드 + 문자열 인자)로 채우고, `ko`·`en` 카탈로그에 그 코드의 문장(`calibrate.warning.<code>` 꼴, 합니다체, 원인과 다음 행동)을 더하며, 서비스 테스트를 더합니다. 화면 표시는 P42b(Opus)가 `result.warnings`를 판정 카드에 그리므로 화면 파일은 건드리지 않습니다.
- 내지 않는다면: 이 필드는 죽은 필드입니다. `secs-types`에서 지우고 `cargo test -p secs-types`로 바인딩을 다시 생성하고, 서비스·모의 서비스(`ipc/fixtures.ts`)·테스트에서 참조를 지웁니다. 이 경우 Claude에게 보고서 첫 줄로 알립니다(P42b가 그리는 코드를 지워야 하므로).

### 4. 확신하지 못한 것 7건 (`docs/M5-review.md`)

그 표의 일곱 줄을 서비스 계약(`crates/secs-service/src/`, `crates/secs-types/src/ipc.rs`)과 대조합니다.

- 결함이고 고칠 자리가 상태·호출·훅(`state/`, `screens/*/use*.ts`, `ipc/`)이나 서비스면 고치고 테스트를 더합니다.
- 결함이지만 고칠 자리가 화면 컴포넌트(`*.tsx` 마크업)나 CSS면 **고치지 않고** 보고서에 파일·줄·고칠 방법을 적습니다(Claude가 Opus에게 넘기거나 직접 고침). 단 마지막 줄(대화상자 높이)은 P42b가 이미 맡았으니 판정만 적습니다.
- 결함이 아니면 이유를 적습니다.
- 판정은 `docs/M5-review.md`의 그 표 아래에 "P42 판정" 표(위치 · 판정 · 근거 · 처리)로 더합니다. 원래 표는 그대로 둡니다.

## 허용 파일

`apps/secs2r/ui/src/state/`, `apps/secs2r/ui/src/screens/*/use*.ts`와 그 테스트, `apps/secs2r/ui/src/ipc/`(모의 서비스·고정값; `bindings/`는 생성으로만), `apps/secs2r/ui/src/i18n/ko.json`·`en.json`(3번의 문장, 필요한 키만), `crates/secs-service/`, `crates/secs-types/`(3번에서 필드를 지울 때만), `features.toml`(필요할 때), `docs/M5-review.md`.

건드리지 않음: 화면 컴포넌트(`screens/**/*.tsx` 중 `use*`가 아닌 것, `app/*.tsx`, `ui/`), CSS, `App.tsx`, `src/gallery/`, `scripts/`, `.github/`, 그 밖의 문서.

## 검증 (모두 포그라운드로, 보고를 마친 뒤에만 턴을 끝냅니다)

워크트리 루트 `/home/user/wt/p42-astra`에서. Node 24: `export PATH=/opt/nvm/versions/node/v24.22.0/bin:$PATH`. cargo는 `CARGO_TARGET_DIR=/home/user/cargo-target-p42 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`로 돌려 다른 빌드와 겹치지 않게 합니다.

```sh
(cd apps/secs2r/ui && npm run typecheck && npm test && npm run build)
# 서비스나 secs-types를 고쳤다면:
cargo fmt --all -- --check
cargo clippy -p secs-service -p secs-types --all-targets --locked -- -D warnings
cargo test -p secs-service -p secs-types --locked
python3 .github/scripts/check_feature_tests.py
```

확신 없는 API(vitest fake timers, 서비스 함수)는 코드를 열어 확인하고 씁니다.

## 보고 형식

1. (3번에서 필드를 지웠다면 첫 줄에 그 사실)
2. 바꾼 파일과 각 한 줄 설명
3. 1–3번 각각: 무엇을 했고 근거(업스트림 파일·줄, 서비스 파일·줄)
4. 4번: 일곱 줄마다 판정과 처리, 화면 쪽 수정이 필요한 것의 파일·줄·방법
5. 검증 명령마다 마지막 몇 줄의 출력
