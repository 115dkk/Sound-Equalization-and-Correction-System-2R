# 0002. unsafe는 secs-sys에만 두고, 규칙은 게이트로 지킨다

상태: 채택, 2026-10-13

## 배경

임펄사이퍼 3.x에서 시작해 spatial-audio-softdriver에서 다듬은 규칙을 그대로 씁니다. 손으로 쓰는 unsafe가 여러 크레이트에 흩어지면 검토할 곳이 늘고, 리뷰만으로는 새 unsafe가 들어오는 것을 막지 못합니다. 등록하지 않은 기능이나 테스트 없이 '구현됨'으로 표시한 기능도 같은 이유로 리뷰에서 놓칩니다.

SECS 2R은 자체 측정을 위해 운영체제 오디오(Windows WASAPI, macOS CoreAudio, Linux ALSA)를 다루고(0005), 메모리 조회, 잠자기 방지, 스레드 우선순위 같은 운영체제 기능도 씁니다. 임펄사이퍼의 `impulcifer-sys-win`은 wasapi-rs 0.25 이상의 안전한 API만으로 WASAPI를 다뤄 손으로 쓰는 unsafe 없이 이것을 해냈습니다.

## 결정

- 손으로 쓰는 unsafe를 가질 수 있는 크레이트를 격리 크레이트라고 부르고, 이름은 `-sys`로 끝냅니다. SECS 2R의 격리 크레이트는 `secs-sys` 하나입니다.
- 크레이트별 허용량은 `unsafe-budget.toml`에 적습니다. `secs-sys`도 0에서 시작합니다. 숫자를 올리거나 격리 크레이트를 더하려면 ADR이 먼저입니다.
- 예산이 0인 크레이트는 모든 루트(lib, main, build, bin, tests, examples, benches)에 `#![forbid(unsafe_code)]`를 걸고 `[lints] workspace = true`를 둡니다. 예산이 0보다 큰 격리 크레이트는 루트에 `#![deny(unsafe_code)]`를 걸고 자기 lint 표(`unsafe_code`, `unsafe_op_in_unsafe_fn`, `undocumented_unsafe_blocks`, `multiple_unsafe_ops_per_block`, `missing_safety_doc`을 deny)를 가지며, unsafe를 쓰는 모듈 파일에서만 허용합니다. 테스트, 예제, 벤치는 언제나 forbid입니다.
- 운영체제를 직접 다루는 의존성(`wasapi`, `cpal`, `windows`, `windows-sys`, `windows-core`, `libc`, `nix`, `sysinfo`, `keepawake`, `thread-priority`, `audio_thread_priority`, `coreaudio-rs`)은 `secs-sys`만 가집니다. `secs-sys`에 의존할 수 있는 것은 `apps/` 아래의 앱(`secs2r`, `secs2r-cli`)뿐이고, 앱은 `secs-sys`의 어댑터를 `secs-audio`와 `secs-compute`의 seam에 주입합니다. 그래서 `secs-engine` 이하는 어느 운영체제에서나 오디오 장치 없이 빌드되고 테스트됩니다.
- 격리 크레이트가 밖으로 내보내는 인터페이스는 다음 계약을 지킵니다(임펄사이퍼 조사 보고서의 'Leaf contract'). 소유 값, 검사된 설정, 평범한 빌린 슬라이스, 열거형, `Result`만 내보냅니다. 원시 포인터, 핸들, COM 인터페이스, unsafe fn과 unsafe trait, `static mut`, transmute, 손으로 쓴 `Send`·`Sync`는 공개하지 않습니다. 네이티브 버퍼의 빌림은 크레이트 밖으로 나가지 않습니다. 잘못된 안전 입력은 `Err`나 문서화된 panic을 낳을 뿐 UB를 낳지 않습니다. `Drop`이 반드시 실행된다고 가정하지 않습니다(`mem::forget`은 안전한 함수입니다).
- 안전한 인터페이스로 닿을 수 있는 UB는 보안 취약점으로 신고받습니다(`SECURITY.md`).
- unsafe가 처음 생기면 그 PR에서 nvstrapsrebar의 형식으로 경계 지도(`docs/UNSAFE_BOUNDARIES.md`: 인터페이스 뒤에 숨긴 의무, unsafe 전에 흡수하는 입력, 남는 신뢰)를 함께 씁니다.
- 기능은 `features.toml`에 등록합니다. '구현됨'으로 표시한 기능은 실제로 있는 검증 테스트를 하나 이상 가리켜야 하고, 화면이 부를 수 있는 IPC 메서드는 모두 `ipc.<메서드>`로 등록합니다.
- `crates/secs-policy`의 테스트가 저장소 파일을 직접 읽어 위 규칙을 검사하고, 다음도 함께 봅니다.
  - faer는 `std` 기능 없이 씁니다. 그 기능이 끌어오는 `private-gemm-x86`의 어셈블리 커널이 AVX 상위 상태를 더럽혀 뒤따르는 SSE 명령이 모두 느려집니다(임펄사이퍼에서 PEQ 잔차가 8배 느려진 것을 쟀습니다).
  - 다른 프로세스를 띄울 수 있는 파일은 REW 실행기(`crates/secs-rew/src/launch.rs`) 하나입니다.
  - 업스트림 트리와 `UPSTREAM.toml`이 제자리에 있어야 합니다(0001).
- CI는 신뢰할 수 없는 바이트를 읽는 파서(jos, mdat, WAV, `.secs`)를 Miri와 cargo-fuzz로 돌립니다. 대상은 그 파서가 들어오는 M1부터 더합니다.

## 결과

- 새 unsafe, 등록하지 않은 기능, 테스트 없는 '구현됨' 표시, 운영체제 의존성의 누수, 허락받지 않은 서브프로세스는 리뷰보다 먼저 CI에서 걸립니다.
- 게이트는 빌드 설정을 거치지 않고 파일을 읽으므로, cfg나 빠진 `[lints] workspace = true`로 규칙을 피할 수 없습니다.
- 셋째 자리 의존성(wasapi-rs, cpal, Tauri, wgpu) 안의 unsafe는 이 예산에 들어가지 않습니다. 그 크레이트들의 불건전한 API는 쓰지 않고, 발견하면 업스트림에 알린 뒤 해당 API를 금지 목록에 올립니다.
