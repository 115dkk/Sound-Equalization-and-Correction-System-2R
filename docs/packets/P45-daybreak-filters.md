# P45 (Daybreak): 필터 텍스트와 Equalizer APO 내보내기 (`secs-filters`)

업스트림 SECS 2 Beta 1.3.0(이슈 #35)이 더한 필터 내보내기를 옮깁니다. 서비스·IPC 연결은 다음 작업서(P46)이고, 이 작업서는 크레이트와 골든까지입니다.

## 작업 위치

- 워크트리 `/home/user/wt/p45`(브랜치 `p45`)에서만 일합니다. 다른 경로는 읽기만 합니다.
- 커밋하지 않습니다. git으로 인덱스나 작업 트리를 바꾸지 않습니다(add, stash, checkout, reset 금지). 하위 에이전트를 띄우지 않습니다.
- cargo는 `CARGO_TARGET_DIR=/home/user/wt/target-p45 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`로 돌립니다.
- 오라클 venv: `~/.venvs/secs-oracle/bin/python`(Python 3.13, `oracle/requirements.txt`의 판). 내보내기는 반드시 `oracle/run.py`를 거칩니다.

## 먼저 읽을 것

- 업스트림(이 저장소의 `src/`가 오라클이고 1.3.0과 같습니다): `src/secs/export_filters.py` 전체, `src/secs/calibration.py`의 `FILTER_EXPORT_SCOPE`부터 `export_eqapo`까지(1392–1448줄 근처), `tests/test_export_filters.py`, 그리고 함께 쓰는 `src/secs/export_camilladsp.py`의 `write_wav`·`fir_to_48k`·`mimo_paths`·`has_lfe`·`LFE_*`.
- 우리 쪽: `crates/secs-types/src/chain.rs`(`SignalChain`, `MimoPaths`, `MimoFilter` — 입력은 이것뿐), `crates/secs-types/src/ipc.rs`의 `filter_export_block`·`ExportBlock`(범위 규칙, 이미 있음), `crates/secs-camilla/src/lib.rs`와 `pyfmt.rs`(같은 방식으로 옮긴 카밀라 내보내기 — 파이썬 서식, wav, 파일 이름, 원자적 쓰기), `crates/secs-dsp/src/response.rs`(`peaking_response`, `band_filter`, `allpass2`, `lr4`)와 `crates/secs-dsp`의 FFT, `docs/packets/P11-daybreak-camilla.md`(같은 꼴의 이전 작업서), `oracle/README.md`, `oracle/export_camilla.py`, `oracle/engine_cases.py`.

## 할 일

`export_filters.py`를 통째로(MIMO 갈래 포함) 옮기고, `calibration.export_filter_text`·`export_eqapo`의 감싸개를 옮깁니다. 같은 입력에서 업스트림과 **바이트 단위로 같은 파일**을 씁니다.

### 크레이트

`crates/secs-filters`를 새로 만듭니다. 루트에 `#![forbid(unsafe_code)]`, `Cargo.toml`에 `[lints] workspace = true`와 다른 크레이트와 같은 `*.workspace = true` 필드, 설명 한 줄("Filter-value text and Equalizer APO configuration writers, reading the engine's signal-chain value."). 의존: `secs-types`, `secs-dsp`, `secs-camilla`, `thiserror`. 루트 `Cargo.toml`의 `[workspace.dependencies]`에 다른 크레이트처럼 올립니다. `secs-engine`·`secs-project`·`secs-service`에는 의존하지 않습니다.

`secs-camilla`에서 함께 쓸 것(wav 바이트, 파일 이름 정리, 원자적 쓰기, LFE 상수, 파이썬 서식)은 공개 함수로 꺼내 씁니다. 카밀라의 동작과 골든은 그대로여야 합니다. 서식이 모자라면 `pyfmt`에 더하고 같은 방식으로 오라클 대조 테스트를 붙입니다(쓰는 서식: `:9.2f`, `:7.3f`, `:+.2f`, `:.2f`, `:.3f`, `:.4f`, `:.6f`, `:.1f`, `:g`, `:<3d`, `:2d`, `:4d`, `:6s`, `:>7s`, `:,`).

### 공개 표면

이름과 모양은 아래를 따르고, 바꿔야 하면 보고서에 이유를 적습니다.

```rust
/// 업스트림 `calibration.py`의 `CalibrationRun`·`MimoRun` 가운데 필터 내보내기가 읽는 값.
pub struct FilterRun<'a> {
    pub chain: &'a SignalChain,
    /// MIMO를 내보내기에 넣을 때만 Some(업스트림 `mimo_run is not None`). 경로가 빈 값은 넘기지 않습니다.
    pub mimo: Option<&'a MimoPaths>,
    /// `settings.mode` ("zero" | "mixed" | "linear" | "ultra").
    pub mode: &'a str,
    pub ultra_ms: f64,
    /// `settings.label()` (`secs_types::calibration::mode_label`).
    pub label: &'a str,
    pub latency_ms: f64,
    pub rc_hi_hz: f64,
    /// `calibration.rc_pieces(run)`; 서비스가 `secs_export::rc_pieces`로 만들어 넘깁니다.
    pub rc_pieces: &'a [String],
    /// `export_filters.stamp()`의 값(`%Y-%m-%d %H:%M`); 부르는 쪽의 시계로 만듭니다.
    pub stamp: &'a str,
}

pub fn export_filter_text(run: &FilterRun<'_>, folder: &Path) -> Result<Vec<PathBuf>, FiltersError>;
pub fn export_eqapo(run: &FilterRun<'_>, folder: &Path) -> Result<Vec<PathBuf>, FiltersError>;

#[derive(Debug, thiserror::Error)]
pub enum FiltersError {
    /// `filter_export_block`이 막은 경우(아무것도 쓰지 않음).
    #[error(...)] Scope(secs_types::ipc::ExportBlock),
    /// 값이 유한하지 않거나 이름이 ASCII가 아닌 경우 등 쓰기 전 검사.
    #[error(...)] Invalid(String),
    #[error(...)] Io(#[from] std::io::Error),
}
```

모듈 함수도 업스트림 이름대로 공개합니다: `make_plan`, `plan_responses`, `headroom_db`, `windows_channels`, `windows_name`, `eqapo_filters`, `eqapo_config`, `eqapo_guide`, `write_eqapo`, `filter_text`, `write_text`, `SECTION_Q`, `EQAPO_GUIDE`. 필터 한 칸은 업스트림의 튜플 대신 열거형(`Op`)으로 두되 같은 정보를 갖고, `fixed` 표시(고정 서브 필터)를 따로 둡니다. `FilterPlan`·`SubFeed`·`MimoFeed`는 업스트림 순서를 지키도록 `Vec`으로 둡니다(dict 삽입 순서).

### 업스트림에서 꼭 지킬 것

- 범위 검사(`filter_export_block`)는 무엇이든 쓰기 전에 합니다. 막히면 폴더도 만들지 않습니다(`tests/test_export_filters.py`의 `not (tmp_path / 't').exists()`). 판정은 `secs_types::ipc::filter_export_block(kind, !chain.subs.is_empty(), run.mimo.is_some())`입니다.
- 이름의 모드: `mode + f'{ultra_ms:g}'`(초고지연일 때). 텍스트는 `SECS 필터 {label}.txt`(UTF-8 BOM) + `secs_{mode}_` 접두의 FIR wav, 제목 `SECS 2 보정 필터 — {label}`, 정보 두 줄은 `export_filter_text`의 문자열 그대로입니다. EqAPO는 `config.txt`와 하위 폴더의 필터 본체(ASCII), 안내(`EqAPO 적용 방법.txt`, UTF-8 BOM), Include 경로의 역슬래시, `note`의 ASCII 아닌 글자 버림, Preamp = `ceil(headroom × 2) / 2`.
- `make_plan`: 입력 FIR이 `[1.0]`(길이 1)이면 넣지 않음, XO가 있는 메인만 센드, LFE는 서브가 있을 때만(`lfe_gain_db` 기본 10, LP 120 Hz LR24), 서브 버스 PEQ → 고정 필터(HP·LP·PEQ, `fixed`) → 올패스 → 딜레이. MIMO 갈래: FIR이면 지연 → HP(하한, LR24) → FIR(`mimo_{입력}_{서포트}`), IIR이면 게인 → (극성 반전) → 지연(경로 지연 + 서포트 지연) → PEQ → 올패스 → HP(하한) → LP(Fsiso), 그 뒤에 서포트 스피커의 고정 필터. 우리 `MimoPath`의 `delay_ms`·`low_hz`·`MimoFilter::Fir.taps_48k`가 업스트림 `mimo_paths`의 `d_ms`·`low`·`fir_to_48k(...)`입니다.
- `headroom_db`: `n = max(2^16, 다음 2의 거듭제곱 ≥ 가장 긴 FIR)`, `f = rfftfreq(n, 1/48000)`, 20 Hz–20 kHz, LFE 입력은 뺌, 출력마다 갈래를 더한 뒤 최댓값. FIR 응답은 `rfft(taps, 2·(len(f)−1))`. PEQ는 RBJ 디지털(48 kHz), HP·LP·올패스는 아날로그 원형(`secs-dsp`의 같은 함수).
- `windows_channels`·`windows_name`의 번호 규칙(첫 서브와 LFE 입력은 LFE 자리, 자리가 없는 출력은 맨 뒤 번호를 차례로).
- 텍스트의 `_db`(반올림 뒤 0이면 `+0.00`), 딜레이 샘플 수 `int(round(ms × 48000 / 1000))`(반올림은 짝수 쪽), FIR 탭 수의 천 단위 쉼표.
- 쓴 파일 목록의 순서(텍스트는 텍스트 파일 먼저, EqAPO는 `config.txt`, 필터 본체, 안내, wav 순서).
- 파일 쓰기는 `secs-camilla`와 같은 방식(쓰기 전에 모든 검사, 원자적 쓰기, 같은 이름이 있으면 바꿈)을 따릅니다.

### 골든 (`oracle/export_filters.py`, 가족 `filters`)

- 계산한 실행: 카밀라 골든과 같은 공유 사례(`export_camilla.py`의 `COMPUTED`)를 `calibration.compute`로 돌리거나, 시간이 너무 들면 카밀라처럼 덕 타이핑한 `eng`·`res`·`run`을 써도 됩니다(어느 쪽인지 보고). 서브가 있는 실행마다 `cal.export_eqapo`, 서브를 뺀 RC 전용 실행(업스트림 테스트의 `rc_run`처럼 공유 사례에서 서브를 걷어 낸 것; `mixed`와 FIR 없는 `zero` 둘)마다 `cal.export_filter_text`와 `cal.export_eqapo`를 부릅니다. 업스트림 `stamp`는 고정 문자열로 바꿔 두고 그 값을 기록합니다.
- 기록: 체인(`SignalChain` 필드 이름, 카밀라의 `chain_from`과 같게), `FilterRun`에 들어갈 값(모드, 초고지연 ms, 라벨, 지연, RC 상한, `rc_pieces`, stamp), 쓴 파일의 상대 경로 목록과 순서, 파일마다 바이트(blob), 계획(칸의 종류와 값), `headroom_db`, `windows_channels`.
- 직접 부른 경우(덕 타이핑): 업스트림 테스트처럼 손으로 만든 IIR MIMO 갈래(`IirSupport`), FIR MIMO 갈래(짧은 저속 FIR + `fs_low`), XO 없는 서브(소스 없음 → `0.0`), LFE 없음(`lfe_gain_db=None`), 극성 반전, 0 ms 딜레이(줄 없음), 음의 0과 반올림 경계의 게인·PEQ, LR12·LR48·BW12·BW24 고정 필터, 자리가 없는 역할(Tml, 하이트, 서브 셋 이상)의 번호, ASCII 아닌 note. 각 경우에 `eqapo_filters`·`eqapo_config`·`eqapo_guide`·`filter_text`의 문자열과 `plan_responses`를 기록합니다.
- `plan_responses`: 몇 사례에서 입력마다, 출력마다 `n = 2^16` 격자의 표본(예: 0–2000 Hz 전부와 그 위 127번째 칸마다)을 기록합니다.
- 크기 6 MB 이하. `oracle/README.md`의 가족 표에 한 줄을 더합니다.

### 테스트 (`crates/secs-filters/tests/golden.rs` 등)

- 파일 바이트가 같음(텍스트, config, 필터 본체, 안내, wav), 쓴 파일 목록과 순서가 같음, 계획이 같음(정수·문자열·종류는 정확히, 값은 체인에서 그대로 오므로 비트 단위로).
- `plan_responses`와 `headroom_db`는 측정한 오차의 1.5배 예산(`MEASURE` 줄을 찍음, `oracle/README.md`의 규칙), Preamp 줄은 정확히 같음.
- 범위: 서브가 있으면 텍스트가 `Scope(Subs)`, MIMO가 있으면 텍스트와 EqAPO가 `Scope(Mimo)`이고 폴더가 생기지 않음. 쓰기 전 검사 실패(유한하지 않은 게인 등)도 아무것도 쓰지 않음.
- `features.toml`에 두 항목을 더합니다. `kind = "export"`, `milestone = "M5"`, `status = "implemented"`, `upstream`은 `"calibration.py export_filter_text; export_filters.py"`와 `"calibration.py export_eqapo; export_filters.py"`, `tests`는 실제 테스트 이름(`secs-filters::…`). id는 `export.filter_text`와 `export.eqapo`.

## 고쳐도 되는 파일

- `crates/secs-filters/**`(새로)
- `crates/secs-camilla/src/**`(공유할 도우미를 공개로, `pyfmt` 서식 추가와 그 테스트만; 동작 변화 없음)
- 루트 `Cargo.toml`(작업 공간 의존), `Cargo.lock`(cargo가 쓰는 대로만)
- `oracle/export_filters.py`(새로), `oracle/goldens/filters/**`, `oracle/README.md`(가족 표 한 줄)
- `features.toml`(위 두 항목)

그 밖(`crates/secs-types`, `crates/secs-service`, `apps/`, `docs/`, `.github/`, `src/`, `tests/`)은 읽기만 합니다. `SignalChain`에 내보내기에 필요한 값이 없으면 고치지 말고 보고합니다.

## 검증 (모두 포그라운드로, 보고를 마친 뒤에만 턴을 끝냅니다)

```sh
cd /home/user/wt/p45
export CARGO_TARGET_DIR=/home/user/wt/target-p45 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
~/.venvs/secs-oracle/bin/python oracle/run.py filters
du -sh oracle/goldens/filters
cargo fmt --all -- --check
cargo clippy -p secs-filters -p secs-camilla --all-targets --locked -- -D warnings
cargo clippy -p secs-filters -p secs-camilla --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
cargo test -p secs-filters -p secs-camilla -p secs-policy -p secs-oracle --locked 2>&1 | grep -E "test result|FAILED|panicked|MEASURE"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p secs-filters
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

확신 없는 API(secs-dsp의 FFT 함수, pyfmt의 반올림)는 코드를 열어 확인하고 씁니다.

## 보고 형식

1. 바꾼 파일과 각 한 줄 설명
2. 공개 표면(위와 다르게 한 것과 이유)
3. 골든: 사례 목록, 계산했는지 덕 타이핑했는지, 크기, 내보내기 시간
4. 바이트가 같지 않았던 자리와 고친 방법, 새로 더한 파이썬 서식
5. 예산을 쓴 값의 `MEASURE` 줄
6. 검증 명령마다 마지막 몇 줄의 출력
