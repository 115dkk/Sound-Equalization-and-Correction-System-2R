# SECS 2R 아키텍처

이 문서가 SECS 2R 구조의 정본입니다. 결정의 근거는 [`docs/adr/`](adr)에 있고, 낱말의 뜻은 [`CONTEXT.md`](../CONTEXT.md)에 있습니다. 여기 적힌 크레이트 경계, seam, 의존 방향은 작업서가 그대로 따라야 하는 계약입니다.

## 1. 원칙

1. **업스트림의 모든 기능을 옮깁니다.** 기능 목록은 `features.toml`이고, 업스트림 출처는 각 항목의 `upstream`에 적습니다. 업스트림 파이썬 트리는 고치지 않고 오라클로 씁니다(0001).
2. **먼저 그대로, 그다음 고칩니다.** 이식은 골든을 맞추고, 결함 수정은 번호 붙은 편차로 따로 넣습니다(0004).
3. **unsafe는 격리 크레이트에만.** OS 경계 `secs-sys`와 선택적 GPU 경계 `secs-cuda-sys`의 예산을 정책으로 검사합니다(0002, 0008).
4. **결정적인 계산.** 같은 입력이면 스레드 수와 무관하게 같은 바이트가 나옵니다. GPU 경로만 허용오차로 다루고 그 사실을 결과에 적습니다(0007).
5. **기능마다 테스트.** `features.toml`에 없는 기능은 없는 것이고, '구현됨'에는 실제 테스트가 있어야 합니다.

## 2. 워크스페이스

```text
Cargo.toml                워크스페이스 루트 (lint, 공용 의존성)
rust-toolchain.toml       1.99.0 고정
features.toml             기능 등록부
unsafe-budget.toml        크레이트별 unsafe 허용량 (secs-sys = 0, secs-cuda-sys = 80)
deny.toml                 GPL 호환 라이선스, 권고, 출처
UPSTREAM.toml             이식을 마친 업스트림 커밋
crates/
  secs-types/             도메인 어휘(역할, 타겟 곡선, 신호 사슬 값), 오류 코드, IPC 타입 (ts-rs 바인딩 생성)
  secs-dsp/               f64 프리미티브, TRF·차분 진화·RandomState 이식
  secs-compute/           스레드, 메모리 예산, 캐시, 취소, 진행
  secs-engine/            보정 설계 전 단계 (깊은 모듈)
  secs-gpu/               GPU 장치 정책과 이상 FIR f64 RidgeBackend 어댑터
  secs-cuda-sys/          선택적 CUDA/cuBLAS/cuSOLVER 네이티브 격리 경계
  secs-project/           .secs, 이주, 구성·DSP 프로파일·타겟, 측정 계획, .irs 저장소
  secs-jos/               자바 직렬화 그래프 (바이트 동일 왕복)
  secs-mdat/              REW mdat 읽기·만들기
  secs-dirac/             Dirac Live .liveproject의 측정 IR 읽기
  secs-export/            보정 단계·전후 REW mdat 내보내기
  secs-rew/               REW API (Real, Fake)
  secs-audio/             자체 측정 (스윕, 세션, 디컨볼루션, 레벨 체크)
  secs-camilla/           CamillaDSP v4 설정과 FIR
  secs-filters/           필터 값 텍스트와 Equalizer APO 설정
  secs-service/           애플리케이션 서비스 (깊은 모듈)
  secs-sys/               격리 크레이트: WASAPI, cpal, 운영체제 조회
  secs-policy/            게이트 테스트
  secs-oracle/            골든 읽기 (테스트 전용, 출하하지 않음)
apps/
  secs2r/                 Tauri 2 셸 (커맨드 하나)
  secs2r/ui/              React 19 + TypeScript + Vite 화면
  secs2r-cli/             헤드리스 CLI
src/, tests/, tools/      업스트림 SECS 2 (파이썬, 오라클, 고치지 않음)
oracle/                   골든 내보내기 스크립트와 골든 (oracle/README.md)
```

## 3. 의존 방향

의존은 아래로만 흐릅니다. `secs-sys`는 앱만 의존하고, 앱이 그 어댑터를 seam에 주입합니다.

```text
ui ──IPC──> secs2r ─┬─> secs-service ─┬─> secs-engine ─┬─> secs-dsp
                    │                 │                ├─> secs-compute
                    │                 │                └─> secs-types
                    │                 ├─> secs-project ──> secs-types
                    │                 ├─> secs-rew ──────> secs-types
                    │                 ├─> secs-audio ────> secs-dsp, secs-types
                    │                 ├─> secs-mdat ─────> secs-jos, secs-dsp
                    │                 ├─> secs-dirac
                    │                 ├─> secs-export ───> secs-engine, secs-mdat, secs-types
                    │                 ├─> secs-camilla ──> secs-dsp, secs-types
                    │                 └─> secs-filters ──> secs-camilla, secs-dsp, secs-types
                    ├─> secs-gpu ─────> secs-engine (RidgeBackend seam)
                    │        └───────> secs-cuda-sys (선택적 네이티브 라이브러리)
                    └─> secs-sys ─────> wasapi, cpal, OS 조회 크레이트
secs2r-cli ─────────> secs-service, secs-sys, secs-gpu
secs-policy ────────> (파일 시스템만 읽음; IPC 메서드 목록 때문에 secs-types)
```

## 4. seam과 어댑터

어댑터가 둘 이상인 자리에만 trait를 둡니다.

| seam | 인터페이스가 있는 곳 | 어댑터 |
|---|---|---|
| 측정 방식 | `secs-service` | 자체 측정(`secs-audio`), REW(`secs-rew` RealRew), 모의(시뮬레이션 방) |
| 오디오 장치 `AudioBackend` | `secs-audio` | WASAPI(`secs-sys`), cpal(`secs-sys`), 시뮬레이션 방(테스트) |
| REW 전송 | `secs-rew` 안쪽 | HTTP, 기록된 대화(테스트) |
| 이상 FIR의 정칙화 탐색 `RidgeBackend` | `secs-engine` | CPU RidgeSweep(기준), CUDA f64 spectral(`secs-gpu`) |
| 신호 사슬 소비자 | `secs-types`의 사슬 값 | 응답 시뮬레이터(`secs-engine`), CamillaDSP 작성기(`secs-camilla`), 필터 텍스트·Equalizer APO 작성기와 FIR streaming 참조 소비자(`secs-filters`) |
| 메모리 조회 `MemoryProbe` | `secs-compute` | 운영체제(`secs-sys`), 고정값(테스트) |
| 호스트 기능 `Host` | `secs-service` | Tauri(대화상자, 폴더 열기, 알림), 기록(테스트) |

## 5. 엔진

MIMO FIR 서포트는 설계 레이트의 계수(`MimoFilter::FirLowRate { taps_low, fs_low }`)로
보관하고, 48 kHz WAV가 필요한 작성기에서 펼칩니다. 이전의 `Fir { taps_48k }`도 읽습니다
(ADR 0012). `secs-dsp`의 행렬 컨볼루션과 다중 레이트 커널은 장치에 의존하지 않고,
`secs-filters::playback::MimoFirPlayback`와 `render_mimo` 예제가 이 값을 직접 재생합니다.
다중 레이트 재생과 MIMO IIR의 연산자 풀이는 공개 API의 옵션으로만 고르며, 앱 설정과
IPC의 기본 동작은 바꾸지 않습니다.

`secs-engine`의 인터페이스는 한 번의 호출입니다. 측정, 설정, 튜닝, 계산 환경을 넣으면 바뀌지 않는 결과가 나옵니다. 업스트림의 `CorrectionEngine`은 생성자 인자 21개, 생성 뒤 덮어쓰는 속성 5개, `compute()`가 바꾸는 속성 약 25개를 가졌고, 화면·MIMO·내보내기가 그 내부 배열을 직접 읽었습니다. SECS 2R은 이것을 다음 안쪽 모듈로 나눕니다.

| 모듈 | 맡는 일 | 업스트림 출처 |
|---|---|---|
| 측정 | 시간 축 규약(기준 위치의 가장 이른 도착이 0, 리프트, 시프트), 창 프리셋(설계 500 ms, 감쇠 2 s), 도착·온셋, 모든 IR의 샘플레이트 확인, 제대로 된 저속 투영 | `_prepare`, `windowed`, `long_spectra`, `mimo.to_low` |
| 타겟 | 타겟 곡선과 레벨 기준, 격자마다 한 번 계산 | `TargetCurve`, `target_fn` |
| 필터·PEQ 맞춤 | RBJ, LR/BW 원형, 올패스, PEQ 묶음 응답, 탐욕 배치 + TRF 다듬기, 이름 붙은 한도 묶음(RC, 버스, 묶음, XO, MIMO 경로) | `peaking_response`, `fit_peq`, `_fit_one` |
| BA | 묶음 레이아웃(DBA, 멀티서브), 수동 계획, 차분 진화(MT19937 이식), 인프라 2-way 레벨 맞춤, 버스 레벨·PEQ·지연·극성 | `_layout_stage`, `_align_subs*`, `_match_two_way`, `_bus_*` |
| XO | 크로스오버 그룹, XO 타겟 PEQ, 추천(후보별 점수) | `xo_peq_band`, `recommend_xo`, `xo_scores` |
| RC | 채널별 크기 설계, 위치 공통 초과위상, FIR 실현 사다리, PEQ 재맞춤, 넓은 다듬기, 위상 해상도 선택, 슈뢰더 범위 | `_correction`, `schroeder_hz` |
| 감쇠 | 늦은 에너지 창, 모드 T60, 늦은 봉우리, 해를 끼치지 않는다 가드 (업스트림의 정의 다섯 벌을 하나로) | `_late_db`, `mode_t60`, `_late_peaks` 등 |
| 신호 사슬 | DSP 토폴로지 값 하나 (입력 PEQ·FIR, 분기, 버스, 서브 필터, MIMO 경로, 지연) | `stage_responses`, `export_camilladsp.make_config` |
| MIMO | 서포트 범위·순위, 정규화 이상 서포트(블록 구조 정규 방정식, λ 이분), IIR 또는 FIR 실현, 가드 | `mimo.py`, `mimo_iir.py` |
| 성적표 | 타입 있는 지표(±1 dB 비율, 위상 바퀴, GD 폭, 1/24 GD 봉우리, 앞울림) | `scorecard.py` |
| 오케스트레이터 | 단계 캐시(입력 지문), 취소, 진행, XO 추천의 공유 단계 | `calibration.compute`, `compute_mimo` |

업스트림이 환경 변수로 숨긴 값(`SECS_BOOST_RING_MS`, `SECS_BA_DELAY_MS`, `SECS_BA_EFF`, `SECS_SUB_BUS_FIX`, `SECS_MAIN_W_LO`, `SECS_PHASE_POS`, `SECS_LONG_BOOST_DB`, `SECS_NOBOOST_MS`, `SECS_MIMO_*`)은 명시적 튜닝 값의 필드로 옮기는 대상입니다. 화면에 제공할 때는 해당 기능의 세부 설정에서 펼치며, 사용자 숙련도로 접근을 나누지 않습니다.

## 6. 수치

- f64를 기본으로 하고, 원 IR은 float32로 저장합니다.
- 설계 격자는 48 kHz, N_FFT = 2^17(0.366 Hz)입니다. MIMO 저속 영역은 600 Hz, 720샘플입니다.
- 측정의 샘플레이트 처리 정책은 프로젝트 보정 설정에 있습니다. 기본값은 비48 kHz 입력 거부이며, 선택적 리샘플링은 작업 안에서 계산용 사본만 변환합니다. 변환한 측정은 보정·MIMO·미리보기·내보내기가 공유하고 원본 측정 저장소는 보존합니다(P60, D39).
- NumPy·SciPy와 맞춰야 하는 세부 사항
  - `least_squares`: TRF, 경계 있음, exact 신뢰 영역 해법, `max_nfev`는 잔차 호출만 셉니다. 임펄사이퍼의 `optimize.rs`(MIT, SciPy 1.18.1 이식)를 들여옵니다.
  - `differential_evolution`: best1bin, 세대마다 dither 다시 뽑기, 재조합 0.7, LHS 초기화, `updating='immediate'`, `polish=False`. 정수 시드라 레거시 `RandomState`(MT19937)를 쓰고 난수 소비 순서(LHS, dither, 채움 위치, 섞기, 교차, 경계 밖 재표본)까지 맞춥니다.
  - `butter`/`sosfilt`: 사전 왜곡 쌍선형 변환과 `zpk2sos` nearest 짝짓기. `firwin`은 해밍 창과 DC 정규화. `find_peaks`는 고원 중앙 규칙.
  - 반올림은 짝수 쪽(`round_ties_even`), 합계는 NumPy의 쌍별 합산 순서입니다.
- 골든 형식과 허용 범위는 0004를 따릅니다.

## 7. 서비스와 화면

- 화면 결과는 역할·개수·성적표 중심의 요약입니다. FIR 계수와 DSP 사슬은 Rust의 최신 결과와 내부 캐시에 두며, 완료 이벤트와 작업 이력에는 싣지 않습니다. 보정·MIMO 완료 뒤 요약을 한 번 조회하고, 작업 기록은 완료 32개와 모든 활성 작업으로 제한합니다(0009).
- `secs-service`의 인터페이스는 `call(Request) -> Envelope`와 잡 이벤트입니다. 잡은 종류마다 하나만 동시에 돌고, 취소 토큰이 엔진 안쪽 반복까지 내려갑니다. 결과는 버리지 않고 낡음으로 표시합니다(다시 계산해도 MIMO 결과는 남고 '다시 계산 필요'로 바뀜).
- 화면은 다섯 단계 레일입니다. 시스템(구성·배선) → 연결·점검(REW 경로 또는 자체 측정 장치, 레벨) → 측정·가져오기 → 보정 → 내보내기. 측정 파일(REW mdat, Dirac 프로젝트) 가져오기 경로를 고르면 연결·점검 단계가 나타나지 않습니다.
- 설정은 주 경로와 해당 절에서 펼치는 세부 항목으로 공개합니다. 전역 전문가 표시나 숙련도별 접근 제한은 두지 않습니다. 로고의 '전문가 모드 / 킹반인 모드'는 외형 선택이며 설정 접근과 무관합니다. 설정마다 정본 위치는 하나입니다(P58).
- 설정의 라벨·단위·선택지는 설명 없이도 의미를 알 수 있게 씁니다. 정적 보충 설명은 해당 항목의 도움말로 열고, 현재 상태·추천값·오류·사용 불가 사유·다음 행동은 화면에 유지합니다(P55).
- 앱 안에서 해결할 수 있는 문제는 짧은 상태와 실제 조작 또는 해당 설정으로 이동하는 버튼을 제공합니다. 다른 단계의 위치를 문장으로 안내하지 않습니다. 내부 처리 해명과 자명한 조작 설명은 도움말에 옮겨 쌓지 않고 삭제합니다(P57).
- 결과는 판정 카드(20–200 Hz ±1 dB 비율 전후, 저역 시간 퍼짐, 추가 지연, 위치 간 편차) → 채널별 성적표 → 그래프(응답, 군지연, XO, 서브, 저역 감쇠) 순서로 보입니다.
- 화면 규칙은 make-interfaces-feel-better를 리뷰 게이트로 씁니다(0003).

## 8. 마일스톤

| 단계 | 내용 | 끝나는 조건 |
|---|---|---|
| M0 | 워크스페이스 뼈대, 정책 게이트, 기능 등록부, CI, 릴리스, 업스트림 감시, 빈 셸 | PR에서 모든 검증 잡 통과, main 머지로 세 OS 설치본이 담긴 0.0.x 사전 출시가 실제로 나옴 |
| M1 | 오라클 골든, 형식(jos·mdat, .secs 이주, .irs), 역할 어휘, DSP 프리미티브 1차, Miri·fuzz | 형식 골든 바이트 일치, 프리미티브 골든 통과 |
| M2 | 엔진: 레벨·딜레이, BA, XO, RC, 성적표, 신호 사슬과 CamillaDSP 작성기(사슬 값을 읽으므로 사슬과 함께). 성능 감사 1차 | 데모 방 끝단이 SECS 2 편차 범위 안, 모든 연산에서 파이썬보다 빠름, 취소가 1초 안에 멈춤 |
| M3 | MIMO(이상 FIR, IIR 맞춤, FIR 서포트), 메모리 예산 | MIMO 골든 통과, MIMO 실행(기본 IIR 모드) 5배 이상, 최대 메모리 1 GB 이하 ([`MIMO.md`](MIMO.md) 6절) |
| M4 | 서비스, 잡, 자체 측정, REW, mdat 가져오기, 내보내기 셋, CLI, 무결성 잡 | CLI만으로 샘플 프로젝트 설계·내보내기, 모의 측정으로 흐름 전체 테스트, 하드웨어 프로브 실측 |
| M5 | 새 화면, 디자인 토큰, 성적표, 작업 트레이, 튜토리얼, 한국어·영어 | 업스트림 기능 목록 전부 화면에서 닿음, 화면 리뷰에서 HIGH 없음 |
| M6 | 실장비 검증, 업데이터, 서명, 출시 전 성능 감사, GPU 실험과 어댑터 | 실제 방에서 측정부터 CamillaDSP 적용까지 완주, 1.0.0 |

## 9. 작업 분배

| 담당 | 맡는 일 |
|---|---|
| Claude | 설계, ADR, 작업서, `secs-types`의 시그니처, 게이트, CI 설계, 디프 검토, 게이트 재실행, 커밋과 PR |
| ASTRA (`gpt-6-astra`) | 수치와 외부 규격이 드는 구현: `secs-dsp`, `secs-engine`, `secs-gpu`, `secs-jos`, `secs-mdat`, `secs-dirac`, `secs-rew`; 화면 연결(호출 코드와 상태), 번역 카탈로그, 감사와 성능 감사 |
| Daybreak Blue (`gpt-daybreak-blue`) | `secs-project`, `secs-camilla`, `secs-filters`, `secs-compute`, `secs-service`, `secs2r-cli`, `secs-sys`, 오라클 내보내기 스크립트, CI 보조 스크립트 |
| Opus 5.5 | 화면 전부: 디자인 토큰, 뷰, 컴포넌트, 레이아웃, 튜토리얼 화면 |

작업서는 `docs/packets/`에 둡니다. 워커의 자기 보고는 게이트가 아니며, 모든 결과는 Claude가 포맷·린트·테스트를 다시 돌린 뒤 커밋합니다.
