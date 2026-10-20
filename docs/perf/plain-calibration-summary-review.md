# 계산 전 보정 요약 검토

모드: **full**. 범위: 계산 전 요약, 계산 후 제목, 보정 모드의 지연 안내와 필터 상세,
공통 도움말 아이콘의 사각형 장식과 간격.
React와 기존 CSS 토큰을 사용합니다. 기준은 make-interfaces-feel-better와 `write.md`입니다.
Qt·게임 규칙은 해당하지 않습니다.

## 확인한 범위

| 항목 | 확인 근거 | 결과 |
|---|---|---|
| Typography | ko/en, 1280×800·1024×700의 실제 앱과 견본 | 본문·도움말 14px, 두 언어 줄바꿈 확인 |
| Surfaces | 계산 전 카드와 모드 영역, 실제 DOM 치수 | 카드 높이 172.89 → 102.30px, 도움말 40×40px, 라벨·아이콘 사이 16px 이상 |
| Animations | 도움말의 hover·pointer down·클릭·키보드·터치 | 눌림 scale 제거, 색상 전환만 유지 |
| Icons | 단계·카드·필드·모드의 HelpTip | 16px 아이콘, 투명 40px 누름 영역, 키보드만 원형 초점 |
| Performance | 조건과 중복 렌더링 삭제, 소스·빌드 확인 | 새 의존성·계산·요청 없음 |
| Color | 실제 Chromium sRGB 변환 후 도움말 대비 측정 | 밝게 14.49:1, 어둡게 15.13:1 |
| Writing | ko/en 문구 및 서비스·엔진 호출 대조 | 완결 문장, 실제 실행하지 않는 작업 안내 제거 |
| Interruptions | 새 필터 구성 도움말의 Tab·Escape | 필요할 때 열고 닫음, 화면을 막지 않음 |

## 수정한 문제

경로는 `apps/secs2r/ui/src/` 기준입니다.

| 심각도 | 위치 | Before | After | 이유 |
|---|---|---|---|---|
| MEDIUM | `screens/calibrate/Results.tsx:40`, `i18n/ko.json:749`, `i18n/en.json` | 계산 전에도 ‘판정’, 측정·서브 작업을 가운데점으로 연결 | ‘계산할 내용’, ‘측정한 1곳을 기준으로 보정값을 계산합니다.’; 계산 후에는 ‘보정 결과’ | 아직 수행하지 않은 일과 결과를 구분하고 해석할 목록을 없앰 |
| HIGH | `screens/calibrate/Results.tsx:40`, 양언어 `calibrate.verdict.plan.xo_first` | 첫 계산에 크로스오버 추천이 포함돼 시간이 더 걸린다고 표시 | 해당 문장과 `needsXo` 조건 삭제 | 실제 `calibrate`는 추천을 자동 실행하지 않음 |
| MEDIUM | `screens/calibrate/Settings.tsx:56`, `screens/calibrate/text.ts:53`, 양언어 모드 문구 | RC·IIR·FIR 탭 수·MIMO를 기본 화면에서 나열 | ‘자동 보정의 추가 지연 한도: 10 ms’; 필터 상세는 ‘필터 구성’ 도움말 | 사용자에게 영향을 주는 값을 먼저 보여 주고 세부 사양은 선택적으로 읽음 |
| HIGH | `screens/calibrate/text.ts:53`, 양언어 `calibrate.mode.spec.*` | 현재 MIMO 설정과 무관하게 ‘MIMO IIR’ 표시 | 모드 사양에서 MIMO 표시 제거 | 서비스의 고정된 사양 값을 실행 예정 기능으로 오해할 수 있음 |
| MEDIUM | 양언어 `calibrate.mode.*.effect`, `calibrate.mode.spec.*` | 모드 효과·기술 사양·요약에 지연 수치 중복 | 한도는 모드 본문, 필터 방식은 상세 도움말, 수동 딜레이·장치 지연은 별도라고 설명 | 보정의 지연 예산과 실제 재생 지연을 구분 |
| MEDIUM | `ui/ui.css:1298` | hover·열림 상태의 사각 배경, 클릭 시 축소, 버튼 전체의 사각 초점과 터치 기본 강조 | 배경·축소·브라우저 터치 강조 삭제, hover는 아이콘 색만 변경, 키보드 초점은 SVG 둘레의 원형 선 | 설명 아이콘이 선택된 설정처럼 보이지 않게 하고 라벨 옆 간격을 보존 |

동작 근거: `useCalibrateScreen.ts`의 `calibrate`와 `recommendXo`는 별도 호출입니다.
`crates/secs-service/src/calibration.rs`는 보정 설계를 직접 시작하고,
`secs-engine/src/calibration.rs`의 `resolve_xo`는 선택값·프로젝트 값·기본 80 Hz를 사용합니다.
서브 고정 모드는 수동 값을 유지하며, 구성된 서브 수와 측정된 서브 수가 다를 수도 있습니다.
따라서 요약에 모든 서브를 자동으로 정렬한다는 문장을 추가하지 않았습니다.

## 검토 후 제외한 대안

| 위치 | 후보 | 제외 이유 |
|---|---|---|
| 계산 전 요약 | XO 추천·추가 시간 문장을 쉬운 말로만 바꿈 | 실제 실행하지 않는다는 사실 오류가 남음 |
| 계산 전 요약 | 측정 역할의 교집합으로 서브 수를 계산해 추가 표시 | 기존 서브 영역의 정보와 중복되며 요약을 위한 판단 로직이 늘어남 |
| 모드 영역 | 기존 기술 목록 전체를 그대로 도움말로 이동 | 현재 MIMO 동작과 무관한 고정 사양을 계속 노출하게 됨 |

## 검증

- `npm ci`, `npm run typecheck`, `npm test`: 48파일, 432개 테스트 통과.
- `npm run build`, `npm run build:gallery`: 통과.
- 견본 데이터 정합성 수정 후 타입 검사·보정 화면 테스트 20개·갤러리 빌드 재검증 통과.
- `screen-tour.mjs --only calibrate`: 실제 앱 9개 장면 × 4변형 = 36장 통과.
- 조정자 Chromium 검사: 계산 전·측정 없음·결과·실패·오래된 결과의 40개 화면,
  전후 비교 8개, 필터 상세 도움말 8개 통과. 가로 넘침·화면 밖 도움말·브라우저 오류 없음.
- 네 모드 및 위치 0/1/3곳·서브 0/1/3대 견본의 조정자 Chrome 재실행: 32개 조합 통과.
- `cargo fmt`, 두 clippy 명령, `cargo test --workspace --locked`(651개),
  `cargo deny`, Python unittest(44개), 업스트림 트리 검사: 통과.
- Windows 기능 등록 검사는 기존 플랫폼 조건부 테스트
  `secs-sys::callback_native_sample_conversion`을 열거하지 못합니다. Linux PR CI에서 최종 확인합니다.
- 최종 CSS로 앱·갤러리 재빌드 및 HelpTip/Popover 12개 테스트 통과.
- `check-help-icons.mjs <out> <final-dist>` 조정자 독립 실행: 8변형 × 4위치 ×
  기본·hover·클릭 후 이탈·키보드·터치 = 160상태 통과. 사각 배경·테두리·scale·터치 강조 없음,
  키보드 SVG 원형 초점, 40×40px 누름 영역, 16px 이상 간격과 영역 비겹침 확인.
- `node scripts/screen-tour.mjs --dist <dist> --out <out>`: 51장면 204장 생성.
  `export.eqapo-done.dark.ko`에서 리소스 로딩 오류 1회 발생.
  `--only export.eqapo-done` 재실행은 4변형 모두 통과했습니다.
- 최종 hover·터치·키보드 PNG 직접 확인. hover 아이콘 대비 최소 5.23:1.

판정: **Approve**. Linux 기능 등록 검사는 PR CI에서 확인할 항목입니다. 화면 검증은 Windows Chromium/Chrome과 모의 서비스 기준이며,
실제 오디오 장치 동작과 네이티브 스크린 리더 낭독은 이번 문구 수정의 검증 범위에 포함하지 않았습니다.
