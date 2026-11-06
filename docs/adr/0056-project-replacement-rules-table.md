# 0056. 프로젝트가 바뀌면 무엇을 비우는지는 표 하나가 정한다

상태: 제안, 2026-11-06

근거: 감사 이슈 #186, 아키텍처 리뷰 A15. 직접 해결하는 기술 부채 발견은 없고, 지난 감사(#155)의 TD-119, TD-122, TD-124가 이 지식이 흩어져 생긴 결함이었습니다.

## 배경

화면 상태(`apps/secs2r/ui/src/state/session.ts`)에서 '프로젝트가 바뀌면 무엇을 비우는가'가 세 곳에 적혀 있습니다(2026-11-06의 코드에서 다시 확인).

- 효과 규칙 `project.replace`(`state/effects.ts:81-91`): `project_new`·`project_open`의 응답 뒤에 `ctx.replaceProject()`를 부르고 `measurementSettings`·`micCalibration`을 비우며 `host.set({ micCalBusy, micCalError, path, selected, mdat, warnings, runs, applyRuns, mdatScope, designName: "", channel, xoGroup, … })`로 호스트 필드를 되돌립니다.
- `replaceProject` 클로저(`session.ts:272`): `jobs.replaceProject()`, `clearProjectResources()`(키 19개, `:167-173`), `autoRefresh.changed()`, epoch과 `curveVersion`, `curveState`, `tutorialReady`를 되돌립니다.
- 외부 교체 `refreshExternal`(`session.ts:636-641`): 같은 일을 다시 적되 `designName: ""`이 빠져 있습니다. 로컬 API가 프로젝트를 바꾸면 내보내기 화면의 설계 이름이 이전 프로젝트의 것으로 남습니다.
- 시작 때 다시 읽지 않는 앱 소유 키 목록(`session.ts:683`): `bootstrap`, `measurementSettings`, `appSettings`, `computeStatus`, `methods`, `rew`, `applyStatus`. 같은 사실(어느 resource가 프로젝트 소유인가)의 세 번째 표현입니다.
- `HostState`(`session.ts:42-66`, 40개 필드)에는 프로젝트 소유 필드와 앱 소유 필드(`current`, `tab`, `camillaDevices`, `extraRates`, `applyExeDraft` 등)가 섞여 있고, 소유는 주석에만 있습니다.

목록이 셋이면 어긋날 자리가 셋입니다. 이미 하나가 어긋났고(외부 교체의 설계 이름), 지난 감사의 결함 셋은 자리마다 따로 고쳤습니다.

## 결정

### `state/projectScope.ts` 하나가 소유를 가진다

- `HostState`를 `ProjectHostState & AppHostState` 두 타입의 교집합으로 나눕니다. 프로젝트 소유 필드는 `ProjectHostState`에, 앱 소유 필드는 `AppHostState`에 선언하므로 새 필드는 둘 중 하나에 들어가야 컴파일됩니다. 소유가 주석이 아니라 타입이 됩니다.
- `PROJECT_HOST_DEFAULTS: ProjectHostState`가 교체 뒤의 값입니다. `micCalBusy: null`, `micCalError: null`, `path: null`, `selected: null`, `mdat: null`, `warnings: []`, `runs: {}`, `applyRuns: {}`, `mdatScope: "all"`, `designName: ""`, `channel: null`, `xoGroup: null`, `previewHz: null`, `xoRecommendation: null`, `xoRecTurnedOffByEdit: false`, `failure: null`, `rangeRejection: null`과 같은 식입니다. `opened`는 교체마다 1씩 오르는 유일한 예외이고 표가 그 규칙도 가집니다.
- `PROJECT_RESOURCES: readonly (keyof Stores)[]`가 프로젝트 소유 resource 키입니다(`project`, `config`, `notices`, `dsp`, `layout`, `estimate`, `records`, `curves`, `levels`, `settings`, `micCalibration`, `modeSpec`, `target`, `results` 등 지금의 19개와 `measurementSettings`). `APP_RESOURCES`는 그 여집합이며 시작 때 제외할 목록은 표에서 계산하므로 손으로 적지 않습니다.
- `replaceProject(session, source: "request" | "external")` 하나가 교체를 수행합니다. 두 출처가 같은 일을 하고, `external`만 `invalidate`를 가진 store를 추가로 무효화합니다(바깥에서 바뀐 값은 캐시를 믿을 수 없기 때문). 효과 규칙 `project.replace`, `session.ts:272`의 클로저, `refreshExternal`이 모두 이 함수를 부릅니다. `jobs.replaceProject()`, epoch과 곡선 세대, `tutorialReady`, `autoRefresh.changed()`도 이 함수 안에 있습니다.

### 바뀌지 않는 것

교체 뒤에 무엇을 다시 읽는지(`effectsOf({ request: "project_open" }).reload`)는 지금처럼 효과 규칙이 정합니다. 이 ADR은 '비우는 것'만 한 곳에 둡니다. 튜토리얼 설정이 `measure_config_get`을 부르는 `apply` 단계도 그대로입니다.

## 시험

- `state/projectScope.test.ts`: 요청 교체와 외부 교체 뒤에 `PROJECT_RESOURCES`의 모든 store가 `null`이고 `ProjectHostState`의 모든 필드가 `PROJECT_HOST_DEFAULTS`와 같으며 `opened`가 1 올랐고, `AppHostState`의 필드(`current`, `tab`, `camillaDevices`, `extraRates`)는 그대로인지 표를 순회해 확인합니다. 외부 교체에서 `designName`이 비는 것이 이 시험의 첫 회귀 사례입니다.
- `effects.test.ts`와 `externalChanges.test.ts`의 교체 관련 사례는 이 표를 가져다 쓰고, 손으로 적은 키 목록을 지웁니다.
- 타입 시험: `HostState`에 필드를 추가하고 두 타입 어느 쪽에도 넣지 않으면 `tsc`가 실패합니다(`satisfies` 검사).

## 이행

1. `projectScope.ts`와 두 타입, 표, `replaceProject`. 세 호출 자리를 바꿉니다. 한 커밋입니다.
2. 시험 셋을 표 기반으로 바꾸고 손 목록을 지웁니다.

## 결과

- 새 프로젝트 소유 필드는 표 한 줄입니다. 두 목록이 다시 어긋날 자리가 없습니다.
- 외부 교체 뒤에 이전 프로젝트의 설계 이름이 더 이상 남지 않습니다. 화면 동작의 작은 변경이므로 CHANGELOG에 한 줄 기록합니다.
- 계산과 내보낸 파일은 바뀌지 않으므로 편차 항목은 없습니다.
- 다음 감사는 효과 규칙의 `reload` 목록이 교체 표와 따로 있는 것을 사본으로 제안하지 않습니다. 하나는 '무엇을 비우는가', 다른 하나는 '무엇을 다시 읽는가'입니다.
