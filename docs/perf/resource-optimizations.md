# 리소스 최적화 적용과 측정

결정: [ADR 0012](../adr/0012-resource-optimizations.md). 작성: 2026-10-23.

v0.24.3(`8648b6a`)을 기준으로 쓴 리소스 최적화 패치를 v0.25.1(`cd67b88`) 위에 옮겨 검토했습니다. 이 문서는 무엇을 넣고 무엇을 뺐는지, 품질이 그대로임을 어떻게 확인했는지, 계산 시간과 메모리가 얼마나 달라졌는지를 적습니다. 업스트림 Python, 오라클 데이터, 허용오차, CI 설정은 바꾸지 않았습니다.

## 1. 넣은 것

넣은 변경은 성격에 따라 셋으로 나뉩니다. 결과를 비트까지 보존하는 재사용, 전달함수를 보존하는 출력·보관 변경, 기본값을 바꾸지 않는 선택형 경로입니다.

| 항목 | 코드 | 바뀌는 것 | 결과 |
|---|---|---|---|
| XO 후보 계산 | `secs-engine/{design,xo,scorecard}.rs` | 후보에서 버릴 성적표와 그림을 만들지 않고 각 위치의 평탄도와 기준 위치의 GD만 계산합니다. 최종 설계는 전체 결과를 만듭니다. | 비트 동등 |
| RC 위상 후보 | `secs-engine/rc.rs` | 위상 분수와 무관한 초기 PEQ, 강도, 기준 응답을 한 번만 준비합니다. 두 후보의 위상, 후속 PEQ, 가드, 선택은 따로 합니다. | 비트 동등 |
| 군지연 | `secs-dsp/{response,smooth}.rs`, `scorecard.rs` | 전체 IRFFT로 찾은 봉우리를 재사용하고, 필요한 주파수의 평활과 기울기만 계산합니다. 원래 전체 격자의 창 경계를 씁니다. | 비트 동등 |
| BA 평가 | `secs-engine/{ba,chain}.rs` | 후보별 이득, 극성, 지연, 올패스 인자를 두 격자에서 한 번 만들어 모든 위치에 씁니다. 고정 기준 파워의 dB도 재사용합니다. | 비트 동등 |
| 저율 FIR 응답 | `secs-dsp/fir.rs`, `mimo/design.rs` | 정수비 격자에서는 작은 FFT로, 그 밖의 격자에서는 기존 직접 DTFT로 계산합니다. | 마지막 비트 반올림 |
| Gram 조립 | `mimo/design.rs` | 상관 블록을 최대 8쌍씩 계산하고 바로 씁니다. | 비트 동등 |
| support 순위 | `mimo/design.rs` | 잔차를 포함한 이차식으로 점수를 매기고, 상쇄가 크거나 순위 구간이 겹치면 직접 컨볼루션으로 확인합니다. | 같은 선택 |
| SPL·T60 가드 | `mimo/{design,iir}.rs` | 적용 응답, 원본 대역 파워, 원본 꼬리·시작점·SOS를 재사용합니다. 후보에서 새로 생긴 모드도 계속 검사합니다. | 비트 동등 |
| headroom·내보내기 복사 | `secs-filters`, `secs-service` | 입력마다 반복하던 서브 응답을 재사용하고, 내보내기 종류에 필요한 데이터만 복사합니다. | 비트 동등 |
| 공통 출력 필터(F13) | `secs-camilla`, `secs-filters` | bass bus EQ, 같은 XO의 LP, 같은 support의 HP·LP·지연, 서브 고정 필터를 합산 뒤 한 번만 겁니다. | 전달함수 동등 |
| 저율 FIR 보관 | `secs-types`, `mimo/run.rs`, 내보내기, 캐시 | `FirLowRate`(600 Hz, 100탭)를 보관하고 WAV를 쓸 때 48 kHz 8,000탭으로 펼칩니다. 캐시는 v5입니다. | 전달함수 동등 |
| IIR 연산자 풀이(D41) | `secs-dsp/optimize/operator.rs`, `mimo/iir.rs` | 공개 API로 고를 때만 `Jv`·`Jᵀv`와 잘린 CG로 신뢰 영역 부분문제를 풉니다. | 선택형 |
| 행렬 컨볼루션·다중 레이트(D42) | `secs-dsp/{convolution,multirate}.rs`, `secs-filters/playback.rs` | 스트리밍 참조 소비자와 `render_mimo` 예제입니다. 다중 레이트는 고를 때만 씁니다. | 선택형 |

`mimo/solve.rs`의 정칙화 탐색과 삼중대각화는 바꾸지 않았습니다. 연산자 풀이의 CG는 고른 IIR 신뢰 영역 부분문제에만 쓰고 FIR 정칙화 탐색을 대체하지 않습니다.

## 2. 넣지 않은 것

받은 패치에는 최적화 궤적을 바꾸는 변경이 셋 있었습니다. 어느 변경이 결과를 바꾸는지 알아내려고 패치에서 하나씩 뺀 판을 따로 빌드해 골든 방 10곳의 설계와 MIMO를 비교했습니다. 셋을 모두 뺀 판은 기준판과 비트까지 같았고, 셋 모두 [ADR 0004](../adr/0004-parity-and-deviations.md)의 판정 규칙을 통과하지 못했습니다. 허용오차나 오라클을 바꿔 통과시키지 않았습니다.

- **D11(BA의 미사용 올패스 변수 제거):** `SubAllpass::None`인 자동 서브마다 차분 진화 변수가 7개에서 3개로 줄어듭니다. 목적함수는 같지만 개체 수와 난수 흐름이 달라져 BA가 다른 해를 찾습니다. 이 변경만 넣은 판은 서브가 모두 자동인 방(dolby 계열, room, room_filters)의 결과를 바꿨습니다. BA 비용은 dolby에서 8.4112가 8.5035로 1.1% 나빠졌고 dolby_infra에서는 5.4522가 4.4506으로 좋아졌습니다. `engine_e2e_spread`에서 dolby 값 63개가 SECS 2 8벌 모두보다 나빠 이항 상한 55를 넘었고, 값별 상한을 넘은 값은 67개였습니다. 기록된 설계 재생(`mimo_run_assemble_recorded_supports_matches_oracle`의 room_filters)과 XO 추천 범위 검사도 실패합니다. 편차 대장의 D11은 계획으로 남깁니다.
- **일반 PEQ의 해석적 미분:** 운영 `fit_peq`로 오라클의 맞춤 97건을 다시 풀면 SECS 2의 폭 안에 듭니다(12벌 모두보다 나쁜 경우 5건, 기준보다 비용이 낮은 경우 81건). 끝단 spread도 통과합니다. 하지만 XO 추천 범위 검사에서 room 센터 100 Hz의 GD 폭이 34.46 ms(SECS 2는 32.23–32.60 ms), dolby 서라운드 100 Hz의 점수가 86.54(SECS 2는 86.76–86.82)로, 둘 다 값별 상한인 폭의 3배를 넘었습니다. 0004는 오라클 libm에서 이 상한을 넘으면 결함으로 봅니다.
- **MIMO IIR의 해석적 미분:** `mimo_iir_joint_costs_within_spread`에서 room_L의 결합 맞춤 비용이 7.376×10⁻³으로 상한 7.298×10⁻³을 넘고 SECS 2 6벌 모두보다 나빴습니다. 늦은 에너지 감소량도 방마다 크게 달라졌습니다(room_dba L은 −6.55 dB에서 −3.50 dB, room_multi C는 −2.53 dB에서 −9.09 dB). v0.28.0부터는 일반 PEQ의 해석적 미분과 함께 기본값을 바꾸지 않는 실험 설정(D43, D44)으로 고를 수 있고, 식과 방별 수치, 쓰는 법은 [`analytic-derivatives.md`](analytic-derivatives.md)에 있습니다.

해석적 미분 자체는 기존 중앙차분보다 정확합니다. 받은 패치의 진단(`room_C`에서 h=10⁻⁴ 중앙차분의 JᵀJ와 해석적 JᵀJ의 차이 2.69×10⁻⁶이 절단오차에서 온다는 것)도 맞습니다. 그래도 지금 규칙은 SECS 2와 같은 궤적의 분포에서 벗어나는지를 보며, 그 규칙 아래에서는 셋 다 탈락입니다. 다시 넣으려면 판정 규칙이나 기준 실행을 바꾸는 ADR이 먼저입니다.

## 3. 품질 확인

### 설계와 MIMO

같은 측정으로 기준판과 후보판을 돌려 결과를 직접 비교했습니다.

| 비교 | 대상 | 결과 |
|---|---|---|
| 설계 신호 사슬 | 골든 방 10곳(dolby, dolby_infra, dolby_linear, dolby_ultra, dolby_zero, room, room_dba, room_filters, room_manual, room_multi) | 10곳 모두 비트까지 같음 |
| 성적표 지표 | 같은 10곳, 역할마다 보정 전후 17개 | 바뀐 값 0개 |
| MIMO IIR | room, room_dba, room_filters, room_multi | 경로가 비트까지 같음 |
| MIMO FIR | 같은 4곳 | 늦은 에너지 감소량이 같고, 펼친 48 kHz 탭이 8×10⁻¹⁵ 안에서 같음 |

MIMO FIR의 경로는 표현이 `Fir`에서 `FirLowRate`로 바뀌어 바이트로는 다릅니다. 펼친 탭의 차이는 작은 FFT와 직접 DTFT의 마지막 비트 반올림에서 옵니다.

### 내보낸 설정을 실제 소비자로 재생

F13은 설정 파일의 배선을 바꾸므로, 설정을 실제 CamillaDSP 4.1.3에 넣어 입력마다 임펄스를 재생하고 모든 출력을 녹음해 비교했습니다(f64 RawFile·File 장치).

| 비교 | 사례 | 피크 대비 최대 차이 |
|---|---|---:|
| 오라클 골든 설정을 기존 작성기와 새 작성기로 내보냄 | 16건(LFE, 빈 버스, 서브 없음, dolby 12입력×14출력, room 계열, MIMO FIR·IIR 4건) | 1.8×10⁻¹⁴ |
| CLI 전체 흐름(room_dba, MIMO IIR)의 기준판·후보판 내보내기 | 4입력×7출력 | 5.5×10⁻¹⁷ |
| CLI 전체 흐름(room_dba, MIMO FIR)의 기준판·후보판 내보내기 | 4입력×7출력 | 6.4×10⁻¹⁶ |

Equalizer APO 설정은 CI의 `eqapo-playback`이 공식 1.4.2와 EqualizerAPO-XT 엔진에 넣어 설계 응답과 비교합니다. 이 검사는 저율 FIR을 펼친 WAV로 재생하도록 고쳤고, room_dba의 FIR 경로를 600 Hz 100탭 `FirLowRate`로 바꾼 사례(`mimo_fir_lowrate_room_dba`)를 더해 지금 앱이 내보내는 표현도 실제 엔진으로 재생합니다. 그 사례의 WAV 샘플은 `as_written`이 펼친 탭과 비트까지 같은지 일반 테스트가 따로 확인합니다.

CLI 흐름의 나머지 출력도 비교했습니다. IIR 모드의 MDAT는 바이트까지 같습니다. FIR 모드의 MDAT 두 개(13,162,296바이트, 7,181,298바이트)는 각각 4바이트가 다르고, FIR WAV는 8,000샘플 중 1개가 2.8×10⁻¹⁷ 다릅니다(피크 9.15×10⁻⁵). 저율 FIR을 펼칠 때 작은 FFT를 쓰면서 생긴 마지막 비트 차이입니다.

### 바꾼 검사

기존 검사 중 몇은 바뀐 구현과 맞지 않아 고쳤습니다. 지키던 계약은 그대로입니다.

- **CamillaDSP·APO 골든:** 오라클 바이트와 비교하던 설정 파일 가운데 그래프 밖의 내용은 지금처럼 바이트로 비교합니다. CamillaDSP는 `filters:` 줄 앞(머리 주석, 장치)과 WAV, APO는 `config.txt`에서 `Copy:`가 아닌 모든 줄, 필터 본문의 첫 빈 줄 앞(머리 주석), WAV와 안내 파일입니다. 그래프는 오라클이 만든 설정과 새 설정을 함께 해석해 모든 입력→출력 전달함수를 11개 주파수에서 `2e-12·max(|기대|, 1)` 안으로 비교합니다. APO는 `config.txt`와 Include한 본문을 함께 실행합니다(`tests/support/{camilla_graph,apo_graph}.rs`).
- **해석기가 놓치지 않게:** APO 해석기는 `Delay:`를 명령마다 정수 샘플(`floor(ms·48 + 0.5)`)로 바꿔 계산하므로, 합계 ms는 같지만 실제 지연이 한 샘플 다른 분할(2.675 ms 한 줄 대 1.345 ms와 1.330 ms 두 줄)을 잡습니다. 명령마다 토큰 수, 리터럴(`ON`, `Fc`, `Hz`, `Gain`, `dB`, `Q`, `ms`), 숫자를 확인하고, 없는 채널을 고르는 `Channel:`은 실패합니다. CamillaDSP 해석기는 필터 종류마다 허용한 매개변수만 받습니다. 두 쪽 모두 기대 응답이 전부 0이면 실패해, 해석기 오류로 양쪽이 같은 무음을 내는 경우를 막습니다. 이 규칙마다 실패해야 하는 입력을 넣은 음성 대조 테스트가 있습니다.
- **RC·설계 재생:** 기록된 fitter 호출을 순서대로 재생하는 검사는 두 위상 후보가 같은 준비 호출을 반복하던 자리에서 두 번째 호출을 뺍니다. 준비를 공유하면 같은 요청이 한 번만 오기 때문입니다. 빠진 호출은 앞 후보의 호출과 비트까지 같은지 확인한 뒤에만 빼고, 빼는 범위는 두 번째 후보의 앞부분으로 한정합니다. PEQ만 쓰는 RC는 두 후보가 통째로 같아야 하고, FIR을 쓰는 RC는 첫 PEQ 맞춤 하나까지만 같을 수 있습니다. 그 밖의 반복은 실패합니다.
- **캐시:** v4와 v2·v3 캐시는 저율 FIR이 들어 있으면 읽지 않습니다. 그런 캐시는 이 판 이전에 만들 수 없으므로 손으로 고친 파일입니다(`legacy_layouts_reject_low_rate_firs`).

## 4. 측정

### 방법

- Linux 6.18.44, x86-64 KVM, Intel Xeon @ 2.10 GHz 논리 CPU 4개, RAM 15 GiB, Rust 1.99.0 release.
- 기준판은 v0.25.1(`cd67b88`), 후보판은 이 PR의 제품 코드입니다. 두 판을 별도 빌드 폴더에서 빌드했습니다.
- `crates/secs-engine/examples/engine_perf.rs`로 연산마다 새 프로세스에서 한 번 예열한 뒤 3–5회의 중앙값을 냅니다. 두 판을 같은 연산마다 번갈아 돌려 기계 상태의 변화가 양쪽에 같이 들게 했습니다.
- 메모리는 `oracle/peak_memory.py`로 새 프로세스의 한 번 실행에서 RSS 봉우리를 잽니다. 표본 간격은 10 ms라 봉우리의 하한입니다.
- CLI 전체 흐름은 `secs2r-cli run --mimo --compute cpu`로 room_dba 프로젝트의 보정, MIMO, 모든 내보내기를 한 번에 돌린 시간입니다.

### 엔진

| 연산 | 스레드 | 기준판(초) | 후보판(초) | 기준/후보 |
|---|---:|---:|---:|---:|
| 설계 dolby | 1 | 11.03 | 10.44 | 1.06 |
| 설계 dolby | 4 | 4.25 | 4.33 | 0.98 |
| 설계 dolby_linear | 1 | 18.21 | 13.99 | 1.30 |
| 설계 room | 1 | 5.74 | 5.17 | 1.11 |
| 설계 room_dba | 1 | 4.21 | 4.49 | 0.94 |
| XO 추천 room | 4 | 6.46 | 5.19 | 1.25 |
| XO 추천 dolby | 4 | 11.78 | 11.31 | 1.04 |
| MIMO FIR room_dba | 4 | 0.397 | 0.365 | 1.09 |
| MIMO IIR room_dba | 4 | 9.24 | 8.71 | 1.06 |
| MIMO FIR big | 4 | 2.063 | 1.788 | 1.15 |
| MIMO FIR 한 출력 big | 1 | 2.63 | 2.56 | 1.03 |
| MIMO IIR 한 출력 big | 1 | 24.28 | 22.79 | 1.07 |
| MIMO IIR big | 4 | 84.37 | 78.15 | 1.08 |

`big`은 `perf_mimo` 픽스처로 support 12개, 180탭, 2160×2160 Gram을 씁니다. 가장 큰 변화는 dolby_linear 설계와 room의 XO 추천입니다. dolby_linear는 FIR 길이가 있는 RC라 위상 후보의 공통 준비를 한 번만 하는 효과가 크고, XO 추천은 후보마다 만들던 성적표와 그림을 빼는 효과가 큽니다.

설계 dolby의 4스레드와 room_dba의 1스레드는 기준판보다 느리게 나왔습니다. 두 판의 설계 결과가 비트까지 같으므로 계산량이 늘 이유는 없습니다. room_dba를 두 판 번갈아 3회씩 3번 다시 재면 중앙값이 기준판 4.08초(3.96–4.30), 후보판 4.02초(3.93–4.21)로 차이가 없었습니다. 4스레드 dolby는 기준판 표본 5개가 4.08–4.83초로 흔들려, 두 판의 차이 0.08초는 그 안에 듭니다.

### 메모리

| 연산 | 스레드 | 기준판 RSS(KiB) | 후보판 RSS(KiB) |
|---|---:|---:|---:|
| 설계 dolby | 4 | 205,816 | 191,880 |
| MIMO FIR big | 4 | 542,048 | 535,780 |
| MIMO IIR big | 4 | 808,540 | 797,148 |

메모리 봉우리는 최종 Gram 행렬(2160×2160 f64, 약 37 MB)과 작업 공간이 정하므로 8쌍 묶음 처리의 효과는 전체 RSS에서 1–2%로 나타납니다.

### CLI 전체 흐름(room_dba, 보정·MIMO·모든 내보내기)

| 모드 | 기준판(초, 3회) | 후보판(초, 3회) | 기준판 RSS(KiB) | 후보판 RSS(KiB) |
|---|---|---|---:|---:|
| MIMO IIR | 12.32, 12.89, 13.00 | 11.67, 11.35, 11.52 | 275,880 | 269,152 |
| MIMO FIR | 3.35, 3.45, 3.18 | 3.42, 3.21, 3.36 | 238,300 | 242,636 |

FIR 모드의 시간과 RSS 차이는 측정 흔들림 안입니다. 캐시 파일(`secs_run_cache.json`)은 FIR 모드에서 3,446,784바이트가 198,271바이트로 줄었습니다. 경로당 f64 계수 본문이 64,000바이트에서 800바이트로 줄기 때문입니다.

## 5. 계산과 보관에서 줄인 것

### 재사용의 범위

RC 두 후보를 위한 초기 PEQ를 한 번만 계산합니다. `rc_phase_candidates_share_preparation`은 같은 모의 fitter로 독립 후보를 계산하면 네 번, 준비를 공유하면 세 번 호출되는 것과, 고른 FIR·응답·PEQ·강도·메모가 같은 것을 확인합니다. 후보별 후속 맞춤은 공유하지 않습니다.

`Fitter`는 `FnMut`이지만 이 경로는 같은 준비 입력의 결과를 재사용합니다. 그래서 호출 횟수에 따라 다른 값을 돌려주는 외부 fitter나, 두 후보의 옛 기록을 그대로 이어 붙인 순차 재생은 호출 계약이 달라집니다. `taps=0`이면 위상과 무관한 완성 결과도 한 번만 계산합니다. 단일 `correction_traced`의 추적 순서와 후보 메모의 합산 순서는 그대로입니다.

GD의 봉우리 검출에는 원래 전체 IRFFT가 필요하므로 그것은 유지하고, 이미 계산한 봉우리를 성적표에 넘깁니다. 평활 창은 원래 전체 격자에서 정한 경계를 씁니다. 저주파 앞부분만 새 격자로 취급하면 창 끝의 값이 바뀌므로 전체 입력의 검증과 기울기에 필요한 이웃도 남겼습니다.

### 작은 FFT

48 kHz FFT 길이를 `N`, 정수 레이트 비를 `R=48000/fs_low`라 하면 `d=gcd(N,R)`, `L=N/d` 길이 FFT의 `(k*R/d) mod L`번째 bin이 같은 격자의 저율 FIR 응답입니다. 600 Hz FIR에서는 다음과 같습니다.

| 격자 | 원래 응답 점 수 | 새 FFT 길이 | 간격 |
|---|---:|---:|---:|
| 설계 `N=131072` | 65,537 | 8,192 | 5 |
| 8,000탭 내보내기용 `N=16384` | 8,193 | 1,024 | 5 |

실제 주파수 격자가 정수비와 맞을 때만 씁니다. 맞지 않거나 FIR이 필요한 FFT보다 길면 직접 DTFT를 씁니다. 270–300 Hz의 선형 감쇠, 300 Hz 이상 0, 원래 48 kHz 역변환과 절단 길이는 그대로입니다.

### Gram과 순위

support 12개, 180탭의 조립에서 모든 쌍의 중간 배열을 한꺼번에 들고 있으면 약 20.22 MB였고, 8쌍 묶음은 약 2.07 MB입니다(십진 MB). 최종 2160×2160 f64 Gram 약 37.32 MB와 계산 작업 공간은 따로입니다. 최종 행렬에 넣는 위치별 누적 순서는 그대로입니다.

support 하나의 순위 점수는 정칙화 해 `u`, 잔차 `r=(A+lambda I)u-b`에 대해

`SSE = x_lateᵀ x_late - uᵀb - lambda uᵀu + uᵀr`

로 계산합니다. 정확한 해라고 가정해 `uᵀr`을 빼지 않습니다. 상쇄의 오차 추정이 크거나 후보 점수의 불확실 구간이 겹치면 이전처럼 직접 계산합니다. 이 불확실성 추정은 보수적인 재평가 기준이며 증명된 오차 상한은 아닙니다. 최종 support 선택은 기존 MIMO 골든과 3절의 비교가 지킵니다.

### 저율 FIR 보관

600 Hz 100탭은 48 kHz 표현에서 8,000탭입니다. f64 계수 본문은 경로당 64,000바이트에서 800바이트로, 132개 경로에서는 8,448,000바이트에서 105,600바이트로 줄어듭니다. JSON 메타데이터와 내보낼 때의 일시적인 확장 버퍼는 그대로입니다.

캐시 v5는 `fir_materialization_version=1`을 확인합니다. v4의 펼친 FIR과 v2·v3의 이전 저장 구조도 읽고, 옛 판 안의 저율 FIR은 거부합니다. 잘못된 저율 레시피는 거부하며 펼친 길이는 최대 `1<<20`탭입니다. 기존 48 kHz FIR에는 이 상한을 적용하지 않습니다.

## 6. 출력 필터의 등가 조건

CamillaDSP와 APO 설정은 최종 서브 고정 HP·LP·PEQ를 기본 서브 경로와 MIMO support의 합 뒤로 옮깁니다. 기본 경로에만 있는 이득, 극성, 올패스, 지연을 MIMO에 덧씌우지 않습니다. bass bus EQ와 같은 XO의 LP도 합산과 바꿔 쓸 수 있는 범위에서 공유합니다. 보호 HP가 다르거나 FIR·IIR 종류가 다르면 다른 묶음으로 남깁니다.

CamillaDSP 지연은 원래 전체 지연을 먼저 정수 샘플로 반올림한 뒤 공통 최솟값과 나머지로 나눕니다. `delay_ms * 48000 / 1000`의 계산 순서도 그대로입니다. `delay_ms * 48`로 줄이면 반 샘플 경계에서 한 샘플이 달라지는 사례가 있습니다. APO는 엔진 안의 시간 반올림을 가정하지 않도록 직렬화한 ms 문자열이 같은 지연만 공유합니다.

## 7. IIR 연산자 풀이 쓰기

`design_mimo_iir`는 지금처럼 조밀한 풀이를 씁니다. 연산자 풀이는 공개 API로만 고를 수 있고, 앱 설정이나 IPC로는 켜지지 않습니다.

```rust
use secs_engine::{IirSolveOptions, IirSolver, design_mimo_iir_with_options};

let options = IirSolveOptions {
    solver: IirSolver::MatrixFree {
        max_cg_iterations: 64,
        cg_rtol: 1e-3,
    },
    dense_fallback: false,
};
let result = design_mimo_iir_with_options(
    &design, &measurements, &config, &ideal, &request, &tuning, &options, &context,
)?;
```

Coleman–Li 척도, 경계, 반사·Cauchy 후보 선택은 그대로이고, 신뢰 영역 부분문제를 전처리한 잘린 CG로 풉니다. 미분은 기존 중앙차분(h=10⁻⁴)입니다. 잔차×변수 Jacobian과 변수×변수 Gram은 만들지 않지만, 반복곱의 임시 벡터와 support별 복소 미분은 필요합니다. 늘 더 빠르다는 보장은 없습니다.

`dense_fallback: true`이면 연산자 풀이가 실패한 뒤 기존 조밀한 풀이로 다시 풉니다. 이때 메모리 예약에는 조밀한 작업 공간도 들어가므로, 메모리 효과를 보려면 `false`를 씁니다. 시험점의 잔차가 거부돼도 기준점의 선형화가 덮어써지지 않도록 따로 보관합니다. 수반 연산자 검사는 실수 IRFFT의 DC, 나이퀴스트, 홀수·짝수 길이, 시간 절단을 포함합니다.

## 8. 스트리밍 FIR 실행

### API와 연산량

`secs_dsp::convolution::MatrixConvolver`는 B탭씩 나누어 2B FFT와 overlap-add로 처리합니다. 입력 변환 기록을 모든 경로가 공유하고 출력 묶음에서 스펙트럼을 합칩니다. 입력 I개, 출력 묶음 G개이면 블록마다 정변환 I번, 역변환 G번입니다. 132개 경로, 입력 11개, 출력 13개가 각각 한 묶음이면 경로마다 따로 변환하는 264번 대신 24번입니다. 변환 횟수의 비교이며 실행 시간을 잰 값은 아닙니다.

계수는 분할 스펙트럼으로 보관하므로 같은 48 kHz 탭을 시간영역에 둘 때보다 계수 메모리가 약 두 배입니다. 입력 기록과 overlap 버퍼도 필요합니다. `process`와 `process_buffers`는 작업 버퍼를 재사용해 블록마다 할당하지 않습니다. 길이가 틀리거나 유한하지 않은 입력을 받으면 상태를 진행하기 전에 오류를 돌려줍니다.

`secs_filters::playback::MimoFirPlayback`는 실제 `SignalChain`과 `MimoPaths`를 받습니다. 입력은 RC 전의 원래 메인 입력 순서이고 출력은 메인 다음 서브입니다. FIR support만 받으며 IIR support를 조용히 빼지 않습니다. support의 HP나 지연이 다르면 다른 묶음으로 만들고, support의 합에 서브 고정 필터를 한 번 겁니다. `process_with_base`의 base는 이미 서브 고정 필터까지 처리한 기본 출력이므로 고정 필터를 다시 걸지 않습니다.

### 저율 실행(D42)

`PlaybackMode::Multirate { low_block_size: 32, factor: 80 }`는 600 Hz 저율 FIR을 직접 실행합니다. 기본 원형 필터는 대칭 Hamming FIR 5,121탭, 차단 240 Hz이고 입력 데시메이터와 출력 인터폴레이터가 함께 씁니다. 변환기마다 군지연이 2,560샘플이라 합은 5,120샘플(106.67 ms)이고, base 출력도 그만큼 늦춰 합칩니다. 호스트의 블록 버퍼링, 설계된 FIR·출력 지연은 여기에 따로 더해집니다. support가 없는 사슬은 FFT도 추가 지연도 없습니다.

지금 내보내는 8,000탭 FIR은 주파수 응답에 감쇠를 곱하고 역변환한 뒤 자른 것이라, 다중 레이트 경로의 두 변환 필터와 같은 전달함수가 아닙니다. 그래서 이 모드는 기본값이 아니고 호환 내보내기로 보지 않습니다. 실제 시스템에 쓰려면 통과대역 리플, 앨리어싱, 대역 경계, 지연을 따로 확인해야 합니다.

### 오프라인 예제

`render_mimo`는 `{"chain": <SignalChain>, "mimo": <MimoPaths>}` JSON이나 v4·v5 `secs_run_cache.json`을 받습니다. PCM은 헤더 없는 48 kHz 인터리브 little-endian f64입니다. 출력의 채널 순서와 지연은 stdout JSON으로 알려 줍니다.

```sh
# 기본: 호환 방식으로 펼친 f64 FIR. MIMO 기여분만 출력합니다.
cargo run --release --locked -p secs-filters --example render_mimo -- \
  plan.json raw-mains.f64 support-full.f64 --block 1024

# 따로 계산한 base 출력과 합칩니다. base의 더 긴 잔향도 남습니다.
cargo run --release --locked -p secs-filters --example render_mimo -- \
  plan.json raw-mains.f64 combined-low.f64 \
  --multirate --factor 80 --block 32 --base rendered-base.f64
```

기본 꼬리 길이는 FIR·변환기·명시적 지연의 꼬리에 IIR 안정 구간 96,000샘플을 더한 것입니다. IIR 꼬리는 끝이 없으므로 필요하면 `--tail-frames`로 정합니다. 48 kHz 계산은 WAV의 f32 양자화 전 f64 계수를 씁니다. 이미 있는 출력 파일은 덮어쓰지 않고, 실행 중 오류가 나면 일부만 쓴 파일이 남을 수 있습니다. 이 예제는 참조 소비자이며, CamillaDSP YAML에 행렬 컨볼루션이나 여러 샘플레이트 분기를 넣지 않습니다.

## 9. 다시 돌리는 법

품질 비교와 렌더링에 쓴 스크립트는 저장소에 넣지 않았습니다. 같은 확인은 아래 검사와 CI가 지킵니다.

```sh
cargo test --locked -p secs-engine --lib rc_phase_candidates_share_preparation
cargo test --locked -p secs-engine --lib xo_partial_metrics_match_full
cargo test --locked -p secs-engine --lib iir_
cargo test --locked -p secs-dsp --lib
cargo test --locked -p secs-camilla
cargo test --locked -p secs-filters
cargo test --locked -p secs-service --lib cache::tests
cargo test --locked -p secs-engine --lib engine_e2e_spread
cargo test --locked -p secs-engine --lib xo_recommendation_production_envelope
cargo test --locked -p secs-engine --lib mimo_iir_joint_costs_within_spread

# 측정
SECS_PERF_THREADS=1 cargo run -p secs-engine --release --example engine_perf -- design dolby_linear
SECS_PERF_THREADS=4 cargo run -p secs-engine --release --example engine_perf -- recommend room
SECS_PERF_THREADS=4 cargo run -p secs-engine --release --example engine_perf -- mimo_iir big
```
