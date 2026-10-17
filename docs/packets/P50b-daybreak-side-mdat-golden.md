# P50b (Daybreak): 곁 mdat과 `.src.json` 읽기를 업스트림 바이트로 맞추기

P50(`docs/packets/P50-daybreak-source-info-service.md`)이 원 측정 환경을 프로젝트와 서비스에 이었습니다. 두 가지가 업스트림과 비교되지 않은 채 남았습니다.

1. 저장할 때 쓰는 곁 mdat(`session.rs`의 `write_side_mdat`, 업스트림 `wizard/window.py`의 `_write_side_mdat`)은 제목·메모·순서를 업스트림대로 고쳤지만 골든이 없습니다.
2. `.src.json` 읽기(`secs-project`의 관대한 변환)는 파이썬 입력 사전만 골든에 있고, 파이썬 `apply`가 그 사전으로 만든 mdat 바이트와 비교되지 않았습니다. 예를 들어 numpy는 `np.asarray(['20', True, 30], dtype=float32)`를 `[20, 1, 30]`으로 바꾸는데, 지금 Rust는 문자열이 섞인 배열을 버립니다.

## 작업 위치

- 워크트리 `/home/user/wt/p50b`(브랜치 `p50b`)에서만 일합니다. 다른 경로는 읽기만 합니다.
- 커밋하지 않습니다. git으로 인덱스나 작업 트리를 바꾸지 않습니다. 하위 에이전트를 띄우지 않습니다.
- cargo: `CARGO_TARGET_DIR=/home/user/wt/target-p50b CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`. 다른 대상 폴더(특히 워크트리 안의 `target/`)를 만들지 않습니다. 디스크가 빠듯하므로 퍼징과 Miri는 돌리지 않습니다.
- 오라클: `~/.venvs/secs-oracle/bin/python oracle/run.py service_mdat`.

## 먼저 읽을 것

`oracle/export_service_mdat.py`(업스트림 함수 본문을 `ast`로 꺼내 셈으로 돌리는 방식, `file_*`·`source_cases`), `crates/secs-service/tests/mdat.rs`, `crates/secs-service/src/session.rs`(`write_side_mdat`), `crates/secs-project/src/record.rs`(`.src.json` 읽기와 변환), 업스트림 `src/secs/wizard/window.py`의 `_write_side_mdat`·`_spl_offsets`, `src/secs/rewapi/session.py`의 `IrSaver.load_source`·`load_project_sources`, `src/secs/rew_mdat/source_info.py`의 `apply`·`_Boxes.make`, `src/secs/rew_mdat/builder.py`.

## 할 일

### 1. 곁 mdat 골든

- `file_*` 사례마다(업스트림 `load_mdat`로 가져온 프로젝트와 `.irs`) 업스트림 `_write_side_mdat` 본문을 `ast`로 꺼내 셈(`self.project`, `self.step5.plan().roles` = 그 사례의 계획 역할, `self._spl_offsets()` = `.secs` 기록의 값; REW에 묻는 갈래는 측정기가 없어 타지 않음)으로 돌려 나온 mdat 바이트를 blob으로 남깁니다. UUID와 시각은 Rust 테스트의 결정적 서비스와 같은 값으로 고정합니다(`uuid.uuid4`를 패치, `time.time()` = `1704164645.0`). 측정이 없어 mdat을 쓰지 않는 사례는 없음으로 기록합니다.
- 위치가 여럿이고 계획에 없는 역할이 있는 사례가 없으면 하나 더합니다.
- Rust 테스트(`tests/mdat.rs`): 같은 mdat을 서비스로 가져오고 프로젝트를 저장해 생긴 `<이름>.mdat`의 바이트가 골든과 같음.

### 2. `.src.json` 읽기 골든

- 손으로 만든 `.src.json` 텍스트 여러 벌을 임시 폴더에 쓰고, 업스트림 `IrSaver.load_source`로 읽어 `build_mdat_bytes([MeasurementInput(..., source_info=info)])`(UUID·시각 고정)로 빌드한 바이트를 남깁니다. 파이썬이 예외를 내지 않는 모양만 둡니다: 정상, 키 빠짐, 문자열 칸에 숫자·불·null, 상자에 문자열 숫자(`"1.5"`, `"42"`), `Boolean`에 `0`·`"False"`(파이썬 `bool("False")`는 참), 알 수 없는 상자 종류, 배열에 불·정수·숫자 문자열, `inverseCSelected`에 정수, 날짜가 실수·불·음수·0, 메모·형식이 숫자·빈 문자열, 최상위가 목록(없음), 깨진 JSON(없음).
- 예외를 내는 모양(배열에 숫자가 아닌 문자열, `Integer`에 `"4.2"`, i32 밖의 `Integer`, NaN 날짜, `strings`가 목록)은 바이트 대신 파이썬 예외 종류를 기록합니다. Rust는 그 부분을 없는 것으로 보고 빌드를 계속합니다(F10). 이 사례의 Rust 테스트는 빌드가 성공함만 봅니다.
- Rust 테스트: 같은 텍스트를 `.src.json` 읽기로 읽어 빌드한 바이트가 골든과 같음. 다르면 `secs-project`의 변환을 파이썬에 맞춥니다(numpy의 `float32` 변환과 파이썬 `str()`·`bool()`·`int()`·`float()`의 결과).
- P50이 넣은 `source_cases`의 `partial`처럼 파이썬이 예외를 낼 모양(`'gainArray': 'bad'`)을 정상 사례로 다루는 곳이 있으면 고칩니다.

### 3. 등록

`features.toml`의 `service.session`과 `project.measurement_source`의 `tests`에 새 테스트를 더합니다. `docs/DEVIATIONS.md`의 F10 문장이 고친 규칙과 맞는지 봅니다.

## 고쳐도 되는 파일

`oracle/export_service_mdat.py`, `oracle/goldens/service_mdat/**`, `crates/secs-project/**`, `crates/secs-service/**`, `features.toml`(위 두 항목), `docs/DEVIATIONS.md`(F10 줄).

## 검증 (모두 포그라운드로, 보고를 마친 뒤에만 턴을 끝냅니다)

```sh
cd /home/user/wt/p50b
export CARGO_TARGET_DIR=/home/user/wt/target-p50b CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
time ~/.venvs/secs-oracle/bin/python oracle/run.py service_mdat
du -sh oracle/goldens/service_mdat
cargo fmt --all -- --check
cargo clippy -p secs-project -p secs-service --all-targets --locked -- -D warnings
cargo test -p secs-project -p secs-service --locked 2>&1 | grep -E "test result|FAILED|panicked"
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
df -h /home/user | tail -1
```

끝나면 `/home/user/wt/target-p50b`를 지웁니다.

## 보고 형식

1. 바꾼 파일과 각 한 줄 설명
2. 곁 mdat 사례 목록과 셈의 내용
3. `.src.json` 사례 목록(바이트 / 예외), 파이썬과 달랐던 변환 규칙과 고친 방법
4. 검증 명령마다 마지막 몇 줄의 출력
