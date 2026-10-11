"""실행 중인 REW(-api)로 실제 스윕을 1회 돌리고 구독 콜백 본문을 그대로 남긴다.

    .venv-secs\\Scripts\\python.exe tools\\rew_live_sweep.py [--level -24] [--start 20] [--end 20000]

**스피커에서 소리가 난다.** 출력·입력 채널과 타이밍 레퍼런스는 REW 에 지금 잡혀 있는
값을 건드리지 않는다(사용자 설정 그대로). 바꾸는 것은 스윕 대역·레벨·반복 1·이름이고,
이름 설정과 blocking 은 끝나고 되돌린다.

목적: `rew.py` 의 "아직 추정" 2곳 — `/measure/subscribe`, `/input-levels/subscribe`
콜백 본문 — 을 실제 본문으로 확정한다. 입력 레벨 모니터는 스윕과 입력 장치를 다툴 수
있어서 스윕 **전에** 2초만 돌리고 끈다(소리 없음).
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
sys.stdout.reconfigure(encoding="utf-8")

from secs.rewapi.interfaces import RewError  # noqa: E402
from secs.rewapi.rew import RealREW  # noqa: E402


def _safe(rew: RealREW, path: str):
    try:
        return rew.get(path)
    except RewError as error:
        return f"<오류: {str(error)[:120]}>"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--level", type=float, default=-24.0)
    parser.add_argument("--start", type=float, default=20.0)
    parser.add_argument("--end", type=float, default=20000.0)
    parser.add_argument("--length", default="256k")
    parser.add_argument("--title", default="SECS 실스윕")
    args = parser.parse_args()

    out_dir = ROOT / "보고서" / "04 실측·REW" / f"{datetime.now():%Y-%m-%d} REW 실스윕"
    out_dir.mkdir(parents=True, exist_ok=True)
    stamp = f"{datetime.now():%H%M%S}"

    rew = RealREW()
    if not rew.is_running():
        print("REW API 에 연결할 수 없다 (roomeqwizard.exe -api)")
        return 1

    log: dict = {"started": datetime.now().isoformat(timespec="seconds"),
                 "args": vars(args), "callbacks": {"levels": [], "measure": []}}
    started = time.perf_counter()

    def stamp_payload(bucket: str):
        def handler(payload):
            log["callbacks"][bucket].append(
                {"t": round(time.perf_counter() - started, 3), "body": payload})
        return handler

    naming_before = _safe(rew, "/measure/naming")
    blocking_before = _safe(rew, "/application/blocking")
    log["before"] = {key: _safe(rew, key) for key in (
        "/audio/driver", "/audio/asio/device", "/audio/asio/output", "/audio/asio/input",
        "/audio/samplerate", "/measure/level", "/measure/sweep/configuration",
        "/measure/timing/reference", "/measure/naming")}

    try:
        # 1) 입력 레벨 구독 — 소리 없음, 2초.
        server = rew._ensure_callback()
        level_url = server.route("/levels", stamp_payload("levels"))
        rew._level_url = level_url
        try:
            rew.post("/input-levels/subscribe",
                     {"url": level_url,
                      "parameters": {"unit": "dBFS", "timeSpanSeconds": 0.25}})
            rew.post("/input-levels/command", {"command": "Start"})
            time.sleep(2.0)
            log["last_levels_poll"] = rew.last_levels()
        except RewError as error:
            log["levels_error"] = str(error)
        finally:
            for path, body in (("/input-levels/command", {"command": "Stop"}),
                               ("/input-levels/unsubscribe", {"url": level_url})):
                try:
                    rew.post(path, body)
                except RewError as error:
                    log.setdefault("levels_stop_error", []).append(str(error))
            rew._level_url = None
        time.sleep(0.5)
        print(f"입력 레벨 콜백 {len(log['callbacks']['levels'])}건")

        # 2) 측정 구독 + 스윕.
        rew.subscribe_progress(stamp_payload("measure"))
        rew.set_blocking(True)
        rew.set_sweep(args.start, args.end, args.length)
        rew.set_level(args.level)
        rew.set_repetitions(1)
        rew.set_naming(args.title)
        rew.set_notes(f"SECS 실스윕 검증 {args.start:g}-{args.end:g} Hz {args.level:g} dBFS")
        log["applied"] = {key: _safe(rew, key) for key in (
            "/measure/level", "/measure/sweep/configuration", "/measure/sweep/repetitions")}

        before = rew.measurement_ids()
        print(f"스윕 시작: {args.start:g}~{args.end:g} Hz, {args.level:g} dBFS, "
              f"출력 {log['before']['/audio/asio/output']}")
        sweep_started = time.perf_counter()
        try:
            log["command_reply"] = rew.start_sweep(timeout=120.0)
        except RewError as error:
            log["sweep_error"] = str(error)
            print(f"스윕 오류: {error}")
        log["sweep_s"] = round(time.perf_counter() - sweep_started, 2)
        log["process_result"] = rew.process_result()
        time.sleep(1.0)                 # 늦게 오는 콜백

        uuid = rew._new_measurement_id(before)
        log["uuid"] = uuid
        if uuid:
            ir, rate, t0 = rew.impulse_response(uuid)
            ir = np.asarray(ir, dtype=np.float64)
            peak = int(np.argmax(np.abs(ir)))
            noise = ir[: max(peak - rate // 20, 1)]
            log["ir"] = {
                "samples": int(ir.size), "sample_rate": int(rate), "t0_s": float(t0),
                "peak_index": peak, "peak_value": float(ir[peak]),
                "peak_abs_time_ms": round((peak / rate + t0) * 1000.0, 4),
                "pre_peak_rms_db_re_peak": round(float(
                    20 * np.log10(max(np.sqrt(np.mean(noise ** 2)), 1e-15)
                                  / max(abs(ir[peak]), 1e-15))), 1),
            }
            log["summary"] = _safe(rew, f"/measurements/{uuid}")
            np.savez_compressed(out_dir / f"ir_{stamp}.npz",
                                ir=ir.astype(np.float32), sample_rate=rate, t0_s=t0)
            print(f"측정 완료: uuid={uuid} 샘플 {ir.size} sr={rate} t0={t0:.6f} "
                  f"피크 {ir[peak]:.5f}@{peak}")
        else:
            print("새 측정을 찾지 못했다")
    finally:
        rew.unsubscribe_progress()
        if isinstance(naming_before, dict):
            try:
                rew.post("/measure/naming", naming_before)
            except RewError as error:
                log["naming_restore_error"] = str(error)
        if isinstance(blocking_before, bool):
            try:
                rew.set_blocking(blocking_before)
            except RewError:
                pass
        rew.close()

    path = out_dir / f"sweep_{stamp}.json"
    path.write_text(json.dumps(log, ensure_ascii=False, indent=2, default=str),
                    encoding="utf-8")
    print(f"측정 콜백 {len(log['callbacks']['measure'])}건, 스윕 {log.get('sweep_s')} s")
    print(f"기록: {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
