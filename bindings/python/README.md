# secs2r

`secs2r` exposes the SECS 2R room-correction engine and its authenticated local API client to Python. It accepts and returns the same JSON-shaped dictionaries as the desktop application. Native audio-device measurement and GPU acceleration are not included.

## Install

```console
python -m pip install secs2r
```

Python 3.9 or newer is required.

## Embedded engine

```python
from secs2r import Engine

engine = Engine()
info = engine.call({"method": "bootstrap"})
print(info["data"]["version"])
```

Long-running requests return a job identifier. Use `wait_job` for its terminal event or iterate over `events()` to receive every later job event.

## Running application

Enable the local API in the SECS 2R application, then read its status with the default token file:

```python
from secs2r import Client

client = Client()
status = client.status()
print(status["app_name"], status["app_version"])
```

You may pass `token` directly or use `token_path` for another token file.

## Licence

`secs2r` is licensed under AGPL-3.0-only (GNU Affero General Public License v3), like the Rust crates it is built from. If you modify it and offer it to users over a network, you must offer those users the corresponding source. The SECS 2R desktop application remains GPL-3.0-only. Repository: <https://github.com/115dkk/Sound-Equalization-and-Correction-System-2R>.

## 한국어

`secs2r`는 SECS 2R의 방 보정 엔진과 실행 중인 앱의 로컬 API 클라이언트를 Python에서 부르는 패키지입니다. 데스크톱 앱과 같은 JSON 모양의 `dict`를 주고받으며, 장치 측정과 GPU 가속은 들어 있지 않습니다. Python 3.9 이상이 필요합니다.

```python
from secs2r import Engine

engine = Engine()
info = engine.call({"method": "bootstrap"})
print(info["data"]["version"])
```

판은 crates.io의 공개 Rust 크레이트와 같습니다. 라이선스는 AGPL-3.0-only(GNU Affero GPL v3)이며, 네트워크로 사용자에게 서비스하는 수정판은 그 사용자에게 소스 코드를 제공해야 합니다. SECS 2R 데스크톱 앱은 지금처럼 GPL-3.0-only입니다.

### 도움 주신 분들

SECS는 한국 오디오 커뮤니티 스피커 갤러리의 Sound Equalization and Correction System 프로젝트에서 시작했습니다. 그 뒤를 이은 [SECS 2](https://github.com/Kim-O-O/Sound-Equalization-and-Correction-System-2)가 소스를 GPL v3로 공개한 덕분에 SECS 2R과 이 패키지가 있습니다. 보정 계산은 SECS 2의 방법을 그대로 따릅니다. SECS와 SECS 2를 만든 스피커 갤러리 여러분과 업스트림 저작자께 감사드립니다. SECS 2의 저작자 KIM-O-O(스피커 갤러리 김ㅇㅇ)님은 이 패키지와 이 패키지가 쓰는 Rust 크레이트를 AGPL-3.0-only로 내도록 허락해 주셨습니다(저장소의 `docs/licensing/SECS2-AGPL-PERMISSION.md`).
