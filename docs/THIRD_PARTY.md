# 가져온 코드와 묶여 나가는 것

## 다른 저장소에서 가져온 코드

SECS 2R(GPL-3.0-only)에 원래 라이선스를 지닌 채 들어온 코드입니다. 각 파일 머리에도 출처를 적습니다.

| 출처 | 라이선스 | 들어온 곳 | 비고 |
|---|---|---|---|
| [SECS 2](https://github.com/Kim-O-O/Sound-Equalization-and-Correction-System-2) | GPL-3.0 | 저장소 전체의 기반, `apps/secs2r/icons/`(업스트림 로고 `logo_b6.svg`에서 생성) | 업스트림 |
| [Impulcifer-py313](https://github.com/115dkk/Impulcifer-pip313) | MIT | `oracle/common.py`(골든 인코딩·libm 확인·끼어들기, `tests/migration/export_goldens_secs2.py`에서), `crates/secs-oracle`(골든 읽기·libm 탐침, `crates/impulcifer-dsp/tests/golden_secs2_helpers.rs`에서) | 테스트 도구, 출하하지 않음 |

M1에서 더 들어올 예정인 것: 임펄사이퍼 3.x의 `optimize.rs`(MIT, SciPy 1.18.1 `least_squares` TRF 이식, SciPy 부분은 BSD-3-Clause)와 SECS 2 이식 코드 일부(MIT).

## 실행 파일에 묶여 나가는 의존성

러스트 크레이트와 npm 패키지의 라이선스 목록은 M6에서 릴리스마다 생성해 설치본에 함께 넣습니다. 지금은 `cargo deny check licenses`가 GPL-3.0과 호환되는 라이선스만 허용합니다(`deny.toml`).

업스트림의 `THIRD_PARTY_NOTICES.md`는 SECS 2(PyQt6 등)의 고지이고 SECS 2R의 실행 파일과는 관계가 없습니다. 업스트림 파일이라 고치지 않고 그대로 둡니다.
