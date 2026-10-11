"""제품 경로(`RealREW.run_position`)로 실제 스윕 1회 — **스피커에서 소리가 난다.**

    .venv-secs\\Scripts\\python.exe tools\\rew_live_run_position.py --output 17 [--role L] [--level -24]

출력 채널은 반드시 직접 준다(REW ASIO 출력 번호). 타이밍 레퍼런스는 기본 None —
음향 레퍼런스는 레퍼런스 출력 채널에서도 소리가 나므로 `--timing acoustic` 을 줄 때만.
측정 진행 콜백 본문을 끝까지 받아 같이 남긴다. 이름 설정은 끝나고 되돌린다.
"""
from __future__ import annotations

import argparse
import json
import sys
import time
from datetime import datetime
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "src"))
sys.path.insert(0, str(ROOT / "tools"))
sys.stdout.reconfigure(encoding="utf-8")

from rew_live_fetch import summarize  # noqa: E402
from secs.rewapi.interfaces import MeasurementPlan, RewError  # noqa: E402
from secs.rewapi.rew import RealREW  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=int, required=True)
    parser.add_argument("--role", default="L")
    parser.add_argument("--level", type=float, default=-24.0)
    parser.add_argument("--start", type=float, default=20.0)
    parser.add_argument("--end", type=float, default=20000.0)
    parser.add_argument("--length", default="256k")
    parser.add_argument("--timing", default="None")
    args = parser.parse_args()

    plan = MeasurementPlan(roles=[args.role], outputs={args.role: args.output},
                           positions=[1], sweep_level_db=args.level,
                           sweep_length=args.length, timing_reference=args.timing,
                           start_frequency=args.start, end_frequency=args.end)
    problems = plan.validate()
    if problems:
        print("계획 오류:", problems)
        return 2

    rew = RealREW()
    if not rew.is_running():
        print("REW API 에 연결할 수 없다 (roomeqwizard.exe -api)")
        return 1

    out_dir = ROOT / "보고서" / "04 실측·REW" / f"{datetime.now():%Y-%m-%d} REW 실스윕"
    out_dir.mkdir(parents=True, exist_ok=True)
    stamp = f"{datetime.now():%H%M%S}"
    started = time.perf_counter()
    log: dict = {"args": vars(args), "callbacks": [], "progress": []}

    try:
        naming_before = rew.get("/measure/naming")
    except RewError:
        naming_before = None

    try:
        rew.subscribe_progress(lambda body: log["callbacks"].append(
            {"t": round(time.perf_counter() - started, 3), "body": body}))
        results = rew.run_position(
            plan, 1, on_progress=lambda event: log["progress"].append(
                {"t": round(time.perf_counter() - started, 3), "event": str(event)}))
        log["run_s"] = round(time.perf_counter() - started, 2)
        time.sleep(1.0)
        for result in results:
            ir = np.asarray(result.ir, dtype=np.float64)
            log.setdefault("results", []).append({
                "role": result.role, "name": result.name, "uuid": result.uuid,
                "spl_db": result.spl_db,
                "ir": summarize(ir, int(result.sample_rate), float(result.t0_s)),
                "meta": rew.get(f"/measurements/{result.uuid}"),
            })
            np.savez_compressed(out_dir / f"run_{stamp}_{result.role}.npz",
                                ir=ir.astype(np.float32),
                                sample_rate=result.sample_rate, t0_s=result.t0_s)
        print(f"결과 {len(results)}건, {log['run_s']} s, 콜백 {len(log['callbacks'])}건")
    finally:
        rew.unsubscribe_progress()
        try:
            rew.set_blocking(False)         # 사용자의 REW 기본 상태로
        except RewError:
            pass
        if isinstance(naming_before, dict):
            try:
                rew.post("/measure/naming", naming_before)
            except RewError:
                pass
        rew.close()

    path = out_dir / f"run_{stamp}.json"
    path.write_text(json.dumps(log, ensure_ascii=False, indent=2, default=str),
                    encoding="utf-8")
    tail = [item["body"] for item in log["callbacks"][-6:]]
    print("마지막 콜백:", json.dumps(tail, ensure_ascii=False))
    for item in log.get("results", []):
        print(json.dumps({k: item[k] for k in ("role", "name", "uuid", "spl_db")},
                         ensure_ascii=False))
        print(json.dumps(item["ir"], ensure_ascii=False))
        print("SNR", item["meta"].get("signalToNoisedB"))
    print(f"기록: {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
