# MIMO 이식 계획 (M3)

업스트림 `mimo.py`, `mimo_iir.py`, `calibration.compute_mimo`와 그 내보내기를 `secs-engine`, `secs-dsp`, `secs-camilla`, `secs-export`로 옮기는 방법입니다. 엔진의 원칙(설계는 바뀌지 않는 값, 단계 경계 골든, SECS 2 자신의 편차로 판정)은 [`ENGINE.md`](ENGINE.md)를 그대로 따르고, 계산과 메모리의 결정은 [ADR 0007](adr/0007-compute-and-gpu.md)에 있습니다. 여기서는 MIMO에만 해당하는 것을 정합니다.

## 1. 무엇을 옮기나

MIMO는 서브와 메인을 서로의 "서포트"로 써서 방의 늦은 에너지(모드)를 줄입니다. 입력 하나마다 다른 스피커로 가는 경로 필터를 설계합니다.

1. **서포트 범위** (`support_ranges`): 스피커마다 위치 평균 파워의 1/3옥타브 스무딩에서 기준보다 10 dB 낮아지는 아래·위 끝을 찾습니다. 서브의 통과 대역, 사용자 하한, 제외 목록, 그룹을 따릅니다.
2. **이상 FIR 서포트** (`design_mimo`): 600 Hz 저속 영역(720샘플)에서 늦은 구간 가중 최소제곱으로 경로 FIR(180탭, FIR 모드는 70탭)을 풉니다. 서포트 레벨이 목표(−12 dB)에 맞도록 정칙화 λ를 로그 이분(19번 풂)으로 정하고, 대역 늦은 에너지와 모드 T60이 나빠지면 3 dB씩 낮춥니다(최대 다섯 단계, 그다음 끔). 서포트가 상한보다 많으면 하나씩 따로 풀어 순위를 매깁니다(`_rank_supports`).
3. **IIR 실현** (`design_mimo_iir`): 경로마다 이상 FIR을 PEQ 9개 + 올패스 3개 + 게인·지연·극성으로 맞춘 시작값(`_fit_one`)에서, 입력 하나의 모든 경로를 MIMO 목적 그대로 함께 맞춥니다(`_solve_problem`, 해석적 야코비안, `x_scale='jac'`, 최대 150번). 레벨 초과를 낮추고, 위반이 남으면 경로를 탐욕적으로 빼고, 3 dB씩 낮추고, 끕니다.
4. **FIR 실현**: FIR 모드에서는 이상 FIR(70탭)을 그대로 48 kHz FIR(8000탭)로 올립니다(`fir_to_48k`).
5. **적용 곡선과 내보내기**: 기준 위치의 보정 후 응답에 경로 응답을 더한 `spl_mimo` 곡선, CamillaDSP의 경로 채널(`mimo_fork`, `mimo_merge`)과 경로 필터(IIR 바이쿼드 또는 Conv wav), mdat의 "4 MIMO 적용" 단계와 "보정 후 + MIMO" 항목.

업스트림의 쓰이지 않는 함수(`apply_mimo`, `apply_iir`, `design_mimo`의 `ref`)는 옮기지 않습니다.

## 2. 모양

| 모듈 | 파일 | 업스트림 | 작업서 |
|---|---|---|---|
| 서포트 범위·순위·이상 FIR·가드 | `secs-engine/src/mimo/design.rs` | `support_ranges`, `support_path`, `design_mimo`, `_rank_supports`, `_worst_band_db`, `worst_t60_change` | P14 |
| 정규 방정식 TRF | `secs-dsp/src/optimize.rs` | SciPy `trf_bounds`(exact) | P15 |
| IIR 실현 | `secs-engine/src/mimo/iir.rs` | `IirSupport`, `_fit_one`, `_solve_problem`, `design_mimo_iir` | P16 |
| 실행과 적용 곡선 | `secs-engine/src/mimo/run.rs` | `compute_mimo`, `MimoRun`, `mimo_availability`, `mimo_kind`, `mimo_*_text`, `fir_to_48k`(FIR 서포트는 여기서 한 번 48 kHz로 올립니다) | P17 |
| 경로 값 | `secs-types::chain` | `mimo_paths`, `FirSupport`, `IirSupport` | P17 |
| 내보내기 | `secs-camilla`, `secs-export` | `make_config`의 MIMO 부분, `calibration.export_camilladsp`의 메모, mdat MIMO 항목 | P17 |
| 성능·메모리 감사 | `docs/perf/mimo.md` | — | P18 |
| GPU 이상 FIR 정칙화 탐색 | `secs-gpu`, `secs-cuda-sys` | — | P53 / ADR 0008 |

엔진 바깥 인터페이스는 `mimo::compute(설계, 측정, 설정, 튜닝, 계산 환경) -> MimoRun` 하나입니다. MIMO는 보정 설계가 끝난 뒤 따로 돌고(업스트림도 별도 버튼), 설계 값을 바꾸지 않습니다. 업스트림이 엔진 객체에서 읽던 것(`long_spectra`, `input_response`, `stage_responses()['final']`, `align_delay_ms`, `chain[i]['F']`, `sub_passband`, 그룹)은 모두 `Design`과 측정 준비에서 얻습니다.

## 3. 계산의 결정

ADR 0007의 "같은 수학의 다른 분해"를 MIMO에 이렇게 씁니다. 모두 결과를 바꾸지 않는 최적화이고, 업스트림도 BLAS 순서에 따라 마지막 비트가 바뀌므로 판정은 SECS 2 자신의 편차로 합니다(4절).

- **AᵀA를 설계 행렬 없이 만든다.** 업스트림은 2·P·720 × R·taps 행렬 `A`(위치 P, 서포트 R; 0.5–1.3 GB)를 만들어 `A.T @ A`를 구합니다. `A`의 열 묶음은 경로 신호를 한 칸씩 민 토플리츠이고 행 가중치는 위치마다 온셋+60 ms 앞(√10)과 뒤(1)로 나뉜 계단이므로, 블록 `(j, j')`의 원소 `M[k+1, l+1]`은 `M[k, l]`에서 끝 항 하나와 계단 경계 항 하나만 바꿔 얻습니다. 첫 행과 첫 열만 시간 합으로 구하고(시간 순서대로) 나머지는 대각선을 따라 점화식으로 채웁니다. 블록마다 O(taps² + taps·n)이고, 메모리는 AᵀA(서포트 12개에서 37 MB)뿐입니다. 테스트가 작은 사례에서 밀집 행렬로 만든 AᵀA와 상대 1e-10 안인지 확인합니다.
- **λ마다의 풀이는 촐레스키.** `AᵀA + λI`는 λ > 0에서 양의 정부호이므로 `np.linalg.solve`(LU) 대신 faer의 촐레스키(`Par::Seq`)로 풉니다. 분해가 실패하면 부분 피벗 LU로 다시 풉니다. 같은 수학이고 비용은 LU의 절반입니다.
- **IIR 맞춤은 야코비안을 쌓지 않는다.** 업스트림은 (2·P·720 + 1 + 2·R·721 + R·K) × 35·R 야코비안을 매번 만들어 SVD합니다(입력 하나 236–355초). 경로 r의 열은 위치마다 `irfft(G[p,r]·dS_r)`라 JᵀJ의 블록 `(r, r')`은 위치별 시간 신호의 가중 내적의 합이고, 노력 행과 정칙 행은 대각 블록에만 들어갑니다. P15의 TRF는 야코비안 대신 **JᵀJ와 Jᵀf를 주는 공급자**를 받고, 신뢰 영역 부분 문제는 `J_hᵀJ_h + diag(diag_h)`의 고유분해로 풉니다(SVD의 `s²`, `V`, `s·uf = Vᵀ J_hᵀ f`가 그대로 나옵니다). `x_scale='jac'`의 열 노름은 JᵀJ의 대각에서, 이차 모형 값은 `pᵀ(JᵀJ)p`에서 얻습니다. 고유값이 `s²`라 상대 정밀도가 `√ε` 수준인 거의 특이한 방향에서는 SVD와 걸음이 달라질 수 있습니다. 이것도 편차 판정 안에서 봅니다.
- **병렬은 입력 사이에서만.** 입력마다의 IIR 맞춤과 이상 FIR 설계를 `secs-compute`의 순서 보존 map으로 나누고, 입력 안의 누적은 순차입니다(D36을 이것으로 닫습니다). 각 작업은 추정 바이트만큼 메모리 허가를 받고 시작합니다. 요청마다 다시 만드는 500 ms·2 s 스펙트럼(IR 칸마다 한 벌씩)도 만들기 전에 두 벌이 함께 있는 최대량을 예약하고, 500 ms 벌을 버린 뒤에는 2 s 벌의 몫만 준비를 버릴 때까지 쥡니다. 입력별 허가는 그 나머지에서 나옵니다. 업스트림의 프로세스 풀, 낮은 우선순위, 프로세스 강제 종료(D17)는 스레드와 협조적 취소로 바꿉니다. 취소는 λ 이분 한 걸음, TRF 평가 한 번마다 확인합니다.
- **SIMD는 pulp로.** 바이쿼드·올패스 응답과 복소 곱 누적만 대상입니다(ADR 0007).

## 4. 골든과 판정

새 오라클 가족 둘을 만듭니다. 사례는 위치가 넷인 `room` 묶음(`room`, `room_dba`, `room_filters`, `room_multi`)에 MIMO 설정 변형(서포트 상한 2, FIR 모드, 사용자 하한, 서브 제외)을 더하고, 성능 감사용으로 위치 여섯의 큰 사례(7.1.4 + 서브 둘) 하나를 `engine_cases.py`에 더합니다.

- **`engine_mimo`** (P14): 사례마다 서포트 범위, 입력마다 후보 순서와 순위 점수, λ 이분의 궤적(중간값, 레벨 비, 받아들였는지), 최종 λ와 레벨, 서포트 FIR, 가드 단계(배율, 대역 최악 dB, T60 변화), 노트를 기록합니다. 같은 사례를 여덟 벌(기준, BLAS 커널 둘, NumPy SIMD, IR을 1 ulp 흔든 넷)로 돌려 값마다 폭을 잽니다.
- **`engine_mimo_iir`** (P16): 입력마다 맞춤 문제(`pb`: X, WL, WE, G, LP, 이상 FIR, 범위, 모드, 레벨 목표)와 경로별 시작값, 최종 x와 비용, `nfev`·`status`, 시작점의 잔차·JᵀJ·Jᵀf, 후처리(레벨 낮춤, 뺀 경로, 3 dB 단계)를 기록합니다. 시작값 맞춤과 함께 맞춤을 각각 열두 벌(기준, BLAS 커널 셋, SIMD, 목표를 1 ulp 흔든 여덟)로 돌려 비용과 `nfev`의 폭을 잽니다.

판정은 M2와 같습니다.

- 범위, 후보, 순위, 이름, 구조, 경로 수는 단언합니다(오라클 밖 libm에서는 ENGINE.md 3절의 규칙대로 문턱 뒤의 값을 보고만).
- 잔차·JᵀJ·Jᵀf 같은 함수 값은 기록한 입력에서 상대 1e-10으로 단언합니다.
- 맞춤의 끝값은 P07c 규칙입니다. Rust 비용이 기준 + 1.25 × 폭(바닥 1e-6 × 기준)을 넘지 않아야 하고, 상한에서 멈춘 맞춤은 오라클 libm에서만 단언하며, Rust가 열두 벌 모두보다 나쁜 호출 수는 이항 99.9% 한도 안이어야 합니다.
- 서포트 FIR, 레벨, 가드 결과, 적용 곡선은 ADR 0004의 '끝단 판정의 통계 규칙'으로 봅니다.
- CamillaDSP 설정 문자열과 FIR wav는 기록한 `MimoRun`을 넣어 바이트 일치를 봅니다(P11과 같은 방식).

## 5. 순서

```text
P14 이상 FIR 설계·가드 ─┬─────────────── P16 IIR 실현 ─┬─ P17 실행·경로 값·내보내기 ─ P18 감사
P15 정규 방정식 TRF ─────┘                               └─ P19 GPU 실험
```

P14와 P15는 바로 나란히 돌고, P16은 둘이 들어온 뒤 돕니다. P17의 경로 값과 CamillaDSP 부분은 기록한 결과로 확인하므로 P16과 나란히 시작할 수 있습니다. P19(GPU)는 P16의 CPU 경로를 기준으로 재며, ADR 0007대로 3배 이상 빠를 때만 들어갑니다.

2026-10-19의 [ADR 0008](adr/0008-native-gpu-acceleration.md)과 P53은 P19의
초기 범위를 대체합니다. 실측 결과에 따라 이상 FIR의 큰 f64 선형계부터
GPU로 옮기며, CPU λ 선택과 음향 가드를 유지합니다. 최종 검증에서 IIR의
비선형 맞춤이 작은 목표 차이를 증폭해, 출하 GPU 적용은 FIR 모드로
한정합니다. IIR 모드는 목표 설계부터 CPU 기준 계산을 유지합니다.

## 6. 끝나는 조건

[`ARCHITECTURE.md`](ARCHITECTURE.md) 8절의 M3 조건을 이렇게 잽니다.

- **MIMO 골든 통과**: 4절의 판정이 세 운영체제 CI에서 통과합니다.
- **MIMO 실행 5배 이상**: 큰 사례에서 사용자가 기다리는 기본 MIMO 실행(`mimo::compute`, IIR 모드)이 같은 기계의 SECS 2보다 다섯 배 이상 빠릅니다(출하 설정과 한 스레드 설정 모두, P13의 측정 방법). 측정한 어느 MIMO 연산도 SECS 2보다 느리지 않습니다.
- **최대 메모리 1 GB 이하**: 큰 사례의 `mimo::compute` 전체가 프로세스 트리 PSS 1 GB를 넘지 않고, SECS 2보다 적습니다.
- 업스트림 MIMO 테스트(`tests/test_calibration_mimo_fir_1012.py`, `test_calibration_r28_mimo_cap.py`, `test_mimo_band_limit_1012.py`, `test_export_camilladsp_mimo_delay.py`, `test_calibration_r27_sub_manual.py`의 MIMO 부분)가 확인하는 동작을 Rust 테스트가 모두 확인합니다.

**기준 개정(2026-10-14, 유지보수자 결정).** 처음 기준은 "이상 FIR 설계와 IIR 맞춤을 입력 하나씩 잰 시간 5배"였습니다. P18 감사([`perf/mimo.md`](perf/mimo.md))에서 이상 FIR 설계는 밀집 행렬 분해가 대부분이고, SECS 2는 여기에 다중 스레드 LAPACK을 쓰는 반면 이 포트는 결과가 스레드 수와 무관하도록 분해를 순차로 한다는 것(ADR 0007)이 드러났습니다. 그래서 이 단계만 떼어 비교하면 λ 탐색을 한 번의 삼중대각화로 바꾸고(P18b) 공통 준비를 한 번으로 줄인(P18c) 뒤에도 5배가 나오지 않습니다. 기준을 사용자가 기다리는 실행 단위로 바꾸고, 남는 미달을 그대로 적습니다.

- 남는 미달: 입력 하나의 이상 FIR 설계(180탭) 2.3배(한 스레드)·1.5배(출하), FIR 모드 전체 3.4배(한 스레드). FIR 모드 출하 설정은 6.9배입니다.
- 이 미달은 블록 삼중대각화(P18c Part B, 미실시)와 GPU 실험(P19, M6로 옮김)에서 다룹니다.
