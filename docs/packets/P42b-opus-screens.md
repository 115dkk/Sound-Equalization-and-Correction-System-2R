# P42b: M5 감사 후속 — 화면 (Opus 5.5)

`docs/packets/P42-m5-followups.md`의 Opus 절과, `docs/M5-review.md` "확신하지 못한 것" 표의 마지막 줄(대화상자 높이)을 구현합니다. 배경은 `docs/M5-review.md`의 ASTRA 감사 표입니다.

## 작업 위치

- **워크트리 `/home/user/wt/p42-opus`(브랜치 `p42-opus`)에서만 일합니다.** 다른 경로의 파일은 읽기만 합니다. 같은 시각에 ASTRA가 `/home/user/wt/p42-astra`에서 연결·상태 쪽(P42a: `state/`, `screens/*/use*.ts`, `ipc/`, 서비스)을 고치고, 또 다른 Opus가 본 저장소에서 화면 투어(P43)를 만듭니다.
- 커밋하지 않습니다. git으로 인덱스나 작업 트리를 바꾸지 않습니다.
- 먼저 `CLAUDE.md`, `docs/DESIGN.md`, `docs/UI-TERMS.md`, 그리고 각 항목의 파일을 읽으세요. 토큰은 `src/styles/tokens.css`가 정본이고, 구성 요소 CSS에 색 값이나 글자 크기 값을 직접 쓰지 않습니다(`styles/tokens.test.ts`가 검사).

## 할 일

### 1. 튜토리얼 포커스와 Esc (`screens/tutorial/Tutorial.tsx`)

- 코치 카드가 열리면 포커스가 카드로 갑니다. 알림 카드(사용자가 '다음'만 누르는 카드)는 그 주 단추로, 행동 카드(허용 표적을 직접 조작하는 카드)는 카드 자체(`tabIndex={-1}`, 제목으로 이름 붙음)로 가서, Tab이 카드의 단추들과 허용 표적 사이를 오가게 합니다. 모달처럼 문서 전체를 가두지는 않습니다(표적은 앱 안에 있음).
- 완료 카드(`EndCard`)가 열리면 그 주 단추로 포커스를 옮깁니다.
- Esc는 어느 상태에서든(완료 상태 포함, 지금은 `engaged`가 거짓이 되면 리스너가 빠짐) 튜토리얼 끝내기와 이어집니다. 확인이 필요한 흐름이 이미 있으면 그것을 따릅니다.
- 테스트: `Tutorial.test.tsx`에 카드 열림 → 포커스 위치, 완료 카드 포커스, 완료 상태 Esc.

### 2. 이어 하기의 되감기 (`App.tsx` → `TutorialLayer`의 `stepOpen`)

`engine.ts:resumeIndex`의 주석이 뜻을 정합니다: 지금 과제의 단계를 더 이상 떠날 수 없으면(`stepOpen` 거짓) 그 단계의 첫 미완료 행동 과제로 되감습니다. `App.tsx`가 `stepOpen`을 넘기되, 판단은 틀의 단계 상태(`useFrameModel`의 `model.steps`)로 합니다. 어떤 상태가 "떠날 수 없음"인지는 `engine.ts`의 주석과 `app/model.ts`의 `StepStatus`·`BlockedReason`을 읽고 정해 보고서에 적습니다. 테스트: `quests.test.tsx` 또는 `Tutorial.test.tsx`에 막힌 단계로의 되감기 한 경우.

### 3. 곡선 불러오는 중 (`screens/calibrate/ResultGraph.tsx`)

`CalibrateView`에 `curvesLoading: boolean`이 있습니다(뜻은 `screens/calibrate/types.ts`의 주석). 이 워크트리에서는 훅이 늘 `false`를 넣고, 실제 값은 ASTRA가 채웁니다. 그림 규칙:

- 불러오는 동안 `curves`(이전 선택의 곡선)를 그대로 그리되 흐리게 둡니다. 이미 있는 낡은 결과 표시(`Graph`의 `stale`, 갤러리 `plot.stale`)의 흐림과 같은 시각 언어를 씁니다.
- "불러오는 중" 상태 표시는 요청이 150 ms를 넘길 때만 나타나 빠른 응답에서 깜빡이지 않게 합니다. 줄인 움직임 설정을 따르고, 읽기 프로그램에는 `aria-busy`와 짧은 상태 문구로 알립니다.
- `curves`가 null이고 불러오는 중이면 지금의 빈·불러오는 중 상태와 같은 자리를 씁니다.
- 견본: `screens/calibrate/specimens.tsx`에 불러오는 중 견본 하나(고정값에 `curvesLoading: true`). 테스트: 불러오는 중에 흐림 클래스와 `aria-busy`.

### 4. 캡션 크기 (`screens/export/export.css`)

`.export-fact dt`·`.export-line dt`(라벨)를 `--text-ui`로. 막힌 이유(`rail-step__reason`)와 지도 채널 코드는 캡션으로 둡니다.

### 5. 떠 있는 마이크 미터 (`App.tsx`의 `app.mic`, `screens/connect/MicMeterPanel.tsx`)

열리면(Ctrl+M) 패널의 닫기 단추나 제목으로 포커스가 가고, 패널 안에서 Esc를 누르면 닫히며, 닫힌 뒤 연 직전의 요소로 포커스가 돌아갑니다. 모달이 아니므로 포커스를 가두지 않습니다. 테스트 하나.

### 6. 판정 카드의 경고 (`screens/calibrate/Results.tsx`)

판정 카드가 `result.warnings`(`Notice[]`)를 노트와 같은 `noteLines` 방식으로, 경고 톤으로 노트 위에 그립니다. 비어 있으면 아무것도 그리지 않습니다. 이 그림은 한 자리에 모아 둡니다(ASTRA가 서비스 쪽에서 이 필드를 지우기로 판정하면 Claude가 그 자리만 걷어냅니다). 견본이나 테스트용 고정값에는 카탈로그에 있는 노트 코드 하나를 경고로 넣어 씁니다.

### 7. 대화상자 높이 (`ui/ui.css`의 `.ui-dialog`, `screens/project/project.css`의 최근 목록)

창 1024×700에서 최근 프로젝트 10개와 펼친 오류 자세히가 함께 있어도 대화상자의 단추가 창 안에 남아야 합니다: 대화상자 높이를 창 높이에서 여백을 뺀 값으로 막고, 본문만 스크롤되며 머리와 단추 줄은 고정. 공통 `Dialog`의 구조를 바꿔야 하면 모든 대화상자가 그대로 보이는지 갤러리로 확인합니다.

## 문구

새 문구가 필요하면 키를 정하고 `src/i18n/ko.json`·`en.json`에 초안을 함께 넣습니다(합니다체, 상태는 명사구). 새 키와 초안 문장을 보고서에 따로 적습니다. 문장 다듬기는 Claude가 ASTRA에게 넘깁니다. 이미 있는 키로 되면 그것을 씁니다.

## 허용 파일

`src/screens/tutorial/`, `src/screens/calibrate/ResultGraph.tsx`·`Results.tsx`·`calibrate.css`·`specimens.tsx`·`fixtures.ts`와 그 컴포넌트 테스트, `src/screens/export/export.css`, `src/screens/connect/MicMeterPanel.tsx`·`connect.css`와 그 테스트, `src/screens/project/` 중 컴포넌트·CSS·테스트, `src/ui/`(Dialog와 그 CSS·테스트), `src/App.tsx`, `src/app/`(필요할 때), `src/i18n/ko.json`·`en.json`(새 키만).

건드리지 않음: `src/state/`, `src/screens/*/use*.ts`, `src/ipc/`, `src/gallery/`, `scripts/`, `crates/`, `.github/`, `docs/`.

## 검증 (모두 포그라운드로, 보고를 마친 뒤에만 턴을 끝냅니다)

```sh
export PATH=/opt/nvm/versions/node/v24.22.0/bin:$PATH
cd /home/user/wt/p42-opus/apps/secs2r/ui
npm run typecheck && npm test && npm run build
npm run build:gallery
CHROMIUM_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome node scripts/gallery-shots.mjs --out /home/user/wt/p42-opus-shots
```

바뀐 견본(튜토리얼, 결과 그래프, 내보내기, 마이크 미터, 대화상자)의 PNG를 Read 도구로 직접 열어 봅니다. 7번은 1024×700 창에서 긴 목록을 띄운 상태를 Playwright로 한 번 찍어 확인합니다(스크립트는 저장소에 남기지 않음).

## 보고 형식

1. 바꾼 파일과 각 한 줄 설명
2. 1–7번 각각: 무엇을 했는지, 정한 규칙(2번의 "떠날 수 없음" 판정 등)
3. 새 카탈로그 키와 ko·en 초안
4. 검증 명령마다 마지막 몇 줄의 출력(테스트 수, 찍은 수)
5. 본 것: 작업 중 눈에 띈 다른 화면 결함(고치지 않음)
