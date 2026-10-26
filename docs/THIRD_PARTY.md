# 가져온 코드와 묶여 나가는 것

## 다른 저장소에서 가져온 코드

SECS 2R(GPL-3.0-only)에 원래 라이선스를 지닌 채 들어온 코드입니다. 각 파일 머리에도 출처를 적습니다. Impulcifer-pip313은 예전 이름이 Impulcifer-py313이라 파일 머리에는 그 이름으로 적힌 곳이 있습니다.

| 출처 | 라이선스 | 들어온 곳 | 고지 |
|---|---|---|---|
| [SECS 2](https://github.com/Kim-O-O/Sound-Equalization-and-Correction-System-2) | GPL-3.0 | 저장소 전체의 기반입니다. 엔진, 서비스, 파일 형식은 업스트림 파이썬 코드를 옮겼습니다. 앱 아이콘은 `apps/secs2r/icons/render_icons.py`가 업스트림 `src/secs/wizard/assets/`의 `app.ico`, `app_256.png`(그대로)와 `logo_b8.svg`(큰 크기)로 만들고, 창 아이콘 `icons/window/`는 이 저장소의 `icons/logo/civilian.svg`로 만듭니다. | 업스트림, `LICENSE` |
| [Impulcifer-pip313](https://github.com/115dkk/Impulcifer-pip313) | MIT | `crates/secs-dsp/src/{fft,optimize,iir,peaks,numeric,response,grid,smooth,minphase,lowrate,fir}.rs`: 원본 `crates/impulcifer-dsp/src/{fft,optimize,filters,peaks}.rs`, `stages/room_tuning2/{primitives,peq}.rs` | `crates/secs-dsp/THIRD_PARTY_LICENSES.txt` |
| [Impulcifer-pip313](https://github.com/115dkk/Impulcifer-pip313/tree/cac2217) 커밋 `cac2217` | MIT | `crates/secs-audio/src/{audio,session,policy,null_backend,cached_backend}.rs`: `impulcifer-types/src/audio.rs`와 `impulcifer-audio-io/src`의 같은 이름 파일; `estimator.rs`: `impulcifer-dsp/src/estimator.rs`의 지수 스윕과 Farina 역필터를 고정 샘플 수, 대역, 레벨에 맞춤; `tests/{session_fake,null_backend,estimator}.rs` | `crates/secs-audio/THIRD_PARTY_LICENSES.txt` |
| [Impulcifer-pip313](https://github.com/115dkk/Impulcifer-pip313/tree/cac2217) 커밋 `cac2217` | MIT | `crates/secs-sys/src/wasapi.rs`: `impulcifer-sys-win/src/lib.rs`; `crates/secs-sys/src/cpal.rs`: `impulcifer-audio-io/src/cpal_backend.rs`; `crates/secs-sys/examples/hardware_probe.rs`: `impulcifer-sys-win/examples/hardware_probe.rs`. 저장소 인터페이스와 Markdown 실측 보고서에 맞췄습니다. | `crates/secs-sys/THIRD_PARTY_LICENSES.txt` |
| [Impulcifer-pip313](https://github.com/115dkk/Impulcifer-pip313/tree/cac2217) 커밋 `cac2217` | MIT | `crates/secs-service/src/jobs.rs`: `crates/impulcifer-jobs/src/registry.rs`의 잠금 복구, 취소 토큰, 패닉 격리 방식을 잡 무리와 푸시 이벤트에 맞춤 | 파일 머리 |
| [Impulcifer-pip313](https://github.com/115dkk/Impulcifer-pip313) | MIT | 테스트 도구이며 출하하지 않습니다. `oracle/common.py`(골든 인코딩, libm 확인, 끼어들기)와 `oracle/trf_trace.py`, `oracle/export_dsp.py`의 TRF 관찰 방식: `tests/migration/export_goldens_secs2.py`; `crates/secs-oracle`(골든 읽기, libm 탐침): `crates/impulcifer-dsp/tests/golden_secs2_helpers.rs` | 파일 머리 |
| [SciPy 1.18.1](https://github.com/scipy/scipy) | BSD-3-Clause | `crates/secs-dsp/src/optimize.rs`, `optimize/{normal,operator}.rs`: `optimize/_lsq/{trf,common}.py`와 `_numdiff.py`의 경계 있는 TRF와 2점 차분(Impulcifer 이식을 거쳐 들어옴); `optimize/de.rs`: `optimize/_differentialevolution.py`의 best1bin, LHS, 즉시 갱신, `x0` 처리 | `crates/secs-dsp/THIRD_PARTY_LICENSES.txt` |
| [NumPy 2.5.3](https://github.com/numpy/numpy/tree/v2.5.3) | BSD-3-Clause (MT19937: Jean-Sebastien Roy, Makoto Matsumoto·Takuji Nishimura의 BSD형 고지) | `crates/secs-dsp/src/random.rs`: 레거시 `RandomState`의 정수 시드, MT19937, `random_sample`, `uniform`, 마스크 `randint`, `shuffle`·`permutation`(`mtrand.pyx`, `_mt19937.pyx`, `src/mt19937/`, `src/distributions/distributions.c`) | `crates/secs-dsp/THIRD_PARTY_LICENSES.txt`, 파일 머리 |
| [Howard Hinnant, chrono-Compatible Low-Level Date Algorithms](https://howardhinnant.github.io/date_algorithms.html) | 공개 영역 | `crates/secs-service/src/clock.rs`: `civil_from_days`(내보내기 폴더 이름의 날짜 계산) | 파일 머리 |

## 실행 파일에 묶여 나가는 의존성

`cargo deny check licenses`가 러스트 의존성에 GPL-3.0과 호환되는 라이선스만 허용합니다(`deny.toml`). 설치본에 들어가는 라이선스 문서는 지금 SECS 2R의 `LICENSE` 하나이고, 러스트 크레이트와 npm 패키지의 고지 목록은 아직 함께 넣지 않습니다.

업스트림의 `THIRD_PARTY_NOTICES.md`는 SECS 2(PyQt6 등)의 고지이고 SECS 2R의 실행 파일과는 관계가 없습니다. 업스트림 파일이라 고치지 않고 그대로 둡니다.

## 묶여 나가는 글꼴

화면이 쓰는 글꼴은 `apps/secs2r/ui/src/fonts/`에 woff2로 들어 있고 설치본에 함께 나갑니다(`docs/DESIGN.md` §4). 라이선스 원문은 같은 폴더에 있고, 파일마다의 출처·버전·SHA-256은 `manifest.json`에 있습니다.

| 글꼴 | 라이선스 | 들어온 곳 | 비고 |
|---|---|---|---|
| [Pretendard](https://github.com/orioncactus/pretendard) 1.3.9 (Kil Hyung-jin) | OFL-1.1, 예약 글꼴 이름 'Pretendard' | `PretendardVariable.woff2`: npm `pretendard@1.3.9`의 공식 variable woff2를 고치지 않고 그대로 | 모든 스킨의 라벨·본문. 이름이 예약되어 있어 서브셋을 뜨지 않음 |
| [Sarasa Gothic](https://github.com/be5invis/Sarasa-Gothic) 1.0.42 (Renzhi Li; Iosevka와 Source Han Sans 기반) | OFL-1.1 | `SarasaMonoK-{Regular,SemiBold,Bold}.woff2`: 'Sarasa Mono K' 면을 `apps/secs2r/ui/scripts/build-fonts.py`로 서브셋(라틴·기호·호환 자모·한글 음절 전부, 합자 제거) | Matrix 스킨의 판독값·좌표·코드 |
| [Orbit](https://github.com/JAMO-TYPEFACE/Orbit) 1.000 (JAMO, 조수운) | OFL-1.1 | `Orbit-Regular.woff2`: 저장소의 TTF를 같은 스크립트로 서브셋 | Matrix 스킨의 띠 활자 |

## 별도로 설치하는 GPU 런타임

선택적 GPU 기능은 NVIDIA CUDA 13 런타임, cuBLAS, cuSOLVER의 공개 ABI를
호출합니다. NVIDIA SDK 헤더와 DLL/SO는 저장소나 기본 설치본에 포함하지
않으며, 사용자가 설치한 런타임을 동적으로 읽습니다. 설치 구성과 적용 범위는
[`GPU.md`](GPU.md), 자체 작성한 네이티브 경계는 ADR 0008을 참조합니다.
동적 로더인 `libloading` 0.9.0은 Cargo 의존성과 라이선스 검사에 포함됩니다.
