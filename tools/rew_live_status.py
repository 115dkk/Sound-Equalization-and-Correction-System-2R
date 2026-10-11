"""실행 중인 REW(-api)의 현재 오디오·측정 설정을 읽기만 한다(소리 안 남).

    .venv-secs\\Scripts\\python.exe tools\\rew_live_status.py
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "src"))
sys.stdout.reconfigure(encoding="utf-8")

from secs.rewapi.rew import RealREW  # noqa: E402
from secs.rewapi.interfaces import RewError  # noqa: E402

PATHS = (
    "/version", "/audio/driver", "/audio/samplerate",
    "/audio/asio/device", "/audio/asio/output", "/audio/asio/input",
    "/audio/asio/timing-reference-output",
    "/audio/java/output-device", "/audio/java/input-device",
    "/audio/java/output", "/audio/java/input",
    "/measure/level", "/measure/sweep/configuration", "/measure/sweep/repetitions",
    "/measure/timing/reference", "/measure/measurement-mode", "/measure/naming",
    "/measure/protection-options", "/application/blocking", "/measurements",
)


def main() -> int:
    rew = RealREW()
    if not rew.is_running():
        print("REW API 에 연결할 수 없다 (roomeqwizard.exe -api)")
        return 1
    for path in PATHS:
        try:
            value = rew.get(path)
        except RewError as error:
            value = f"<오류: {str(error)[:80]}>"
        text = json.dumps(value, ensure_ascii=False)
        print(f"{path:45s} {text[:300]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
