# Equalizer APO 내보내기 상호운용 확인

확인일: 2026-10-21 (KST). 관련 작업서: [P61](packets/P61-equalizer-apo-eval.md).

## 판정

원작 SECS와 기존 SECS 2R의 설정 작성기가 여러 변수 할당을 하나의 `Eval:`
명령에 세미콜론으로 연결합니다. EqualizerAPO-XT의 실제 수식 파서는 이를
거부합니다. 입력·출력 번호가 정의되지 않아 채널 조건도 실패하고, FIR과
게인이 적용되지 않습니다. 전역 Preamp는 그보다 앞에서 별도로 적용될 수
있으므로, 소리가 작아졌다는 사실만으로 FIR 적용 여부를 판단할 수 없습니다.

추가로 원본 APO의 프로젝트가 지정한 muparserx 3.0.1과 XT의 4.0.12를 각각
빌드하여 비교했습니다. **3.0.1은 세미콜론을 받아들이고, 4.0.12는 거부**합니다.
따라서 원작 파일의 실패는 XT에서 구형 문법 호환이 사라진 문제입니다.
XT에는 기존 파일을 읽는 호환 처리를 복원하고, SECS 2R 작성기는 **변수마다
별도의 `Eval:` 명령을 내도록 수정**하여 기존 XT에서도 쓸 수 있게 합니다(F11).
FIR WAV와 필터 본문은 그대로 둡니다.

```text
# 기존
Eval: inL = 1; inR = 2
Eval: outL = 1; outR = 2

# 수정
Eval: inL = 1
Eval: inR = 2
Eval: outL = 1
Eval: outR = 2
```

이 재현은 출력 파일과 소비자의 오프라인 엔진을 대상으로 합니다. 제보자의
장치·설정 파일·사용 버전은 확보하지 않았으므로 동일 원인인지 개별 확인은
남아 있습니다.

## 기준과 방법

- 생산자: `UPSTREAM.toml`의 원작 SECS `587159ed8a6077f146ccd7de023de80203462739`,
  수정하지 않은 `secs.calibration.export_eqapo` 공개 함수.
- 기존 Rust 기준: SECS 2R `7c520c5b2d08ff2001838f5c204dd6771a0edf45`.
- 소비자: EqualizerAPO-XT `3a6a213c`의 `Common`과 `Benchmark`, Release x64,
  실제 `FilterEngine`, muparserx, libsndfile, FFTW.
- 원본 문법 비교: 공식 Equalizer APO 1.4.2 소스 zip의 SHA-256은
  `bacd5c78be71f3aaab010684ab39f0e5c1bbb45fcd75cbb92c103a145a731dc4`입니다.
  지정 의존성인 muparserx 3.0.1(`06b928eedaaf0306b8dc010573436d4e62fa4875`)을
  별도로 빌드하여 같은 식을 실행했습니다. 후속
  [muparserx 변경](https://github.com/beltoforion/muparserx/commit/68c12e06f16aed9568392e3d4c02f8089bd29b7b)이
  세미콜론 구분자를 없앴습니다. 공식 x64 설치 파일도 공개 SHA-256
  `7403be7427bbe1936a40dded082829b6e217fc4f5990fee5cba501f0ae055afa`와 대조한 뒤
  설치 없이 추출했고, Benchmark 내부의 `3.0.1 (20140527; release)` 문자열을
  확인했습니다. 공식 Benchmark에는 별도 설정 경로 인자가 없으므로, 장치의
  전역 설정을 변경해 처리하는 시험은 하지 않았습니다. 원본 문법은 별도로
  빌드한 3.0.1 실행으로, 전체 오디오 처리는 XT 엔진으로 검증했습니다.
- 48 kHz 입력, 블록 512, 길이 131072, 4096번째 샘플에 진폭 0.5 임펄스.
  입력 채널마다 별도로 실행하여 출력 채널의 누설과 뒤바뀜을 확인했습니다.
- Benchmark의 저장 출력은 PCM16입니다. 양자화 간격 `1/32768`을 기준으로
  비교합니다. 합성 FIR의 아래 탭은 이 형식에서 정확히 표현됩니다.
- 설정 원본과 `Eval:`만 분리한 대조본을 28회 실행하고, 조정자가 전체
  실행을 독립적으로 반복했습니다. 실제 장치의 설치·레지스트리·오디오
  설정을 변경하지 않았습니다.

일반적인 한 번의 실행 명령은 다음과 같습니다. 필요한 런타임 DLL을
Benchmark 옆 또는 PATH에서 찾을 수 있어야 합니다.

```powershell
Benchmark.exe --nopause --verbose --batchsize 512 `
  --config-path <내보낸폴더>/config.txt `
  --input <입력임펄스.wav> --output <결과.wav>
```

## 재현 결과

| 사례 | 원작 설정 | 할당별 Eval 대조본 |
| --- | --- | --- |
| 모노 FIR `[0.5, -0.25, 0.125]` | 오류 없이 적용 | 같은 출력 |
| 스테레오 FIR, L 위와 같음 / R `[0.25, 0.125, -0.0625]` | Eval 오류, 입력 그대로 출력 | L `[0.25, -0.125, 0.0625]`, R `[0.125, 0.0625, -0.03125]`; 다른 채널은 0 |
| 스테레오 게인만 L −6 dB / R −12 dB | Eval 오류, 입력 그대로 출력 | L `0.250579833984375`, R `0.125579833984375`; 기대 게인과 PCM16 한 간격 이내 |
| 2.1, 80 Hz LR24 분기, LFE 입력 | Eval 오류, 입력 그대로 출력 | 오류 없이 메인 HP·서브 LP·LFE 경로 출력 |
| 계산 골든 `computed_room_rc_mixed`의 L/R/C | Eval 오류, 전역 −1.5 dB Preamp만 적용 | 오류 없이 채널별 FIR·PEQ·지연 적용 |
| 저수준 MIMO 작성기 진단 | Eval 오류 | 오류 없이 경로 출력. 공개 Equalizer APO 내보내기는 MIMO를 지원하지 않으며, 이 사례는 지원 범위 판정에 포함하지 않음 |

계산 사례는 저장된 실제 계산 결과와 f64 사이드카를 읽어 원작 공개 작성기를
호출했습니다. 최적화를 다시 실행한 사례가 아닙니다. FIR WAV 3개는 기존
골든과 SHA-256이 같고, 설정·본문·안내는 줄바꿈을 LF로 맞추면 같습니다.
전체 원작 자료의 WAV 11개는 유한한 mono float32, 48 kHz이고 상대 경로가
모두 실제 파일을 가리켰습니다.

2.1의 L/R/LFE와 계산 골든의 L/R/C 출력은 원작 사슬과 직렬화 정밀도에서
독립적으로 만든 SciPy RBJ SOS·FIR·게인·지연 결과와도 비교했습니다.
시간 정렬을 맞추거나 오프셋을 보정하지 않고 총 2,359,296개 샘플을 대조했으며,
최대 오차는 PCM16 한 간격의 0.572468배였습니다. 할당하지 않은 출력 채널은
정확히 0이었습니다. 조정자가 이 수치 검증도 독립적으로 반복했습니다.

대표 오류:

```text
Error while evaluating expression inL = 1; inR = 2:
Undefined token "; inR = 2" found at position 7.
```

## 유지하는 사용 조건

FIR WAV는 48 kHz입니다. 장치 샘플레이트는 이 값과 맞춰야 하며,
`Convolution:` 상대 경로는 그 명령이 들어 있는 설정 파일을 기준으로 합니다.
이는 [Equalizer APO 설정 규격](https://sourceforge.net/p/equalizerapo/wiki/Configuration%20reference/)의
조건이며, 이번 변수 할당 수정과 별개입니다.

채널 번호는 실제 장치 배선에 맞춰야 합니다. 예를 들어 내보낸 2.1 기본
매핑의 LFE/SW1은 3번이지만 일반적인 5.1 장치의 LFE 위치는 4번일 수 있습니다.
설정 머리의 입력·출력 번호를 확인합니다.

## 회귀 검증

기존 원작 트리와 오라클 파일은 보존합니다. Rust 테스트는 F11의 정확한
설정·안내 차이만 기대값에 반영하며, FIR WAV와 본문 등 나머지 파일은 기존
바이트·해시 검사를 유지합니다. 별도 회귀가 할당 순서·번호·0 매핑·역할의
대소문자·모노와 다채널 안내를 확인합니다.

수정한 Rust 공개 작성기의 실제 파일로도 9개 구성, 입력 채널별 50회 실행을
확인했습니다. 비교 가능한 위 5개 구성의 11개 출력은 원작에 Eval 분리만
적용한 대조본과 PCM16 전체 배열이 정확히 같았습니다. 나머지 돌비·인프라·
초고지연·최소 지연 계산 사례는 오류 없이 출력이 만들어지는지를 확인했습니다.

같은 실제 작성기 파일은 다음 테스트로 보존할 수 있습니다. 출력 환경 변수가
없으면 테스트는 임시 폴더만 사용합니다.

```powershell
$env:SECS_EQAPO_F11_DUMP_DIR = 'E:/scratch/eqapo-f11'
cargo test -p secs-filters --test golden --locked
Remove-Item Env:SECS_EQAPO_F11_DUMP_DIR
```

## MIMO 재생 검증

확인일: 2026-10-23 (KST). 관련 작업서: [P66](packets/P66-daybreak-eqapo-playback.md),
편차 F12.

SECS 2R은 MIMO를 켠 결과도 Equalizer APO 설정으로 내보냅니다. 작성기는 서포트
경로마다 가공하지 않은 입력을 가상 채널 `VP1…`로 복사해 경로 필터(지연, 하이패스,
FIR 또는 PEQ·올패스·로우패스, 반전)를 걸고, 지원 스피커의 출력에 더합니다. CI의
`eqapo-playback` 잡이 이 파일이 실제 엔진에서 설계대로 재생되는지 확인합니다.

### 방법

- 엔진: `.github/eqapo-engines.toml`에 판과 SHA-256을 고정한 EqualizerAPO-XT의
  x64 SSE2 페이로드와 공식 Equalizer APO 1.4.2 x64 설치본 안의 `Benchmark.exe`.
  공식판은 설정 폴더 인자가 없어, 버리는 러너에서 레지스트리 `ConfigPath`를
  시험 폴더로 돌립니다.
- 사례: 실제 계산 골든 `export_mimo/room_dba_fir`(메인 3, 서브 4, FIR 서포트 18),
  `room_dba_iir`(IIR 서포트 17, 반전과 수십 ms 지연 포함), 같은 사슬의 MIMO 없는 기준,
  `filters/computed_dolby`.
- 들뜸: 입력 채널마다 주기 131072의 무작위 위상 멀티사인을 두 주기 넣고, 둘째
  주기의 출력을 들뜸 스펙트럼으로 나눠 전달 함수를 얻습니다. 두 Benchmark 모두
  출력을 PCM16으로 쓰지만, 에너지가 모든 칸에 고르게 퍼져 양자화 잡음이 단위
  응답보다 약 75 dB 낮습니다.
- 기준: 파일에 적힌 정밀도(PK 2·2·4자리, 지연 3자리 뒤 정수 샘플 반올림, FIR f32)로
  되돌린 필터 계획의 `plan_responses`에 전역 Preamp를 곱한 값.
- 판정: 10 Hz–20 kHz의 모든 칸에서 `|측정 − 기준| ≤ 2e-3·|기준| + 1e-4·최대값`.
  기준이 0인 출력은 누설 검사가 됩니다. 사례·입력·출력마다 최악의 칸과 남는
  지연을 표로 남깁니다(잡 요약).

### 결과

첫 CI 실행 결과를 여기에 적습니다.
