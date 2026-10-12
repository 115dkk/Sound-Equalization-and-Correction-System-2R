# -*- coding: utf-8 -*-
"""MIMO(다입력 다출력) 저역 감쇠 — 서포트 설계(2026-09-25, 새 엔진 위에).

입력 채널마다 **다른 스피커(서브 각각 + 다른 메인)** 에 저역 서포트 신호를 더해(입력 여럿 × 출력 여럿의 필터
매트릭스), 청취 영역의 모드 감쇠(늦은 에너지)를 지운다. 초기 응답(직접음 + `early_ms`)은 건드리지 않게 벌점을 준다.
파라미터: Fsiso(서포트 상한, 기본 150 Hz), Support Level(서포트 세기, `LEVEL_DB`), 측정 위치 3곳 이상일 때만(위치가
적으면 청취 영역의 모드를 가늠할 수 없다).

수식(입력 i, 위치 p, 저속 fs_low): y_p = x_p + Σ_j g_jp * s_j,  s_j = lp * u_j (Fsiso 저역 통과로 대역 제한).
최소화  Σ_p ‖W_late y_p‖² + μ Σ_p ‖W_early (Σ_j g_jp * s_j)‖² + λ Σ_j ‖u_j‖² — 선형 최소제곱.
λ 는 서포트 출력 레벨(입력 대비, 대역 안 RMS)이 Support Level 이하가 되게 이분 탐색.
누가 부르나: `secs.calibration.compute_mimo`(보정 설계 화면의 [MIMO 계산]). 결과 서포트 FIR 은 저속(fs_low) — DSP 에
올리는 IIR 근사는 `secs.mimo_iir`, FIR 그대로 올릴 때(10-12 'FIR 사용')는 `FirSupport`.
"""
from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np

from secs.correction_engine import SR

FS_LOW = 600
TAPS_LOW = int(__import__('os').environ.get('SECS_MIMO_TAPS_LOW', '180'))   # 서포트 FIR 길이(저속, 180 = 300 ms)
LEN_S = 1.2                     # 목표 창 길이
EARLY_MS = 60.0
MU_EARLY = 10.0
MAIN_FLOOR_HZ = 50.0            # 서브가 아닌 스피커의 서포트 하한은 50 Hz 밑으로 안 내려간다
SUB_FLOOR_HZ = 20.0
RANGE_DROP_DB = 10.0            # 서포트 하한 = 대역 기준 레벨보다 10 dB 낮아지는 곳
LEVEL_DB = -12.0                 # 서포트 세기 기본값(09-26 맥 시뮬 4차·가드 수정 뒤: −18 → 2.9 dB, −12 → 4.2 dB 감소, 과적합 없음)
#: FIR 서포트(10-12 사용자: 'FIR 사용' 체크, 서포트당 8,000탭 상한) — 48 kHz 8,000탭 = 저속 100 표본. 그중 Fsiso 저역 통과
#: (`lowpass_fir` 31탭)가 30 을 먹으므로 풀 길이는 70(117 ms). IIR 은 180(300 ms)을 풀어 근사한다.
FIR_TAPS_48K = 8000
FIR_TAPS_LOW = FIR_TAPS_48K * FS_LOW // SR - 30


@dataclass
class MimoResult:
    supports: dict = field(default_factory=dict)    # input → {support role: 저속 FIR}
    level_db: dict = field(default_factory=dict)    # input → 실제 서포트 레벨
    fs_low: int = FS_LOW
    ranges: dict = field(default_factory=dict)      # 스피커 → (F-support Low, High) Hz, None 이면 제외
    notes: list = field(default_factory=list)


@dataclass
class FirSupport:
    """FIR 서포트 경로 하나 — 저속 FIR(`MimoResult.supports` 의 것, Fsiso 저역 통과 포함). `IirSupport` 와 같은 자리에 들어가
    `response(f)` 로 응답을 준다. DSP 에는 `export_camilladsp.fir_to_48k` 로 48 kHz Conv 가 된다(`taps_48k` 탭)."""
    taps: np.ndarray
    fs_low: int = FS_LOW

    def response(self, f: np.ndarray) -> np.ndarray:
        """48 kHz 격자 f(Hz)의 복소 응답 — `fir_to_48k` 와 같게 fs_low/2 아래만(가장자리 10 % 코사인), 위는 0."""
        f = np.asarray(f, float)
        edge = self.fs_low / 2
        out = np.zeros(f.size, complex)
        m = f < edge
        out[m] = np.exp(-2j * np.pi * np.outer(f[m], np.arange(self.taps.size)) / self.fs_low) @ self.taps
        out[m] *= np.clip((edge - f[m]) / (0.1 * edge), 0, 1)
        return out

    @property
    def taps_48k(self) -> int:
        return int(np.ceil(self.taps.size * SR / self.fs_low))


def to_low(H: np.ndarray, f: np.ndarray, n: int) -> np.ndarray:
    """48 kHz rfft 격자 복소 응답 → fs_low 의 길이 n IR(0–fs_low/2 만 남겨 역변환)."""
    fl = np.fft.rfftfreq(2 * n, 1.0 / FS_LOW)
    Hl = np.interp(fl, f, H.real) + 1j * np.interp(fl, f, H.imag)
    Hl *= np.clip((FS_LOW / 2 * 0.95 - fl) / (FS_LOW * 0.1), 0, 1)
    return np.fft.irfft(Hl, 2 * n)[:n]


def conv_matrix(g: np.ndarray, taps: int, n: int) -> np.ndarray:
    """g * u (u 길이 taps) 의 앞 n 표본 = A @ u."""
    A = np.zeros((n, taps))
    for k in range(taps):
        A[k:, k] = g[:n - k]
    return A


def lowpass_fir(fc: float, taps: int = 31) -> np.ndarray:
    from scipy.signal import firwin
    return firwin(taps, fc, fs=FS_LOW)


def hp2(f: np.ndarray, fc: float) -> np.ndarray:
    s = 1j * f / fc
    return (s * s) / (s * s + np.sqrt(2) * s + 1)


def support_ranges(engine, H: dict, fsiso: float, positions: list) -> dict:
    """스피커마다 서포트 대역(F-support Low/High 자동 감지): 위치 평균 1/3 oct 응답에서 기준 레벨(메인 200 Hz–1 kHz,
    서브 30–80 Hz 중앙값)보다 RANGE_DROP_DB 낮아지는 곳을 기준 대역(서브 50 Hz, 메인 min(200, Fsiso))에서 아래·위로 찾아 하한·상한으로. 메인 하한 ≥ 50 Hz,
    하한이 Fsiso × 0.8 이상이면 서포트 못 함(None)."""
    from secs.correction_engine import smooth_power

    f = engine.f
    out = {}
    for j in list(engine.mains) + list(engine.subs):
        if not all(j in H[p] for p in positions):
            continue
        power = np.mean([np.abs(H[p][j]) ** 2 for p in positions], axis=0)
        db = 10 * np.log10(smooth_power(f, power, 3.0) + 1e-30)
        is_sub = j in engine.subs
        ref_band = (f >= 30) & (f <= 80) if is_sub else (f >= 200) & (f <= 1000)
        start = 50.0 if is_sub else min(200.0, fsiso)
        band = engine.sub_passband(j) if is_sub and hasattr(engine, 'sub_passband') else None
        ref = float(np.median(db[ref_band]))
        if band:
            # 고정 대역 필터가 걸린 서브(R27 — 예: INFRA LP 25 Hz): 기준 = 통과 대역(15–80 Hz 안) 1/3 oct 응답의 최댓값, 출발점 =
            # 그 자리. 30–80 Hz 중앙값을 쓰면 기준이 저지 대역 레벨이 되어 LP 위까지 서포트 대역으로 잡혔고, 통과 대역 중앙값도
            # 스피커 자체 저역 롤오프(15–20 Hz)에 끌려 낮아 합성 방에서 LP 25 Hz 의 상한이 62 Hz 였다(INFRA 가 자기 LP 위를 돕는 갈래).
            hp, lp = band
            lo_b = max(15.0, hp) if hp else 15.0
            hi_b = min(80.0, lp) if lp else 80.0
            hi_b = max(hi_b, lo_b * 1.5)
            win = np.nonzero((f >= lo_b) & (f <= hi_b))[0]
            k = int(win[np.argmax(db[win])])
            ref, start = float(db[k]), float(f[k])
        floor = SUB_FLOOR_HZ if is_sub else MAIN_FLOOR_HZ
        down = np.geomspace(start, 15.0, 200)
        lv = np.interp(down, f, db)
        below = np.nonzero(lv < ref - RANGE_DROP_DB)[0]
        low = max(float(down[below[0]]) if below.size else 15.0, floor)
        up = np.geomspace(start, fsiso, 200)
        lv = np.interp(up, f, db)
        above = np.nonzero(lv < ref - RANGE_DROP_DB)[0]
        high = float(up[above[0]]) if above.size else float(fsiso)
        out[j] = (round(low, 1), round(high, 1)) if low < min(high, fsiso) * 0.8 else None
    return out


def support_path(engine, design: 'MimoResult', j: str, H: dict, pos: int) -> np.ndarray:
    """서포트 스피커 j 의 위치 pos 음향 경로(48 kHz 격자) — 도착 정렬 + 서포트 하한 보호 HP(4차)."""
    f = engine.f
    gH = H[pos][j] * engine._delay(engine.align_delay_ms.get(j, 0.0))
    low = (design.ranges.get(j) or (MAIN_FLOOR_HZ, 0))[0]
    return gH * hp2(f, low) ** 2          # 상한은 설계에서 Fsiso 저역 통과로


def design_mimo(engine, fsiso: float = 150.0, level_db: float = LEVEL_DB, positions: list | None = None,
               inputs: list | None = None, groups: set | None = None, max_supports: int | None = None,
               f_low: float | None = None, exclude=(), taps_low: int | None = None) -> MimoResult:
    """fsiso = 서포트 상한, f_low = 서포트 하한(None = 자동). groups: 서포트에 참여할 그룹(None = 전부). max_supports: 입력당 서포트 수 상한(DSP 자원) —
    넘으면 하나씩 따로 풀어 늦은 에너지를 가장 많이 줄이는 순서로 고른다. exclude: 서포트로 쓰지 않을 스피커(10-05 R27 — 서브별
    'MIMO 서포트로 쓰기' 체크를 끈 것. DBA 처럼 배치로 감쇠를 만든 서브를 그대로 두고 싶을 때). taps_low: 풀 서포트 FIR 길이
    (저속, None = `TAPS_LOW` — FIR 서포트로 DSP 에 올릴 때는 `FIR_TAPS_LOW`)."""
    taps = int(taps_low or TAPS_LOW)
    out = MimoResult()
    positions = positions or engine.positions
    if len(positions) < 3:
        out.notes.append(f'측정 위치 {len(positions)}곳 — MIMO 는 3곳 이상일 때만')
        return out
    H = engine.long_spectra()
    f = engine.f
    n = int(LEN_S * FS_LOW)
    lp = lowpass_fir(fsiso)
    out.ranges = support_ranges(engine, H, fsiso, positions)
    if f_low:                                   # 사용자 하한(F-support Low) — 자동 판정보다 낮게는 안 내린다
        out.ranges = {r: ((max(v[0], float(f_low)), v[1]) if v and max(v[0], float(f_low)) < v[1] * 0.8 else None)
                      for r, v in out.ranges.items()}
    group_of = {r: g for g, rs in engine.groups.items() for r in rs}
    usable = [r for r, rng in out.ranges.items() if rng is not None and (groups is None or group_of.get(r) in groups)
              and r not in set(exclude or ())]
    if exclude:
        out.notes.append('서포트 제외(사용자): ' + ', '.join(sorted(exclude)))
    out.notes.append('서포트 대역 ' + ', '.join(f'{r} {v[0]:.0f}–{v[1]:.0f}' if v else f'{r} 제외'
                                            for r, v in out.ranges.items()))
    for i in (inputs or list(engine.chain)):
        cands = [r for r in usable if r != i]
        if max_supports and len(cands) > max_supports:
            cands = _rank_supports(engine, out, i, cands, H, positions, n, lp, taps)[:max_supports]
        blocks, early_blocks, xs, lates = [], [], [], []
        for p in positions:
            x = to_low(engine.input_response(i, H, p), f, n)
            onset = int(np.argmax(np.abs(x) > 0.1 * np.max(np.abs(x))))
            e_end = onset + int(EARLY_MS / 1000 * FS_LOW)
            wl = np.zeros(n)
            wl[e_end:] = 1.0
            we = 1.0 - wl
            cols = []
            for j in cands:
                g = np.convolve(to_low(support_path(engine, out, j, H, p), f, n), lp)[:n]
                cols.append(conv_matrix(g, taps, n))
            A = np.hstack(cols)
            blocks.append(wl[:, None] * A)
            early_blocks.append(np.sqrt(MU_EARLY) * we[:, None] * A)
            xs.append(wl * x)
            lates.append(float(np.sum((wl * x) ** 2)))
        A = np.vstack(blocks + early_blocks)
        b = -np.concatenate(xs + [np.zeros(n)] * len(positions))
        AtA, Atb = A.T @ A, A.T @ b
        ref = float(np.mean([np.sum(to_low(engine.input_response(i, H, p), f, n) ** 2) for p in positions]))
        target = 10 ** (level_db / 10)

        def solve(lam):
            u = np.linalg.solve(AtA + lam * np.eye(AtA.shape[0]), Atb)
            # 서포트 레벨: 서포트 출력(대역 제한 뒤) 에너지 합 / 입력 자체 응답 에너지(위치 평균, 음향)
            sup = sum(float(np.sum(np.convolve(lp, u[k * taps:(k + 1) * taps]) ** 2)) for k in range(len(cands)))
            direct = float(np.sum(to_low(engine.chain[i]['F'], f, n) ** 2))
            return u, sup / max(direct, 1e-30)

        lo, hi = -8.0, 4.0                      # log10 λ(AtA 대각 평균 배)
        scale = float(np.mean(np.diag(AtA)))
        u, ratio = solve(scale * 10 ** hi)
        for _ in range(18):
            mid = (lo + hi) / 2
            u_m, r_m = solve(scale * 10 ** mid)
            if r_m > target:
                lo = mid
            else:
                hi, u, ratio = mid, u_m, r_m
        sup = {j: np.convolve(lp, u[k * taps:(k + 1) * taps]) for k, j in enumerate(cands)}
        # 안 나빠지게(09-26: 167 ms 서포트가 64–68 Hz T60 을 0.8 → 1.6 s 로 늘림): 설계 위치들의 1/3 oct 대역 늦은
        # 에너지가 한 대역이라도 1 dB 넘게 늘면 서포트를 3 dB 씩 낮춘다(최대 12 dB, 그래도 나쁘면 이 입력 MIMO 끔).
        xs = [to_low(engine.input_response(i, H, p), f, n) for p in positions]
        gs = [{j: to_low(support_path(engine, out, j, H, p), f, n) for j in cands} for p in positions]
        scale = 1.0
        for step in range(5):
            trial = {j: s * scale for j, s in sup.items()}
            ys = [x + sum(np.convolve(g[j], sv)[:n] for j, sv in trial.items()) for x, g in zip(xs, gs)]
            if _worst_band_db(xs, gs, trial, fsiso, n, lp) <= WORSE_DB and worst_t60_change(xs, ys) <= T60_WORSE_S * WORSE_DB:
                break
            scale = 0.0 if step == 4 else scale * 10 ** (-3 / 20)
        out.supports[i] = {j: s * scale for j, s in sup.items()} if scale > 0 else {}
        out.level_db[i] = 10 * np.log10(max(ratio * scale ** 2, 1e-30))
        if scale < 1.0:
            out.notes.append(f'{i}: 대역 늦은 에너지 악화로 서포트 {20 * np.log10(max(scale, 1e-9)):.0f} dB' if scale > 0 else f'{i}: MIMO 끔(악화)')
    return out


def _worst_band_db(xs: list, gs: list, sup: dict, fsiso: float, n: int, lp) -> float:
    """위치들의 저속 IR x 와 서포트 음향 경로 g(대역 제한 전), 서포트 FIR sup(Fsiso 저역 통과 포함)로, 도착 + 60 ms 뒤 에너지를 1/3 oct 대역(20 Hz–fsiso)마다 전·후 비교한 가장 나빠진 dB."""
    nf = 2 * n
    fl = np.fft.rfftfreq(nf, 1.0 / FS_LOW)
    edges = 20.0 * 2 ** (np.arange(0, np.log2(fsiso / 20.0) + 1e-9, 1 / 3))
    # 모드 단위(09-26: 대역 에너지는 줄어도 모드 하나가 0.8 → 1.6 s 로 길어졌다): 1/3 oct 대역에 더해 늦은 스펙트럼의
    # 센 봉우리 6개 둘레 ±1.5 Hz 도 따로 본다.
    lows, highs = list(edges[:-1]), list(edges[1:])
    for fm in _late_peaks(xs, 6):
        lows.append(fm - 1.5)
        highs.append(fm + 1.5)
    edges_pairs = list(zip(lows, highs))
    before = np.zeros(len(edges_pairs))
    after = np.zeros(len(edges_pairs))
    for x, g in zip(xs, gs):
        k = int(np.argmax(np.abs(x) > 0.1 * np.max(np.abs(x))))
        wl = np.zeros(n)
        wl[k + int(EARLY_MS / 1000 * FS_LOW):] = 1.0
        add = np.zeros(n)
        for j, s in sup.items():
            add += np.convolve(g[j], s)[:n]
        for acc, y in ((before, x), (after, x + add)):
            Y = np.abs(np.fft.rfft(wl * y, nf)) ** 2
            acc += np.array([Y[(fl >= lo) & (fl < hi)].sum() for lo, hi in edges_pairs])
    ok = before > 0
    return float(np.max(10 * np.log10((after[ok] + 1e-30) / before[ok]))) if ok.any() else -99.0


WORSE_DB = float(__import__('os').environ.get('SECS_MIMO_WORSE_DB', '1.0'))   # FIR 가드: 1/3 oct·모드 대역 늦은 에너지 악화 한도(dB)
TAIL_IGNORE_DB = 20.0            # 꼬리 가드: 가장 센 모드 꼬리보다 이만큼 작은 모드는 안 본다(09-26)
TAIL_WINDOWS_S = (0.3,)          # 안 나빠지게 가드가 보는 꼬리 창(도착 뒤 s). 09-26 (0.3, 0.6) 시험: 600 ms 창까지 보면 39 Hz 서포트까지
                                 # 빠져 MIMO 효과가 사라졌고(꼬리 −48.7 → −46.6 dB), 절대 꼬리는 300 ms 창만으로도 안 나빠졌다
T60_WORSE_S = 0.15              # 모드 T60 이 이만큼 넘게 길어지면 서포트를 낮춘다(09-26: IIR 에서 한 모드 +2.96 s)


def mode_t60(ir: np.ndarray, rate: float, center_hz: float, half_band: float = 3.0) -> float | None:
    """모드 하나의 T60(s): ±half_band Hz 대역 통과(시간 역방향) → Schroeder 적분 → −5~−25 dB 직선을 −60 dB 로 외삽
    (`tests/room_model_decay.mode_t60` 과 같은 방법)."""
    from scipy.signal import butter, sosfilt

    low, high = max(center_hz - half_band, 1.0), min(center_hz + half_band, rate * 0.45)
    sos = butter(2, [low, high], btype='bandpass', fs=rate, output='sos')
    padded = np.concatenate((np.zeros(int(2.0 * rate)), np.asarray(ir, dtype=float)))
    energy = sosfilt(sos, padded[::-1])[::-1] ** 2
    start = int(np.argmax(energy))
    edc = np.cumsum(energy[start:][::-1])[::-1]
    if edc[0] <= 0:
        return None
    edc_db = 10 * np.log10(np.maximum(edc / edc[0], 1e-30))
    hi_i, lo_i = int(np.argmax(edc_db <= -5.0)), int(np.argmax(edc_db <= -25.0))
    if lo_i <= hi_i + 2:
        return None
    slope = np.polyfit(np.arange(hi_i, lo_i) / rate, edc_db[hi_i:lo_i], 1)[0]
    return float(-60.0 / slope) if slope < 0 else None


def worst_t60_change(xs: list, ys: list, count: int = 6) -> float:
    """(이름은 호환용) 위치들의 저속 IR 전(xs)·후(ys) — 전·후 늦은 스펙트럼의 센 모드마다 ±1.5 Hz 대역에서
    **도착 + 300 ms 뒤 남은 에너지를 절대값으로** 비교해 가장 많이 늘어난 양을 초 단위 척도로 돌려준다(1 dB ≈ 0.15 s —
    T60_WORSE_S 와 같은 문턱으로 쓰려고). T60 기울기(자기 레벨 기준)는 에너지를 크게 줄여도 남은 작은 꼬리 때문에
    '길어졌다'로 나와 좋은 서포트까지 뺐다(09-26: −33 → −26 dB) — 들리는 건 절대 레벨의 꼬리다."""
    from scipy.signal import butter, sosfilt

    # T60 기울기가 길어져도(63.5 Hz 0.77 → 1.1 s) 절대 꼬리는 줄어 있었다(09-26 진단) — 들리는 건 절대 꼬리다.
    modes = sorted(set(round(m, 1) for m in _late_peaks(xs, count) + _late_peaks(ys, count)))
    # 위치 합으로 본다(09-26 진단: 9점 중 한 점·한 모드만 1 dB 넘게 늘어도 MIMO 를 통째로 꺼서, 가드를 끄면 늦은 에너지
    # 5–7 dB 가 줄던 설계가 0.3 dB 로 줄었다). 가장 센 꼬리보다 TAIL_IGNORE_DB 넘게 작은 모드는 안 들리니 무시한다.
    tails = []
    for fm in modes:
        sos = butter(2, [max(fm - 1.5, 1.0), fm + 1.5], btype='bandpass', fs=FS_LOW, output='sos')
        for sec in TAIL_WINDOWS_S:
            a = b = 0.0
            for x, y in zip(xs, ys):
                k = int(np.argmax(np.abs(x) > 0.1 * np.max(np.abs(x))))
                t = k + int(sec * FS_LOW)
                a += float(np.sum(sosfilt(sos, x)[t:] ** 2))
                b += float(np.sum(sosfilt(sos, y)[t:] ** 2))
            tails.append((a, b))
    top = max((a for a, _b in tails), default=0.0)
    worst = -99.0
    for a, b in tails:
        if a > 0 and a >= top * 10 ** (-TAIL_IGNORE_DB / 10):
            worst = max(worst, 10 * np.log10((b + 1e-30) / a))
    return worst * T60_WORSE_S          # dB → 문턱 척도(1 dB = T60_WORSE_S)


def _late_peaks(xs, count: int) -> list:
    """저속 IR 들의 늦은 부분(도착 + 150 ms 뒤) 스펙트럼 합에서 센 봉우리 주파수(Hz)."""
    from scipy.signal import find_peaks

    nf = 4096
    spec = 0
    for x in xs:
        k = int(np.argmax(np.abs(x) > 0.1 * np.max(np.abs(x))))
        late = np.zeros_like(x)
        late[k + int(0.15 * FS_LOW):] = x[k + int(0.15 * FS_LOW):]
        spec = spec + np.abs(np.fft.rfft(late, nf)) ** 2
    fr = np.fft.rfftfreq(nf, 1.0 / FS_LOW)
    idx, _ = find_peaks(spec * ((fr >= 20) & (fr <= 160)))
    return [float(fr[i]) for i in sorted(idx, key=lambda i: -spec[i])[:count]]


def apply_mimo(engine, design: MimoResult, role: str, H: dict, pos: int, n: int) -> tuple[np.ndarray, np.ndarray]:
    """위치 pos 에서 입력 role 의 저속 IR (MIMO 전, 후)."""
    f = engine.f
    x = to_low(engine.input_response(role, H, pos), f, n)
    y = x.copy()
    for j, s in design.supports.get(role, {}).items():
        y += np.convolve(to_low(support_path(engine, design, j, H, pos), f, n), s)[:n]
    return x, y


def _rank_supports(engine, design, i, cands, H, positions, n, lp, taps: int = TAPS_LOW) -> list:
    """후보마다 혼자 서포트할 때 늦은 에너지를 얼마나 줄이는지(같은 비례 규제)로 줄 세운다."""
    f = engine.f
    scores = {}
    for j in cands:
        rows, rhs = [], []
        for p in positions:
            x = to_low(engine.input_response(i, H, p), f, n)
            onset = int(np.argmax(np.abs(x) > 0.1 * np.max(np.abs(x))))
            wl = np.zeros(n)
            wl[onset + int(EARLY_MS / 1000 * FS_LOW):] = 1.0
            g = np.convolve(to_low(support_path(engine, design, j, H, p), f, n), lp)[:n]
            A = conv_matrix(g, taps, n)
            rows.append(wl[:, None] * A)
            rhs.append(-wl * x)
        A, b = np.vstack(rows), np.concatenate(rhs)
        AtA = A.T @ A
        u = np.linalg.solve(AtA + 1e-2 * np.mean(np.diag(AtA)) * np.eye(AtA.shape[0]), A.T @ b)
        scores[j] = float(np.sum((A @ u - b) ** 2))       # 남는 늦은 에너지(작을수록 좋음)
    return sorted(cands, key=lambda r: scores[r])
