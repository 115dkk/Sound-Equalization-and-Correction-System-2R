# CamillaDSP 재생 검증

SECS 2R이 내보낸 CamillaDSP 설정을 실제 CamillaDSP로 재생해, 설계한 응답이 나오는지 확인한 기록입니다. 이슈 #107에서는 Equalizer APO XT와 CamillaDSP가 같은 결과를 내는지 물었습니다. Equalizer APO 쪽 기록은 [`EQUALIZER_APO_INTEROP.md`](EQUALIZER_APO_INTEROP.md)에 있습니다.

## 방법

`crates/secs-camilla/tests/camilla_playback.rs`가 검사를 맡고, CI의 `camilla-playback` 잡이 PR마다 돌립니다. 엔진은 `.github/camilladsp-engines.toml`에 판과 SHA-256으로 고정했고 `.github/scripts/camilla_engine.py`가 받습니다.

- 사례는 RC만 있는 배치, 서브와 LFE가 있는 돌비 배치(`dolby_computed`), MIMO IIR, MIMO FIR, 저율 FIR입니다. 사례마다 앱이 지금 쓰는 설정(헤드룸, 라벨, `secs:correction`·`secs:mimo` 단계)과 우회 설정을 함께 재생합니다.
- 입력은 32비트 float `RawFile`이고 출력은 32비트 float `File`입니다. 입력마다 임펄스 하나를 서로 겹치지 않게 넣어, 모든 입력·출력 쌍의 임펄스 응답을 얻습니다.
- 기준은 Equalizer APO 검사와 같은 `plan_responses`(아날로그 원형 기준)에 헤드룸 이득을 곱한 값입니다. 판정 한도도 같습니다(`REL` 1e-2, `ABS` 1e-3, 10 Hz–20 kHz). 우회 설정은 RC와 MIMO를 뺀 사슬의 응답과 비교합니다.
- 같은 사슬을 컨볼루션 렌더러(`secs_filters::render::render_paths`)로 만든 응답과의 절대 오차도 따로 봅니다. 렌더러는 CamillaDSP가 돌려야 하는 디지털 필터를 그대로 계산하므로, 이 값은 CamillaDSP 자체의 연산 오차에 가장 가깝습니다.

## 결과 (CamillaDSP 4.1.3, 2026-10-29, Windows 빌드로 로컬 실행)

| 사례 | 설정 | 쌍 | 기준 대비 최악 err/bound | 렌더러와의 최대 절대 오차 |
|---|---|---:|---:|---:|
| baseline_room_dba | 본 설정 | 28 | 0.636 | 5.29e-4 |
| baseline_room_dba | 우회 | 28 | 0.647 | 4.30e-4 |
| mimo_fir_room_dba | 본 설정 | 28 | 0.636 | 2.32e-4 |
| mimo_fir_lowrate_room_dba | 본 설정 | 28 | 0.636 | 2.32e-4 |
| mimo_iir_room_dba | 본 설정 | 28 | 0.636 | 2.96e-4 |
| dolby_computed | 본 설정 | 168 | 0.640 | 2.06e-4 |
| dolby_computed | 우회 | 168 | 0.668 | 3.26e-4 |

MIMO 세 사례의 우회 설정은 baseline_room_dba의 우회와 같은 값입니다. 모든 쌍이 한도 안에 들었습니다. 기준 대비 최악값은 모두 19,999.9 Hz의 자기 채널 쌍(C → C, Rs → Rs)에서 나왔습니다. 아날로그 원형 기준과 디지털 필터가 나이퀴스트 가까이에서 서로 어긋나기 때문이고, Equalizer APO 두 엔진도 같은 자리에서 최악값(0.51배)을 냅니다. 렌더러와의 차이는 2.1×10⁻⁴–5.3×10⁻⁴이며, 32비트 float 엔진이 긴 FIR을 나눠 컨볼루션할 때 생기는 크기입니다.

## 판정

CamillaDSP 4.1.3은 SECS 2R이 내보낸 설정을 Equalizer APO XT 2.60.4, 공식 Equalizer APO 1.4.2와 같은 한도 안에서 설계대로 재생합니다. 세 엔진이 같은 기준을 같은 한도로 통과하므로, 들리는 범위에서 서로 다르지 않다고 판정합니다. 어느 엔진도 고칠 것이 없고, 사용자에게 알릴 CamillaDSP 쪽 결함도 찾지 못했습니다.

두 검사의 방법은 다릅니다. Equalizer APO는 Benchmark가 PCM16으로만 쓰기 때문에 다중 사인 들뜸으로 응답을 추정하고, CamillaDSP는 32비트 float 임펄스로 바로 잽니다. 그래서 두 엔진의 출력을 샘플 단위로 맞대지는 않았고, 같은 기준과의 거리로 비교했습니다.

## 확인하지 못한 것

- 실제 장치에서 도는 CamillaDSP(WASAPI, ASIO 백엔드)는 돌리지 않았습니다. 필터 처리는 파일 장치와 같지만 장치의 리샘플링과 버퍼는 다를 수 있습니다.
- CamillaDSP의 Linux 빌드는 CI에서 돌고, 이 표는 Windows 빌드의 로컬 결과입니다.
