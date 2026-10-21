# 설명 대신 동작으로 연결하는 화면 검토

모드: **full**. 범위: 시스템의 측정 출력 장치, 막힌 단계의 복구 동작,
시작 도움말과 전 화면 감사에서 확인한 내부 처리·중복 조작 설명.
React 19와 기존 CSS 토큰을 사용하며 make-interfaces-feel-better와 `write.md`를 적용했습니다.
Qt·게임 규칙은 해당하지 않습니다. 실제 오디오 측정과 DSP 계산은 변경하지 않았습니다.

## 확인한 범위

| 항목 | 확인 근거 | 결과 |
|---|---|---|
| Typography | ko/en 카탈로그, 실제 앱 두 창 크기의 장치 행·도움말 | 14px 장치 글자, 두 언어 줄바꿈·가로 넘침 없음 |
| Surfaces | 시스템 장치 행, 단계 머리, 시작 도움말 | 기존 중립 배경과 quiet 버튼 사용, 새 경고 상자 없음 |
| Animations | App 탐색·초점, 공통 버튼과 도움말 스타일 | 새 애니메이션 없음; 설정 이동은 최근접 사용자 스크롤 영역에 한정 |
| Icons | DeviceLine 경고 SVG, StepHeader 복구 버튼 | 기존 currentColor warning 아이콘 사용; 색 외에 아이콘·상태 문구 제공 |
| Performance | App, useSystemScreen, HelpPanel 검색 | 새 의존성·폴링·요청 없음; 삭제한 문단은 검색 데이터에서도 제거 |
| Color | 장치 경고 글자·아이콘과 실제 배경 | 밝게 4.94:1, 어둡게 9.58:1 (Chromium sRGB 변환 후 측정) |
| Writing | 전 화면 감사와 ko/en 변경 키, 서비스 설정 로직 | 내부 처리·자명한 결과 설명 삭제, 실제 실행과 다른 설명 3건 정정 |
| Interruptions | 설정 이동, 도움말·튜토리얼·손실 확인 | 이동만 실행; 장치 선택·소리 재생·REW 실행·계산을 자동 실행하지 않음 |

## 수정한 문제

경로는 `apps/secs2r/ui/src/` 기준입니다.

| 심각도 | 위치 | Before | After | 이유 |
|---|---|---|---|---|
| MEDIUM | `screens/system/SystemScreen.tsx:259`, `screens/system/system.css:154`, 양언어 `system.device.*` | ‘고르지 않음 · 연결·점검에서 고르세요’와 채널 미확인 배지 | 노란 상태 글자·경고 삼각형·설정하기; 실제 방식/출력 선택 칸으로 이동 | 설명 없이 문제를 해결할 동작에 닿음 |
| HIGH | `screens/system/useSystemScreen.ts:62`, `screens/system/types.ts` | 빈 native 선택을 예전 config.device로 대체, REW/native 값 혼용 | 미선택·기본 장치·선택·확인 중·사용 불가·REW 미확인·장치 불필요를 구분 | 운영체제 기본값과 오래된 장치, 연결 해제, REW 소유 설정을 정확히 표시 |
| MEDIUM | `App.tsx:48`, `app/StepHeader.tsx:67`, `app/AppFrame.tsx`, `app/app.css` | 막힌 이유 뒤에 다른 단계로 가라는 문장 | 짧은 원인과 실제 설정 위치로 가는 버튼 | 레벨 점검은 점검 버튼, 구성 오류는 오류 출력, 측정 전제조건은 필요한 첫 설정으로 연결 |
| MEDIUM | `screens/help/content.ts:11`, `screens/help/HelpPanel.tsx`, `screens/help/types.ts`, 양언어 `help.system.intro.*` | 내부 프로젝트 처리·강조색·저장 제한·REW 조건과 긴 흐름 목록 | 짧은 용도 한 문장과 튜토리얼 시작 버튼 | 현재 행동에 필요하지 않은 해명을 삭제 |
| MEDIUM | `screens/help/content.ts`, `screens/project/ProjectDialogs.tsx`, `screens/measure/MeasureScreen.tsx`, `screens/export/ExportScreen.tsx`, `app/menus.tsx`, 양언어 `connect.mdat.body`, `tutorial.q.*`, `project.*`, `frame.blocked.*` | 버튼 후 줄이 생김·배지가 바뀜·다음 창이 열림·결과 필드가 나타남·테마 변경·초기 저장 형식 설명 | 설명과 미사용 키 삭제; 필요한 현재 상태·실제 복구·손실 확인 유지 | 같은 문제를 도움말이나 툴팁으로 다시 옮기지 않음 |
| HIGH | 양언어 `help.calibrate.steps.1.see`, `help.calibrate.steps.9.see`, `help.connect.real.5` | 첫 보정에 XO 추천 포함, 기본 IIR·고정 소요시간·FIR 효과 보장, 권장 스윕 레벨 자동 저장 | 잘못된 보정 설명 삭제, 스윕 레벨은 적용 버튼을 눌러 반영하는 실제 동작으로 정정 | 현재 코드가 수행하는 일과 설명을 일치시킴 |

실제 오류 복구, 클리핑·배선·측정 위치 조건, REW·OS·DSP의 외부 조작,
미저장 변경과 측정 삭제 확인은 보존했습니다. 짧은 도움말에서도 검색·탭·키보드 기능은 유지합니다.

## 검토 후 제외한 대안

| 위치 | 후보 | 제외 이유 |
|---|---|---|
| 장치 미선택 | 경고 배너나 채운 경고 박스 추가 | 짧은 상태와 행동으로 충분하며 기존 정보보다 과도하게 강조됨 |
| 설정하기 | native 방식 또는 첫 장치를 자동 선택 | 이동만 하려는 사용자의 측정 설정을 바꾸며 기본 장치를 추정하게 됨 |
| 시작 도움말 | 긴 설명을 접힌 상세 도움말로 이동 | 필요 없는 내부 처리 설명이 그대로 남음 |
| REW 출력 | native 장치 선택 칸으로 이동 | REW가 실제 사용하는 출력 장치와 소유권이 다름 |
| 레벨 미점검·REW Pro 없음 | 음량 필드·REW 재실행으로 일괄 이동 | 원인을 해결하지 못함; 실제 점검 버튼·다른 측정 방식 선택으로 구분 |

## 검증

- Rust fmt, 두 clippy 명령, workspace 테스트 **651개**, cargo deny: 통과.
- Python unittest **44개**, 업스트림 트리 동일성: 통과.
- Windows 기능 등록 검사는 기존 플랫폼 조건부 테스트
  `secs-sys::callback_native_sample_conversion`을 열거하지 못했습니다. Linux PR CI에서 최종 확인합니다.
- `npm ci`, `npm run typecheck`, `npm test`: **49파일 453개** 테스트 통과.
- `npm run build`, `node node_modules/vite/bin/vite.js build --outDir <after-dist>`: 통과.
- 조정자 `verify.mjs`: ko/en × light/dark × 1280×800/1024×700의 **8변형** 통과.
  방식 미선택의 설정 이동, native 명시 장치 → 기본값 → 시스템 복귀, REW 표시,
  mock/mdat 행 숨김, 단계별 선행 조건 복구, 레벨 점검·측정·계산 버튼 초점,
  구성 오류 입력 초점, 도움말 검색·탭·튜토리얼 시작 창·Escape를 확인했습니다.
  헤더·레일 위치 유지, 가로 넘침·작은 글자·브라우저 오류 없음, 설정 버튼 높이 40px.
- `check-specimens.mjs`: 장치 상태와 구성 오류·삭제 확인의 **36견본** 통과.
  견본 사이드바·단계의 측정 방식 표기를 실제 장치 상태와 맞춘 뒤 최종 타입 검사·갤러리 빌드·36견본 재검증도 통과했습니다.
- `node scripts/screen-tour.mjs --dist <after-dist> --out <after>`: **51장면 204장** 통과.

판정: **Approve**. 남은 HIGH/MEDIUM/LOW 조치 없음. Linux 기능 등록 검사는 PR CI에서 확인할 항목입니다. 화면 검증은 Windows Chromium과 모의 서비스 기준이며, 실제 오디오 장치 측정과 네이티브 스크린 리더 낭독은 이번 변경의 검증 범위에 포함하지 않았습니다.


