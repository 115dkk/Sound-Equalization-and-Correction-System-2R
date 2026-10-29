# 0019. PyPI 패키지 secs2r

상태: 채택, 2026-10-29

## 배경

ADR 0016으로 엔진과 로컬 API 클라이언트를 crates.io에 Rust 라이브러리로 공개했습니다. 유지보수자는 같은 기능을 Python에서도 쓰도록 PyPI에 올리기로 했습니다(2026-10-29). 처음 원한 이름 `secs`는 2017년부터 다른 프로젝트가 쓰고 있어, 비어 있던 `secs2r`로 정했습니다. 내용은 엔진과 로컬 API 클라이언트이고, 첫 업로드는 유지보수자가 준 일회용 토큰으로 한 뒤 신뢰할 수 있는 게시자(trusted publishing)로 바꿉니다.

Python 바인딩은 PyO3로 만듭니다. PyO3 0.30.1의 `#[pymodule]`, `#[pyfunction]`, `#[pyclass]`, `#[pymethods]` 매크로를 실제로 컴파일해 확인했으며, 크레이트 루트의 `#![forbid(unsafe_code)]`와 함께 쓸 수 있습니다.

## 결정

- **크레이트.** `bindings/python`에 `secs-py`를 둡니다. `cdylib`이고 crates.io에는 올리지 않습니다. 공개 Rust API(`sound-equalization-and-correction-system-rust-api`)의 `engine`과 `client` 기능만 쓰며, 운영체제 의존성과 GPU는 넣지 않습니다.
- **unsafe 규칙.** `secs-py`도 다른 비격리 크레이트와 마찬가지로 루트에 `#![forbid(unsafe_code)]`를 두고 작업 공간 린트를 그대로 씁니다. 손으로 쓴 `unsafe`의 예산은 0입니다.
- **Python API.** Rust API처럼 요청을 JSON 모양 그대로 주고받습니다.
  - `secs2r.Engine(app_settings_path=None)`: `call(request: dict) -> dict`, `wait_job(job_id: int, timeout: float) -> dict`, `events()`(작업 이벤트 반복자).
  - `secs2r.Client(port=4712, token=None, token_path=None)`: 실행 중인 앱의 로컬 API. `call`, `status`, `events`.
  - 오류는 `secs2r.SecsError`(코드, 메시지, 세부)로 올립니다. 오래 걸리는 호출은 GIL을 놓고 기다립니다.
  - 타입 힌트(`.pyi`, `py.typed`)를 함께 싣습니다.
- **빌드.** maturin과 abi3(Python 3.9 이상)로, Windows x64, macOS(x86_64와 arm64), Linux manylinux x86_64 휠과 sdist를 만듭니다. PR에서는 바인딩이나 그 의존이 바뀔 때 세 운영체제에서 휠을 빌드하고 가져오기 시험을 돌립니다.
- **판.** PyPI 판은 crates.io 공개 크레이트의 판(`api-v<판>` 태그)과 같습니다. 첫 판은 0.1.0입니다.
- **게시.** 첫 0.1.0은 CI가 빌드한 휠을 받아 유지보수자의 일회용 토큰으로 한 번 올립니다. 그 뒤로는 `release.yml`의 `pypi` 잡이 crates 잡 뒤에 돌며, 환경 `pypi`에서 GitHub OIDC(`id-token: write`)로 `pypa/gh-action-pypi-publish`를 써서 올리고, PyPI에 이미 있는 판은 건너뜁니다. 저장소에는 PyPI 토큰을 두지 않습니다.
- **라이선스.** GPL-3.0-only입니다. 패키지 메타데이터와 README에 적습니다.

## 결과

- Python에서 프로젝트를 열고, 측정을 가져오고, 보정과 MIMO를 계산하고, 내보낼 수 있습니다. 장치 측정과 GPU는 Rust API처럼 빠집니다.
- `secs-py`에도 기존 정책 게이트의 `forbid` 규칙이 그대로 적용됩니다. 손으로 쓴 `unsafe`는 0개이고, 예산을 올리려면 ADR이 먼저입니다.
- 릴리스가 PyPI까지 올리므로 `release.yml`이 길어집니다. PyPI 쪽 신뢰할 수 있는 게시자 등록은 유지보수자가 PyPI 화면에서 합니다(소유자 `115dkk`, 저장소 `Sound-Equalization-and-Correction-System-2R`, 워크플로 `release.yml`, 환경 `pypi`).
