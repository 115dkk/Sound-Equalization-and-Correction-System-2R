# M5 갤러리 리뷰

M5 판정(`docs/M5.md` §6)의 화면 리뷰 기록입니다. 화면을 그린 쪽(Opus 5.5)과 다른 쪽이 봅니다. Claude의 리뷰(아래)와 ASTRA의 읽기 전용 감사를 둘 다 거쳐, HIGH가 없을 때 끝납니다.

## Claude 리뷰 (full)

- **범위:** 갤러리 견본 131개 × 밝게·어둡게 × 한국어·영어(+튜토리얼 강조색) = 655장, `main`의 0.15.0 머지 커밋 `e0de246`.
- **틀과 스타일:** React 19, CSS 변수 토큰(`src/ui/tokens.css`), uPlot.
- **경계:** 화면을 실제로 본 것은 아래 "본 견본"이고, 나머지는 CI 갤러리 잡의 자동 검사(페이지 오류 없음, 13 px 미만 글자 없음)에 기댑니다.

본 견본: `screen.system.sample`, `screen.connect.mock`, `screen.measure.done`, `screen.calibrate.spl`, `screen.export.ready`(밝게·한국어, 1280), `screen.calibrate.ba.min`(어둡게·영어, 1024×700), `screen.calibrate.mimo-running`(어둡게·한국어), `screen.measure.mdat-assign.min`(밝게·영어), `screen.export.failed.min`(밝게·한국어), `frame.aside`, `app.live`, `tutorial.live.system`, `tutorial.live.calibrate`(P35 검토 때).

| 범주 | 본 것 | 결과 |
| --- | --- | --- |
| Typography | 위 견본의 제목, 라벨, 표, 숫자 열 | LOW 2 |
| Surfaces | 카드, 표 줄, 대화상자, 알림 | Clear |
| Color | 주 동작 하나(채운 단추), 상태 점·알약, 어둡게의 켜짐/꺼짐 주 동작 | Clear |
| Writing | 단계 설명, 카드 설명, 오류 알림, 대화상자 | Clear(P34 뒤) |
| Interruptions | mdat 배정 대화상자, 저장 확인, 튜토리얼 덮개 | Clear: 닫기 단추와 Esc, 취소가 있음 |
| Animations | 갤러리는 정지 화면 | Not reviewed: 10% 속도 재생은 앱에서 따로 |
| Icons | 레일, 단추, 알림 아이콘 | Clear |

### 찾은 것

#### Typography
| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| LOW | `screen.measure.*` 위치 목록(영어) | "Front right · 20 cm up"이 130 px 칸에서 두 줄 | 위치 목록 칸을 넓히거나 높이를 아이콘으로(`docs/UI-TERMS.md`) | 같은 목록 안에서 줄 높이가 들쭉날쭉합니다 |
| LOW | `screen.calibrate.*` 판정 타일(영어, 1024×700) | "GD spread"의 대역 글자가 둘째 줄로 | 라벨과 대역을 두 줄로 고정(다른 타일과 같은 꼴) | 타일들의 숫자 기준선이 어긋납니다 |

### 고려했지만 넘긴 것

| Location | Candidate | Rejected because |
| --- | --- | --- |
| 시스템·연결·점검의 채널 이름 | 한국어 화면의 "Left", "Subwoofer 1"을 한국어로 | SECS 2가 두 언어 모두에서 돌비 채널 이름을 영어로 보입니다(`src/secs/model.py:50`). 업스트림과 같게 둡니다 |
| 측정 곡선의 범례 13색 | 팔레트 절제 | 채널을 가르는 데이터 색이라 상태색 규칙과 별개입니다 |
| 갤러리의 "사전 출시판 0.13.0" | 앱 버전과 다름 | 갤러리 고정값입니다. 앱은 `bootstrap`의 버전을 보입니다 |

### 검증과 판정

- 검증: `npm run build:gallery`, `node scripts/gallery-shots.mjs`(655장, 오류 없음), 위 견본을 눈으로 확인.
- **Not verified:** 움직임(10% 속도), 실제 Tauri 창에서의 확대 배율(125%, 150%).
- **판정(Claude):** HIGH 없음. LOW 2건은 다음 화면 손질 묶음에서 다룹니다.

### 처리 (P40, 0.15.1)

- 위치 목록: 좁은 틀(카드 폭 ≤ 880 px)에서 행마다 상태를 라벨 아래 둘째 줄에 둡니다. 모든 행이 같은 높이이고, 지도와 목록은 나란히 남습니다. 지도와 목록을 위아래로 쌓는 안은 1024×700에서 목록이 첫 화면을 벗어나 버렸습니다.
- 판정 타일: 라벨 줄, 대역 줄, 값 줄을 공유하는 서브그리드로 모든 값이 한 기준선에 섭니다.
- 리뷰 밖에서 함께 고친 것: 계산 메모를 업스트림처럼 늘 보임(`step_calibration.py:2300`), 새 프로젝트·프로젝트 열기 대화상자의 첫 포커스, 상태 패널의 `custom`, 번들 경고. 도달표(`M5-reach.md`)의 빈 편차 칸 60곳을 채웠습니다.

## ASTRA 감사 (읽기 전용)

대기 중: GPT 토큰 오류(401 "Could not parse your authentication token")로 아직 돌리지 못했습니다.
