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

`secs2r` is licensed under GPL-3.0-only. A program that links this extension forms a combined work and must use GPL-3.0-only-compatible distribution terms when conveyed to others.
