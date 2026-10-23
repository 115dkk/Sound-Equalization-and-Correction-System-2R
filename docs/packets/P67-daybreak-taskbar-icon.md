# P67. 작업 표시줄 아이콘이 로고 선택을 따르게 (Daybreak)

설계: [ADR 0011](../adr/0011-taskbar-icon-unsafe.md). 먼저 읽으십시오. 이 작업서는
그 결정을 그대로 구현합니다.

## 증상과 원인

설정에서 로고를 '킹반인 모드'로 바꾸면 창의 작은 아이콘만 바뀌고 Windows 작업
표시줄과 Alt+Tab은 '전문가 모드' 로고로 남습니다. `apps/secs2r/src/icon.rs`의
`WebviewWindow::set_icon`은 tao `set_window_icon` → `WM_SETICON(ICON_SMALL)`만
보내고(`~/.cargo/registry/src/*/tao-0.37.1/src/platform_impl/windows/window.rs:846`),
큰 아이콘(`ICON_BIG`)은 아무도 설정하지 않습니다(같은 파일 858행 `set_taskbar_icon`,
1435행 창 클래스의 빈 `hIcon`).

## 할 일

1. `crates/secs-sys`
   - Windows 전용 모듈 `src/taskbar.rs`(`#[cfg(target_os = "windows")]`)를 만들고,
     그 파일 머리에서만 `#![allow(unsafe_code)]`로 unsafe를 허용합니다. 블록마다
     `// SAFETY:` 주석을 답니다(`clippy::undocumented_unsafe_blocks`). 블록 하나에
     unsafe 연산 하나(`multiple_unsafe_ops_per_block`).
   - 공개 함수(크레이트 루트에서 다시 내보냄, 모든 운영체제에서 컴파일됨):

     ```rust
     /// Sets the large icon of a window: the picture the Windows taskbar and
     /// Alt+Tab draw (ADR 0011). Does nothing on other systems.
     pub fn set_large_icon(
         window: &impl raw_window_handle::HasWindowHandle,
         rgba: &[u8],
         width: u32,
         height: u32,
     ) -> Result<(), LargeIconError>;
     ```

     `LargeIconError`는 `thiserror` 열거형: `Size { width, height }`(1–256 밖),
     `Length { expected, got }`, `Handle`(핸들을 얻지 못했거나 Win32가 아님),
     `Create`(아이콘 생성 실패). 검사는 unsafe보다 먼저, 모든 운영체제에서 합니다
     (Windows가 아니면 검사 뒤 `Ok(())`).
   - Windows 구현: RGBA → BGRA 색 비트맵과 AND 마스크(모두 0, 알파 채널이 투명을
     정함)로 `CreateIcon`(tao `platform_impl/windows/icon.rs`의 `from_rgba`와 같은
     방식이니 그 파일을 읽고 맞추십시오), `SendMessageW(hwnd, WM_SETICON, ICON_BIG,
     hicon)`, 돌려받은 이전 아이콘이 0이 아니면 `DestroyIcon`. 생성에 실패하면
     `Err(Create)`. 핸들은 `window.window_handle()` → `RawWindowHandle::Win32(h)` →
     `h.hwnd`.
   - `Cargo.toml`: Windows 대상 의존성에 `windows-sys`(Cargo.lock에 이미 있는 판 중
     하나, 새 판을 내려받지 않게), 기능 `Win32_Foundation`, `Win32_UI_WindowsAndMessaging`.
     모든 대상 의존성에 `raw-window-handle = "0.6"`(잠금 파일의 판). `[lints] workspace
     = true`를 secs-cuda-sys와 같은 자기 lint 표로 바꿉니다(ADR 0002: 예산 > 0인
     격리 크레이트). 루트 `src/lib.rs`는 `#![forbid(unsafe_code)]`를 `#![deny(unsafe_code)]`로
     바꾸고, 머리 주석의 '예산 0' 문장을 ADR 0011에 맞게 고칩니다.
   - 입력 검사 단위 테스트(모든 운영체제): 크기 0과 257, 길이 불일치. Windows 핸들이
     필요한 경로는 테스트하지 않습니다(Linux 컨테이너).
2. `unsafe-budget.toml`: `secs-sys = <실제 unsafe 토큰 수>`, 위 주석에 ADR 0011.
3. `crates/secs-policy`: 예산 > 0이 된 secs-sys를 정책 테스트가 받아들이는지 확인하고,
   필요하면 secs-cuda-sys를 다루는 방식 그대로 맞춥니다(정책을 약하게 하지 않습니다).
4. `apps/secs2r/src/icon.rs`: `show`가 작은 아이콘(지금의 `set_icon`)과 함께, Windows에서
   같은 그림으로 `secs_sys::set_large_icon`을 부릅니다. 창 스레드에서 부르도록
   `app.run_on_main_thread`(또는 창의 같은 API)로 넘깁니다. 전문가 모드는 기본 창 아이콘
   (`app.default_window_icon()`)의 RGBA를 넘깁니다. 큰 아이콘 실패는 지금처럼 조용히
   넘어가되 `Shown` 상태는 작은 아이콘 기준을 유지합니다. `apps/secs2r/Cargo.toml`이
   `raw-window-handle`을 따로 필요로 하면 더하지 말고 `WebviewWindow`가 구현하는
   `HasWindowHandle`을 그대로 넘기십시오.
5. `docs/UNSAFE_BOUNDARIES.md`: 끝에 '작업 표시줄 아이콘 (secs-sys, ADR 0011)' 절을 기존
   형식(공개 계약과 입력 검사 / unsafe 뒤에 숨긴 의무 표 / 남는 신뢰)으로 더합니다.
   한국어, 합니다체, em-dash 없이.

## 허용 파일

`crates/secs-sys/**`, `unsafe-budget.toml`, `crates/secs-policy/**`(필요할 때만),
`apps/secs2r/src/icon.rs`, `apps/secs2r/src/main.rs`(필요할 때만), `apps/secs2r/Cargo.toml`
(필요할 때만), `Cargo.lock`(위 의존성 추가로 생기는 변경만), `docs/UNSAFE_BOUNDARIES.md`.
화면(`apps/secs2r/ui`), 다른 문서, `CHANGELOG.md`, 워크플로는 건드리지 않습니다.
같은 작업 트리에서 조정자가 화면 파일을 동시에 고치고 있습니다.

## 검증 (포그라운드, 끝난 뒤에만 보고)

```sh
cd /home/user/Sound-Equalization-and-Correction-System-2R
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --exclude secs2r --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo clippy -p secs2r --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings   # 되면; 안 되면 이유를 보고
cargo test -p secs-sys -p secs-policy --locked
cargo test -p secs2r --locked
cargo deny --all-features check
```

`--target x86_64-pc-windows-msvc`는 이 컨테이너에 대상이 설치되어 있습니다(전체 게이트가
씁니다). `--locked`가 잠금 파일 변경 때문에 실패하면 `--locked` 없이 한 번 잠금 파일을
갱신하고, 그 변경이 새 크레이트를 내려받지 않았는지(새 `[[package]]`가 없는지) 보고합니다.

## 보고 형식

바꾼 파일, 최종 unsafe 토큰 수, 각 검증 명령의 결과, Cargo.lock 변경 요약, 확신하지 못한
Win32·tao 동작. 커밋하지 않습니다.
