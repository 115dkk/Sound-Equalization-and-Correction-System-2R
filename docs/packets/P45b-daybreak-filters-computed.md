# P45b (Daybreak): 필터 내보내기 골든에 계산한 실행과 업스트림 감싸개를 더하기

P45(`docs/packets/P45-daybreak-filters.md`)가 `crates/secs-filters`와 `oracle/export_filters.py`(가족 `filters`)를 만들었습니다. 일곱 사례 모두 바이트가 같습니다. 그런데 사례가 모두 손으로 만든 덕 객체이고, 업스트림 감싸개 `calibration.export_filter_text`·`export_eqapo`를 부르지 않고 그 제목·정보 줄·note 문자열을 내보내기 스크립트가 다시 써서 만듭니다. 그래서 Rust `export_filter_text`·`export_eqapo`의 문자열이 업스트림 감싸개와 같은지는 검사되지 않고, 메인 11개 · 서브 3개 · LFE · 긴 FIR 같은 실제 사슬도 다루지 않습니다. 이 둘을 채웁니다.

## 작업 위치

- 워크트리 `/home/user/wt/p45`(브랜치 `p45`)에서 P45의 결과 위에 이어 일합니다. 다른 경로는 읽기만 합니다.
- 커밋하지 않습니다. git으로 인덱스나 작업 트리를 바꾸지 않습니다. 하위 에이전트를 띄우지 않습니다.
- cargo: `CARGO_TARGET_DIR=/home/user/wt/target-p45 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`. 오라클: `~/.venvs/secs-oracle/bin/python oracle/run.py filters`.
- 먼저 읽을 것: P45 작업서, `oracle/export_filters.py`, `crates/secs-filters/src/lib.rs`·`tests/golden.rs`, `oracle/export_camilla.py`(계산한 실행에서 사슬을 뽑는 `chain_from`·`fill_firs`, `COMPUTED`), `oracle/engine_cases.py`, 업스트림 `src/secs/calibration.py`의 `rc_pieces`·`export_filter_text`·`export_eqapo`·`_filter_plan`, `tests/test_export_filters.py`의 `rc_run`(서브를 걷어 낸 RC 전용 실행을 만드는 법).

## 할 일

### 1. 계산한 실행 (`oracle/export_filters.py`)

`calibration.compute`로 공유 사례를 돌려 **업스트림 감싸개를 그대로** 부릅니다(`export_filters.stamp`만 고정 문자열로 바꿈).

- `dolby`(mixed, 메인 11 · 서브 3 · LFE): `cal.export_eqapo(run, tmp)`
- `dolby_infra`(서브 고정 대역 필터): `cal.export_eqapo`
- `dolby_ultra`(긴 FIR, 초고지연 이름): `cal.export_eqapo`
- `room`에서 서브를 걷어 낸 RC 전용 실행 두 개, `mixed`와 `zero`(FIR 없음): 각각 `cal.export_filter_text`와 `cal.export_eqapo`
- 위 실행 하나에서 범위 오류: 서브가 있는 실행의 `cal.export_filter_text`가 `CalibrationError`를 내고 폴더를 만들지 않음을 기록

기록할 것: 사슬(`export_camilla.chain_from`·`fill_firs`와 같은 꼴, `SignalChain` 필드 이름 — 모듈을 import해 같은 함수를 씁니다), `FilterRun`에 들어갈 값(`run.settings.mode`, `run.settings.ultra_ms`, `run.settings.label()`, `run.latency_ms`, `run.rc_hi`, `cal.rc_pieces(run)`, stamp), 쓴 파일의 상대 경로 목록과 순서, 텍스트 파일(txt)의 바이트(blob), WAV는 바이트 대신 길이와 SHA-256(사슬의 FIR 사이드카에서 다시 만들 수 있으므로). 계산 시간이 너무 길면 사례를 줄이되 `dolby`와 RC 전용 `mixed`는 남깁니다. 가족 전체 6 MB 이하, 내보내기 10분 이하를 지키고 걸린 시간을 보고합니다.

### 2. Rust 테스트 (`crates/secs-filters/tests/golden.rs`)

계산한 사례마다 사슬과 `FilterRun` 값을 읽어 `export_filter_text`·`export_eqapo`를 임시 폴더에 부르고, 쓴 파일 목록·순서가 같고 텍스트 파일 바이트가 같으며 WAV의 길이와 SHA-256이 같은지 봅니다. 범위 오류 사례는 `FiltersError::Scope`이고 폴더가 생기지 않습니다. Preamp 줄(머리 여유)이 오라클과 같지 않으면 그 사례의 머리 여유 값 차이를 `MEASURE` 줄로 보고합니다(0.5 dB 올림 경계에 걸린 경우만 다를 수 있음).

`features.toml`의 `export.filter_text`·`export.eqapo` 항목 `tests`에 새 테스트를 더합니다.

## 고쳐도 되는 파일

`oracle/export_filters.py`, `oracle/goldens/filters/**`, `oracle/README.md`(가족 표의 그 줄), `crates/secs-filters/**`, `features.toml`(두 항목). 그 밖은 읽기만 합니다. 업스트림 감싸개와 Rust의 문자열이 다르면 Rust를 고칩니다.

## 검증 (모두 포그라운드로, 보고를 마친 뒤에만 턴을 끝냅니다)

```sh
cd /home/user/wt/p45
export CARGO_TARGET_DIR=/home/user/wt/target-p45 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
time ~/.venvs/secs-oracle/bin/python oracle/run.py filters
du -sh oracle/goldens/filters
cargo fmt --all -- --check
cargo clippy -p secs-filters --all-targets --locked -- -D warnings
cargo test -p secs-filters -p secs-camilla -p secs-oracle --locked 2>&1 | grep -E "test result|FAILED|panicked|MEASURE"
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## 보고 형식

1. 바꾼 파일과 각 한 줄 설명
2. 계산한 사례 목록, 크기, 내보내기 시간
3. 업스트림 감싸개와 Rust가 달랐던 자리와 고친 방법(없으면 없음)
4. 검증 명령마다 마지막 몇 줄의 출력
