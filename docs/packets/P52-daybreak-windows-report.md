# P52 (Daybreak): Windows 실기 검증 결함 F1~F4의 백엔드

2026-10-18 Windows 실기 검증(SECS 2R 0.18.0, VB-Audio Virtual Cable, REW 5.40 Beta 134 무료판)이 찾은 결함 넷의 백엔드 부분입니다. 화면과 카탈로그는 이 작업서의 범위가 아닙니다(Claude가 뒤에 잇습니다).

## 작업 위치와 규칙

- 저장소 `/home/user/Sound-Equalization-and-Correction-System-2R`(브랜치 `claude/secs2r-fork-development-a2fh08`)에서 그대로 일합니다. 커밋하지 않고, git으로 인덱스나 브랜치를 바꾸지 않습니다. 하위 에이전트를 띄우지 않습니다.
- 디스크가 빠듯합니다. `CARGO_TARGET_DIR`를 따로 두지 말고 기본 `target/`을 씁니다. 빌드 전후로 `target/debug/incremental`을 지워도 됩니다.
- 업스트림 파이썬 트리(`src/`, `tests/`, `tools/`)는 고치지 않습니다. 읽기는 됩니다.
- 화면 파일(`apps/secs2r/ui/src/**`의 `.tsx`, `.css`, `i18n/*.json`)은 고치지 않습니다. 예외는 `cargo test -p secs-types`가 다시 만드는 `apps/secs2r/ui/src/ipc/bindings/*.ts`뿐이며, 이것은 반드시 다시 생성해 둡니다. 바인딩 타입이 바뀌어 화면의 `npm run typecheck`가 깨지는 것은 Claude가 고치므로 그대로 두되, 보고에 깨지는 파일을 적습니다.
- 모든 크레이트 루트의 `#![forbid(unsafe_code)]`는 그대로입니다. unsafe를 쓰지 않습니다.
- 확신 없는 API(wasapi 0.25, thiserror, ts-rs)는 `~/.cargo/registry/src/*/` 아래 소스를 열어 확인하고 씁니다.

## 배경(먼저 읽을 것)

- 검증 보고서: `/tmp/claude-0/-home-user/aee2471c-3739-50c5-a4c5-ffe157de8460/scratchpad/winreport/SECS2R_Windows_검증_20261018/읽어주세요.md`와 `native-trace-summary.json`.
- 코드: `crates/secs-sys/src/wasapi.rs`(특히 `PacketFlags`, `PacketHeader`, `CaptureQueue`, `WasapiInputSession::read_into`, 끝의 `mod tests`), `crates/secs-audio/src/{audio.rs,measurement.rs,level.rs,session.rs}`, `crates/secs-service/src/measurement.rs`(`measure_error`, `rew_status`, `measure_methods_for`, `rew_info`, `rew_launch`), `crates/secs-service/src/measure/{mod.rs,rew.rs,native.rs}`, `crates/secs-service/src/lib.rs`(`Service`), `crates/secs-rew/src/{client.rs,types.rs,fake.rs,interfaces.rs}`, `crates/secs-rew/tests/rewapi_real.rs`, `crates/secs-types/src/ipc.rs`(`ErrorCode`, `RewStatusResult`, `RewInfoResult`).
- 업스트림 REW 클라이언트: `src/secs/rewapi/rew.py`(286행 근처의 401·402·403 처리, `pro_state`·`is_pro`), `tools/rew_live_status.py`(장치 경로 목록).

## F1 · 첫 캡처 패킷의 불연속 표시로 녹음 전체를 버림 (`secs-sys`)

**원인.** WASAPI는 스트림을 시작한 뒤 첫 패킷에 `AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY`를 붙이는 일이 흔합니다(앞 패킷이 없으므로 '앞 패킷과 장치 위치가 이어지지 않음'은 시작 전이일 뿐입니다). 보고서의 추적에서 414패킷 가운데 표시가 붙은 것은 ordinal 0, 장치 위치(`buffer_index`) 0인 첫 패킷 하나뿐이었습니다. 지금 `read_into`와 `CaptureQueue::push_packet`은 모든 패킷의 표시를 `CaptureRead.discontinuity`로 올리고, `secs-audio`의 측정(`measurement.rs:220`)과 레벨 점검(`level.rs:215`)이 그것을 보고 녹음을 버립니다.

**고칠 동작.**

1. 플랫폼과 무관한 순수 함수로 패킷마다 '녹음이 끊겼는가'를 정합니다(`cfg(windows)` 밖에 두어 모든 OS의 단위 테스트가 닿게 합니다).
   ```rust
   /// What one capture packet says about the continuity of the recording.
   #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
   struct PacketContinuity {
       /// The packet's DATA_DISCONTINUITY flag on the first packet of the stream:
       /// the start transition, not lost audio.
       start_flag: bool,
       /// Audio was lost before this packet.
       gap: bool,
       /// The device position jumped forward (frames lost without a flag).
       position_jump: bool,
   }
   fn packet_continuity(ordinal: u64, header: &PacketHeader, expected_position: Option<u64>) -> PacketContinuity
   ```
   - `ordinal == 0`이면 표시가 있어도 `gap = false`, `start_flag = header.flags.discontinuity`입니다.
   - `ordinal > 0`에서 표시가 있으면 `gap = true`입니다.
   - `expected_position`(앞 패킷의 `index + frames`)이 있고 `header.index > expected`이면 `position_jump = true`, `gap = true`입니다. 장치 위치가 뒤로 가거나 그대로인 경우(위치를 0으로만 주는 드라이버 등)는 끊김으로 보지 않습니다. 이유: 그런 드라이버에서 모든 측정이 실패하는 회귀를 막고, 끊김은 앞으로 건너뛴 위치와 드라이버의 표시만으로 판정합니다.
   - `header.index`는 `IAudioCaptureClient::GetBuffer`의 장치 위치(스트림 시작부터 센 프레임 수)입니다(wasapi 0.25 `BufferInfo.index`, `api.rs`의 `read_from_device`).
2. `CaptureQueue::push_packet`이 패킷 표시를 그대로 OR하지 않고, 호출하는 쪽이 정한 `gap`만 큐의 불연속으로 남기게 바꿉니다(시그니처에 `gap: bool`을 더하거나 같은 효과의 방법). `silent`는 지금처럼 남깁니다.
3. `WasapiInputSession::read_into`가 `packet_continuity`를 쓰고, 다음 패킷의 `expected_position`을 기억합니다. 추적(`TraceRecord`)의 `discontinuity`·`initial_discontinuity` 칸은 지금처럼 장치가 준 원래 표시를 남기고, 위치 점프가 있으면 `detail`에 알 수 있게 남깁니다.
4. `CaptureStats`의 `discontinuity_packets`는 원래 표시를 센 값 그대로 두고, `position_jumps: u64`와 `start_discontinuity: bool`을 더합니다(`hardware_probe` 같은 진단 출력에 쓰이면 그 출력에도 나오게 합니다).
5. 같은 코드가 `Impulcifer-pip313`에도 있지만 그 저장소는 이번 범위가 아닙니다.

**테스트(모든 OS에서 돌아야 함, `wasapi.rs`의 `mod tests`).**
- 첫 패킷만 표시가 있는 스트림은 큐가 끊김을 보고하지 않는다(보고서의 실제 모양: 첫 패킷 480프레임, 위치 0, 표시 있음, 뒤 패킷 위치가 이어짐).
- 둘째 이후 패킷의 표시는 끊김이다.
- 표시 없이 위치가 앞으로 건너뛰면 끊김이다. 위치가 0으로 고정이거나 뒤로 가면 끊김이 아니다.
- 기존 `CaptureQueue` 테스트(`drain_into`가 큐에 남은 표본의 표시를 넘기는 것)는 새 규칙에 맞게 고치되, 표시가 큐에 남은 표본과 함께 넘어간다는 원래 뜻은 지킵니다.

## F2 · 녹음 품질 거절을 '장치를 열지 못함'으로 알림 (`secs-audio`, `secs-types`, `secs-service`)

**원인.** 측정과 레벨 점검이 녹음을 거절할 때 `AudioError::Backend`를 쓰고, 서비스의 `measure_error`가 모든 `Backend`를 `ErrorCode::DeviceUnavailable`로 바꿉니다. 화면은 이것을 '오디오 장치를 열지 못했습니다 / 다른 앱이 쓰고 있으면 닫으세요'로 보여 줍니다.

**고칠 동작.**

1. `secs-audio`에 녹음 거절을 따로 둡니다.
   ```rust
   /// Why a finished recording was rejected.
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub enum RecordingIssue { Gap, Underrun, NonFinite }
   // AudioError에 추가
   #[error("recording rejected ({issue}): {detail}")]
   Recording { issue: RecordingIssue, detail: String },
   ```
   `RecordingIssue`의 `Display`는 `gap`·`underrun`·`nonfinite`입니다. 판정 순서는 끊김(`capture.discontinuity`) → 재생 언더런(`playback.underruns > 0`) → 유한하지 않은 표본입니다. `detail`에는 언더런 횟수 등 진단에 쓸 영어 문장을 넣습니다.
2. `measurement.rs`(측정)와 `level.rs`(레벨 점검)의 거절이 이 변형을 씁니다. 다른 `Backend` 오류(장치를 열거나 읽다 실패)는 그대로 `Backend`입니다.
3. `secs-types`의 `ErrorCode`에 `RecordingFailed`를 더합니다(직렬화 이름 `recording_failed`, 문서 주석: 녹음은 끝났지만 끊기거나 깨져서 쓰지 않았다). 다시 생성한 바인딩을 둡니다.
4. `measure_error`가 `AudioError::Recording { issue, .. }`를 `ErrorCode::RecordingFailed`로, 매개변수 `reason`(= `issue`의 `Display`)을 붙여 바꿉니다. 이 오류는 다시 시도할 수 있는 오류입니다(`retryable`을 정하는 곳이 있으면 `DeviceUnavailable`과 같은 쪽).

**테스트.** 모의 백엔드(이미 있는 시험용 백엔드나 `sim_room`)로 측정과 레벨 점검이 끊김·언더런·NaN 녹음에서 각각 `RecordingFailed`와 알맞은 `reason`을 내는지, 장치를 못 찾을 때는 여전히 `DeviceUnavailable`인지 확인합니다.

## F3 · REW Pro를 확인하지 않고 '확인됨'으로 알리고, 설정 단계의 401을 놓침 (`secs-rew`, `secs-types`, `secs-service`)

**원인.** (a) `RewClient::is_pro()`는 모를 때 `true`이고, 서비스 `rew_status`가 `pro: reachable && client.is_pro()`를 화면에 보냅니다. REW API에는 라이선스를 알려 주는 곳이 없습니다(업스트림 주석, REW 도움말 확인). 업스트림도 같은 결함이 있습니다. (b) 우리 편차 R1이 Pro 판정을 `/measure/command`의 401·402·403으로 좁혔는데, 실제 REW 5.40은 스윕 설정(`POST /measure/sweep/configuration`)에서 `401 "A Pro upgrade license is required for this action"`을 줍니다. 업스트림은 경로와 무관하게 401·402·403을 Pro 필요로 봅니다.

**고칠 동작.**

1. `RewClient::request`: 상태가 401·402·403이면 경로와 무관하게 `RewError::ProRequired`입니다(업스트림과 같은 상태 규칙). 본문 문구로는 여전히 판정하지 않습니다(R1의 나머지 절반). 주석의 R1 설명을 고칩니다.
2. `crates/secs-rew/tests/rewapi_real.rs`의 `test_r1_pro_required_uses_only_status_and_measure_command_path`를 `test_r1_pro_required_uses_only_status`로 이름을 바꾸고 기대값을 새 규칙에 맞춥니다(`/measure/sweep/configuration` 401, `/version` 403, `/measurements` 403은 Pro 필요, 400·500은 아님, 본문의 'pro'는 판정에 쓰지 않음).
3. `secs-types`의 `RewStatusResult.pro`를 `Option<bool>`로 바꿉니다. 뜻: `None` 아직 모름, `Some(true)` 이 세션에서 REW 자동 스윕이 성공함, `Some(false)` REW가 Pro 필요로 거절함. 문서 주석을 이 뜻으로 고칩니다.
4. 서비스가 REW 라이선스 관찰을 기억합니다: `Service`에 `rew_license: Arc<Mutex<Option<bool>>>`(이름은 바꿔도 됨)를 두고,
   - REW를 거친 어떤 작업(측정 잡, 레벨 점검, 레벨 모니터, 설정 읽기 등)에서든 `RewError::ProRequired`가 나면 `Some(false)`로 둡니다. `measure_error` 같은 공통 경계 한 곳에서 처리할 수 있으면 그렇게 합니다.
   - REW 측정 잡이 끝까지 성공하면 `Some(true)`로 둡니다.
   - `rew_status`가 REW에 닿지 않는 것을 보면(꺼짐, 재시작) `None`으로 되돌립니다. `rew_launch`(화면의 '다시 확인' 단추)도 `None`으로 되돌립니다. 사용자가 REW에서 Pro를 켠 뒤 다시 확인할 수 있게 하려는 것입니다.
   - `rew_status`는 `pro: if reachable { 기억한 값 } else { None }`을 돌려줍니다.
   - `measure_methods_for`는 `reachable && pro != Some(false)`일 때 `"rew"`를 넣습니다(지금은 `pro == true`).
5. `RewInfoResult.pro`도 같은 기억 값을 씁니다(지금은 클라이언트의 `pro_hint`).

**테스트.** 가짜 전송(이미 있는 스크립트 전송)으로: 연결된 직후 `pro`는 `None`이고 `"rew"` 방식이 있다 → 스윕 설정 401로 측정이 실패하면 오류 코드가 `rew_pro_required`이고 그 뒤 상태의 `pro`가 `Some(false)`, `"rew"` 방식이 빠진다 → `rew_launch` 뒤에는 `None`으로 돌아온다. REW 측정이 성공하면 `Some(true)`.

## F4 · REW 방식에서 고른 장치가 REW에 적용되지 않음 (`secs-rew`, `secs-types`, `secs-service`)

**원인.** 연결 화면은 REW 방식에서도 입력·출력 장치와 입력 채널을 고르게 하고 프로젝트에 저장하지만, REW 경로는 그 값을 REW에 보내지 않습니다(업스트림도 같습니다: `rew.py`에는 장치 목록 읽기만 있고 지정은 없습니다). REW는 자기 환경설정의 장치로 잽니다. REW 도움말은 `/audio/java/input-device`, `/audio/java/output-device`, `/audio/java/input`, `/audio/asio/device`, `/audio/asio/input`이 '읽거나 쓸 수 있다'고만 하고 쓰는 본문 형식을 밝히지 않으므로, 이번에는 쓰지 않고 **읽어서 보여 주는** 쪽으로 고칩니다. 화면은 REW 방식에서 장치를 고르는 칸 대신 REW가 실제로 쓰는 장치를 보여 주고 'REW 환경설정에서 바꿉니다'라고 적을 것입니다(Claude 몫).

**고칠 동작.**

1. `secs-rew`: REW의 현재 장치와 입력을 읽는 메서드를 둡니다.
   ```rust
   /// REW's own current audio selection (what a sweep will use).
   pub struct CurrentAudio { pub input_device: Option<String>, pub output_device: Option<String>, pub input: Option<String> }
   pub fn current_audio(&mut self) -> CurrentAudio
   ```
   - Java 드라이버: `GET /audio/java/input-device`, `GET /audio/java/output-device`, `GET /audio/java/input`.
   - ASIO 드라이버: `GET /audio/asio/device`(입력·출력 장치 둘 다 이 값), `GET /audio/asio/input`.
   - 드라이버는 기존 `driver()`/`asio()`로 고릅니다. 반대 드라이버 경로는 REW가 400을 주므로 부르지 않습니다.
   - 응답 모양은 확인된 적이 없으므로 너그럽게 읽습니다: JSON 문자열이면 그 값, 객체이면 `device`·`name`·`input`·`output`·`channel`·`value` 키 가운데 처음 나오는 문자열 값, 그 밖이나 오류면 `None`. 빈 문자열도 `None`.
   - 상태 확인용 차단기(R2)의 규칙은 장치 목록 읽기와 같게 둡니다.
   - `FakeRew`도 같은 메서드를 주고, 시험에서 값을 정할 수 있게 합니다.
2. `secs-types`의 `RewInfoResult`에 `input_device: Option<String>`, `output_device: Option<String>`, `input: Option<String>`(REW의 현재 입력 채널 이름)를 더하고 `rew_info`가 채웁니다.
3. REW 측정(`crates/secs-service/src/measure/rew.rs`)에서 원 측정 환경(`from_rew_measurement`)에 넣는 장치 이름: 측정을 시작할 때 `current_audio()`로 읽은 REW의 실제 장치를 씁니다. 읽지 못한 쪽은 프로젝트에 저장된 이름을 쓰되, 그 이름이 REW 장치 목록(`output_devices`/`input_devices`)에 있을 때만 쓰고 아니면 빈 문자열입니다(자체 측정의 WASAPI 장치 ID가 REW 측정의 원 측정 환경에 들어가지 않게).
4. REW 방식에서 `measure_configure`가 `input_device`·`output_device`·`input_channel`을 받으면 지금처럼 저장은 하되(옛 프로젝트와 업스트림 `.secs` 호환), 측정에는 쓰지 않는다는 것을 그 함수의 주석에 적습니다. 동작은 바꾸지 않습니다.

**테스트.** 스크립트 전송으로 Java와 ASIO 각각에서 `current_audio`가 문자열 응답과 객체 응답(`{"device":"CABLE Output"}` 등)을 읽고, 400·빈 값·이상한 모양에서 `None`이 되는지. REW 측정의 원 측정 환경이 REW의 현재 장치 이름을 쓰고, 읽지 못하면 목록에 있는 저장 이름만 쓰는지.

## 문서와 등록부

- `features.toml`: 바꾼 테스트 이름(R1)을 고치고, 새 테스트를 알맞은 항목(`measure.native`, `measure.rew_api`, `ipc.*` 등)에 더합니다. `python3 .github/scripts/check_feature_tests.py`가 통과해야 합니다.
- `docs/DEVIATIONS.md`, `CHANGELOG.md`는 고치지 않습니다(Claude 몫). 보고에 바뀐 동작과 테스트 이름을 정확히 적어 주세요.

## 검증 명령(포그라운드로 실행)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --exclude secs2r --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test --locked -p secs-sys -p secs-audio -p secs-rew -p secs-service -p secs-types
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

Windows 대상 clippy는 `cfg(windows)` 코드(`read_into`)를 검사하는 유일한 로컬 수단이므로 반드시 돌립니다. 모든 명령이 끝나고 보고를 쓴 뒤에만 턴을 끝냅니다.

## 보고 형식

1. 결함마다 바꾼 파일과 동작(한두 문장씩).
2. 새 타입·함수·IPC 필드의 최종 시그니처.
3. 더하거나 이름을 바꾼 테스트의 전체 이름(`크레이트::함수`).
4. 위 검증 명령의 결과(통과 개수, 실패가 있으면 원문).
5. 바인딩 변경으로 화면 typecheck가 깨질 것으로 보이는 파일 목록.
6. 판단이 필요했던 자리와 고른 쪽.
