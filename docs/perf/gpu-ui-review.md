# GPU 설정 UI 검토 — 2026-10-19

모드: full. 범위는 보정 화면 MIMO 카드의 계산 장치 선택, 다음 실행 상태,
지난 결과의 실제 경로, 오류·재시도·설치 안내입니다. React와 기존 일반 CSS
토큰을 사용했습니다. 설치한 `make-interfaces-feel-better`의 SKILL.md,
writing/typography/color/surfaces 규칙을 적용했습니다. 기존 다른 화면은
이번 검토의 범위가 아닙니다.

## 검토 근거

| 분류 | 확인한 근거 | 결과 |
|---|---|---|
| Typography | ko/en, light/dark, 넓은 화면과 1024×700 캡처·computed style | 새 UI 14px 이상, 가로 넘침 없음 |
| Surfaces | 기존 MIMO 카드, Select/button/details 클릭 영역 | 40px 이상, 새 중첩 카드 없음 |
| Animations | ComputeControl과 새 CSS | 새 애니메이션 없음, 10% 재생 대상 없음 |
| Icons | 공용 Select/ErrorNotice/Icon | 기존 currentColor 체계 재사용 |
| Performance | 새 CSS, 비동기 상태 조회/저장 테스트 | 광범위 transition 없음, 오래된 조회 종료로 최신 loading을 덮지 않음 |
| Color | 브라우저에서 실제 색상 대비 측정 | 최소 light 5.845:1, dark 6.690:1 |
| Writing | 신규 ko/en 키와 화면·상태 비교 | 합니다체, 선택/실제 결과 구분, 원시 오류는 detail |
| Interruptions | 설치 안내와 오류 동작 | 전체 화면 점유·카운트다운 없음 |

Qt와 game-writing은 이 React 설정 화면에 해당하지 않습니다.

## 해결한 항목

| 심각도 | 위치 | Before | After | 이유 |
|---|---|---|---|---|
| MEDIUM | `screens/calibrate/calibrate.css:447` | 공통 Select 높이 36px | 새 계산 영역에 40px 히트 영역 | 데스크톱 입력 영역 규칙 |
| MEDIUM | `screens/calibrate/ComputeControl.tsx:21` | 저장된 GPU가 사용 불가인데 다음 계산 CPU라고 표시 | GPU 사용 불가와 자동/CPU 변경 안내, 선택은 유지하고 실패할 MIMO 실행 비활성화 | 서비스의 명시적 GPU 오류와 화면의 약속 일치 |
| MEDIUM | 계산 장치 상태 | IIR에도 GPU 예고 | GPU 선택을 저장한 채 IIR의 다음 실행과 지난 결과는 CPU로 안내 | 끝단 음향 검증에 따른 FIR 전용 출하 범위 반영 |
| MEDIUM | `compute.selected.gpu` | 작은 자동 계산에도 다음 GPU라고 표시 | FIR 가속 장치라는 가용 장치 설명으로 변경 | 크기 기준으로 실제 실행은 CPU가 될 수 있음 |

## 검토 후 제외한 후보

| 위치 | 후보 | 제외 이유 |
|---|---|---|
| MIMO 카드 | 계산 장치만 별도 카드로 분리 | 기존 설정 맥락 안에서 선택과 결과를 함께 읽는 편이 적절함 |
| 장치 상태 | GPU에 성공색 배지 추가 | GPU 종류는 성공 판정이 아니며 기존 상태 문구로 충분함 |

## 검증

- UI 워커가 초기 10상태×2언어×2테마 40캡처를 만들고 확인했습니다.
- 부모가 자동 GPU 한국어, 런타임 미설치 영어, 최소 창 영어 화면을 독립
  이미지 검토했습니다. 코드 검토에서 발견한 명시적 GPU 사용 불가 상태는
  추가 4캡처·테스트로 보완했습니다.
- `npm run typecheck`, `npm test`: IIR 범위 반영 뒤 부모 재실행에서 44파일/398테스트 통과. 마지막 GPU 상태 문구 변경 뒤 관련 34테스트도 부모가 다시 확인했습니다.
- `npm run build`, `npm run build:gallery`: 통과.
- IIR CPU와 FIR 가속 장치 상태를 ko/en·light/dark로 추가 확인하고, 부모가 두 한국어 화면을 독립 이미지 검토했습니다.
- 최종 revision에서 부모가 12상태×2언어×2테마의 48캡처를 다시 생성했습니다. 자동 스타일·넘침·대비·포커스 검사에서 문제 0개였습니다.
- 재현: 갤러리 빌드 뒤 `node scripts/gpu-shots.mjs`. 12상태의 ko/en·
  light/dark 검사와 캡처를 `screen-shots/gpu/`에 저장합니다.
- 설치 안내의 typed IPC와 Host의 고정 URL·opener 연결을 테스트했습니다.
  실제 Tauri 창에서 외부 브라우저가 열리는 클릭은 별도 수동 검증하지 않았습니다.

판정: **Approve**. 남은 수동 확인은 실제 Tauri의 외부 브라우저 실행입니다.
