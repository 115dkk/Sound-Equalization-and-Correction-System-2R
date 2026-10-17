# P51 (Opus 5.5): 접힌 튜토리얼 알약의 자리

P44(`docs/packets/P44-tour-followups.md`)의 두 번째 항목입니다. 접힌 튜토리얼 알약('튜토리얼 · 연결·점검 5 / 9', `data-ui="tutorial.panel.pill"`)이 연결·점검 측정 방식 카드의 오른쪽 위 모서리를 덮습니다. 화면 투어의 `tutorial.connect` 장면에서 보입니다. 알약이 카드 내용과 겹치지 않는 자리를 정합니다.

## 작업 위치

- 워크트리 `/home/user/wt/p51`(브랜치 `p51`)에서만 일합니다. 다른 경로는 읽기만 합니다.
- 커밋하지 않습니다. git으로 인덱스나 작업 트리를 바꾸지 않습니다. 화면 의존성은 설치되어 있지 않으면 `npm ci`로 설치합니다(Node 24: `export PATH=/opt/nvm/versions/node/v24.22.0/bin:$PATH`).
- 같은 시각에 Claude가 다른 체크아웃에서 `screens/tutorial/engine.ts`의 `resumeIndex`와 `quests.ts`의 관문 표시를 고칩니다. 이 두 파일과 `types.ts`는 건드리지 않습니다.

## 먼저 읽을 것

`CLAUDE.md`(화면 규칙), `docs/DESIGN.md`, `apps/secs2r/ui/src/screens/tutorial/Tutorial.tsx`(알약 `tutorial-pill`, 이어 하기 `tutorial-resume`, `pillCovers`, 카드 자리 `placeCoach`가 피하는 상자), `tutorial.css`(290행 근처 '퀘스트 목록과 알약'), 프레임의 머리 줄과 내용 영역(`src/app/frame/`), `src/styles/tokens.css`.

업스트림은 알약을 진행바 아래 오른쪽 끝에 둡니다(`src/secs/wizard/tutorial_overlay.py:464-477`의 `place`). 강조 테두리를 덮으면 그동안 숨기고(같은 파일 940행 근처), 펼친 퀘스트 목록이 안내 카드를 가리면 접습니다. 우리 화면은 진행바 대신 레일과 단계 머리 줄이 있어 같은 좌표가 카드 위에 떨어집니다.

## 할 일

1. 투어로 지금 모양을 봅니다(밝게·어둡게, 1280·1024 폭). 어느 단계에서 알약이 무엇을 덮는지 적습니다(연결·점검 말고도 있으면 함께).
2. 알약이 내용 카드와 겹치지 않는 자리를 정합니다. 후보는 단계 머리 줄의 오른쪽 빈자리(머리 줄의 기본 동작이나 상태와 겹치지 않게), 또는 내용 영역 위쪽 여백입니다. 정한 자리가 펼친 퀘스트 목록(`tutorial-panel`)과 이어 하기 단추(`tutorial-resume`)의 자리와 어긋나지 않게 셋을 함께 봅니다(같은 모서리에서 접고 펴는 관계가 보이게).
3. 알약을 그 자리로 옮기고, 그 자리에서도 강조 테두리를 덮을 때 숨기는 규칙(`pillCovers`)이 실제 알약 상자로 판정하게 합니다(지금은 폭 280, 높이 36으로 짐작합니다).
4. 안내 카드 자리(`placeCoach`)가 피하는 상자에 알약이 그대로 들어가는지 확인합니다.
5. 테스트(`Tutorial.test.tsx`): 접힌 알약의 자리가 머리 줄(또는 정한 자리) 안에 있음, 강조 테두리를 덮으면 숨김.

## 건드리면 안 되는 인터페이스 id

`tutorial.panel.pill`, `tutorial.panel*`, `tutorial.resume`, `tutorial.coach*`, 프레임의 `frame.*` id. 새 id는 만들지 않습니다.

## 고쳐도 되는 파일

`src/screens/tutorial/Tutorial.tsx`, `tutorial.css`, `Tutorial.test.tsx`, 꼭 필요하면 프레임 머리 줄의 CSS(`src/app/frame/*.css`)와 그 테스트. 문구는 바꾸지 않습니다.

## 검증 (모두 포그라운드로, 보고를 마친 뒤에만 턴을 끝냅니다)

```sh
export PATH=/opt/nvm/versions/node/v24.22.0/bin:$PATH
cd /home/user/wt/p51/apps/secs2r/ui
npm run typecheck && npm test && npm run build
npm run build:gallery
CHROMIUM_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome node scripts/screen-tour.mjs --out /home/user/wt/p51-shots --only tutorial.
```

바뀐 장면의 PNG를 Read 도구로 직접 열어 봅니다(밝게·어둡게, 한국어·영어 각 하나 이상, 1024 폭 하나 이상).

## 보고 형식

1. 바꾼 파일과 각 한 줄 설명
2. 고치기 전에 덮던 자리(단계마다)와 정한 자리, 그 이유
3. 검증 명령마다 마지막 몇 줄의 출력(테스트 수, 찍은 수)과 본 PNG
4. 작업 중 눈에 띈 다른 화면 결함(고치지 않음)
