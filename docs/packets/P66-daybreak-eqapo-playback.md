# P66. Equalizer APO 재생 검증과 MIMO 내보내기 (Daybreak)

## 배경

업스트림 SECS 2는 MIMO를 켠 결과를 Equalizer APO로 내보내지 않습니다
(`filter_export_block`). 그런데 작성기(`secs-filters`)는 이미 MIMO 서포트
경로를 가상 채널 `VP1…`로 복사·처리하고 출력에 더합니다. 조정자가
`crates/secs-types/src/ipc.rs`의 막음을 풀었습니다(편차 F12). 이 작업은
그 결과가 실제 Equalizer APO 엔진에서 설계대로 재생되는지 CI에서 확인할
수단을 만듭니다. CI 워크플로(`.github/workflows/ci.yml`)는 조정자가 씁니다.

엔진은 둘입니다. EqualizerAPO-XT의 릴리스 페이로드에 든 `Benchmark.exe`
(설정 폴더 인자 `--config-path`가 있음)와 공식 Equalizer APO 1.4.2
설치본에 든 `Benchmark.exe`(인자가 없어 레지스트리
`HKLM\SOFTWARE\EqualizerAPO` 의 `ConfigPath`를 읽음)입니다. 두 Benchmark
모두 출력 WAV를 PCM16으로 씁니다. 지연은 정수 샘플로 반올림합니다
(`floor(ms * rate / 1000 + 0.5)`).

이 컨테이너는 Linux라 Benchmark를 직접 돌릴 수 없습니다. 분석 부분은
Linux에서 합성 시스템으로 단위 테스트하고, 실제 실행은 CI가 합니다.

## 해야 할 일

### 1. `export_eqapo`의 머리 메모 (`crates/secs-filters/src/lib.rs`)

`export_eqapo`의 `note`가 지금 `"...; no MIMO"`로 고정되어 있습니다.
업스트림 `calibration.export_eqapo`의 식대로 MIMO가 있으면
`MIMO {FIR|IIR} support paths {n} (Fsiso {fsiso:g} Hz)`를, 없으면
`no MIMO`를 씁니다. `n`은 `plan.mimo.len()`, 종류는 경로 필터가 하나라도
`MimoFilter::Fir`이면 `FIR`, 아니면 `IIR`(경로가 전부 한 종류입니다).
`fsiso`는 `format_g(fsiso, false)`로 씁니다. 나머지 바이트는 바꾸지 않습니다.

### 2. 기존 테스트 갱신

- `crates/secs-filters/tests/golden.rs` 1124행 근처: `mimo_iir`로
  `export_eqapo`가 `Scope(Mimo)`로 막히는지 보던 단언을, 성공해서
  `config.txt`가 `VP` 채널을 출력에 더하고(`+VP1` 형태) 본문 머리에
  `MIMO IIR support paths` 메모가 있는지 보는 단언으로 바꿉니다. 텍스트
  내보내기가 `Scope(Subs)`로 막히는 단언은 그대로 둡니다. 테스트 이름에
  F12를 붙입니다(예: `..._eqapo_carries_mimo_f12`). 함수 이름을 바꾸면
  `features.toml`의 해당 이름도 바꿉니다.
- `crates/secs-service/tests/service.rs` 1066행 근처: `(EqualizerApo,
  "mimo")` 막음 단언을 빼고, 같은 MIMO 결과로 `EqualizerApo` 내보내기가
  끝나서 쓴 파일 중 본문(`secs_*/secs_*.txt`)에 `# 6) MIMO support paths`가
  있는지 확인합니다. `FilterText`는 계속 `"subs"`로 막힙니다.

### 3. 재생 검증 테스트 `crates/secs-filters/tests/eqapo_playback.rs`

`#[test] #[ignore = "Equalizer APO Benchmark.exe가 필요합니다. ci.yml의 eqapo-playback이 돌립니다."]`
`fn exported_configs_play_as_designed()` 하나를 둡니다. 파일 머리에
`#![forbid(unsafe_code)]`. 새 의존성은 넣지 않습니다(WAV 읽기·쓰기는
직접 씁니다. `secs_dsp::fft::{rfft, irfft}`, `secs_camilla::pyfmt::format_fixed`
를 씁니다).

환경 변수:

| 변수 | 뜻 |
|---|---|
| `SECS_EQAPO_BENCHMARK` | 필수. `Benchmark.exe` 경로. 없으면 패닉(이 테스트는 `--ignored`로만 돌므로 없으면 실패가 맞습니다). |
| `SECS_EQAPO_CONFIG_DIR` | 선택. 있으면 엔진이 이 폴더의 `config.txt`를 읽는 것으로 보고(원판, 레지스트리), 사례마다 이 폴더의 내용을 비운 뒤 그 안에 내보내고 `--config-path`를 넘기지 않습니다. 없으면 임시 폴더에 내보내고 `--config-path <폴더>`를 넘깁니다. |
| `SECS_EQAPO_ENGINE` | 선택. 보고서 제목에만 씁니다(`xt`, `original`). |
| `SECS_EQAPO_CASES` | 선택. 쉼표로 고른 사례 이름. 없으면 전부. |
| `SECS_EQAPO_REPORT` | 선택. 있으면 아래 표를 마크다운으로 이 경로에 씁니다. |

사례(모두 `secs_oracle::Fixture`로 읽습니다. 사슬과 MIMO 경로를 읽는
도우미는 `crates/secs-camilla/tests/golden.rs`의 `chain`, `mimo_paths`를
옮겨 씁니다. `filters` 사례는 `crates/secs-filters/tests/golden.rs`의
`chain`을 씁니다):

| 이름 | 사슬 | MIMO |
|---|---|---|
| `baseline_room_dba` | `export_mimo/room_dba_fir` | 없음 |
| `mimo_fir_room_dba` | `export_mimo/room_dba_fir` | 그 사례의 경로(FIR 18개) |
| `mimo_iir_room_dba` | `export_mimo/room_dba_iir` | 그 사례의 경로(IIR 17개, 반전·긴 지연 포함) |
| `dolby_computed` | `filters/computed_dolby` | 없음 |

각 사례:

1. `FilterRun { chain, mimo, mode: "mixed", ultra_ms: 200.0, label: "playback",
   latency_ms: chain.latency_ms, rc_hi_hz: 100.0, rc_pieces: &[], stamp:
   "2026-10-23 00:00" }`로 공개 함수 `export_eqapo(&run, folder)`를 부릅니다.
   사용자가 받는 파일과 같은 것을 재생해야 합니다.
2. `plan = make_plan(chain, mimo)`, `(inputs, outputs) = windows_channels(&plan)`.
   장치 채널 수 `C`는 입력·출력 번호의 최댓값입니다.
3. `config.txt`에서 처음 나오는 `Preamp: <x> dB` 줄의 값을 읽어 전역
   이득 `p = 10^(x/20)`로 씁니다.
4. `plan.inputs`의 입력마다(LFE 포함) 한 번씩 실행합니다.
   - 들뜸: 주기 `N = 131072`의 무작위 위상 멀티사인. `k = 1..N/2-1`에서
     `X[k] = e^{jφ_k}`, `X[0] = X[N/2] = 0`, `φ_k`는 결정적 의사난수
     (xorshift64*, 씨앗은 상수와 입력 순번으로). `x = irfft(X, N)`를 RMS
     0.05로 맞추고 f32로 바꿉니다. 비교에는 f32로 바꾼 값의 `rfft`를
     씁니다(실제로 넣은 신호의 스펙트럼).
   - 입력 WAV: `C`채널, 48 kHz, IEEE float32(형식 3), `2N` 프레임. 그 입력의
     번호(1부터) 채널에 `x`를 두 주기 넣고 나머지는 0.
   - 실행: 작업 폴더는 `Benchmark.exe`가 있는 폴더, 인자는
     `--nopause --verbose --batchsize 512 -i <입력 절대경로> -o <출력 절대경로>`
     (+ 경로 모드면 `--config-path <내보낸 폴더>`). 종료 코드가 0이 아니거나
     표준 출력·오류에 `Error`가 든 줄이 있으면, 그 출력 전체를 붙여 실패합니다.
   - 출력 WAV 읽기: 형식 1(PCM16), 3(float32), `0xFFFE`(WAVE_FORMAT_EXTENSIBLE,
     부형식 GUID 앞 두 바이트로 PCM/float 판정)을 받습니다. `fmt `·`data`
     밖의 청크는 건너뜁니다. 채널 수가 `C`이고 프레임이 `2N` 이상이어야
     합니다. 절댓값이 0.9999 이상인 표본이 있으면 '잘림'으로 실패합니다.
   - 추정: 출력 채널마다 `y[N..2N]`의 `rfft`를 넣은 스펙트럼으로 나눠
     `H[k]`를 얻습니다(두 번째 주기는 정상 상태라 원형 합성곱이 정확히 성립).
5. 기준: `plan_ref = as_written(&plan)`에 `plan_responses(&plan_ref, f, input,
   false)`를 쓰고 `p`를 곱합니다. `f_k = k * 48000 / N`. 응답에 없는 출력
   채널의 기준은 0입니다. `as_written`은 파일에 적힌 정밀도로 값을 되돌린
   계획입니다.
   - PK: 중심 `format_fixed(v,2)`, 이득 `format_fixed(v,2)`, Q `format_fixed(v,4)`를
     다시 f64로 읽은 값.
   - Gain: `|db| < 0.005`면 0, 아니면 `format_fixed(db,2)`.
   - Delay: `ms <= 0`이면 0. 아니면 `s = floor(format_fixed(ms,3) * 48 + 0.5)`,
     `ms = s / 48`(엔진의 정수 샘플 반올림).
   - HP/LP: 중심만 `format_fixed(v,2)`(형은 그대로, 단 Q 0.7071 대 1/√2 차이는
     무시한다는 주석).
   - AP: 중심 2자리, Q 4자리.
   - FIR: 탭을 f32로 바꿨다가 f64로.
   - 서브 버스 계수(`format_fixed(factor,6)`)의 반올림은 무시한다는 주석.
6. 비교: `10 <= f_k <= 20000`인 칸만. 입력마다 `peak` = 그 입력의 모든 출력
   기준 `|R|`의 최댓값. 칸마다 `err = |H - R|`, `bound = REL*|R| + ABS*peak`,
   `REL = 2e-3`, `ABS = 1e-4`(상수 두 개를 파일 머리에 이름 붙여 둡니다).
   `err/bound`가 1을 넘는 칸이 하나라도 있으면 실패합니다. 기준이 0인
   출력은 누설 검사가 됩니다.
7. 보고: 사례·입력·출력마다 한 줄. 열은 사례, 입력, 출력(장치 번호도),
   최악의 `err/bound`, 그 칸의 주파수, `20*log10(err/peak)`, 그 칸의
   `|H|`·`|R|`(dB), 위상차(도), 그리고 `|R| > 0.1*peak`인 칸들에서
   `H/R`의 펼친 위상 기울기로 잰 남는 지연(샘플). 이 표를 언제나 표준
   출력에 찍고(`--nocapture`로 보임), `SECS_EQAPO_REPORT`가 있으면 그
   파일에도 씁니다. 실패할 때는 전부 비교한 뒤 넘은 줄을 모아 한 번에
   패닉합니다(첫 실패에서 멈추지 않습니다).

분석 부분은 함수로 나눠 같은 파일 안에 `#[test]`(무시하지 않음)로 Linux에서
확인합니다. WAV 쓰기→읽기 왕복(PCM16, float, EXTENSIBLE), `as_written`의
반올림, 그리고 `H` 추정이 알려진 FIR·정수 지연을 원형 합성곱으로 적용한
합성 출력에서 그 FIR의 응답을 1e-9 안에서 되찾는지, PCM16로 양자화해도
`REL/ABS` 안에 드는지. 이 단위 테스트들은 `cargo test --workspace`에서 돕니다.

### 4. 엔진 내려받기 스크립트

`/home/user/Sound-Equalization-and-Correction-System-2R/.github/eqapo-engines.toml`
(조정자가 값을 정했습니다. 그대로 씁니다):

```toml
# Equalizer APO engines the playback check plays SECS 2R exports through
# (ci.yml eqapo-playback, crates/secs-filters/tests/eqapo_playback.rs).
# Bump deliberately: a new engine version is a new claim about playback.

[xt]
version = "2.60.4"
url = "https://github.com/115dkk/EqualizerAPO-XT/releases/download/v2.60.4/EqualizerAPO-XT-x64-sse2-2.60.4-x64-sse2-full.nupkg"
sha256 = "334a18580c0e65e6243693ab56377a1146f185d464ae64b72ba4b6f11a1436ce"
format = "nupkg"

[original]
version = "1.4.2"
url = "https://downloads.sourceforge.net/project/equalizerapo/1.4.2/EqualizerAPO-x64-1.4.2.exe"
sha256 = "7403be7427bbe1936a40dded082829b6e217fc4f5990fee5cba501f0ae055afa"
format = "nsis"
```

`/home/user/Sound-Equalization-and-Correction-System-2R/.github/scripts/eqapo_engine.py`
(표준 라이브러리만, Python 3.11 이상, `tomllib`):

```
eqapo_engine.py fetch --engine {xt,original} --dest DIR
                      [--manifest PATH] [--register] [--github-env PATH]
```

- 매니페스트 기본값은 스크립트 기준 `../eqapo-engines.toml`. 키 넷이 다
  있는지, `sha256`이 소문자 16진 64자인지, `format`이 `nupkg`·`nsis` 중
  하나인지 확인합니다.
- 내려받기: `urllib.request`, `User-Agent: secs2r-ci`, 리다이렉트를 따름.
  최대 4번 시도, 실패 사이 2·4·8초. `DEST/download/` 아래 임시 파일로 받고
  SHA-256이 맞을 때만 이름을 바꿉니다. 맞지 않으면 파일을 지우고
  `checksum mismatch: expected … got …`로 끝냅니다. 같은 해시의 파일이
  이미 있으면 다시 받지 않습니다(캐시).
- 풀기: `nupkg`는 `zipfile`로 `lib/app/` 아래 항목만, 그 접두어를 떼고
  `DEST/engine/`에 풉니다. 절대 경로나 `..`가 든 항목은 거부합니다.
  `nsis`는 `7z x -y -o<DEST/engine> <파일>`. `7z`는 PATH 또는
  `C:\Program Files\7-Zip\7z.exe`.
- `DEST/engine` 아래에서 `Benchmark.exe`(대소문자 무시)를 찾습니다. 정확히
  하나가 아니면 후보를 나열하고 실패합니다.
- `--register`(원판만 의미 있음, Windows에서만): `DEST/config`를 만들고
  `winreg`로 `HKEY_LOCAL_MACHINE\SOFTWARE\EqualizerAPO`의 `ConfigPath`(REG_SZ)를
  그 폴더로 씁니다(`KEY_WOW64_64KEY`). Windows가 아니면 실패합니다. 이것은
  버리는 CI 러너용입니다. 도움말에 적어 둡니다.
- `--github-env PATH`: `SECS_EQAPO_BENCHMARK=<절대경로>`, `SECS_EQAPO_ENGINE=<engine>`,
  등록했으면 `SECS_EQAPO_CONFIG_DIR=<DEST/config>`를 그 파일 끝에 덧붙입니다.
- 마지막에 엔진, 판, Benchmark 경로, 설정 폴더를 한 줄씩 찍습니다.

`/home/user/Sound-Equalization-and-Correction-System-2R/.github/scripts/test_eqapo_engine.py`
(`unittest`, 네트워크·7z 없이): 매니페스트 검증, 해시 불일치 시 파일 삭제와
오류, 캐시된 파일 재사용, 재시도(가짜 열기 함수 주입), nupkg 접두어 떼기와
zip slip 거부(합성 zip), Benchmark 찾기(0·1·2개), `--github-env` 줄 형식,
Windows가 아닐 때 `--register` 거부. 스크립트의 다른 테스트들
(`test_release_gate.py` 등)과 같은 꼴로 씁니다.

### 5. `features.toml`

`id = "export.eqapo"`의 `tests`에 `secs-filters::exported_configs_play_as_designed`와
바꾼 골든 테스트 이름을 넣습니다. `.github/scripts/check_feature_tests.py`가
무시된 테스트를 어떻게 다루는지 먼저 읽고, 무시된 테스트를 이름으로 걸 수
없으면 그 사실을 보고하고 넣지 않습니다.

## 허용 파일

- `crates/secs-filters/src/lib.rs`(1번 메모만)
- `crates/secs-filters/tests/golden.rs`, `crates/secs-filters/tests/eqapo_playback.rs`(새 파일)
- `crates/secs-service/tests/service.rs`(2번 단언만)
- `.github/eqapo-engines.toml`, `.github/scripts/eqapo_engine.py`, `.github/scripts/test_eqapo_engine.py`(새 파일)
- `features.toml`

워크플로 파일, `crates/secs-types`, 화면(`apps/`), 문서, `CHANGELOG.md`,
업스트림 파이썬 트리(`src/`, `tests/`, `tools/`)는 건드리지 않습니다.

## 검증 (포그라운드로 돌리고, 끝난 뒤에만 보고하십시오)

```sh
cd /home/user/Sound-Equalization-and-Correction-System-2R
cargo fmt --all -- --check
cargo clippy -p secs-filters -p secs-service --all-targets --locked -- -D warnings
cargo test -p secs-filters --locked
cargo test -p secs-service --locked --test service
cargo test -p secs-types --locked
python3 -m unittest discover -s .github/scripts -p "test_*.py"
python3 .github/scripts/check_feature_tests.py
cargo test -p secs-policy --locked
```

`cargo test -p secs-filters --test eqapo_playback -- --ignored`는 이 컨테이너에서
`SECS_EQAPO_BENCHMARK`가 없어 실패하는 것이 맞습니다. 한 번 돌려 그 메시지가
뜻이 분명한지만 확인합니다.

## 보고 형식

바꾼 파일 목록, 각 검증 명령의 결과(통과/실패와 실패 출력), 기준 근사에서
일부러 무시한 것의 목록과 그 크기 어림, 그리고 확신하지 못한 API나 판단을
따로 적습니다. 커밋은 하지 않습니다.
