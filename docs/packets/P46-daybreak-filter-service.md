# P46 (Daybreak): 필터 텍스트와 Equalizer APO 내보내기를 서비스와 CLI에 잇기

P45가 `crates/secs-filters`(업스트림 `export_filters.py`와 `calibration.export_filter_text`·`export_eqapo`)를 옮겼습니다. 이 작업서는 그것을 서비스의 `export` 잡과 CLI에 잇습니다(이슈 #35, 업스트림 SECS 2 Beta 1.3.0).

## 작업 위치

- 워크트리 `/home/user/wt/p46`(브랜치 `p46`)에서만 일합니다. 다른 경로는 읽기만 합니다.
- 커밋하지 않습니다. git으로 인덱스나 작업 트리를 바꾸지 않습니다. 하위 에이전트를 띄우지 않습니다.
- cargo는 `CARGO_TARGET_DIR=/home/user/wt/target-p45 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`로 돌립니다(P45의 빌드를 이어 씁니다).

## 먼저 읽을 것

- `crates/secs-filters/src/`(공개 표면 `FilterRun`, `export_filter_text`, `export_eqapo`, `FiltersError`)
- `crates/secs-service/src/export.rs`(지금 두 종류를 `not_implemented`로 돌려주는 자리와 카밀라·mdat의 살아 있는 결과·캐시 경로), `export_folder.rs`, `cache.rs`, `curves.rs`의 `result_lines`(RC 노트), `calibration.rs`의 노트 변환
- `crates/secs-types/src/ipc.rs`의 `ExportKind`, `ExportBlock`, `filter_export_block`, `ErrorCode::ExportScope`
- `crates/secs-export/src/lib.rs`의 `rc_pieces`
- `apps/secs2r-cli/src/main.rs`, `.github/workflows/ci.yml`의 `integrity` 잡, `.github/scripts/integrity.py`
- 업스트림 `src/secs/wizard/step8_mdat.py`의 `export`(폴더 이름 `SECS_{kind}_{what}_{mode}_{시각}`, 막힌 경우), `src/secs/calibration.py`의 `filter_export_block`·`export_filter_text`·`export_eqapo`

## 할 일

### 1. 서비스의 `export` 잡 (`crates/secs-service/src/export.rs`, `export_folder.rs`)

- `ExportKind::FilterText`·`EqualizerApo`의 `not_implemented`를 걷어 내고 잇습니다.
- **범위 검사**는 잡을 시작하기 전에 동기로 합니다(튜토리얼 검사와 같은 자리). 서브가 있음 = 내보낼 체인(살아 있는 결과 또는 캐시)의 `subs`가 비어 있지 않음, MIMO가 있음 = 이 내보내기에 MIMO 경로가 들어감(카밀라가 쓰는 것과 같은 판정: 살아 있는 결과의 `mimo`, 캐시만 있으면 `cached_mimo`). `filter_export_block`이 막으면 `IpcError::new(ErrorCode::ExportScope, …).with_param("reason", block.name())`를 돌려주고 잡도 폴더도 만들지 않습니다.
- **폴더**: 빈 `folder`면 업스트림처럼 `SECS_text_{what}_{mode}_{시각}`·`SECS_eqapo_{what}_{mode}_{시각}`(`what` = MIMO가 들면 `RC_BA_MIMO`, 아니면 `RC_BA`; `mode`는 설정의 모드 이름, 초고지연도 `ultra`). `export_folder.rs`의 `mdat_folder`처럼 공개 함수를 둡니다(`text_folder`, `eqapo_folder`). 파일은 그 폴더에 바로 씁니다(카밀라처럼 하위 폴더를 두지 않음).
- **`FilterRun`의 값**: 체인, MIMO 경로, 모드 이름, 초고지연 ms, 라벨(`mode_label`), 전체 지연, RC 상한, `rc_pieces`, `stamp`(서비스 시계 `(self.deps.clock)()`의 지역 시각을 `%Y-%m-%d %H:%M`로).
  - 살아 있는 결과: 카밀라와 같은 체인·라벨, `rc_pieces`는 `secs_export::rc_pieces`.
  - 캐시만 있을 때(다시 연 프로젝트): 설계를 다시 돌리지 않습니다. 체인과 라벨은 카밀라처럼 캐시에서, `rc_pieces`는 캐시된 요약(`CalibrationSummary.notes`)의 `rc_limit` 노트(hz, auto, schroeder_hz)와 프로젝트 설정의 방 부피로 같은 문장을 만듭니다. `secs_export::rc_pieces`의 문장 규칙을 값만 받는 함수로 나눠 두 경로가 같은 함수를 쓰게 합니다(`secs-export`의 동작과 골든은 그대로). 노트가 없는 옛 캐시면 RC 상한만으로 업스트림의 대체 문장(`RC 상한 {hz} Hz (자동)` / `(수동)`)을 씁니다.
- 취소 검사와 `job.begin_commit()`은 카밀라와 같은 순서로 둡니다. `secs-filters`의 오류는 `Scope` → `export_scope`, 그 밖 → `export_failed`.
- `ExportResult.files`는 `secs-filters`가 돌려준 순서, `folder`는 쓴 폴더.

### 2. CLI (`apps/secs2r-cli/src/main.rs`)

- `export --kind`에 `filter-text`·`equalizer-apo`를 더합니다.
- `run`은 지금처럼 모든 내보내기를 하되, 범위가 막는 종류는 부르지 않고 건너뛴다는 한 줄을 지금의 출력 규칙대로 남깁니다(서브가 있으면 텍스트, MIMO가 들면 둘 다). 폴더 이름은 지금 꼴(`export/filter-text`, `export/equalizer-apo`).
- `--deterministic-uuids`일 때 서비스 시계도 고정합니다(텍스트 파일에 만든 시각이 들어가므로, `ci.yml`의 `integrity` 잡이 스레드 수별 실행을 바이트로 비교할 때 분이 바뀌어도 같아야 함). 플래그 도움말을 그 뜻으로 고칩니다. 플래그 이름은 CI가 쓰므로 그대로 둡니다.

### 3. 테스트

- `crates/secs-service/tests/service.rs`: 두 종류의 서비스 내보내기가 같은 결과에서 `secs-filters`를 직접 부른 것과 바이트로 같음(지금 카밀라·mdat 비교와 같은 꼴, 기반 커밋에서 `unreachable!()`로 둔 자리를 채움). 서브가 있는 결과에서 텍스트가 `export_scope`(reason `subs`)이고 폴더가 생기지 않음, MIMO가 든 결과에서 둘 다 `export_scope`(reason `mimo`), 프로젝트를 다시 연 뒤(캐시 경로) 내보낸 바이트가 닫기 전과 같음(`exports_after_reopening_match_pre_close_bytes`에 종류를 더함; stamp는 결정적 시계로).
- `export_folder.rs`의 폴더 이름 테스트.
- CLI: `export --kind` 두 값이 파싱되고, `run`이 막힌 종류를 건너뜀(있는 CLI 테스트 꼴을 따름).
- `features.toml`: `ipc.export`와 `service.exports`의 `tests`에 새 테스트를 더하고, `note`가 있으면 두 종류를 적습니다.

## 고쳐도 되는 파일

`crates/secs-service/**`, `crates/secs-export/src/**`(rc_pieces 나누기만), `apps/secs2r-cli/**`, `features.toml`(위 항목), `Cargo.toml`·`Cargo.lock`(의존 추가만).

건드리지 않음: `crates/secs-filters/**`(고칠 일이 있으면 보고), `crates/secs-types/**`, `apps/secs2r/**`, `.github/**`, `docs/**`, `oracle/**`, `src/`, `tests/`.

## 검증 (모두 포그라운드로, 보고를 마친 뒤에만 턴을 끝냅니다)

```sh
cd /home/user/wt/p46
export CARGO_TARGET_DIR=/home/user/wt/target-p45 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --exclude secs2r --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test -p secs-service -p secs-export -p secs-filters -p secs2r-cli -p secs-policy --locked 2>&1 | grep -E "test result|FAILED|panicked"
python3 .github/scripts/check_feature_tests.py
# integrity와 같은 흐름 두 번(스레드 1과 기본)을 돌려 같은지 봅니다
cargo build --release --locked -p secs2r-cli
B=$CARGO_TARGET_DIR/release
for t in 1 4; do rm -rf /tmp/p46-run; RAYON_NUM_THREADS=$t $B/secs2r-cli --quiet --deterministic-uuids run --fill mock --out /tmp/p46-run; mv /tmp/p46-run/export /tmp/p46-sample-$t; done
python3 .github/scripts/integrity.py compare /tmp/p46-sample-1 /tmp/p46-sample-4
ls -R /tmp/p46-sample-1 | head -60
```

## 보고 형식

1. 바꾼 파일과 각 한 줄 설명
2. 1–3번 각각: 한 일과 정한 규칙(캐시 경로의 rc_pieces, 시계 고정)
3. CLI `run`의 출력에서 건너뛴 줄과 쓴 폴더 목록
4. 검증 명령마다 마지막 몇 줄의 출력
