# 0011. 작업 표시줄 아이콘을 위해 secs-sys에 unsafe를 둔다

상태: 채택, 2026-10-23

## 배경

설정에서 로고를 '킹반인 모드'로 바꾸면 앱 안의 로고와 창 아이콘은 바뀌는데,
Windows 작업 표시줄과 Alt+Tab의 아이콘은 '전문가 모드'로 남습니다(사용자
제보, DEVIATIONS U12).

Tauri 2.12의 `WebviewWindow::set_icon`은 tao의 `set_window_icon`을 부르고,
tao는 Windows에서 이것을 `WM_SETICON(ICON_SMALL)`로만 보냅니다. 작업 표시줄과
Alt+Tab은 큰 아이콘(`ICON_BIG`)을 그리는데, Tauri는 창을 만들 때도 바꿀 때도
큰 아이콘을 넘기지 않고, tao의 창 클래스에도 아이콘이 없습니다. 그래서 Windows는
실행 파일의 아이콘 리소스(전문가 모드 로고)로 돌아갑니다. tao에는
`set_taskbar_icon`(`ICON_BIG`)이 있지만 Tauri가 공개하지 않습니다.

큰 아이콘을 바꾸는 안전한 API는 지금 쓰는 의존성 어디에도 없습니다. 남는 길은
Win32의 `CreateIcon`, `SendMessageW(WM_SETICON, ICON_BIG)`, `DestroyIcon`을 직접
부르는 것이고, 셋 다 unsafe입니다. `secs-sys`의 예산은 0이므로 0002에 따라 이
ADR이 먼저입니다.

## 결정

- `secs-sys`에 Windows 전용 모듈 하나(`taskbar`)를 두고, 그 파일에서만 unsafe를
  허용합니다. 예산은 그 파일의 unsafe 블록 수만큼만 올립니다(아이콘 생성, 메시지
  전송, 이전 아이콘 해제의 세 곳을 기준으로 하고, 실제 개수를 `unsafe-budget.toml`에
  적습니다).
- 공개 함수는 0002의 잎 계약을 지킵니다. 창은 `raw_window_handle::HasWindowHandle`로
  빌려 받고, 아이콘은 RGBA 슬라이스와 너비·높이로 받으며, 결과는 `Result`입니다.
  원시 핸들, `HICON`, unsafe fn은 밖으로 나가지 않습니다. Windows가 아니면 아무것도
  하지 않고 `Ok(())`를 돌려줍니다(그 운영체제들에서는 Tauri의 `set_icon`으로 충분합니다).
- unsafe 전에 입력을 모두 검사합니다. 너비와 높이는 1–256, 슬라이스 길이는
  `너비 × 높이 × 4`, 창 핸들은 Win32 핸들이어야 하고, 어긋나면 `Err`입니다.
- `HICON`의 소유는 이렇게 정합니다. 새 아이콘을 만든 뒤 `WM_SETICON(ICON_BIG)`이
  돌려주는 이전 큰 아이콘은 이 함수가 앞서 만든 것뿐이므로(Tauri와 tao는 큰
  아이콘을 두지 않습니다) 해제합니다. 마지막 아이콘은 창과 함께 프로세스가 끝날 때
  돌아갑니다.
- 부르는 쪽(`apps/secs2r/src/icon.rs`)은 창의 스레드에서 부르도록
  `run_on_main_thread`로 넘깁니다. 작은 아이콘은 지금처럼 Tauri의 `set_icon`이
  바꾸고, 큰 아이콘을 같은 그림으로 함께 바꿉니다. 전문가 모드로 돌아갈 때도 같은
  함수에 기본 창 아이콘의 그림을 넘깁니다.
- 새 의존성은 이미 잠금 파일에 있는 판의 `windows-sys`(`Win32_Foundation`,
  `Win32_UI_WindowsAndMessaging`)와 `raw-window-handle` 0.6이고, `secs-sys`만 가집니다.
- 경계 지도는 `docs/UNSAFE_BOUNDARIES.md`에 '작업 표시줄 아이콘' 절로 더합니다.

## 결과

- 로고를 바꾸면 작업 표시줄과 Alt+Tab의 아이콘도 함께 바뀝니다. 작업 표시줄에
  고정한 바로가기의 아이콘은 바로가기 파일의 것이라 바뀌지 않습니다(Windows의 동작).
- `secs-sys`의 손으로 쓴 unsafe가 0에서 이 모듈의 몇 곳으로 늘어납니다. 크레이트
  루트는 `#![deny(unsafe_code)]`와 자기 lint 표로 바뀌고(0002), 나머지 모듈은 지금처럼
  unsafe를 쓸 수 없습니다.
- 이 경로는 Linux 컨테이너에서 실행할 수 없습니다. Windows 대상 clippy와 입력 검사의
  단위 테스트가 지키고, 실제 작업 표시줄 표시는 Windows에서 손으로 확인합니다.
- Tauri가 큰 아이콘을 바꾸는 API를 공개하면 이 모듈을 지우고 예산을 0으로 되돌립니다.
