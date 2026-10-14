# 가져온 코드와 묶여 나가는 것

## 다른 저장소에서 가져온 코드

SECS 2R(GPL-3.0-only)에 원래 라이선스를 지닌 채 들어온 코드입니다. 각 파일 머리에도 출처를 적습니다.

| 출처 | 라이선스 | 들어온 곳 | 비고 |
|---|---|---|---|
| [Impulcifer-py313 DSP](https://github.com/115dkk/Impulcifer-pip313) | MIT | `crates/secs-dsp/src/{optimize,iir,peaks,numeric,response,grid,smooth,minphase,lowrate,fir}.rs`: 원본 `crates/impulcifer-dsp/src/optimize.rs`, `filters.rs`, `peaks.rs`, `stages/room_tuning2/{primitives,peq}.rs`; `oracle/export_dsp.py`: `tests/migration/export_goldens_secs2.py`의 TRF 관찰 방식 | 원문 고지: `crates/secs-dsp/THIRD_PARTY_LICENSES.txt` |
| [SciPy 1.18.1](https://github.com/scipy/scipy) | BSD-3-Clause | `crates/secs-dsp/src/optimize.rs`: `optimize/_lsq/{trf,common}.py`, `_numdiff.py`의 bounded exact TRF 및 2-point 차분, Impulcifer 이식을 통해 도입 | 원문 저작권·재배포 조건: `crates/secs-dsp/THIRD_PARTY_LICENSES.txt` |
| [NumPy 2.5.3](https://github.com/numpy/numpy/tree/v2.5.3) | BSD-3-Clause (MT19937: Jean-Sebastien Roy, Makoto Matsumoto·Takuji Nishimura의 BSD형 고지) | `crates/secs-dsp/src/random.rs`: 레거시 `RandomState`의 정수 시드, MT19937, `random_sample`, `uniform`, 마스크 `randint`, `shuffle`·`permutation`(`mtrand.pyx`, `_mt19937.pyx`, `src/mt19937/`, `src/distributions/distributions.c`) | 원문 고지: `crates/secs-dsp/THIRD_PARTY_LICENSES.txt`와 파일 머리 |
| [SciPy 1.18.1](https://github.com/scipy/scipy) | BSD-3-Clause | `crates/secs-dsp/src/optimize/de.rs`: `optimize/_differentialevolution.py`의 best1bin·LHS·즉시 갱신·`x0` 경로 | 원문 고지: `crates/secs-dsp/THIRD_PARTY_LICENSES.txt` |
| [SECS 2](https://github.com/Kim-O-O/Sound-Equalization-and-Correction-System-2) | GPL-3.0 | 저장소 전체의 기반, `apps/secs2r/icons/`(업스트림 로고 `logo_b6.svg`에서 생성) | 업스트림 |
| [Impulcifer-py313](https://github.com/115dkk/Impulcifer-pip313) | MIT | `oracle/common.py`(골든 인코딩·libm 확인·끼어들기, `tests/migration/export_goldens_secs2.py`에서), `crates/secs-oracle`(골든 읽기·libm 탐침, `crates/impulcifer-dsp/tests/golden_secs2_helpers.rs`에서) | 테스트 도구, 출하하지 않음 |
| [Impulcifer-pip313](https://github.com/115dkk/Impulcifer-pip313/tree/cac2217) | MIT | `crates/secs-audio/src/{audio,session,policy,null_backend,cached_backend}.rs`: `impulcifer-types/src/audio.rs`, `impulcifer-audio-io/src`의 동명 파일; `estimator.rs`: `impulcifer-dsp/src/estimator.rs`의 지수 스윕·Farina 역필터를 고정 샘플 수·대역·레벨에 맞춤; `tests/{session_fake,null_backend,estimator}.rs` | 커밋 `cac2217`. 원문 고지: `crates/secs-audio/THIRD_PARTY_LICENSES.txt`. OS 어댑터는 이식하지 않음 |
| [Impulcifer-pip313 jobs](https://github.com/115dkk/Impulcifer-pip313/tree/cac2217) | MIT | `crates/secs-service/src/jobs.rs`: `crates/impulcifer-jobs/src/registry.rs`의 잠금 복구·취소 토큰·패닉 격리 패턴을 잡 무리와 푸시 이벤트에 맞춤 | 커밋 `cac2217`; 원본 저장소 `/home/user/Impulcifer-pip313` |

M1에서 더 들어올 예정인 것: 임펄사이퍼 3.x의 `optimize.rs`(MIT, SciPy 1.18.1 `least_squares` TRF 이식, SciPy 부분은 BSD-3-Clause)와 SECS 2 이식 코드 일부(MIT).

## 실행 파일에 묶여 나가는 의존성

러스트 크레이트와 npm 패키지의 라이선스 목록은 M6에서 릴리스마다 생성해 설치본에 함께 넣습니다. 지금은 `cargo deny check licenses`가 GPL-3.0과 호환되는 라이선스만 허용합니다(`deny.toml`).

업스트림의 `THIRD_PARTY_NOTICES.md`는 SECS 2(PyQt6 등)의 고지이고 SECS 2R의 실행 파일과는 관계가 없습니다. 업스트림 파일이라 고치지 않고 그대로 둡니다.
