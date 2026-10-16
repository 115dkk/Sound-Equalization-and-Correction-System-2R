# P43: 화면 투어 (Opus 5.5)

사용자가 앱을 직접 띄우지 않고도 화면 전체를 단계·상태별로 볼 수 있게, **실제 `App`을 mock 서비스 위에 띄워 장면마다 창 전체를 찍는 투어**를 만듭니다. 찍은 PNG는 Claude가 만드는 워크플로(`.github/workflows/ui-gallery.yml`)가 리뷰 이슈로 올립니다. 이 작업서는 투어 페이지와 그것을 찍는 스크립트까지입니다.

기존 견본 갤러리(`gallery.html`, `scripts/gallery-shots.mjs`)는 부품 단위이고, `app.live`·`tutorial.live.*` 외에는 화면 전체가 없습니다. 투어는 그 빈자리를 채웁니다: 실장비 없이는 닿을 수 없는 상태(측정 중, 실패, 결과 그래프, 내보내기 완료 등)까지 실제 화면 코드로 그립니다.

저장소: `/home/user/Sound-Equalization-and-Correction-System-2R`. 화면 루트: `apps/secs2r/ui`(아래 경로는 여기 기준). 먼저 `CLAUDE.md`, `docs/DESIGN.md` §7(갤러리), `src/gallery/appSpecimen.tsx`, `src/gallery/tutorialLive.tsx`, `src/ipc/mock.ts`, `src/App.tsx`, `src/app/prefs.tsx`를 읽으세요.

## 1. 고정된 설계

### 1.1 페이지: `gallery.html`의 투어 모드

새 Vite 입구를 만들지 않습니다(`vite.config.ts`, `package.json`, `index.html`, `gallery.html`은 건드리지 않음 — 릴리스 게이트가 이 파일들을 출하물로 셉니다). 대신 `src/gallery/main.tsx`가 주소 인자를 보고 갈립니다.

- `gallery.html?tour=<scene id>&theme=light|dark&lang=ko|en` → 투어 장면 하나를 창 전체에 그림. `tour`가 없으면 지금 그대로 `Gallery`.
- 투어 모드는 진짜 앱 창과 같아야 합니다. `gallery.css`의 전역 규칙(지금 `body { min-width: 1344px }`)이 투어에 새면 안 됩니다. 갤러리 모드에서만 걸리게 바꾸세요(예: `html[data-gallery-page] body`처럼 갤러리 모드가 표시를 단 경우에만). 갤러리 모드의 모양은 바뀌면 안 됩니다(§3의 갤러리 찍기가 그대로 통과).
- 투어는 `App`을 `root` 그대로(=`<html>`에 테마·강조색·언어를 거는 실제 앱과 같은 방식)로 마운트합니다. `initialPrefs`는 주소 인자의 theme·lang과 장면의 `expert`(기본 false), `accent: "default"`. 튜토리얼 장면은 `tutorialStart`를 넘기고, 강조색은 앱이 프로젝트를 보고 스스로 바꿉니다.

### 1.2 장면 표: `src/gallery/tour/scenes.ts(x)`

```ts
export type TourOp =
  | { kind: "press"; key: string }            // Playwright keyboard.press 문자열, 예: "Control+3", "Escape", "F1"
  | { kind: "click"; selector: string }        // CSS 선택자. 언어에 따라 바뀌는 글자로 찾지 않습니다
  | { kind: "hover"; selector: string }
  | { kind: "page"; run: (ctx: TourContext) => void | Promise<void> }; // 페이지 안에서 실행(mock 조작 등)

export type TourContext = { mock: MockService };

export type TourScene = {
  id: string;                                  // 소문자, 점과 하이픈. 파일 이름이 됨
  step: StepId | "start" | "frame" | "tutorial"; // 이슈에서 묶는 절
  title: { ko: string; en: string };           // 이슈에 쓰는 장면 이름(명사구)
  note?: { ko: string; en: string };           // 무엇을 보라는지 한 문장(합니다체), 필요할 때만
  project?: "none" | "stereo" | "sample";      // mock의 시작 프로젝트. 기본 "sample"(아래)
  expert?: boolean;
  tutorialStart?: string;
  setup?: (mock: MockService) => void | Promise<void>; // App 마운트 전에 mock을 원하는 상태로
  ops?: TourOp[];                              // 마운트 뒤 차례로
};
export const TOUR_SCENES: TourScene[];
```

- mock은 `createMockService({ project: …, tickMs: 0 })`: **자동 틱 없음**. 작업 진행은 `setup`/`page` 연산에서 `mock.tick()`·`mock.drain()`으로 정해, 찍을 때마다 같은 그림이 나오게 합니다. 진행 중 장면은 몇 틱만 돌려 중간에서 멈춥니다. 어떤 장면이 자동 틱 없이는 그려지지 않으면 그 이유를 보고서에 적고 그 장면만 다른 값을 쓰세요.
- `project: "sample"`의 뜻은 `tutorialLive.tsx`처럼 7.1.4 + 서브 3의 프로젝트를 만든 상태입니다(필요하면 `sample_load`까지는 장면별 `setup`이 합니다). `"stereo"`는 2.0 새 프로젝트, `"none"`은 프로젝트 없음.
- 페이지는 `window.__tour`를 둡니다:
  `{ scene: { id, opCount, ops: ("press"|"click"|"hover"|"page")[] } , run(i: number): Promise<void> /* page 연산 i를 실행 */, op(i): 직렬화한 연산 i(press/click/hover의 인자) }`
  그리고 `setup`이 끝나고 첫 렌더가 자리를 잡으면 `document.documentElement.dataset.tourReady = "1"`. 장면 id가 없으면 `"missing"`을 달고 오류를 콘솔에 냅니다.

### 1.3 찍는 스크립트: `scripts/screen-tour.mjs`

```
node scripts/screen-tour.mjs [--dist dist] [--out screen-shots] [--only <id 접두어,…>]
```

- `gallery-shots.mjs`와 같은 정적 서버 방식. 겹치는 서버 코드는 `scripts/static-server.mjs`로 빼서 둘이 같이 써도 됩니다(그 경우 `gallery-shots.mjs`의 동작은 그대로).
- `CHROMIUM_PATH`가 있으면 그 Chromium으로. 이 컨테이너: `CHROMIUM_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome`.
- 장면 목록은 페이지에서 얻습니다(예: `gallery.html?tour=` 목록 모드가 `window.__tourScenes`로 id·step·title·note를 내고, 스크립트가 그것을 읽음). 스크립트에 장면 목록을 따로 적지 않습니다.
- 변형 넷: `light.ko`, `dark.ko`, `light.en`, `dark.en`. 창 1280×800(`tauri.conf.json`의 기본 창), `deviceScaleFactor: 1`, `reducedMotion: "reduce"`, `colorScheme`·`locale`은 변형대로. 장면마다 새 페이지.
- 흐름: 이동 → `tourReady` 대기(10초 넘으면 실패) → `document.fonts.ready` → 연산을 차례로(press/click/hover는 Playwright로, page는 `__tour.run(i)`) → 연산마다 두 번의 `requestAnimationFrame`으로 자리 잡기 → 찍기(`page.screenshot({ animations: "disabled" })`, 전체 창).
- 파일: `<out>/<id>.<theme>.<lang>.png`. 그리고 `<out>/manifest.json`:
  ```json
  { "viewport": { "width": 1280, "height": 800 },
    "variants": ["light.ko", "dark.ko", "light.en", "dark.en"],
    "scenes": [ { "id": "...", "step": "...", "title": { "ko": "...", "en": "..." }, "note": { "ko": "...", "en": "..." } } ] }
  ```
  `note`가 없는 장면은 키를 빼고, 장면 순서는 표 순서입니다. 파일 이름은 id와 변형에서 정해지므로 manifest에 적지 않습니다.
- 실패로 끝내는 경우(종료 코드 1, `::error::` 줄): 페이지 오류, 콘솔 오류, 준비 시간 초과, 연산 실패(선택자가 없음 등), 장면 0개, id 중복, 13px 미만 글자(`gallery-shots.mjs`의 같은 검사를 창 전체에), 찍은 수 ≠ 장면 수 × 4.

### 1.4 검사: `src/gallery/tour/scenes.test.ts`

id가 유일하고 형식(`/^[a-z0-9]+([.-][a-z0-9]+)*$/`)에 맞는지, `title.ko`·`title.en`이 비지 않았는지, 다섯 단계(`STEPS`)가 모두 한 장면 이상 있는지, click/hover 선택자에 한글·영문 문장이 들어가지 않았는지(대략: 공백이 든 따옴표 안 글자를 금지) 정도. vitest(`npm test`)에 들어갑니다.

## 2. 장면 목록 (이것을 덮으세요)

표의 id를 그대로 씁니다. "보이는 것"이 화면에 실제로 나와야 합니다. 어떤 상태가 mock과 UI 조작으로 닿지 않으면 **화면 코드를 고치지 말고** 그 장면을 빼고 보고서에 이유를 적습니다. 비슷한 상태가 화면에서 구분되지 않으면 하나로 합쳐도 됩니다. 반대로 꼭 보여야 할 상태가 표에 없으면 더해도 됩니다(보고서에 적음).

| id | step | 보이는 것 |
| --- | --- | --- |
| `start.new` | start | 프로젝트 없이 켰을 때 자동으로 뜨는 새 프로젝트 대화상자 |
| `start.empty` | start | 그 대화상자를 닫은 뒤의 빈 화면(새 프로젝트 버튼) |
| `start.tutorial-offer` | start | 튜토리얼 제안 대화상자 |
| `start.unsaved` | start | 저장하지 않은 변경이 있을 때 다른 프로젝트를 열거나 닫으려 하면 뜨는 확인 대화상자 |
| `system.stereo` | system | 2.0 새 프로젝트의 시스템 단계 |
| `system.sample` | system | 7.1.4 + 서브 3, 측정 경로를 아직 고르지 않음 |
| `system.invalid` | system | 구성 오류(같은 출력 두 번 등)와 뒤 단계가 막힌 레일 |
| `system.path-native` | system | 측정 경로: 이 컴퓨터의 장치 |
| `system.path-rew` | system | 측정 경로: REW |
| `system.path-mdat` | system | 측정 경로: mdat 가져오기(연결·점검을 건너뜀) |
| `system.expert` | system | 전문가 표시를 켠 시스템 단계 |
| `connect.ready` | connect | 장치를 고른 연결·점검, 레벨 점검 전 |
| `connect.level-running` | connect | 레벨 점검 진행 중 |
| `connect.level-done` | connect | 레벨 점검 결과가 채워진 상태 |
| `connect.rew` | connect | REW 경로의 연결 상태 |
| `connect.device-error` | connect | 장치를 열지 못한 실패 알림 |
| `connect.meter` | connect | 떠 있는 마이크 미터(Ctrl+M)에 읽기가 들어온 상태 |
| `measure.empty` | measure | 경로 준비됨, 아직 측정한 위치 없음 |
| `measure.running` | measure | 위치 측정 진행 중(작업 트레이의 진행 포함) |
| `measure.partial` | measure | 일부 위치만 측정됨 |
| `measure.complete` | measure | 모든 위치 측정 완료(견본 측정 불러옴) |
| `measure.low-snr` | measure | 어떤 위치에 낮은 SNR 경고 |
| `measure.mdat` | measure | mdat 가져오기 대화상자(위치 표시가 있는 줄과 없는 줄) |
| `measure.no-path` | measure | 측정 경로가 없어 막힌 화면 |
| `calibrate.blocked` | calibrate | 측정이 없어 막힌 보정 단계 |
| `calibrate.ready` | calibrate | 계산 전 설정 화면 |
| `calibrate.running` | calibrate | 보정 계산 진행 중 |
| `calibrate.result` | calibrate | 결과(기본 그래프) |
| `calibrate.result-alt` | calibrate | 결과의 다른 보기 하나(단위나 탭이 다른 것, 가장 다르게 생긴 것) |
| `calibrate.sub` | calibrate | 서브우퍼 설정 편집 |
| `calibrate.xo` | calibrate | 크로스오버 추천을 받은 상태 |
| `calibrate.stale` | calibrate | 설정을 바꿔 결과가 낡은 상태 |
| `calibrate.failed` | calibrate | 계산 실패 알림 |
| `export.blocked` | export | 결과가 없어 막힌 내보내기 |
| `export.ready` | export | 내보내기 전 |
| `export.running` | export | 내보내기 진행 중 |
| `export.done` | export | 내보내기 완료(파일 목록) |
| `export.failed` | export | 내보내기 실패 알림 |
| `frame.help` | frame | 도움말 패널(F1) |
| `frame.jobs` | frame | 펼친 작업 트레이: 진행 중 하나와 완료·실패·취소 기록 |
| `frame.project-menu` | frame | 프로젝트 메뉴를 연 상태 |
| `frame.shortcuts` | frame | 단축키 안내가 있으면 그것(없으면 빼기) |
| `tutorial.system` | tutorial | 튜토리얼: 시스템 단계의 첫 과제(`tutorialLive.tsx`와 같은 시작점) |
| `tutorial.connect` | tutorial | 튜토리얼: 연결·점검 |
| `tutorial.measure` | tutorial | 튜토리얼: 측정 |
| `tutorial.calibrate` | tutorial | 튜토리얼: 보정 |
| `tutorial.export` | tutorial | 튜토리얼: 내보내기 |

`title.ko`는 위 "보이는 것"을 짧은 명사구로(예: "레벨 점검 진행 중"), `title.en`은 그 영어. 화면 용어는 `docs/UI-TERMS.md`를 따릅니다. `note`는 리뷰어가 놓치기 쉬운 것이 있을 때만 한 문장.

## 3. 허용 파일

- 새로 만듦: `src/gallery/tour/`(장면 표, 투어 마운트, 테스트), `scripts/screen-tour.mjs`, 선택으로 `scripts/static-server.mjs`.
- 고침: `src/gallery/main.tsx`(모드 분기), `src/gallery/gallery.css`(전역 규칙을 갤러리 모드로 가두기만), 선택으로 `scripts/gallery-shots.mjs`(공용 서버로 바꾸기만), `.gitignore`(`screen-shots/` 추가가 필요하면 — `apps/secs2r/ui/.gitignore`나 루트 중 `gallery-shots`가 이미 있는 곳).
- `src/ipc/mock.ts`: 어떤 장면이 정말로 닿지 않을 때만, **더하기만** 하는 작은 조작 훅(기존 동작과 기존 테스트는 그대로). 고쳤다면 보고서에 왜인지 적습니다.
- 그 밖은 건드리지 않습니다: 화면 컴포넌트·스타일·카탈로그(`src/i18n/*.json`)·`App.tsx`·`state/`·`ipc/bindings/`·기존 견본 파일·`vite.config.ts`·`package.json`·`*.html`·`.github/`·`docs/`. 화면에서 결함을 보면 고치지 말고 보고서의 "본 것"에 적습니다.

## 4. 검증 (모두 포그라운드로 돌리고, 보고를 마친 뒤에만 턴을 끝냅니다)

Node 24: `export PATH=/opt/nvm/versions/node/v24.22.0/bin:$PATH`.

```sh
cd /home/user/Sound-Equalization-and-Correction-System-2R/apps/secs2r/ui
npm ci && npm run typecheck && npm test && npm run build      # 앱만 짓는 빌드도 통과
npm run build:gallery
CHROMIUM_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome node scripts/gallery-shots.mjs   # 갤러리 그대로 통과
CHROMIUM_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome node scripts/screen-tour.mjs     # 투어 통과
```

- 투어 PNG를 **직접 열어 보세요**(Read 도구로 PNG를 볼 수 있습니다). 적어도 모든 장면의 `light.ko`와, `dark.en` 몇 장. 장면이 의도한 상태를 실제로 보여 주는지(예: "진행 중"이 정말 진행 막대를 보이는지, 대화상자가 열려 있는지) 확인합니다. 그림이 빈 화면이거나 다른 장면과 같으면 고칩니다.
- 커밋하지 않습니다. 커밋은 Claude가 합니다.

## 5. 보고 형식

1. 만든/고친 파일 목록과 각 한 줄 설명.
2. 장면 표 결과: 표의 id마다 "됨 / 합침(무엇과) / 뺌(이유)", 더한 장면.
3. `mock.ts`를 고쳤으면 그 이유와 변경 요약.
4. 검증 명령마다 마지막 몇 줄의 출력(찍은 수, 테스트 수 포함).
5. 본 것: PNG를 보다가 눈에 띈 화면 결함이나 어색한 문구(고치지 않음), 파일 이름과 함께.
