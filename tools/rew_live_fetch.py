"""REW 에 있는 측정의 IR 을 받아 요약한다(소리 안 남).

    .venv-secs\\Scripts\\python.exe tools\\rew_live_fetch.py [uuid]     # 생략하면 마지막 측정
"""
from __future__ import annotations

import json
import sys
from datetime import datetime
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "src"))
sys.stdout.reconfigure(encoding="utf-8")

from secs.rewapi.rew import RealREW  # noqa: E402

BANDS = ((20, 40), (40, 80), (80, 160), (160, 315), (315, 630), (630, 1250),
         (1250, 2500), (2500, 5000), (5000, 10000), (10000, 20000))


def summarize(ir: np.ndarray, rate: int, t0: float) -> dict:
    peak = int(np.argmax(np.abs(ir)))
    size = 1 << int(np.ceil(np.log2(ir.size)))
    spectrum = np.abs(np.fft.rfft(ir, size))
    freqs = np.fft.rfftfreq(size, 1.0 / rate)
    level = {}
    for low, high in BANDS:
        band = (freqs >= low) & (freqs < high)
        level[f"{low}-{high}"] = round(float(
            10 * np.log10(np.mean(spectrum[band] ** 2) + 1e-30)), 1)
    tail = ir[int(ir.size * 0.8):]            # IR 끝 20% = 잡음 바닥
    return {
        "samples": int(ir.size), "sample_rate": int(rate), "t0_s": float(t0),
        "peak_index": peak, "peak_value": float(ir[peak]),
        "peak_abs_time_ms": round((peak / rate + t0) * 1000.0, 4),
        "peak_to_tail_rms_db": round(float(20 * np.log10(
            abs(ir[peak]) / max(np.sqrt(np.mean(tail ** 2)), 1e-15))), 1),
        "band_level_db": level,
    }


def main() -> int:
    rew = RealREW()
    items = rew.measurements()
    if not items:
        print("REW 에 측정이 없다")
        return 1
    wanted = sys.argv[1] if len(sys.argv) > 1 else str(items[-1].get("uuid"))
    meta = next((m for m in items if str(m.get("uuid")) == wanted), {})
    ir, rate, t0 = rew.impulse_response(wanted)
    ir = np.asarray(ir, dtype=np.float64)
    out = {"meta": meta, "ir": summarize(ir, rate, t0)}

    out_dir = ROOT / "보고서" / "04 실측·REW" / f"{datetime.now():%Y-%m-%d} REW 실스윕"
    out_dir.mkdir(parents=True, exist_ok=True)
    np.savez_compressed(out_dir / f"ir_{wanted[:8]}.npz",
                        ir=ir.astype(np.float32), sample_rate=rate, t0_s=t0)
    (out_dir / f"ir_{wanted[:8]}.json").write_text(
        json.dumps(out, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(out, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
