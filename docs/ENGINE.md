# 엔진 이식 계획 (M2)

`secs-engine`을 업스트림 `correction_engine.py`와 `calibration.py`의 계산 절반, `scorecard.py`에서 옮기는 방법입니다. 모듈 경계는 [`ARCHITECTURE.md`](ARCHITECTURE.md) 5절, 등가와 편차의 원칙은 [ADR 0004](adr/0004-parity-and-deviations.md)를 따릅니다. MIMO(`mimo.py`, `mimo_iir.py`)는 M3입니다.

## 1. 모양

바깥 인터페이스는 한 번의 호출입니다. `design(측정, 설정, 튜닝, 계산 환경) -> 설계`이고, 설계는 바뀌지 않는 값입니다. 업스트림의 `CorrectionEngine`처럼 생성 뒤 속성을 덮어쓰거나 화면이 내부 배열을 읽는 길은 두지 않습니다. 업스트림이 엔진 안쪽을 읽던 자리(XO 미리 보기, 서브 정렬 보기, mdat 단계 IR, MIMO, CamillaDSP)는 모두 설계에 담긴 신호 사슬 값과 측정 스펙트럼으로 응답 시뮬레이터를 돌려 얻습니다.

| 모듈 | 파일 | 업스트림 | 작업서 |
|---|---|---|---|
| 설계 격자 | `grid.rs` | `SR`, `N_FFT`, `freqs()` | 뼈대 |
| 입력과 튜닝 | `input.rs` | 생성자 인자, 생성 뒤 쓰는 속성, `SECS_*` | 뼈대 |
| 설정 정규화 | `setup.rs` | `__init__` 본문 | P05 |
| 측정 | `measurement.rs` | `onset_index`, `windowed`, `spectrum`, `_prepare`, `long_spectra`, 서브 고정 필터, `_delay` | P05 |
| 레벨 | `levels.rs` | `_weights`, `_freq_weights`, `_band_level`, `schroeder_hz`, `_TargetFn` | P05 |
| 맞춤 | `fit.rs` | `fit_peq`, `_ring_q_cap`, `_ring_boxes`, `fit_gd_cuts`, `_xo_target_peq` | P07 |
| 감쇠 | `decay.rs` | BA의 `late_db`, `calibration._late_db`, `mimo.mode_t60`, `mimo._late_peaks` | P08 |
| 성적표 | `scorecard.rs` | `scorecard.metrics` | P08 |
| RC | `rc.rs` | `_correction`, `_no_boost_mask`, `_long_mode_mask`, 메인 반복 | P09 |
| BA | `ba.rs` | `_layout_stage`, `_ba_*`, `_sub_sum`, `_align_subs*`, `_match_two_way`, `_bus_*`, `_term` | P10 |
| 신호 사슬과 시뮬레이터 | `chain.rs`, `secs-types::chain` | `stage_responses`, `input_response`, `_sub_out`, `xo_preview`, `sub_combine`, `stage_items` | P12 |
| XO | `xo.rs` | `xo_peq_band`, `recommend_xo`, `xo_scores`, `pick_xo` | P12 |
| 설계(오케스트레이터) | `design.rs` | `compute()`, `make_engine`, `calibration.compute` | P12 |
| CamillaDSP 작성기 | `secs-camilla` | `export_camilladsp.py` | P11 |
| 계산 환경 | `secs-compute` | (새로) 취소, 진행, 순서 보존 map, 메모리 허가 | P06 |

## 2. 시간 축

업스트림에는 시간 축이 둘 있습니다. 스펙트럼과 진단 응답(`H`, `before`, `after`, `combined`, 사슬 재생)은 **안쪽 축**에 있고, 기준 위치의 가장 이른 도착이 0입니다. DSP에 나가는 지연(`ChannelOut.delay_ms`, `SubAlign.delay_ms`)은 **출력 축**에 있고, 안쪽 축에 공통 시프트(`main_extra_ms`)를 더한 값입니다. 업스트림은 이 구분을 이름으로 드러내지 않았고, mdat 내보내기가 시프트를 빼지 않은 응답에서 전체 지연을 빼는 결함(D24)이 여기서 나왔습니다.

SECS 2R은 두 축을 타입으로 가릅니다. 사슬 값은 출력 축 지연과 시프트를 함께 들고, 시뮬레이터는 어느 축으로 재생할지를 인자로 받습니다. 등가 이식 동안에는 업스트림과 같은 축을 고르고, D24는 그 뒤 편차로 고칩니다.

## 3. 골든: 단계 경계

오라클은 업스트림 함수에 끼어들어 인자와 반환값을 기록합니다(`oracle/common.py`의 `Recorder`, `interpose`). 단계마다 다음을 지킵니다.

- **입력 사례는 한 벌.** `oracle/engine_cases.py`가 모든 엔진 골든이 쓰는 사례(데모 방과 합성 방, 설정 변형)를 만들고, `engine_inputs` 가족이 원 IR과 엔진 설정을 한 번만 저장합니다. 다른 가족은 사례 이름으로 가리킵니다. IR은 float32로 둡니다(실제 측정의 정밀도이고, 합성 IR도 float32로 바꾼 뒤 오라클에 넣습니다).
- **결정은 오라클 것을 넣고, 산술은 Rust가 다시 만든다.** 한 단계의 입력 중 최적화나 이산 선택이 정한 값(PEQ 목록, BA 행, 버스 지연·극성, 레벨 기준 등)은 골든에서 읽어 넣습니다. 그 밖의 큰 배열(전체 격자 스펙트럼)은 이미 골든에 맞춘 앞 단계의 Rust 구현으로 원 IR에서 다시 만듭니다. 1 MiB짜리 스펙트럼을 단계마다 저장하지 않기 위해서이고, 앞 단계가 1e-12 수준으로 맞으므로 판정에 영향이 없습니다.
- **맞춤기는 재생으로 끊는다.** RC·BA·XO 안의 `fit_peq`는 단계 함수가 맞춤기를 인자로 받아, 골든 테스트에서는 오라클이 그 호출에서 돌려준 결과를 돌려주는 재생 맞춤기를 넣습니다(인자가 오라클 인자와 허용 범위 안인지 먼저 확인). `fit_peq` 자체는 P07이 M1의 TRF 게이트 방식(오라클 궤적을 따라 한 걸음씩 재생, 수렴한 맞춤은 결과까지, 상한에서 멈춘 맞춤은 보고만)으로 따로 확인합니다. 차분 진화는 오라클과 같은 libm에서 비트 단위로 같으므로(M1) BA는 그 위에서 끝까지 확인합니다.
- **큰 출력은 표본으로.** 전체 격자 배열은 촘촘한 저역 구간과 성긴 고역 표본으로 저장하고, 비트까지 같아야 하는 정수·짧은 배열은 그대로 저장합니다. 가족마다 크기 예산을 작업서에 적습니다.
- **끝단은 SECS 2 자신의 편차로.** `engine_e2e` 가족은 같은 사례를 오라클 환경 두 벌(BLAS 커널 종류와 NumPy SIMD 디스패치를 바꾼 것)로 돌려 성적표 지표의 차이를 잽니다. Rust의 끝단 결과는 그 차이의 1.25배 안이어야 합니다(ADR 0004). M2의 끝나는 조건인 "데모 방 끝단이 SECS 2 편차 범위 안"이 이 검사입니다.

테스트는 크레이트 안쪽(`crates/secs-engine/src/tests/<가족>.rs`)에 두어 단계 함수를 공개하지 않고도 부를 수 있게 합니다. 사례 읽기는 `src/tests/cases.rs` 하나를 같이 씁니다.

## 4. 순서

```text
뼈대 ─┬─ P05 사례·측정·설정·레벨 ─┬─ P07 맞춤 ─┬─ P09 RC ─┐
      │                            ├─ P08 감쇠·성적표 ┘     ├─ P12 설계·사슬·XO ─ P13 성능 감사
      │                            ├─ P10 BA ───────────────┤
      │                            └─ P11 CamillaDSP 작성기 ┘
      └─ P06 계산 환경 ───────────────────────────────────────┘
```

P05와 P06은 바로 나란히 돌고, P07·P08·P10·P11은 P05의 사례가 들어온 뒤 나란히 돕니다. 단계 경계 골든 덕분에 RC와 BA는 맞춤기가 끝나기 전에도 재생 맞춤기로 확인할 수 있습니다.

## 5. 성능과 취소

- 채널별 RC, XO 추천의 후보, 위치별 스펙트럼 준비는 서로 독립이라 `secs-compute`의 순서 보존 map으로 나눕니다. 부동소수점 누적은 원래 순서대로입니다(ADR 0007). 업스트림은 보정 계산을 한 스레드에서, XO 추천만 프로세스로 돌렸습니다.
- 스펙트럼은 `Arc`로 나눠 쓰고, 업스트림이 별칭으로 공유하던 배열(`H0`와 `H`)은 복사하지 않습니다.
- 취소는 단계, 채널, 맞춤기 평가, DE 세대 경계에서 확인합니다. 맞춤기 클로저는 취소된 뒤 바로 돌아오고, 그 결과는 버립니다. 목표는 취소 뒤 1초 안에 멈추는 것입니다.
- P13이 단계마다 파이썬(같은 기계, 최신 CPython)과 비교 측정합니다. 모든 연산에서 파이썬보다 빨라야 끝납니다.
