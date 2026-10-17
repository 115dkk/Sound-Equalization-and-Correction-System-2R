# P49 (Daybreak): mdat의 원 측정 환경 — 꺼내기, 넣기, 빌더, 보정 전후 내보내기

업스트림 SECS 2 Beta 1.3.1(커밋 `587159e`, PR #38)은 mdat 빌더가 템플릿 측정을 복제할 때 템플릿 제작자의 측정 환경(마이크 캘 곡선, 캘 파일 경로, 오디오 장치 이름)이 남의 파일에 그대로 들어가던 버그를 고쳤습니다. 측정마다 그 환경을 먼저 비우고, 원 측정의 환경을 알면 그 값을 넣습니다. 템플릿 파일(`src/secs/rew_mdat/template_1.mdat`)도 비운 판으로 바뀌었습니다. 이 작업서는 형식 쪽(`secs-mdat`)과 내보내기 쪽(`secs-export`)을 옮깁니다. 프로젝트 저장(`.src.json`)과 서비스 연결은 다음 작업서(P50)가 합니다.

## 작업 위치

- 워크트리 `/home/user/wt/p49`(브랜치 `p49`)에서만 일합니다. 다른 경로는 읽기만 합니다.
- 커밋하지 않습니다. git으로 인덱스나 작업 트리를 바꾸지 않습니다. 하위 에이전트를 띄우지 않습니다.
- cargo: `CARGO_TARGET_DIR=/home/user/wt/target-p49 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`. 오라클: `~/.venvs/secs-oracle/bin/python oracle/run.py <가족>`.
- 업스트림 파이썬 트리(`src/`, `tests/`)는 고치지 않습니다(CI가 비교합니다).

## 먼저 읽을 것

- 업스트림 변경 전체: `git -C /home/user/wt/p49 show 587159e -- src tests`. 특히 `src/secs/rew_mdat/source_info.py`(새 파일), `rew_mdat/builder.py`의 `MeasurementInput`·`_replace_measurement`·`build_mdat_bytes`, `mdat_loader.py`의 `load_mdat_project`, `calibration.py`의 `before_after_items`·`export_before_after_mdat`, `tests/test_mdat_source_info.py`.
- 이식판: `crates/secs-mdat/src/{builder.rs,ir.rs,adapter.rs,models.rs,lib.rs}`, `crates/secs-jos/src/{nodes.rs,writer.rs}`, `crates/secs-mdat/tests/`, `crates/secs-export/src/lib.rs`(`measured_item`, `before_after_items*`, `export_*mdat*`), `crates/secs-export/tests/golden.rs`.
- 오라클: `oracle/README.md`, `oracle/common.py`, `oracle/export_mdat.py`, `oracle/export_export_mdat.py`, `oracle/export_export_mimo.py`.
- `docs/DEVIATIONS.md`의 F4(UUID 주입), F8(서러게이트).

## 정한 설계

### `secs_mdat::source_info` (새 모듈, 업스트림 `rew_mdat/source_info.py`)

원 측정 환경은 형식 있는 값으로 다룹니다. 업스트림의 사전 키 순서가 `.src.json`의 바이트이므로(P50이 씀) 순서를 지킵니다.

```rust
/// 업스트림 `extract()`가 만드는 사전. 키 순서: strings, boxed, cal, date, notes, format.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceInfo {
    pub strings: Vec<(String, String)>,        // STRING_FIELDS 중 있는 것, 넣은 순서
    pub boxed: Vec<(String, Boxed)>,           // BOXED_FIELDS 중 있는 것
    pub cal: Vec<(String, CalInfo)>,           // "meterCal", "scCal"
    pub date: Option<SourceDate>,              // 측정 시각(ms). extract는 정수만, JSON은 실수도 올 수 있음
    pub notes: Option<String>,
    pub format: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SourceDate { Int(i64), Float(f64) }
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Boxed { Double(f64), Integer(i64), Boolean(bool) }   // java.lang.Double / Integer / Boolean
#[derive(Clone, Debug, PartialEq)]
pub struct CalInfo {
    pub inverse_c_selected: bool,
    pub source_name: String,
    pub source_path: String,
    pub freq: Option<Vec<f32>>,
    pub gain: Option<Vec<f32>>,
    pub phase: Option<Vec<f32>>,
    pub boxed: Vec<(String, Boxed)>,           // CAL_BOXED 중 있는 것
}

pub fn extract(stream: &secs_jos::Stream, measurement: NodeId, notes: &str) -> SourceInfo;
pub fn apply(stream: &mut Stream, measurement: NodeId, info: Option<&SourceInfo>, boxes: &mut Boxes) -> Result<(), MdatError>;
pub fn scrub(stream: &mut Stream, measurement: NodeId) -> Result<(), MdatError>;
pub struct Boxes { … }                          // 업스트림 `_Boxes` + float[] 서술자(`boxes_for`)
pub fn boxes_for(stream: &Stream, prototype: NodeId) -> Boxes;
pub fn from_rew_measurement(cal: Option<&CalFile>, output_device: &str, input_device: &str,
                            output_channel: &str, now_ms: i64) -> Result<Option<SourceInfo>, MdatError>;
pub struct CalFile { pub path: String, pub text: Option<Vec<u8>> }   // text None = 파일을 못 읽음(OSError)
```

이름과 세부 시그니처는 필요하면 바꿔도 되지만, 바꾸면 보고합니다. 지켜야 할 것:

- **`extract`**: 업스트림 `extract(fields)`와 같은 값. `mdat_loader`는 측정의 원래 필드에 `measNotes`를 측정 메모(`measurement.notes or ''`)로 바꿔 넣은 사전을 넘깁니다. `sourceFileDate`는 정수이고 0보다 클 때만, `notes`·`format`은 빈 문자열이 아닐 때만 들어갑니다. 캘 배열은 float32로 읽어 그대로 둡니다(파이썬 `tolist()`가 float32 값을 그대로 옮김). 상자 값의 `Double`/`Integer`/`Boolean` 구분을 지킵니다.
- **`scrub`·`apply`**: 업스트림과 같은 필드를 같은 값으로 바꿉니다. 템플릿에 없는 필드는 건드리지 않습니다(`_put`). `apply`는 먼저 그 측정에서 상자·float[] 서술자를 더 배우고(먼저 배운 것이 이김, `setdefault`), 비운 뒤, `info`가 있으면 넣습니다. 비울 때 `usesEmbeddedCalData`는 같은 클래스 서술자의 새 `Boolean(false)` 객체이고, 개인 값(`inputVolume`, `outputVolume`, `micSensitivity`)은 null입니다.
- **상자 객체 공장**(`_Boxes.make`): 같은 종류는 클래스 서술자 하나를 함께 씁니다. 템플릿에서 배운 서술자가 없으면 표준 JDK `serialVersionUID`로 새로 만들고, `Double`·`Integer`는 `java.lang.Number`(배운 것, 없으면 새로) 아래에 둡니다. 서술자와 float[] 서술자는 **복제하기 전의 원본 프로토타입**에서 모읍니다(업스트림 `build_mdat_bytes`가 `boxes_for([prototype])`를 복제 전에 부름). 그래서 새 객체가 가리키는 서술자는 복제본의 서술자와 다른 노드일 수 있고, 쓰기의 핸들 순서가 그것에 따라 정해집니다. 바이트 골든으로 확인합니다.
- **`from_rew_measurement`**: SECS가 REW로 잰 측정의 환경(그때 REW에 걸린 마이크 캘 파일 + 장치 이름). 파이썬 그대로입니다.
  - 파일 텍스트는 `encoding='utf-8', errors='replace'`와 텍스트 모드의 줄바꿈 변환(`\r\n`·`\r` → `\n`)을 거친 줄입니다.
  - 줄마다 `Sens\s*Factor\s*=\s*([-+]?[0-9.]+)`(대소문자 무시)의 마지막 일치가 감도입니다. 그 값이 `float()`로 안 읽히면(`1.2.3`, `.`) 파이썬은 `ValueError`를 냅니다. 여기서는 `Err`로 돌려주고 골든에 그 사례를 둡니다(서비스가 어떻게 다룰지는 P50).
  - 행은 `,`·`;`를 공백으로 바꾼 뒤 파이썬 `str.split()`(파이썬의 공백 집합)으로 나눈 앞 세 칸을 파이썬 `float()` 문법으로 읽습니다. 밑줄(`1_0`), `inf`·`nan`·`infinity`(대소문자 무시), 앞뒤 부호를 파이썬처럼 받습니다. 하나라도 안 읽히면 그 줄은 건너뜁니다. 두 값 이상이고 첫 값이 0보다 크면 행이고, 모자란 칸은 0.0입니다.
  - `sourceName`은 `os.path.basename(cal_path)`입니다. 플랫폼마다 다르므로(posix는 `/`만, Windows는 `\`와 `/`) 그 플랫폼의 구분자 뒤 문자열을 씁니다(끝이 구분자면 빈 문자열). 골든은 Linux에서 만들어지니 posix 규칙을 검사하고, Windows 규칙은 단위 테스트로 둡니다.
  - `date`는 인자로 받은 `now_ms`입니다(업스트림 `int(time.time() * 1000)`).
  - 장치도 캘도 없으면 `None`입니다.

### 빌더 (`builder.rs`, 업스트림 `_replace_measurement`)

- `MeasurementInput`에 `pub source_info: Option<SourceInfo>`와 `pub computed: bool`을 더합니다.
- `build`에 `now_ms: i64` 인자를 더합니다. 업스트림은 측정마다 `time.time()`을 부르지만, 여기서는 한 파일에 한 값을 씁니다(F4처럼 시각을 주입). 앱은 실제 시각, 테스트와 골든은 고정값입니다. `docs/DEVIATIONS.md`의 F4 줄에 시각 주입을 한 문장으로 더합니다.
- 바꾸는 순서와 값은 업스트림과 같습니다: `apply`, 메모(계산값이 아니고 원 측정 메모가 있으면 `"\n\n[원 측정 메모]\n" + 원 메모`; 원 메모는 `str(...).strip()`), `shortDesc`, `measNotes`, `sampleRate`, `sourceFileDate`(계산값이거나 원 날짜가 없거나 0 이하이면 `now_ms`, 아니면 원 날짜를 정수로), `sourceFileFormat`(계산값이면 `SECS2 correction result (computed, not measured)`, 아니면 원 형식, 없으면 `SECS2 measurement`), 그 뒤는 지금과 같습니다.
- 할당 상한 계산(`allocation`)에 원 측정 환경의 크기(캘 배열 원소 × 4바이트 × 측정 수, 문자열 길이 × 3)를 더합니다. 신뢰할 수 없는 입력에서 상한이 지켜져야 합니다.

### mdat 불러오기 (`ir.rs`)

`MeasurementInfo`에 `pub source_info: SourceInfo`(업스트림 `MeasurementInfo.source_info`; `extract`는 언제나 사전을 돌려주므로 Option이 아님)를 더하고 `extract_ir`에서 채웁니다. `Serialize` 꼴이 IPC로 나가는 곳이 있으면 그 꼴을 바꾸지 않게 `#[serde(skip)]`을 둡니다(확인하고 보고).

### 보정 전후 내보내기 (`secs-export`)

- `pub type Sources = BTreeMap<u32, BTreeMap<String, SourceInfo>>;`(위치 → 역할 → 환경, 업스트림 `load_project_sources`의 꼴).
- `before_after_items*`와 `export_before_after_mdat*`에 `sources: Option<&Sources>`를 더합니다. 업스트림처럼 그 실행의 위치에서 역할의 환경을 찾아, 보정 전 항목은 `source_info`만, 보정 후 항목은 `source_info`와 `computed: true`를 넣습니다. MIMO 항목(`_3`)과 서브의 원 측정 항목은 업스트림이 넘기는 그대로입니다(서브 원 측정은 `measured`를 거치므로 환경이 들어가고, MIMO 항목은 넘기지 않음 — 코드로 확인).
- 단계별 mdat(`stage_items*`, `export_mdat*`)은 업스트림이 바꾸지 않았으므로 환경 없이 빌드합니다(비운 템플릿, `SECS2 measurement`, `now_ms`).
- mdat을 빌드하는 모든 공개 함수에 `now_ms: i64`를 더합니다.

### 부르는 쪽 (컴파일만 되게)

`secs-service`(`session.rs`의 곁 mdat, `export.rs`)와 `crates/secs-project/examples/project_perf.rs`, 테스트들이 새 인자로 컴파일되게만 고칩니다. 서비스는 `sources`에 `None`, `now_ms`에 `std::time::SystemTime`으로 잰 지금 시각(ms)을 넘기고 `// P50: 서비스 시계와 .src.json으로 바꿈` 주석을 답니다. CLI 무결성(스레드별 바이트 비교)이 같은 실행 안에서 시각을 다르게 받으면 깨지므로, `apps/secs2r-cli`의 `--deterministic-uuids`가 있는 경로는 확인만 하고 보고합니다(고치는 것은 P50).

## 골든

### `mdat` 가족 (`oracle/export_mdat.py`)

- 템플릿이 바뀌었으므로 기존 빌드 사례를 모두 다시 냅니다. `builder.time`을 고정 시각으로 바꿔 둡니다(예: `1792195538.802` → `now_ms = int(1792195538.802 * 1000)`, 입력에 `now_ms`를 기록).
- 새 사례(`source_info_*`):
  - 비운 템플릿에서 `extract`(측정 환경이 비어 있음 — 업스트림 `test_template_file_has_no_author_environment`).
  - `test_mdat_source_info.py`의 `INFO`처럼 만든 환경 두세 벌(캘 배열 있음/없음, `serialNum` 정수 상자 — 템플릿에 없는 `Integer` 서술자를 새로 만드는 경로, `scCal`, 문자열 일부만, `notes`·`format`·`date` 있음/없음/0)로 보정 전 + 계산값 측정을 빌드한 바이트(blob)와 그 결과를 다시 불러 `extract`한 값.
  - 빌드 → 불러오기(`load_mdat_project`) → 그 `source_info`로 다시 빌드한 바이트(왕복).
  - `from_rew_measurement`: 캘 파일 텍스트 여러 벌(업스트림 테스트의 것, CRLF·CR 줄바꿈, BOM, 잘못된 UTF-8 바이트, 탭·`;`·`,` 구분, 두 칸만 있는 줄, 첫 값 0·음수, `1_0`·`inf`·`nan`, 머리줄 없음, `Sens Factor = 1.2.3`(오류), 빈 파일, 파일 없음)와 장치 이름 조합. 시각은 고정, 경로는 posix 꼴.
- 크기는 가족 전체 6 MB 이하.

### `export_mdat`, `export_mimo` 가족

- 템플릿이 바뀌었으므로 다시 냅니다(시각 고정).
- `export_mdat`에 `sources`를 넘긴 보정 전후 사례를 한두 개 더합니다(케이스의 역할마다 서로 다른 환경, 한 역할은 환경 없음).

### 오라클 다시 내기 규칙

`oracle/README.md`대로 `meta.oracle`만 바뀐 파일은 그대로 남습니다. 가족 폴더에 쓰지 않은 파일이 남으면 내보내기가 실패하니 낡은 파일은 손으로 지웁니다.

## Rust 테스트

- `secs-mdat`: 위 골든 사례마다 `extract`·빌드 바이트·`from_rew_measurement`가 같음(빌드 바이트의 기존 규칙: 아홉 float32 캐시 배열은 1 ULP, 나머지는 정확히). 업스트림 `tests/test_mdat_source_info.py`의 다섯 테스트에 해당하는 단위 테스트. Windows `basename` 규칙 단위 테스트.
- `secs-mdat/tests/untrusted.rs`: 큰 캘 배열·긴 문자열을 단 환경이 할당 상한을 넘으면 `Err`.
- `secs-export`: 새 사례와 기존 사례의 항목·바이트.
- `features.toml`: `format.mdat_build`의 `tests`·`note`에 원 측정 환경을 더하고, 새 항목 `format.mdat_source_info`(kind `format`, milestone `M5`, upstream `rew_mdat/source_info.py`, tests는 실제 테스트 이름)를 둡니다. `export.before_after_mdat`의 `tests`에 sources 사례 테스트를 더합니다.

## 고쳐도 되는 파일

`crates/secs-mdat/**`, `crates/secs-jos/**`(쓰기에 꼭 필요할 때만, 보고), `crates/secs-export/**`, 부르는 쪽 컴파일 수정(`crates/secs-service/src/**`, `crates/secs-service/tests/**`, `crates/secs-project/examples/**`, `apps/secs2r-cli/**` — 인자 추가만), `oracle/export_mdat.py`, `oracle/export_export_mdat.py`, `oracle/export_export_mimo.py`, `oracle/goldens/{mdat,export_mdat,export_mimo}/**`, `oracle/README.md`(가족 표), `features.toml`(위 항목), `docs/DEVIATIONS.md`(F4 한 문장), `fuzz/**`(mdat 대상이 빌드를 부르면 인자만).

## 검증 (모두 포그라운드로, 보고를 마친 뒤에만 턴을 끝냅니다)

```sh
cd /home/user/wt/p49
export CARGO_TARGET_DIR=/home/user/wt/target-p49 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
time ~/.venvs/secs-oracle/bin/python oracle/run.py mdat export_mdat export_mimo
du -sh oracle/goldens/mdat oracle/goldens/export_mdat oracle/goldens/export_mimo
git status --short oracle/goldens | wc -l
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-jos -p secs-mdat -p secs-export -p secs-project -p secs-service -p secs-policy --locked 2>&1 | grep -E "test result|FAILED|panicked"
python3 .github/scripts/check_feature_tests.py
python3 .github/scripts/upstream.py check-tree
```

## 보고 형식

1. 바꾼 파일과 각 한 줄 설명
2. 공개 표면(최종 시그니처), 작업서와 다르게 정한 것과 이유
3. 골든: 다시 낸 사례 수와 새 사례 목록, 크기, 내보내기 시간
4. 업스트림과 맞추며 걸린 자리(핸들 순서, 서술자 공유, float 문법 등)와 푼 방법
5. 검증 명령마다 마지막 몇 줄의 출력
