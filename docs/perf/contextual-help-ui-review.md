# 설정과 문맥 도움말 UI 검토

모드: **full**. 기준: make-interfaces-feel-better와 `write.md`.
범위: 시스템, 연결·점검, 측정·가져오기, 보정, 내보내기, 프로젝트와 앱 설정,
공통 입력·도움말·대화상자 및 ko/en 카탈로그입니다. React 19와 기존 CSS 토큰을
사용하며 전용 도움말·튜토리얼 본문은 유지합니다. Qt와 게임 규칙은 해당하지 않습니다.

## 범위와 근거

| 항목 | 확인 근거 | 결과 |
|---|---|---|
| Typography | 공통 `ui.css`, 전체 화면 투어, 1024×700의 ko/en·light/dark | 도움말 14px, 설정 이름·단위 보존, 40×40px 별도 도움말 버튼 |
| Surfaces | 다섯 작업 화면, 고급 서브 편집, 프로젝트·설정 패널 | 입력 옆이나 묶음 제목 옆에 도움말 배치, 반복 설명 제거 |
| Animations | hover/focus/active 상호작용, 브라우저 애니메이션 10% 재생 | 기존 색 전환·누름 토큰 사용, hover 중 대상 위치 변화 없음 |
| Icons | 공통 `HelpTip` 및 밝고 어두운 화면 | 기존 16px info 아이콘과 currentColor 사용, 누름 영역은 40px |
| Performance | `Tooltip.tsx` 수명·이벤트 정리, 앱·갤러리 빌드 | 열린 도움말 하나, 닫힐 때 관찰자·리스너 해제, 새 의존성 없음 |
| Color | 브라우저가 변환한 sRGB 픽셀의 상대 휘도 측정 | 도움말 본문 14.49:1 이상, info 아이콘의 기본·열림 상태 3.09:1 이상 |
| Writing | 양언어 카탈로그, 현재값·조건 보존 감사 | 대상이 드러나는 명사구 라벨, 효과·선택 기준을 짧은 도움말로 제공 |
| Interruptions | hover·키보드·터치·Escape·바깥 클릭 및 비활성 입력 시험 | 설정 변경 없이 도움말 확인, 첫 Escape는 도움말만 닫음 |

## 해결한 사항

경로는 `apps/secs2r/ui/src/` 기준입니다. 반복되는 같은 문제는 한 행으로 묶었습니다.

| 심각도 | 위치 | Before | After | 이유 |
|---|---|---|---|---|
| MEDIUM | `app/StepHeader.tsx:1`, `app/menus.tsx:130`, `screens/system/SystemScreen.tsx:599`, `screens/connect/ConnectScreen.tsx:137`, `screens/measure/MeasureScreen.tsx:154`, `screens/measure/MdatDialogs.tsx:168`, `screens/calibrate/Settings.tsx:38`, `screens/calibrate/ComputeControl.tsx:30`, `screens/export/ExportScreen.tsx:1`, `screens/project/ProjectDialogs.tsx:71` | 설정·카드마다 정적 설명을 상시 표시 | 명시적인 `help` 속성과 도움말 버튼으로 이전 | 값과 선택지를 먼저 읽을 수 있게 함 |
| MEDIUM | `screens/calibrate/Results.tsx:71` | 계산 계획을 긴 두 문단으로 설명 | 측정 위치 수·서브 수·첫 XO 추천·모드만 짧게 표시, 계산의 일반 설명은 도움말 | 현재 프로젝트의 사실과 일반 설명을 구분 |
| MEDIUM | `i18n/ko.json:1`, `i18n/en.json:1` | ‘입력 채널’, ‘출력 장치’, ‘마스터 출력’, ‘RC 범위’, ‘XO PEQ’ 등 대상이 불명확하거나 축약된 라벨 | ‘마이크 입력 채널’, ‘측정 출력 장치’, ‘점검 출력 레벨’, ‘RC 주파수 범위’, ‘크로스오버 PEQ’ 및 대응 영어 | 도움말을 열지 않아도 설정 대상을 식별 |
| MEDIUM | 양언어의 측정·보정·DSP·내보내기 도움말 | 조작 순서 반복, 내부 용어, 긴 설명 | 무엇이 달라지는지와 언제 선택하는지 1–2문장으로 표현 | 예: 긴 스윕의 잡음 감소와 시간 증가를 함께 설명. 네 측정 방식의 요구 조건은 짧은 항목 목록으로 비교 |
| HIGH | `ui/Tooltip.tsx:18`, `ui/HelpTip.tsx:8`, `ui/Dialog.tsx:108` | hover 전용에 가까운 툴팁, 본문으로 이동하면 종료, Escape가 부모 창도 닫음 | 400ms hover, 즉시 키보드 초점, 클릭·터치 고정, 본문 읽기 유지, Escape 우선 처리 | 포인터·키보드·터치 모두 같은 정보에 접근 |
| MEDIUM | `ui/Field.tsx:28`, `ui/NumberField.tsx:112`, `ui/controls.tsx:41`, `ui/Card.tsx:23` | 인라인 보충 설명과 상태의 구분 없음 | `help`는 선택적 배경 설명, `hint`·오류는 인라인 상태, 입력과 분리된 도움말 버튼·안정된 ARIA 설명 ID | 비활성 설정도 설명을 읽고 현재 제약을 확인 |
| MEDIUM | `ui/Tooltip.tsx:18`, `ui/Popover.tsx:34`, `ui/layer.tsx:66`, `ui/HelpTip.test.tsx:15` | 늦은 hover가 새 키보드 초점을 덮거나 portal 본문 클릭·터치가 도움말과 부모 패널을 닫음 | 최신 입력 의도를 따르고 본문 읽기 동작·소유 패널을 구분 | 혼합 입력에서 읽던 도움말이 사라지는 문제 방지 |
| MEDIUM | `screens/system/SystemScreen.tsx:599`, `screens/connect/ConnectScreen.tsx:137`, `screens/measure/MeasureScreen.tsx:154`, `screens/calibrate/Settings.tsx:38` | 설명 이전 과정에서 숫자 조건까지 숨을 가능성 | 목표 −30…−15 dBFS, 음향 타이밍 반복 1회, MIMO 위치 부족, 0=제한 없음 등을 인라인 유지 | 선택·사용 가능 여부를 결정하는 정보는 계속 보임 |
| LOW | `gallery/kitSpecimens.tsx:75`, `gallery/gallery.css:1`, `gallery/tour/scenes.ts:1`, `scripts/static-server.mjs:26` | 새 상호작용 견본 없음, Segmented 직계 선택자와 Windows favicon 경로 가정 | 도움말·가장자리 견본 추가, 새 DOM 선택자 반영, 정규화 경로 비교 | 재현 가능한 화면 검증 유지 |

추천 XO 주파수, 현재 서브 구성, 측정 위치·예상 시간, GPU 사용 불가·CPU 복귀,
오류 원인·복구 동작, 내보내기 제한과 EqAPO 백업 안내도 보존했습니다.

팝오버의 바깥 클릭 판정도 `ui/Popover.tsx:34`와 `ui/layer.tsx:66`에서 수정했습니다.
설정 패널 안 버튼의 ARIA 참조와 툴팁 ID가 연결된 경우만 같은 패널로 취급합니다.
다른 패널의 도움말을 누르면 기존 바깥 클릭 동작을 유지합니다. 실제 설정 패널과
Popover 회귀 테스트를 함께 사용해 portal 본문을 읽을 때 부모 패널이 닫히는 문제를 확인했습니다.

## 화면 밀도

같은 모의 서비스 상태를 같은 Chromium에서 1280×800으로 열어 비교했습니다.
값은 스크롤 영역 전체의 높이이며 캡처 한 장의 높이가 아닙니다.

| 장면 | 한국어 전 → 후(px) | 영어 전 → 후(px) | 표시 문단 수 전 → 후(양언어 동일) |
|---|---:|---:|---:|
| 시스템 | 2280 → 2259 | 2343 → 2322 | 6 → 1 |
| 연결·점검 | 1441 → 1415 | 1482 → 1415 | 8 → 3 |
| 측정 | 954 → 954 | 979 → 979 | 6 → 3 |
| 보정 | 2302 → 2212 | 2302 → 2212 | 20 → 9 |
| 내보내기 | 1053 → 962 | 1093 → 982 | 13 → 8 |

고정 크기 지도·그래프가 높이를 결정하는 측정 화면은 전체 높이가 같습니다.
별도 40px 도움말 누름 영역을 확보하므로 줄어든 문단 수와 높이 감소가 비례하지는 않습니다.
[브라우저 측정·소스 및 PNG 해시 기록](contextual-help-ui-comparison.json)에 36개 화면의
도움말 크기·대비·위치, 10개 전후 비교와 실제 상호작용 결과를 남겼습니다.

## 검토 후 제외한 변경

| 위치 | 후보 | 제외 이유 |
|---|---|---|
| 공통 Field·Card | 모든 `hint`와 `description`을 자동으로 툴팁 처리 | 실제 현재값·제약·복구 안내도 숨게 되어 명시적인 `help` 속성 사용 |
| 입력 라벨 | 라벨 자체를 도움말 버튼으로 사용 | 라벨 클릭으로 입력에 초점·선택을 주는 동작과 충돌 |
| 도움말 | hover만 사용하거나 브라우저 `title` 속성 사용 | 터치·키보드와 본문 hover 유지·닫기 동작을 일관되게 제공하기 어려움 |
| 보정·측정 | 용어·단위까지 모두 일반어로 대체 | Hz, dB, ms, RC, PEQ, FIR, MIMO는 실제 조작 대상을 구분하는 데 필요 |

## 검증과 판정

- `npm ci`, `npm run typecheck`, `npm test`: 통과(48파일, 432개 테스트).
- `npm run build`, `npm run build:gallery`: 통과.
- `node scripts/screen-tour.mjs`: 51개 장면 × 4변형 = 204장 통과. 마지막 문구 수정 뒤 보정·설정 4개 장면의 16장을 추가 확인했습니다.
- 9개 실제 화면 × ko/en × light/dark = 36개 최소 크기 화면과 열린 도움말: 통과. 가로 넘침·40px 미만 도움말 대상·본문 14px 미만·도움말 화면 이탈 없음.
- 실제 브라우저에서 hover 지연·본문 이동·고정·두 번째 클릭·Tab·Escape·비활성 MIMO·스크롤·창 크기 변경·터치를 확인했습니다. 설정 패널에서 도움말 본문을 터치해도 유지되고, 두 번째 버튼 터치로 닫히며 설정값이 바뀌지 않는 회귀도 통과했습니다.
- 별도 Chrome에서 1280×800/1024×700 × ko/en × light/dark의 공통 견본 8조합을 조정자가 재실행했습니다. 본문 텍스트 선택, 기본 터치 스크롤, 가장자리 배치, 인접 input/select/button의 누름 영역 비겹침과 실제 설정 팝오버의 본문 터치가 모두 통과했습니다. 새 대화상자 견본의 부스트 값도 실제 범위 0–10 dB 안의 6 dB입니다.
- `cargo fmt`, 두 clippy 명령, `cargo test --workspace --locked`(651개), `cargo deny`, Python unittest(44개), 업스트림 트리 검사: 통과.
- Windows의 `check_feature_tests.py`는 기존 플랫폼 조건부 `secs-sys::callback_native_sample_conversion`을 열거하지 못합니다. Linux PR CI의 같은 검사가 최종 판정입니다.

판정: **Approve (UI 검토 범위)**. 실행 환경은 Windows Chromium과 모의 서비스입니다.
네이티브 스크린 리더 낭독 및 실제 오디오 장치 동작은 이번 UI 검증 범위에 포함하지 않았습니다.
