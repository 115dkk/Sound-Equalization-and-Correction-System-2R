# P50 (Daybreak): 원 측정 환경을 프로젝트와 서비스에 잇기

업스트림 SECS 2 Beta 1.3.1(커밋 `587159e`, PR #38)의 나머지 절반입니다. P49가 `secs-mdat`에 원 측정 환경(`SourceInfo`, `extract`, `apply`, `from_rew_measurement`)과 빌더·보정 전후 내보내기의 새 인자(`source_info`, `computed`, `sources`, `now_ms`)를 옮겼습니다. 이 작업서는 그 환경을 측정 기록과 함께 남기고(`<프로젝트>.irs/<역할>_P<위치>.src.json`), mdat 가져오기·REW 측정·저장할 때의 곁 mdat·보정 전후 내보내기에 잇습니다.

## 작업 위치

- 워크트리 `/home/user/wt/p50`(브랜치 `p50`)에서만 일합니다. 다른 경로는 읽기만 합니다.
- 커밋하지 않습니다. git으로 인덱스나 작업 트리를 바꾸지 않습니다. 하위 에이전트를 띄우지 않습니다.
- cargo: `CARGO_TARGET_DIR=/home/user/wt/target-p50 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`. 오라클: `~/.venvs/secs-oracle/bin/python oracle/run.py <가족>`.
- 업스트림 파이썬 트리(`src/`, `tests/`)는 고치지 않습니다.

## 먼저 읽을 것

- 업스트림: `git show 587159e -- src tests`. 특히 `rewapi/session.py`(`IrSaver.source_path`·`save_source`·`load_source`, `MeasurementSession.record`, `load_project_sources`), `rewapi/rew.py`의 `_run_roles`·`_source_info`, `wizard/step5_measure.py`의 mdat 가져오기 저장(1510행 근처), `wizard/step8_mdat.py`의 `export`, `wizard/window.py`의 `_write_side_mdat`·`_spl_offsets`, `tests/test_mdat_source_info.py`.
- P49의 결과: `crates/secs-mdat/src/source_info.rs`, `builder.rs`, `ir.rs`, `crates/secs-export/src/lib.rs`, `docs/packets/P49-daybreak-mdat-source-info.md`.
- 이식판: `crates/secs-project/src/{record.rs,irs.rs,pyjson.rs}`, `crates/secs-service/src/{session.rs,mdat_import.rs,measurement.rs,export.rs,clock.rs,lib.rs}`, `crates/secs-rew/src/{client.rs,types.rs,fake.rs}`, `apps/secs2r-cli/src/main.rs`(결정적 모드), `.github/scripts/integrity.py`.
- 오라클: `oracle/export_service_mdat.py`, `oracle/export_project.py`, `oracle/README.md`.

## 할 일

### 1. `.src.json` (`secs-project`)

- `secs-project`가 `secs-mdat`를 일반 의존으로 씁니다(지금은 개발 의존).
- 업스트림 `IrSaver`와 같은 규칙:
  - 경로는 IR wav 옆의 `<줄기>.src.json`(`L_P1.wav` → `L_P1.src.json`).
  - 쓰기는 `json.dumps(info, ensure_ascii=False)`와 바이트가 같은 UTF-8입니다(`pyjson.rs`의 `dumps`가 이 꼴인지 확인하고, 아니면 이 용도의 함수를 둡니다: 구분자 `, `·`: `, 실수는 파이썬 `repr`, 정수는 그대로, 비 ASCII는 그대로). 키 순서는 `SourceInfo`의 순서(P49가 정함)입니다.
  - 환경이 없으면 옛 파일을 지웁니다(없으면 그대로). 다시 잰 측정에 옛 캘이 붙지 않게 하려는 것입니다.
  - 읽기는 파일이 없거나 JSON이 아니거나 최상위가 객체가 아니면 없음입니다. 객체이면 업스트림 `apply`가 그 사전을 읽는 규칙대로 `SourceInfo`로 바꿉니다: 문자열 칸은 `str(value)`(None은 건너뜀), 상자는 `_Boxes.make`의 규칙(종류가 셋 중 하나이고 값이 있을 때; `Boolean`은 `bool()`, `Integer`는 `int()`, `Double`은 `float()`), 캘은 객체일 때만, 배열은 숫자 목록일 때만, 날짜는 정수나 실수(파이썬에서 `bool`도 정수입니다), 메모·형식은 참으로 읽히는 값의 `str()`. 파이썬이 예외를 내는 모양(예: `strings`가 목록, 배열에 문자열)은 그 부분을 없는 것으로 봅니다. 이것은 편차이므로 `docs/DEVIATIONS.md`의 F 표에 한 줄 더합니다(업스트림은 내보내기가 실패하고 곁 mdat은 쓰지 않음).
- 측정 기록: `secs_project::MeasurementResult`에 `source_info: Option<secs_mdat::SourceInfo>`를 더하고, 기록할 때 IR wav를 쓴 뒤 업스트림 `MeasurementSession.record`처럼 `.src.json`을 쓰거나 지웁니다.
- `load_project_sources(project, project_path) -> secs_export::Sources`에 해당하는 함수(위치 → 역할 → 환경). 업스트림처럼 기록의 `ir` 경로를 프로젝트 폴더 기준으로 풀고, 위치가 정수가 아니거나 경로가 비면 건너뜁니다. 다른 이름으로 저장할 때 `.irs`를 통째로 복사하는 지금 동작으로 `.src.json`도 따라가는지 확인합니다.

### 2. mdat 가져오기 (`secs-service/src/mdat_import.rs`)

업스트림 `step5_measure`처럼 가져온 측정마다 `MeasurementInfo.source_info`를 기록에 넘깁니다(그래서 가져온 측정은 늘 `.src.json`이 생깁니다 — `extract`는 빈 칸까지 사전으로 돌려줌).

### 3. REW 측정 (`secs-rew`)

업스트림 `rew.py`의 `_run_roles`·`_source_info`처럼, REW로 잰 결과마다 그때 REW에 걸린 마이크 캘 파일(`input_calibration_file()`; 묻다가 실패하면 없음)과 계획의 출력 장치·입력 장치·`out <출력 번호>`로 `from_rew_measurement`를 부릅니다. 캘 파일 바이트는 여기서 읽습니다(못 읽으면 `text: None`). 시각은 주입한 시계에서 받습니다(가짜 REW와 테스트는 고정). `from_rew_measurement`가 `Err`(캘 파일의 `Sens Factor` 값이 숫자가 아님)이면 업스트림은 측정 자체가 실패합니다. 여기서는 측정을 그대로 남기고 환경 없이 기록합니다. `docs/DEVIATIONS.md`의 D 표에 한 줄 더합니다. 자체 측정과 모의 측정은 환경이 없습니다.

### 4. 저장할 때의 곁 mdat (`secs-service/src/session.rs`의 `write_side_mdat`)

업스트림 `window._write_side_mdat`과 맞춥니다. 지금 이식판은 제목(`{role} P{pos}`)과 메모(빈 문자열)와 순서가 업스트림과 다릅니다.

- 항목: 위치 오름차순, 위치 안에서는 측정 계획의 역할 순서(계획에 없는 역할은 뒤). 제목 `"{role}_P{pos}"`, 메모 `"SECS · 위치 {pos} · 원 측정"`, 시작 시각은 기록의 t0, SPL 오프셋은 기록의 값, 환경은 `.src.json`.
- 업스트림의 `_spl_offsets`는 `.secs`에 값이 없으면(10-09 이전 측정) REW에 UUID로 묻습니다. 이식판이 저장할 때 REW에 물을 수 있는 구조인지 보고, 없으면 기록의 값만 쓰고 보고합니다.
- 업스트림은 곁 mdat 쓰기가 실패해도 `.secs`·`.irs` 저장을 막지 않습니다(예외를 삼킴). 지금 이식판이 `?`로 저장을 실패시키면 업스트림처럼 고치고, 실패를 로그로 남깁니다.
- 시각은 서비스 시계(아래 6).

### 5. 보정 전후 내보내기 (`secs-service/src/export.rs`)

업스트림 `step8_mdat.export`처럼 `load_project_sources`로 모은 환경을 보정 전후 mdat에 넘깁니다(살아 있는 결과와 다시 연 프로젝트의 캐시 경로 모두). 단계별 mdat은 환경 없이 씁니다. 모든 mdat 빌드에 서비스 시계의 `now_ms`를 넘깁니다(P49가 남긴 `// P50` 자리).

### 6. 서비스 시계와 CLI

- 서비스 의존(`deps`)에 유닉스 시각(ms)을 주는 시계를 더합니다(지금의 `LocalTime` 시계와 같은 출처에서). 앱은 실제 시각입니다.
- `apps/secs2r-cli`의 `--deterministic-uuids` 모드는 이 시계도 고정합니다(지금 고정한 지역 시각 2024-01-02 03:04:05와 같은 순간, UTC 오프셋 0으로 `1704164645000`). `ci.yml`의 `integrity` 잡이 스레드 수별 실행을 바이트로 비교하므로 같은 실행 안의 모든 mdat이 같은 시각을 받아야 합니다.

## 골든

- `service_mdat` 가족(`oracle/export_service_mdat.py`): 업스트림 `load_mdat`가 이제 `.src.json`을 씁니다. `file_*` 사례에 가져온 뒤 `.irs`에 생긴 `.src.json`의 상대 경로와 바이트를 더합니다. 원 측정 환경이 든 mdat(P49의 `mdat` 가족이 남긴 환경 사례 blob)을 가져오는 사례를 하나 더합니다.
- 곁 mdat: 업스트림 `_write_side_mdat`을 셈(프로젝트, `.irs`의 IR과 `.src.json`, 계획의 역할 순서, `.secs`의 SPL 오프셋)으로 돌려 나온 mdat 바이트(UUID·시각 고정)를 남기는 사례 두세 개(환경 있음/없음, 계획에 없는 역할, 위치 여럿). 가족은 `service_mdat`에 둡니다.
- `.src.json` 읽기: 업스트림 `IrSaver.load_source` + `build_mdat_bytes`로 손으로 만든 `.src.json` 몇 벌(정상, 키 빠짐, 값 형이 다름 — 파이썬이 예외를 내지 않는 것만)을 읽어 빌드한 바이트.
- `project` 가족은 업스트림 `MeasurementSession.record`가 바뀌었으므로 다시 내 봅니다(환경 없는 기록은 바뀌지 않아야 함).
- `meta.oracle`만 바뀐 파일은 그대로 남습니다(`oracle/README.md`).

## Rust 테스트

- `secs-project`: `.src.json` 쓰기·지우기·읽기, 키 순서와 바이트(업스트림 `json.dumps`와 같음), 다시 잰 측정이 옛 파일을 지움(업스트림 `test_session_keeps_source_beside_wav_and_drops_stale`), `load_project_sources`.
- `secs-service`: 위 골든 사례(mdat 가져오기의 `.src.json`, 곁 mdat 바이트), REW 가짜로 잰 측정이 캘 파일 환경을 남김(`Sens Factor` 오류 사례는 측정이 남고 환경이 없음), 보정 전후 내보내기가 환경을 넣은 바이트가 `secs-export`를 직접 부른 것과 같음, 다시 연 프로젝트에서도 같음(`exports_after_reopening_match_pre_close_bytes`).
- CLI: 결정적 모드에서 두 번 돌린 `run`의 출력 바이트가 같음(스레드 1과 기본).
- `features.toml`: `measure.mdat_import`, `export.before_after_mdat`, `project.measurement_store`·`service.session`(곁 mdat)의 `tests`에 새 테스트를 더하고, 새 항목 `project.measurement_source`(`.src.json`, upstream `rewapi/session.py`)를 둡니다.

## 고쳐도 되는 파일

`crates/secs-project/**`, `crates/secs-service/**`, `crates/secs-rew/**`, `apps/secs2r-cli/**`, `crates/secs-mdat/**`·`crates/secs-export/**`(P49 표면에 꼭 필요한 보탬만, 보고), `Cargo.toml`·`Cargo.lock`(의존 추가만), `oracle/export_service_mdat.py`, `oracle/export_project.py`, `oracle/goldens/{service_mdat,project}/**`, `oracle/README.md`(가족 표), `features.toml`, `docs/DEVIATIONS.md`(위 두 줄).

건드리지 않음: `apps/secs2r/**`(화면), `crates/secs-types/**`(IPC 꼴이 바뀌어야 하면 멈추고 보고), `.github/**`, `src/`, `tests/`.

## 검증 (모두 포그라운드로, 보고를 마친 뒤에만 턴을 끝냅니다)

```sh
cd /home/user/wt/p50
export CARGO_TARGET_DIR=/home/user/wt/target-p50 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
time ~/.venvs/secs-oracle/bin/python oracle/run.py service_mdat project
git status --short oracle/goldens | head -40
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --exclude secs2r --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test -p secs-mdat -p secs-export -p secs-project -p secs-rew -p secs-service -p secs2r-cli -p secs-policy --locked 2>&1 | grep -E "test result|FAILED|panicked"
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
cargo build --release --locked -p secs2r-cli
B=$CARGO_TARGET_DIR/release
for t in 1 4; do rm -rf /tmp/p50-run; RAYON_NUM_THREADS=$t $B/secs2r-cli --quiet --deterministic-uuids run --fill mock --out /tmp/p50-run; mv /tmp/p50-run /tmp/p50-sample-$t; done
python3 .github/scripts/integrity.py compare /tmp/p50-sample-1/export /tmp/p50-sample-4/export
find /tmp/p50-sample-1 -name "*.src.json" | head
```

## 보고 형식

1. 바꾼 파일과 각 한 줄 설명
2. 1–6번 각각: 한 일과 정한 규칙(`.src.json` 읽기의 관대한 규칙, REW 시계, 곁 mdat의 SPL 오프셋)
3. 골든: 새 사례와 바뀐 사례, 크기, 내보내기 시간
4. 더한 편차 두 줄의 문장
5. 검증 명령마다 마지막 몇 줄의 출력
