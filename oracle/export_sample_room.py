#!/usr/bin/env python3
"""Export SECS 2's single-position real room without pickle or resampling."""
import json
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "crates/secs-service/data"
OUT.mkdir(parents=True, exist_ok=True)
with np.load(ROOT / "src/secs/rewapi/sample_dolby_714.npz", allow_pickle=False) as data:
    meta = json.loads(str(data["meta_json"]))
    index = {"sample_rate": meta["sample_rate"], "positions": [1],
             "source": meta["source"], "entries": []}
    payload = bytearray()
    for role, t0 in zip(meta["roles"], data["t0"]):
        ir = np.asarray(data[f"ir_{role}"], dtype="<f4")
        index["entries"].append({"role": role, "offset": len(payload), "samples": ir.size,
                                 "t0_s": float(t0), "source": meta["sources"][role]})
        payload.extend(ir.tobytes())
(OUT / "sample_room.f32").write_bytes(payload)
(OUT / "sample_room.json").write_text(json.dumps(index, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
print(f"sample room: {len(payload)} bytes float32, {len(index['entries'])} roles, position 1 only")
