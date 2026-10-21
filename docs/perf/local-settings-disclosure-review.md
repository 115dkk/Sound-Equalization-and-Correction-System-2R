# 전역 전문가 표시 제거 — 화면 검토

모드: **full**. 범위는 설정 패널과 로고 선택, 초고지연 모드, 자체 측정의
장치 공유 방식, 시스템 채널 상세, 관련 도움말·저장 계약입니다.
React 19, 기존 CSS 토큰과 공용 disclosure를 사용하며
make-interfaces-feel-better의 `SKILL.md`와 `write.md`를 적용했습니다.
Qt·게임 규칙은 해당하지 않습니다.

## 확인한 범위

| 항목 | 확인 근거 | 결과 |
|---|---|---|
| Typography | 설정 패널·4개 모드·장치 공유·상세 도움말의 ko/en × 두 창 크기 × 두 테마 | 8변형에서 가로 넘침·작은 글자 없음, 공유 disclosure 14px |
| Surfaces | 기존 Advanced, 장치 카드, 스피커 상세 | 별도 패널·전역 상태 없이 해당 기능 안에서 공개 |
| Animations | 공용 disclosure의 기존 150ms chevron rotate | 10% 재생 속도로 펼침 완료 확인, 90도 회전과 aria-expanded 일치 |
| Icons | 기존 chevron, HelpTip, 로고 두 종류 | currentColor 아이콘과 두 로고 원본 유지 |
| Performance | Prefs/AppSettings·뷰의 조건식 제거 | 새 의존성·요청·폴링 없음, 전역 표시 상태와 분기 감소 |
| Color | 공유 disclosure 글자와 첫 불투명 조상 배경의 Chromium sRGB 값 | 밝게 6.67:1, 어둡게 7.70:1 |
| Writing | ko/en, 설정·오류·도움말, 갤러리와 현재 설계 문서 | 전문가 자격을 요구하는 설명 삭제, 로고 농담은 보존 |
| Interruptions | 모드 선택, 장치 공유, 설정 저장 | 새 확인 창 없음, 로고는 설정 접근·프로젝트 값을 바꾸지 않음 |

## 수정한 문제

UI 경로는 `apps/secs2r/ui/src/` 기준입니다.

| 심각도 | 위치 | Before | After | 이유 |
|---|---|---|---|---|
| MEDIUM | `app/menus.tsx:78`, `app/prefs.tsx:16`, `App.tsx:205`, `crates/secs-types/src/ipc.rs:1820` | 별도 전역 전문가 스위치와 사용자별 저장값 | 스위치·상태·IPC 필드 제거, 로고 외형 선택 유지 | 다른 화면의 스위치를 알아야 설정에 접근하는 간접 경로 제거 |
| MEDIUM | `screens/calibrate/Settings.tsx:66` | 초고지연 선택을 전역 상태로 숨김 | 네 모드를 같은 선택 목록에서 제공, 초고지연일 때 지연 값 표시 | 선택지와 그 값이 같은 위치에 있음 |
| MEDIUM | `screens/connect/ConnectScreen.tsx:436`, `ui/Card.tsx:71`, ko/en | 전역 스위치를 켜야 장치 공유 방식이 나타남 | 장치 카드의 ‘장치 공유 방식’을 펼쳐 조절, 저장된 독점/공유 값은 처음부터 펼침 | 항목의 실제 위치에서 발견·조작, 제목 반복 없이 기존 공용 부품 사용 |
| MEDIUM | `screens/system/SystemScreen.tsx:547`, `system.css`, ko/en | 읽기 전용 밴드 열·파일상 역할 이름을 전역 상태로 가림 | 중복 열 제거, 기존 스피커 상세의 밴드·출력·극성과 해당 도움말에서 확인 | 같은 정보의 정본을 하나로 두고 접근 가능하게 함 |
| MEDIUM | `i18n/ko.json:697`, `i18n/en.json`, `gallery/`, 화면 견본·테스트, 현재 설계·용어·도달표 문서 | 전문가 표시를 요구하는 안내와 재현 상태 | 실제 항목명과 공개 위치로 정리, 새 공유·초고지연·다중 밴드 장면; ARCHITECTURE·DEVIATIONS·DESIGN·M5·M5-reach·UI-TERMS 동기화 | 제거한 기능을 찾아야 하는 설명과 검증 사각지대 제거 |

## 검토 후 제외한 대안

| 위치 | 후보 | 제외 이유 |
|---|---|---|
| 전역 표시 설정 | 이름만 ‘상세 설정’으로 변경 | 설정이 필요한 화면 밖의 스위치를 찾아야 하는 문제가 남음 |
| 로고 | 전문가/킹반인 이름 변경 또는 기능 차이 해명 추가 | 사용자가 의도한 농담이며 실제 계산·접근과 무관함 |
| 초고지연 | 별도 전역 모드 또는 숨은 조작 유지 | 네 번째 선택지를 기존 모드 목록에 두면 충분함 |
| 밴드 정보 | 모든 화면의 표에 항상 열 추가 | 기존 스피커 상세가 이미 밴드별 연결을 제공하며 작은 창의 표만 넓어짐 |

## 검증과 한계

- 이전 v1 설정의 expert_display true/false × 두 로고 조합을 읽고 다른 설정을
  보존하며, 재저장 시 폐기 필드를 제외하는 서비스 테스트를 추가했습니다.
  설정 파일 버전은 1입니다. 새 파일을 구버전 앱으로 다시 읽는 역방향 호환은
  제공하지 않으며, 구버전 앱은 해당 필드가 없으면 앱 설정을 기본값으로 읽을 수 있습니다.
- 로고 양방향 변경 후 네 모드·장치 공유 접근과 프로젝트·측정·보정 값이 같은지
  실제 App 통합 테스트로 확인합니다.
- 조정자 `npm ci`, `npm run typecheck`, `npm test`, `npm run build`,
  `node node_modules/vite/bin/vite.js build --outDir <after-dist>`: 통과.
  UI 테스트는 **49파일 457개**입니다.
- 조정자 `verify.mjs`: ko/en × 밝게/어둡게 × 1280×800/1024×700의 **8변형** 통과.
  전역 토글 부재, 로고 두 선택, 네 보정 모드, 초고지연 80ms 값 변경·이동 후 유지,
  공유 방식의 키보드 선택·접기·저장값 재표시·REW 미노출, 스피커 상세 도움말을 확인했습니다.
  가로 넘침·작은 글자·브라우저 오류 없음. disclosure는 기존 pseudo-element를 포함해
  실제 클릭 영역 40px이며, 글자 대비와 기존 회전 효과의 10% 속도도 확인했습니다.
- 변경 전 기준 화면 4장면 16장과 비교했습니다.
- 조정자 `node scripts/screen-tour.mjs --dist <after-dist> --out <after>`:
  **53장면 212장** 통과. 새 다중 밴드 상세·장치 공유·초고지연 장면을 포함합니다.
- Rust fmt, 두 clippy 명령, workspace 테스트 **652개**, cargo deny: 통과.
  Python unittest **44개**, 업스트림 트리 동일성: 통과.
- Windows의 기능 등록 검사는 기존 플랫폼 조건부 테스트
  `secs-sys::callback_native_sample_conversion`을 열거하지 못했습니다.
  새 설정 호환성 테스트는 등록됐으며, 이 플랫폼 차이는 Linux PR CI에서 최종 확인합니다.

판정: **Approve**. 남은 HIGH/MEDIUM/LOW 조치 없음. Linux 기능 등록 검사는 PR CI에서
확인할 항목입니다. 실제 오디오 장치의 공유 방식 협상과 네이티브 스크린 리더
낭독은 이번 화면·저장 계약 변경의 검증 범위에 포함하지 않습니다.
