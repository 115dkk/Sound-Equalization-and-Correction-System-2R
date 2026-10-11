# -*- coding: utf-8 -*-
"""돌비룸 실측 mdat → 샘플 모드용 작은 npz(`src/secs/rewapi/sample_dolby_714.npz`).

원본 `tests/data/250310 돌비룸 보정.mdat`(167 MB, git 제외, **메인 체크아웃에만 있다**)에서
7.1.4 원측정 11채널 + 서브 3개 원측정(SW1 = `DBA L`, SW2 = `DBA R`, SW3 = `INFRA` 인프라 서브 — 2026-10-11 사용자
결정: 튜토리얼에서 서브 정렬 탭 · 인프라 서브가 보이게 원본 서브 3개를 그대로 쓴다. 전에는 `LF 찐`(DBA+INFRA 합 실측)
하나를 SW1 로 썼다)의 임펄스 응답을 role 별로 뽑아, 도착(peak) 앞뒤로 옛 설계 엔진의 창(125 ms / 500 ms — 2026-10-10 에
지운 `secs.design` 의 IMPULSE_WINDOW_LEFT/RIGHT_MS)보다 넉넉히 자르고 float32 npz 로 저장한다. 시간 원점(t0)은
자른 만큼 보정해 채널 사이의 실제 도착 시간 차이(= 거리 차이)를 그대로 남긴다 — 보정 설계 엔진이
`onset_s = onset_index/rate + t0` 로 정렬에 쓴다. 저장 전에 그 식으로 채널마다 도착 시각(ms · m)을 찍고, 모두 같은
시간축(같은 음향 타이밍 레퍼런스)에서 그럴듯한 거리인지 확인한다(`ARRIVAL_RANGE_MS`).

IR 크기는 `secs.mdat_loader` 가 읽은 그대로 = REW API 단위(10-09 `REW_MDAT_IR_SCALE` 정정 뒤 — 그 전 09-24 에 뽑은
npz 는 1/100 이었다). [mdat 불러오기] 로 같은 파일을 읽은 것과 같은 단위다.

메인 매핑은 `tests/test_dolby_room_real.py` 의 DOLBY 표(09-23 검증에서 확정)와 같고, 서브 3개는 `tools/scorecard.load_dolby(3)`
· `tools/qa_flow_dolby3.py` 의 배정과 같다.

사용법(메인 체크아웃의 venv 로, mdat 경로는 환경변수나 인자):
    .venv-secs\\Scripts\\python.exe tools\\make_sample_dolby.py [mdat 경로]

다시 뽑을 일이 있으면(마진·역할 매핑을 바꿀 때) 이 스크립트만 다시 돌리면 된다 — 결과(배열 값)는 결정적.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'src'))

#: (mdat 측정 제목, 같은 제목 가운데 몇 번째(0부터), SECS 역할). 메인 11 = `tests/test_dolby_room_real.py` DOLBY 표.
#:
#: 제목 'INFRA' 는 mdat 에 둘이다(측정 순서 #2 · #6, 0부터). 둘 다 타이밍 레퍼런스는 다른 원측정과 같다
#: ("Acoustic reference played from LINE_OUT R with no timing offset"). 쓰는 것은 첫째(#2)다(10-11 확인):
#: - #2 는 `DBA L`(#0) · `DBA R`(#1) 바로 뒤, 합 · EQ · 예측 측정보다 앞의 원측정 묶음이다. 대역이 그대로(1/3 oct 16–250 Hz
#:   가 63 Hz 방 딥 −14 dB 말고는 −2 … +10 dB)이고 IR 최댓값 0.019 가 DBA L · R(0.011 · 0.014)과 같은 크기 단위다.
#:   도착 7.6 ms(2.6 m).
#: - #6 은 `DBA SUM EQ`(#5) 뒤 · `INFRA DBA SUM` · `LF 찐` 앞, EQ · LP 를 넣은 뒤의 묶음이다. 63 Hz 가 16 Hz 보다 42 dB
#:   낮고(인프라 LP 25 Hz 근처가 걸린 응답), 크기는 23 dB 작고(0.0013 — `L LF` · `C LF` 같은 저역 경로 측정과 같은 단위),
#:   도착이 12.6 ms 로 5 ms 늦다(LP 지연) — 원측정이 아니다.
#: `tools/scorecard.load_dolby` · `qa_flow_dolby3` 도 첫째(`titles.index('INFRA')`)를 써 왔다.
DOLBY_SOURCES: tuple[tuple[str, int, str], ...] = (
    ('L', 0, 'L'), ('C', 0, 'C'), ('R', 0, 'R'), ('LS', 0, 'Ls'), ('RS', 0, 'Rs'), ('LRS', 0, 'Lb'), ('RRS', 0, 'Rb'),
    ('LTF', 0, 'Tfl'), ('RTF', 0, 'Tfr'), ('LTR', 0, 'Tbl'), ('RTR', 0, 'Tbr'),
    ('DBA L', 0, 'SW1'),
    ('DBA R', 0, 'SW2'),
    ('INFRA', 0, 'SW3'),     # 인프라 서브(UM18) — 원측정 #2(위 설명)
)

DEFAULT_MDAT = ROOT / 'tests' / 'data' / '250310 돌비룸 보정.mdat'
OUT_PATH = ROOT / 'src' / 'secs' / 'rewapi' / 'sample_dolby_714.npz'

#: 도착(peak) 앞뒤 마진. 옛 설계 엔진 창(125 ms/500 ms)보다 넉넉히 두고, 합쳐 약 1 s
#: (14 채널 × 1 s × float32 ≈ 2.6 MB, 압축 뒤 3 MB 아래). 샘플 npz 를 다시 뽑아도 같은 크기가 나오게 창 값을 숫자로 둔다.
LEFT_MARGIN_S = 0.20
RIGHT_TAIL_S = 0.80
IMPULSE_WINDOW_LEFT_MS = 125.0
IMPULSE_WINDOW_RIGHT_MS = 500.0

#: 도착 시각(t0 + onset/rate)의 그럴듯한 범위(ms) — 음향 타이밍 레퍼런스 기준이라 청취 위치까지의 거리와 같은 꼴
#: (0.5–20 ms ≈ 0.17–6.9 m, 작은 돌비룸). 벗어나면 채널 시간축이 어긋난 것(다른 레퍼런스 · 처리된 측정)이다.
ARRIVAL_RANGE_MS = (0.5, 20.0)
SPEED_OF_SOUND = 343.0


def main(argv: list[str]) -> int:
    from secs.correction_engine import onset_index
    from secs.mdat_loader import load_mdat_project

    mdat_path = Path(argv[1]) if len(argv) > 1 else DEFAULT_MDAT
    if not mdat_path.is_file():
        print(f'mdat 없음: {mdat_path}', file=sys.stderr)
        return 1

    assert LEFT_MARGIN_S * 1000.0 > IMPULSE_WINDOW_LEFT_MS, '설계 엔진 창보다 짧게 자르면 안 된다'
    assert RIGHT_TAIL_S * 1000.0 > IMPULSE_WINDOW_RIGHT_MS, '설계 엔진 창보다 짧게 자르면 안 된다'

    print(f'읽는 중: {mdat_path}')
    loaded = load_mdat_project(mdat_path)
    titles = [str(m.title or '') for m in loaded.measurements]

    payload: dict[str, np.ndarray] = {}
    meta_roles: list[str] = []
    meta_t0: list[float] = []
    sources: dict[str, dict] = {}
    sample_rate = None

    for title, nth, role in DOLBY_SOURCES:
        matches = [i for i, t in enumerate(titles) if t == title]
        if len(matches) <= nth:
            raise SystemExit(f'제목 {title!r} 측정의 {nth + 1}번째를 못 찾았다(mdat {len(titles)}건 중 {len(matches)}건)')
        index = matches[nth]
        info = loaded.measurements[index]
        ir, rate = loaded.ir_provider.get_ir(info.uuid)
        ir = np.asarray(ir, dtype=np.float64)
        rate = int(rate)
        if sample_rate is None:
            sample_rate = rate
        elif rate != sample_rate:
            raise SystemExit(f'샘플레이트가 채널마다 다르다: {role}={rate} vs {sample_rate}')

        peak = int(np.argmax(np.abs(ir)))
        left = int(round(LEFT_MARGIN_S * rate))
        right = int(round(RIGHT_TAIL_S * rate))
        start = max(0, peak - left)
        stop = min(len(ir), peak + right)
        cropped = ir[start:stop].astype(np.float32)
        t0 = float(info.start_time_s) + start / float(rate)

        # 엔진과 같은 식의 도착 시각 — 자르기 전 · 뒤가 같아야 한다(t0 보정이 맞는지)
        arrival_full = float(info.start_time_s) + onset_index(ir) / rate
        arrival = t0 + onset_index(np.asarray(cropped, dtype=np.float64)) / rate
        if abs(arrival - arrival_full) > 0.5 / rate:
            raise SystemExit(f'{role}: 자른 뒤 도착 시각이 달라졌다 {arrival_full * 1e3:.3f} → {arrival * 1e3:.3f} ms')
        lo, hi = ARRIVAL_RANGE_MS
        if not lo <= arrival * 1e3 <= hi:
            raise SystemExit(f'{role}({title!r} #{index}): 도착 {arrival * 1e3:.3f} ms 가 {lo}–{hi} ms 밖 — 시간축이 다르다')

        payload[f'ir_{role}'] = cropped
        meta_roles.append(role)
        meta_t0.append(t0)
        sources[role] = {'title': title, 'index': index}
        print(f'  {title!r:>8} #{index:<2} -> {role:<4} peak={peak:>6} len={len(cropped):>6} '
              f'({len(cropped) / rate:.3f} s) t0={t0:.6f} 도착={arrival * 1e3:6.3f} ms '
              f'({arrival * SPEED_OF_SOUND:.2f} m) |ir|max={np.max(np.abs(cropped)):.4g}')

    meta = {
        'roles': meta_roles,
        'sample_rate': int(sample_rate),
        'source': str(mdat_path.name),
        'date': '2026-03-10',       # mdat 파일명의 측정일(250310)
        'note': '250310 돌비룸 보정.mdat, 청취 위치 1점, 7.1.4 + 서브 3(SW1 DBA L · SW2 DBA R · SW3 INFRA 원측정). '
                'tools/make_sample_dolby.py 로 재생성.',
        'sources': sources,
        'left_margin_s': LEFT_MARGIN_S,
        'right_tail_s': RIGHT_TAIL_S,
    }
    payload['t0'] = np.asarray(meta_t0, dtype=np.float64)
    payload['meta_json'] = np.asarray(json.dumps(meta, ensure_ascii=False))

    OUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    np.savez_compressed(OUT_PATH, **payload)
    size_mb = OUT_PATH.stat().st_size / (1024 * 1024)
    print(f'저장: {OUT_PATH} ({size_mb:.2f} MB)')
    return 0


if __name__ == '__main__':
    raise SystemExit(main(sys.argv))
