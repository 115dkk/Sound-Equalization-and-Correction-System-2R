# P53 (Daybreak): 업스트림 SECS 2 Beta 1.3.2·1.3.3의 REW 드라이버·장치와 MIMO 표시

업스트림 커밋 `594f49e`(SECS 2 Beta 1.3.3, 1.3.2를 함께 담음)의 백엔드 부분입니다. 병합 커밋 `6cfe492`가 업스트림 파이썬 트리를 이미 가져왔습니다. 화면과 카탈로그는 이 작업서의 범위가 아닙니다(Claude가 뒤에 잇습니다).

## 작업 위치와 규칙

- 저장소 `/home/user/Sound-Equalization-and-Correction-System-2R`(브랜치 `claude/secs2r-fork-development-a2fh08`)에서 그대로 일합니다. 커밋하지 않고, git으로 인덱스나 브랜치를 바꾸지 않습니다. 하위 에이전트를 띄우지 않습니다.
- 디스크가 빠듯합니다. 기본 `target/`만 쓰고, 테스트는 `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`을 붙여 돌립니다. `df -h /`가 5 GB 아래로 내려가면 `target/debug/incremental`을 지웁니다.
- 업스트림 파이썬 트리(`src/`, `tests/`, `tools/`)는 고치지 않습니다. 읽기는 됩니다.
- 화면 파일(`apps/secs2r/ui/src/**`의 `.tsx`, `.css`, `i18n/*.json`, `ipc/mock.ts`, `ipc/fixtures.ts`)은 고치지 않습니다. `cargo test -p secs-types`가 다시 만드는 `apps/secs2r/ui/src/ipc/bindings/*.ts`는 반드시 다시 생성해 둡니다. 바인딩이 바뀌어 화면 typecheck가 깨지는 것은 Claude가 고칩니다(보고에 파일을 적어 주세요).
- `#![forbid(unsafe_code)]`는 그대로입니다.

## 먼저 읽을 것

- 업스트림 변경: `git diff 587159e 594f49e -- src tests`(줄바꿈 차이가 크면 `git diff -w` 또는 `diff <(git show 587159e:PATH | tr -d '\r') <(git show 594f49e:PATH)`). 특히 `src/secs/rewapi/rew.py`(`_AUDIO_PATHS`, `driver`, `set_driver`, `current_device`, `set_device`, `_parse_devices`, `_channel_names`, `set_output_channel`, `open_plan_devices`, `configure`), `src/secs/calibration.py`의 `mode_spec`, `tests/test_rewapi_real.py`의 바뀐 단언과 새 테스트 두 개(`test_java_multichannel_count_uses_output_channels`, `test_set_driver_posts_and_drops_list_cache`), `tests/test_calibration_step.py`의 `test_mode_spec_follows_mimo_fir_1018`.
- 이식판: `crates/secs-rew/src/{client.rs,types.rs,fake.rs,interfaces.rs}`, `crates/secs-rew/tests/rewapi_real.rs`, `crates/secs-service/src/{measurement.rs,project_edit.rs,ipc.rs,measure/rew.rs}`, `crates/secs-types/src/ipc.rs`(`Request`/`Response`, `RewInfoResult`, `ModeSpecParams`, `ModeSpecResult`, 메서드 이름 목록), `features.toml`.
- 0.19.1에서 넣은 편차: `docs/DEVIATIONS.md`의 R1(401·402·403은 경로와 무관하게 Pro 필요), R6(Pro 상태 관찰), R7(REW 장치). R1·R6은 그대로 둡니다.

## 할 일

### 1. REW 클라이언트(`secs-rew`) — 업스트림과 같게

1. **Java 출력 채널 경로.** Java의 출력 채널 선택지는 `GET /audio/java/output-channels`, 지정은 `POST /audio/java/output-channel {"channel": 이름}`입니다(`outputs`·`output`은 채널이 아니라 출력 포트였습니다; 7.1 장치가 2채널로 보이던 원인). ASIO는 그대로 `GET /audio/asio/outputs`, `POST /audio/asio/output {"output": 이름}`입니다. `audio_path`와 `set_output_channel`, 이 경로를 쓰는 모든 곳(채널 수 계산, 타이밍 레퍼런스 등)을 맞춥니다.
2. **채널 이름에서 묶음 제외.** 채널 이름 목록에서 `+`가 든 이름(`"L+R"`)을 뺍니다(업스트림 `_channel_names`). 채널 수도 뺀 목록으로 셉니다.
3. **드라이버 캐시 수명.** `driver()`는 한 번 읽고 영원히 캐시하지 않고, 장치 목록과 같은 수명(`LIST_CACHE`)으로 캐시합니다. 읽지 못하면 마지막으로 읽은 값, 그것도 없으면 `Java`입니다.
4. **`set_driver(name)`.** `POST /audio/driver {"driver": name}`, 시간 제한은 `max(기본, 30 s)`. 성공하면 드라이버 캐시와 목록 캐시를 비웁니다.
5. **`current_device(kind)`와 `set_device(kind, name)`.** 경로는 ASIO `/audio/asio/device`(입·출력 공용), Java `/audio/java/{output|input}-device`. 읽기는 0.19.1의 `current_audio`가 쓰는 너그러운 해석과 같습니다(업스트림은 객체의 `device` 또는 문자열). 쓰기는 `POST 경로 {"device": name}`, 시간 제한 `max(기본, 30 s)`(업스트림 실측: MOTU 24 출력을 여는 데 5 s가 넘음), 성공하면 목록 캐시를 비웁니다. `current_audio`는 `current_device`를 쓰도록 정리해도 됩니다(동작은 그대로).
6. **Java 장치 목록의 채널 수.** Java에서는 REW가 지금 연 장치의 채널 수만 압니다. 장치 목록을 만들 때 `current_device(kind)`와 이름이 같은 장치에만 채널 수를 붙이고, 나머지는 `channels: 0`으로 둡니다(업스트림은 `channels_unknown` 표시; 우리 `DeviceInfo`에 표시 칸을 더할지, 0을 '모름'으로 쓸지는 고르고 보고합니다. 화면은 0이면 채널 수 없이 이름만 보여 줄 것입니다). 현재 장치를 읽지 못하면 모든 장치에 채널 수를 붙입니다(업스트림과 같음). ASIO는 바뀌지 않습니다.
7. **`open_plan_devices(plan)`과 `configure`.** `configure`가 블로킹 모드를 켠 직후, 샘플레이트를 정하기 전에 `open_plan_devices`를 부릅니다: 출력·입력 각각 plan의 장치 이름이 비어 있지 않고 REW 장치 목록에 이름으로 있고 REW의 현재 장치와 다르면 `set_device`. ASIO는 입·출력이 같은 장치이므로 같은 이름을 두 번 보내지 않게 합니다(업스트림은 현재 장치 비교로 두 번째를 건너뜀).
8. `FakeRew`도 `driver`·`set_driver`·`current_device`·`set_device`를 업스트림 `fake.py`처럼 줍니다(호출 기록 포함).
9. 테스트(`crates/secs-rew/tests/rewapi_real.rs`): 업스트림의 바뀐 단언(Java `output-channel` `{"channel": …}`)을 옮기고, 새 테스트 둘을 옮깁니다(`test_java_multichannel_count_uses_output_channels`, `test_set_driver_posts_and_drops_list_cache`). 그리고 `open_plan_devices`(목록에 없는 이름은 보내지 않음, 이미 연 장치는 보내지 않음, ASIO는 한 번), Java 채널 수 규칙(현재 장치만 수가 있음), `set_device` 본문과 캐시 비우기를 시험합니다.

### 2. 서비스와 IPC(`secs-types`, `secs-service`)

1. **REW 드라이버.** `RewInfoResult`에 `driver: Option<String>`(REW의 지금 드라이버, 못 읽으면 `None`)을 더합니다. 새 IPC 메서드 `rew_driver_set`: 매개변수 `RewDriverSetParams { driver: String }`(빈 문자열은 `measurement_input` 오류, 필드 `driver`), 결과는 새로 읽은 `RewInfoResult`. REW에 닿지 않으면 `rew_unreachable`, REW가 거절하면 기존 `measure_error` 규칙(401·402·403은 `rew_pro_required`이고 R6 관찰 값을 갱신)입니다. 세션 잠금을 쥔 채 REW를 기다리지 않습니다. 메서드 이름 목록·`Request`/`Response`·디스패치(`crates/secs-service/src/ipc.rs`)·`features.toml`의 `ipc.rew_driver_set`(실제 테스트를 가리키게)을 함께 등록합니다. Tauri 셸은 요청마다 `spawn_blocking`으로 돌리므로 30 s까지 걸려도 다른 요청을 막지 않습니다.
2. **장치를 고르면 REW에도 엶.** `measure_configure`가 방법이 `rew`인 프로젝트(이번 호출로 `rew`로 바꾸는 경우 포함)에서 `input_device`나 `output_device`를 받으면, 지금처럼 프로젝트에 저장한 뒤 **세션 잠금을 놓고** REW에 그 장치를 엽니다(목록에 이름이 있고 현재 장치와 다를 때만, 업스트림 `send_device`). 이 전송의 실패는 `measure_configure`를 실패시키지 않습니다(업스트림도 실패를 삼키고 다음 새로 읽기가 드러냅니다). 다만 401·402·403이면 R6 관찰 값을 `Some(false)`로 둡니다. 0.19.1에서 단 주석('REW에 적용하지 않는다')을 이 동작에 맞게 고칩니다.
3. **원 측정 환경.** 0.19.1의 규칙(REW가 실제로 쓰는 장치를 우선, 못 읽으면 REW 목록에 있는 저장 이름만)은 그대로 둡니다. 측정 직전에 `configure`가 plan의 장치를 열므로 보통 둘이 같습니다.
4. **MIMO 표시(1.3.3).** `ModeSpecParams`에 `mimo_fir: Option<bool>`(`#[serde(default)]`)을 더하고, `ModeSpecResult.mimo_filter`를 `mimo_fir == Some(true)`이면 `"fir"`, 아니면 `"iir"`로 둡니다(업스트림 `mode_spec(…, mimo_fir=False)`와 같은 기본). 문서 주석의 '`iir` in every user mode'를 고칩니다. 테스트: 업스트림 `test_mode_spec_follows_mimo_fir_1018`처럼 zero·mixed·linear 각각 `true`면 `fir`, `false`면 `iir`.

### 3. 문서와 등록부

- `features.toml`: 새 테스트를 알맞은 항목에 더하고 `ipc.rew_driver_set`을 등록합니다. `python3 .github/scripts/check_feature_tests.py`가 통과해야 합니다.
- `docs/DEVIATIONS.md`, `CHANGELOG.md`는 고치지 않습니다(Claude 몫).

## 검증 명령(포그라운드로 실행)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --exclude secs2r --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p secs-rew -p secs-service -p secs-types
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

모든 명령이 끝나고 보고를 쓴 뒤에만 턴을 끝냅니다.

## 보고 형식

1. 할 일 번호마다 바꾼 파일과 동작(한두 문장씩).
2. 새·바뀐 타입, 함수, IPC 메서드와 필드의 최종 시그니처(와이어 이름 포함).
3. 더하거나 이름을 바꾼 테스트의 전체 이름(`크레이트::함수`).
4. 검증 명령 결과(통과 개수, 실패가 있으면 원문).
5. 바인딩 변경으로 화면 typecheck가 깨질 것으로 보이는 파일 목록.
6. 판단이 필요했던 자리와 고른 쪽(특히 1-6의 '채널 수 모름' 표현).
