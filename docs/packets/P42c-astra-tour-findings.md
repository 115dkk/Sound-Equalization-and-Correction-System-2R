# P42c: 화면 투어가 찾은 상태·틀 결함 (ASTRA)

화면 투어(P43, `apps/secs2r/ui/src/gallery/tour/`)를 처음 찍으며 보인 결함 가운데 상태·틀 모델·모의 서비스 쪽입니다. 같은 목록의 레이아웃·표시 쪽은 Opus가 P42b에 더해 맡습니다.

## 작업 위치

- **워크트리 `/home/user/wt/p42-astra`(브랜치 `p42-astra`)에서 이어서 일합니다.** P42a의 결과가 커밋 `7dce722`로 들어 있습니다. 다른 경로는 읽기만 합니다.
- 커밋하지 않습니다. git으로 인덱스나 작업 트리를 바꾸지 않습니다.
- 투어 장면 표는 본 저장소의 `apps/secs2r/ui/src/gallery/tour/scenes.ts`에 있습니다(읽기만). 장면 id는 아래 괄호에 적었습니다.

## 할 일

1. **프로젝트 상태의 mdat 경로** (`system.path-mdat`). `app/useFrameModel.ts:38`이 `host.path === "mdat"`를 `"none"`으로 바꿔, mdat 가져오기를 고른 프로젝트의 상태가 '고르지 않음'으로 보입니다. `app/model.ts`의 `PathKind`에 `"mdat"`를 더하고, 틀이 그 이름을 보이게 합니다. `app/ProjectStatus.tsx`의 종류→문구 대응표 한 줄(데이터)과 카탈로그 키(`ko`·`en`, 기존 `frame.status.path.*` 꼴을 따름)는 이 일에 한해 고쳐도 됩니다. 길 상태(ok/warning)는 mdat 경로에서 무엇이 '준비됨'인지 `screens/connect/state.ts`의 `pathReady`와 맞춥니다.
2. **레벨 모니터 때문에 막히는 측정 단계** (`connect.ready`, `connect.rew`, `connect.level-*`). 연결·점검에서 레벨 모니터(`level_monitor`)나 상태 감시(`status_watch`)만 도는데도 레일의 측정·가져오기가 `device_busy`로 막힙니다(`screens/measure/types.ts:207`). 이 두 감시 잡은 연결·점검을 떠나면 멈추므로 막는 이유가 아닙니다. 실제로 재생 중(`level_play`, 스윕 점검 등 측정과 같은 장치 무리의 유한 잡)일 때만 막히게 합니다. 서비스의 무리 정의(`crates/secs-service/src/jobs.rs`)를 보고 정합니다.
3. **시스템 단계 자신이 막힘** (`system.invalid`). 구성 오류가 있으면 시스템 단계 자신에 `config_invalid` 자물쇠와 "…시스템에서 고치세요"가 붙습니다. 이미 시스템에 있으므로, 시스템 단계는 고칠 수 있는 단계(`ready`)로 두고 뒤 단계만 막습니다. 단계 상태 함수(`screens/system/types.ts` 또는 `useFrameModel`)에서 고칩니다. 지금 연결·점검이 막히지 않는 것도 맞는지 같이 봅니다(구성 오류면 연결·점검도 막혀야 하는지, 업스트림 `src/secs/wizard/window.py`의 단계 잠금을 확인).
4. **프로젝트 없을 때의 레일** (`start.empty`). 프로젝트가 없는데 레일의 시스템·연결·점검이 열린 단계로 보입니다. 프로젝트가 없으면 모든 단계가 무엇을 기다리는지(새 프로젝트) 보이게 합니다. `BlockedReason`에 새 이유가 필요하면 더하고 카탈로그 문장도 더합니다(이 경우 `app/StepRail.tsx`·`StepHeader.tsx`가 이유를 그리는 대응표 한 줄은 고쳐도 됩니다).
5. **장치 id 표시** (`system.path-native`, `system.path-rew`). 시스템 화면의 측정 출력 장치 줄이 장치 id(`wasapi:{0.0.0.00000000}.{hdmi}`, `hdmi`)를 그대로 보입니다. 연결·점검은 장치 이름('AVR (HDMI)')을 씁니다. 이름은 훅(`screens/system/useSystemScreen.ts`, 이미 `nativeDevice`·`rewDevice`를 찾음)이 보기(`SystemView`)에 담아 내고, 화면이 그것을 쓰게 합니다. 화면 파일(`SystemScreen.tsx`)에서 그 값을 읽는 한 군데만 바꿔도 됩니다. 이름을 못 찾으면 id를 그대로 둡니다.
6. **보정 단계의 빈 채널 선택** (`calibrate.blocked`). 측정이 없을 때 '채널' 선택 상자가 빈 칸입니다. 보기(`measuredRoles`, `channel`)가 무엇을 주는지 보고, 훅에서 고칠 일이면 고치고 화면에서 고칠 일이면 보고서에 방법을 적습니다.
7. **모의 서비스 충실도** (`ipc/mock.ts`, `ipc/fixtures.ts`). 실제 서비스와 다르게 답하는 세 군데를 서비스 계약대로 맞춥니다.
   - `config_validate`의 `out_duplicate` 알림이 `out`만 넣습니다. 서비스(`crates/secs-project/src/config.rs:465-475`)처럼 `first_role`·`role`도 넣습니다.
   - 레벨 점검이 마스터 레벨(−20)을 채널 레벨로 저장해 미터 읽기(−30)와 다릅니다. 서비스(`monitor.rs`의 `store_level`)처럼 잰 입력 레벨을 저장합니다.
   - BA 결과 곡선이 서브마다 같은 값이라 겹쳐 SW3만 보입니다. 서브마다 조금씩 다르게(결정적으로) 만듭니다.
   - `bootstrap`의 버전이 `0.13.0`으로 고정이라 화면에 '사전 출시판 0.13.0'이 나옵니다. 고정값이 `Cargo.toml`의 워크스페이스 버전을 따라가게 할 방법이 마땅치 않으면 그대로 두고 보고서에 적습니다(빌드 시점 상수가 있으면 그것을 씁니다).

각 항목에 테스트를 더합니다(틀 모델은 `app/AppFrame.test.tsx`나 해당 `types` 테스트, 모의 서비스는 `ipc/mock.test.ts`).

## 허용 파일

`apps/secs2r/ui/src/state/`, `src/app/useFrameModel.ts`, `src/app/model.ts`, `src/screens/*/types.ts`, `src/screens/*/state.ts`, `src/screens/*/use*.ts`, `src/ipc/`(바인딩 제외), `src/i18n/ko.json`·`en.json`(새 키만), 위 항목이 허락한 화면 파일의 한 줄 대응표, 그리고 이것들의 테스트.

건드리지 않음: 그 밖의 화면 컴포넌트와 CSS, `App.tsx`, `src/gallery/`, `scripts/`, `crates/`, `.github/`, `docs/`.

## 검증 (모두 포그라운드로, 보고를 마친 뒤에만 턴을 끝냅니다)

```sh
export PATH=/opt/nvm/versions/node/v24.22.0/bin:$PATH
cd /home/user/wt/p42-astra/apps/secs2r/ui && npm run typecheck && npm test && npm run build
```

## 보고 형식

1. 바꾼 파일과 각 한 줄 설명
2. 1–7번 각각: 한 일과 근거(업스트림·서비스 파일과 줄), 6번은 화면 쪽 수정이 필요하면 그 방법
3. 새 카탈로그 키와 ko·en 문장
4. 검증 명령마다 마지막 몇 줄의 출력
