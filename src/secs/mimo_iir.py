# -*- coding: utf-8 -*-
"""저지연 모드 MIMO — 서포트 경로를 IIR 로(2026-09-25).

`secs.mimo` 의 이상적 서포트(저속 FIR, 300 ms)는 48 kHz 로 24,000탭이라 저지연 모드 DSP(2,048탭)에 못 올린다.
서포트 경로 하나를 DSP 에 흔한 직렬 구조로 둔다:  게인 · 극성 · 지연 · 피킹 바이쿼드 K 개(RBJ, 48 kHz 계수) · 올패스 A 개
— 보호 HP(서포트 하한, 4차) + Fsiso LR4 LP(IIR). 지연을 늘리지 않는다(IIR·딜레이뿐).
맞춤: ① 이상적 FIR 의 주파수 응답에 경로별로 맞춤(시작점) ② 모든 서포트를 함께, MIMO 목적(늦은 에너지 + 초기 보호
+ 세기 규제)을 저속 격자에서 직접 최소화(scipy least_squares). ③ 세기가 Support Level 을 넘으면 게인을 같이 낮춘다.
"""
from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np

from secs.mimo import EARLY_MS, FS_LOW, LEN_S, LEVEL_DB, MU_EARLY, T60_WORSE_S, support_path, to_low, worst_t60_change
from secs.correction_engine import allpass2, lr4, peaking_response

K_BANDS = 9                     # 경로당 PEQ — + 올패스 3 + LR4 HP·LP 4 = 바이쿼드 16(09-26 사용자: 서포트당 16, 시뮬 4차: 전부 × 16 이 몰빵보다 낫다)
EFFORT = float(__import__('os').environ.get('SECS_MIMO_EFFORT', '0.05'))                  # 서포트 출력 벌점 — 결합(|G| 위치 평균)이 약한 주파수일수록 크게(유효 대역만 쓰게, 09-26 사용자)
WORSE_DB = float(__import__('os').environ.get('SECS_MIMO_WORSE_DB', '1.0'))                 # 1/3 oct 대역 늦은 에너지가 이만큼 나빠지면 서포트 게인을 낮춘다
A_BANDS = 3                     # 서포트 경로마다 올패스 — 모드마다 위상을 따로 맞춘다(지연·극성은 경로에 하나뿐)
MAX_NFEV = 150


@dataclass
class IirSupport:
    gain_db: float
    delay_ms: float
    polarity: int
    peq: list = field(default_factory=list)       # [(f, q, gain_db)]
    allpass: list = field(default_factory=list)   # [(f, q)]

    def response(self, f: np.ndarray) -> np.ndarray:
        out = (10 ** (self.gain_db / 20)) * self.polarity * np.exp(-2j * np.pi * f * self.delay_ms / 1000.0)
        for f0, q, g in self.peq:
            out = out * peaking_response(f, f0, q, g)
        for f0, q in self.allpass:
            out = out * allpass2(f, f0, q)
        return out


def _per(K, A=A_BANDS):
    return 2 + 3 * K + 2 * A


def _knobs() -> dict:
    """서포트 PEQ 밴드 제한(10-11 외부 의견: 서포트 PEQ 가 +25 / −25 dB — 14채널에서 밴드 30 % 가 ±20 dB 넘고 상쇄 쌍 130,
    대역 밖 11 %). 환경변수로 맞춤마다 바꿀 수 있다(워커가 뜰 때 읽는다):
    SECS_MIMO_BAND_DB 밴드 게인 한도 · SECS_MIMO_Q_MAX Q 상한 · SECS_MIMO_IN_BAND 1 = 중심 주파수를 서포트 대역(하한 … Fsiso) 안으로 ·
    SECS_MIMO_BAND_REG 밴드 게인 벌점(상쇄 쌍 퇴화 해를 막는다).
    기본값 = 10-12 비교의 V2(±15 dB · Q≤10 · 대역 안 · 벌점 0.05): mmio님 14채널에서 늦은 에너지 −1.85 → −1.28 dB(청취 위치
    −0.75 → −0.67), 상쇄 쌍 177 → 1, 대역 밖 118 → 0, 최대 30 → 13.8 dB. 옛 값(30 · 40 · 0 · 0)은 환경변수로 되살린다."""
    import os
    return {'band_db': float(os.environ.get('SECS_MIMO_BAND_DB', '15')), 'q_max': float(os.environ.get('SECS_MIMO_Q_MAX', '10')),
            'in_band': os.environ.get('SECS_MIMO_IN_BAND', '1') == '1', 'reg': float(os.environ.get('SECS_MIMO_BAND_REG', '0.05'))}


def _band_of(lo: float, hi: float, fsiso: float) -> tuple[float, float]:
    """서포트 경로 하나의 PEQ · 올패스 중심 주파수 범위. 끄면 예전 그대로 15–300 Hz."""
    if not _knobs()['in_band']:
        return 15.0, 300.0
    f_lo = max(15.0, float(lo) * 0.9)
    f_hi = min(300.0, float(min(hi, fsiso)) * 1.1)
    return (f_lo, f_hi) if f_hi > f_lo * 1.5 else (15.0, 300.0)


def _bounds(K, A=A_BANDS, f_lo: float = 15.0, f_hi: float = 300.0):
    # 갈래 게인 상한(dB) — SECS_MIMO_GAIN_MAX_DB(기본 20). 10-05 테시라 예산 시험: 0 dB 로 묶으면 갈래 게인을 매트릭스
    # 크로스포인트 레벨(−100–0 dB)로 넣을 수 있어 Level 블록이 필요 없다. 워커가 뜰 때 환경변수를 읽으므로 맞춤마다 바꿀 수 있다.
    gain_max = float(__import__('os').environ.get('SECS_MIMO_GAIN_MAX_DB', '20'))
    kn = _knobs()
    band, q_max = kn['band_db'], kn['q_max']
    lb = [-80.0, 0.0] + [v for _ in range(K) for v in (np.log(f_lo), np.log(0.5), -band)] + \
        [v for _ in range(A) for v in (np.log(f_lo), np.log(0.3))]
    ub = [gain_max, 250.0] + [v for _ in range(K) for v in (np.log(f_hi), np.log(q_max), band)] + \
        [v for _ in range(A) for v in (np.log(f_hi), np.log(20.0))]
    return np.array(lb), np.array(ub)


def _unpack(x, K, pols, A=A_BANDS):
    out = []
    per = _per(K, A)
    for j, pol in enumerate(pols):
        v = x[j * per:(j + 1) * per]
        peq = [(float(np.exp(v[2 + 3 * k])), float(np.exp(v[3 + 3 * k])), float(v[4 + 3 * k])) for k in range(K)]
        b = 2 + 3 * K
        ap = [(float(np.exp(v[b + 2 * a])), float(np.exp(v[b + 2 * a + 1]))) for a in range(A)]
        out.append(IirSupport(float(v[0]), float(v[1]), pol, peq, ap))
    return out


def late_modes(xs: list, n_modes: int, lo: float, hi: float) -> list:
    """위치별 저속 IR 의 늦은 부분(도착 + 150 ms 뒤) 스펙트럼 평균에서 가장 센 봉우리들(울림이 긴 모드)."""
    from scipy.signal import find_peaks

    nf = 4096
    spec = 0
    for x in xs:
        k = int(np.argmax(np.abs(x) > 0.1 * np.max(np.abs(x))))
        late = np.zeros_like(x)
        late[k + int(0.15 * FS_LOW):] = x[k + int(0.15 * FS_LOW):]
        spec = spec + np.abs(np.fft.rfft(late, nf)) ** 2
    fr = np.fft.rfftfreq(nf, 1.0 / FS_LOW)
    m = (fr >= lo) & (fr <= hi)
    idx, _ = find_peaks(spec * m)
    idx = sorted(idx, key=lambda i: -spec[i])[:n_modes]
    return [float(fr[i]) for i in idx]


def _fit_one(s_fir: np.ndarray, lo: float, hi: float, K: int, modes: list | None = None, A: int = A_BANDS,
             band: tuple[float, float] = (15.0, 300.0)) -> tuple[np.ndarray, int]:
    """이상적 저속 FIR 하나 → 직렬 IIR 시작값(복소 응답 맞춤)."""
    from scipy.optimize import least_squares

    from secs.correction_engine import fit_peq

    fl = np.geomspace(max(lo, 15.0), hi, 160)
    T = np.exp(-2j * np.pi * np.outer(fl, np.arange(s_fir.size)) / FS_LOW) @ s_fir
    k = int(np.argmax(np.abs(s_fir)))
    pol = 1 if s_fir[k] >= 0 else -1
    base_db = float(np.median(20 * np.log10(np.abs(T) + 1e-12)))
    peq = fit_peq(fl, 20 * np.log10(np.abs(T) + 1e-12) - base_db, K, lo=fl[0], hi=fl[-1], boost=_knobs()['band_db'])
    while len(peq) < K:
        peq.append((float(np.sqrt(fl[0] * fl[-1])), 1.0, 0.0))
    ap0 = list(modes or [])[:A]
    while len(ap0) < A:
        ap0.append(float(np.sqrt(fl[0] * fl[-1])))
    x0 = [base_db, 1000.0 * k / FS_LOW] + [v for f0, q, g in peq for v in (np.log(f0), np.log(q), g)] + \
        [v for f0 in ap0 for v in (np.log(np.clip(f0, band[0] * 1.03, band[1] * 0.97)), np.log(2.0))]
    lb, ub = _bounds(K, A, *band)
    x0 = np.clip(np.array(x0, float), lb + 1e-6, ub - 1e-6)

    def res(x):
        m = _unpack(x, K, [pol], A)[0].response(fl)
        r = (m - T) / (np.abs(T).mean() + 1e-12)
        return np.concatenate([r.real, r.imag])

    try:
        sol = least_squares(res, x0, bounds=(lb, ub), max_nfev=300)
    except np.linalg.LinAlgError:          # 09-26 맥 시뮬: 큰 방에서 가끔 SVD 수렴 실패 — 시작값으로
        return x0, pol
    return sol.x, pol


def _low_priority() -> None:
    """워커 프로세스를 '보통 이하' 우선순위로(09-25 옛 7단계 하이브리드 계산이 화면을 먹통으로 만든 교훈)."""
    import os
    import sys
    try:
        if sys.platform == 'win32':
            import ctypes
            # 핸들은 포인터 크기로 넘겨야 한다 — 기본(c_int)으로 넘기면 64비트에서 핸들이 깨져 조용히 실패했다
            # (10-09: MIMO 워커가 '보통' 우선순위로 돌아 PC 가 무거웠다).
            from ctypes import wintypes
            k32 = ctypes.WinDLL('kernel32')
            k32.GetCurrentProcess.restype = wintypes.HANDLE
            k32.SetPriorityClass.argtypes = (wintypes.HANDLE, wintypes.DWORD)
            k32.SetPriorityClass(k32.GetCurrentProcess(), 0x00004000)
        else:
            os.nice(5)
    except Exception:  # noqa: BLE001
        pass


def one_blas_thread_for_workers() -> None:
    """앞으로 띄울 워커 프로세스의 BLAS 를 스레드 1 개로(spawn 자식이 환경 변수를 물려받아 numpy 를 불러올 때 읽는다).
    10-09 PC 실측: MIMO 워커 2개가 55분에 CPU 시간 각 12,800 s — 워커마다 BLAS 스레드를 여럿 돌려 코어를 다 먹었다.
    부모는 이미 numpy 를 불러와 영향이 없다. 병렬은 워커 수(코어 절반)로만 한다."""
    import os
    for name in ('OMP_NUM_THREADS', 'OPENBLAS_NUM_THREADS', 'MKL_NUM_THREADS', 'VECLIB_MAXIMUM_THREADS'):
        os.environ[name] = '1'


#: 지금 MIMO 를 푸는 워커 풀들 — [계산 취소]·창 닫기에서 `abort_workers()` 로 프로세스를 바로 끈다.
#: 10-09 사용자: 취소가 결과만 버리고 워커는 계속 돌아, 프로그램을 꺼도 CPU 를 잡아 강제 종료했다.
_ACTIVE_POOLS: set = set()


def abort_workers() -> int:
    """돌고 있는 MIMO 워커 프로세스를 모두 끈다. 끈 프로세스 수."""
    killed = 0
    for pool in list(_ACTIVE_POOLS):
        for proc in list((getattr(pool, '_processes', None) or {}).values()):
            try:
                proc.kill()
                killed += 1
            except Exception:  # noqa: BLE001 - 이미 끝난 프로세스
                pass
        try:
            pool.shutdown(wait=False, cancel_futures=True)
        except Exception:  # noqa: BLE001
            pass
        _ACTIVE_POOLS.discard(pool)
    return killed


def _workers(n_jobs: int) -> int:
    import os
    return max(1, min(n_jobs, int(os.cpu_count() or 2) // 2))


def _solve_problem(pb: dict) -> tuple:
    """입력 하나의 IIR 서포트 맞춤(워커 프로세스에서 돈다). 시작값 = 경로별 이상 FIR 맞춤, 그다음 MIMO 목적을
    해석적 야코비안으로 최소제곱(파라미터를 하나씩 흔드는 수치 야코비안은 7.1.4·9점에서 2시간 걸릴 것으로 추정됐다)."""
    from scipy.optimize import least_squares

    K, A = pb['K'], pb.get('A', A_BANDS)
    fl, LP, nf, n = pb['fl'], pb['LP'], pb['nf'], pb['n']
    X, WL, WE, G = pb['X'], pb['WL'], pb['WE'], pb['G']          # (P,n) (P,n) (P,n) (P,R,nf/2+1)
    roles = pb['roles']
    R, P = len(roles), X.shape[0]
    starts, pols, lbs, ubs = [], [], [], []
    for j in roles:
        lo, hi = pb['ranges'][j]
        band_j = _band_of(lo, hi, pb['fsiso'])
        x0, pol = _fit_one(pb['fir'][j], lo, min(hi, pb['fsiso']), K,
                           [m for m in pb['modes'] if lo <= m <= hi] or pb['modes'], A, band_j)
        starts.append(x0)
        pols.append(pol)
        lb_j, ub_j = _bounds(K, A, *band_j)
        lbs.append(lb_j)
        ubs.append(ub_j)
    lb, ub = np.concatenate(lbs), np.concatenate(ubs)
    kn = _knobs()
    reg = kn['reg']
    x0 = np.clip(np.concatenate(starts), lb + 1e-6, ub - 1e-6)
    per = _per(K, A)
    sq_mu = np.sqrt(MU_EARLY)
    band = (fl >= 15.0) & (fl <= pb['fsiso'] * 1.2)
    gbar = np.mean(np.abs(G), axis=0)                                      # (R, nfl) 결합 세기
    gref = np.array([np.median(gbar[r][band]) for r in range(R)])
    weff = [EFFORT * np.sqrt(float(np.sum(X[0] ** 2)) / max(band.sum(), 1)) *
            np.where(band, gref[r] / (gbar[r] + 0.05 * gref[r]), 0.0) for r in range(R)]
    scale = float(np.sqrt(np.sum(X[0] ** 2)))
    direct = pb['direct']

    def parts(v, pol):
        """경로 하나: S, 그리고 파라미터별 dS (per, nfl)."""
        a = 10 ** (v[0] / 20)
        E = np.exp(-2j * np.pi * fl * v[1] / 1000.0)
        peqs = [peaking_response(fl, np.exp(v[2 + 3 * k]), np.exp(v[3 + 3 * k]), v[4 + 3 * k]) for k in range(K)]
        b = 2 + 3 * K
        aps = [allpass2(fl, np.exp(v[b + 2 * m]), np.exp(v[b + 2 * m + 1])) for m in range(A)]
        S = a * pol * E
        for q in peqs + aps:
            S = S * q
        d = np.empty((per, fl.size), complex)
        d[0] = S * np.log(10) / 20
        d[1] = S * (-2j * np.pi * fl / 1000.0)
        h = 1e-4
        for k in range(K):
            f0, q, g = v[2 + 3 * k], v[3 + 3 * k], v[4 + 3 * k]
            for c, (dx0, dx1, dx2) in enumerate(((h, 0, 0), (0, h, 0), (0, 0, h))):
                up = peaking_response(fl, np.exp(f0 + dx0), np.exp(q + dx1), g + dx2)
                dn = peaking_response(fl, np.exp(f0 - dx0), np.exp(q - dx1), g - dx2)
                d[2 + 3 * k + c] = S * (up - dn) / (2 * h) / peqs[k]
        for m in range(A):
            f0, q = v[b + 2 * m], v[b + 2 * m + 1]
            for c, (dx0, dx1) in enumerate(((h, 0), (0, h))):
                up = allpass2(fl, np.exp(f0 + dx0), np.exp(q + dx1))
                dn = allpass2(fl, np.exp(f0 - dx0), np.exp(q - dx1))
                d[b + 2 * m + c] = S * (up - dn) / (2 * h) / aps[m]
        return S, d

    cache = {}

    def evaluate(x):
        key = x.tobytes()
        if key not in cache:
            cache.clear()
            cache[key] = [parts(x[r * per:(r + 1) * per], pols[r]) for r in range(R)]
        return cache[key]

    def res(x):
        pr = evaluate(x)
        Ssum = [sum(G[p, r] * pr[r][0] for r in range(R)) for p in range(P)]
        out = []
        for p in range(P):
            add = np.fft.irfft(Ssum[p], nf)[:n]
            out.append(WL[p] * (X[p] + add))
            out.append(sq_mu * WE[p] * add)
        st = [np.fft.irfft(pr[r][0] * LP, nf)[:n] for r in range(R)]
        lvl = sum(float(np.sum(s ** 2)) for s in st) / max(direct, 1e-30)
        over = max(0.0, 10 * np.log10(lvl + 1e-30) - pb['level_db'])
        out.append(np.array([over * scale]))
        for r in range(R):
            e = weff[r] * pr[r][0]
            out.append(e.real)
            out.append(e.imag)
        if reg > 0:                                   # 밴드 게인 벌점: reg × scale × (g / 30 dB)
            out.append(np.array([reg * scale * x[r * per + 4 + 3 * k] / 30.0 for r in range(R) for k in range(K)]))
        return np.concatenate(out)

    def jac(x):
        pr = evaluate(x)
        nfl = fl.size
        J = np.zeros((2 * P * n + 1 + 2 * R * nfl + (R * K if reg > 0 else 0), R * per))
        st = [np.fft.irfft(pr[r][0] * LP, nf)[:n] for r in range(R)]
        lvl = sum(float(np.sum(s ** 2)) for s in st) / max(direct, 1e-30)
        over = 10 * np.log10(lvl + 1e-30) - pb['level_db']
        for r in range(R):
            dS = pr[r][1]
            cols = slice(r * per, (r + 1) * per)
            for p in range(P):
                c = np.fft.irfft(G[p, r][None, :] * dS, nf, axis=1)[:, :n]       # (per, n)
                J[2 * p * n:(2 * p + 1) * n, cols] = (WL[p][None, :] * c).T
                J[(2 * p + 1) * n:(2 * p + 2) * n, cols] = (sq_mu * WE[p][None, :] * c).T
            e0 = 2 * P * n + 1 + 2 * r * nfl
            de = weff[r][None, :] * dS
            J[e0:e0 + nfl, cols] = de.real.T
            J[e0 + nfl:e0 + 2 * nfl, cols] = de.imag.T
            if over > 0:
                ct = np.fft.irfft(dS * LP[None, :], nf, axis=1)[:, :n]
                dlvl = 2 * (ct @ st[r]) / max(direct, 1e-30)
                J[2 * P * n, cols] = scale * 10 / (np.log(10) * max(lvl, 1e-30)) * dlvl   # 레벨 한도 줄(벌점 줄들 앞)
        if reg > 0:
            row0 = 2 * P * n + 1 + 2 * R * nfl
            for r in range(R):
                for k in range(K):
                    J[row0 + r * K + k, r * per + 4 + 3 * k] = reg * scale / 30.0
        return J

    try:
        sol = least_squares(res, x0, jac=jac, bounds=(lb, ub), max_nfev=MAX_NFEV, x_scale='jac')
    except np.linalg.LinAlgError:          # 09-26 맥 시뮬(돌비룸급·몰빵 3): SVD 수렴 실패 — 척도 없이 한 번 더, 그래도 안 되면 시작값
        try:
            sol = least_squares(res, x0, jac=jac, bounds=(lb, ub), max_nfev=MAX_NFEV)
        except np.linalg.LinAlgError:
            return pb['input'], x0, pols
    return pb['input'], sol.x, pols


def design_mimo_iir(engine, design, positions: list, K: int = K_BANDS, level_db: float = LEVEL_DB,
                   fsiso: float = 150.0, workers: int | None = None, A: int = A_BANDS, progress=None) -> dict:
    # K = 경로당 PEQ, A = 올패스. 바이쿼드 = K + A + 4(LR4 HP·LP). 09-26 자원 비교(서포트 많이 vs 몰빵)용으로 인자화.
    """입력 → {서포트 role: IirSupport}. design = `design_mimo` 결과(서포트 선정·대역·이상 FIR).
    입력마다 문제를 떼어(배열만) 워커 프로세스에서 동시에 푼다 — CPU 절반·낮은 우선순위(09-25 먹통 교훈)."""
    from concurrent.futures import ProcessPoolExecutor

    H = engine.long_spectra()
    f = engine.f
    n = int(LEN_S * FS_LOW)
    nf = 2 * n
    fl = np.fft.rfftfreq(nf, 1.0 / FS_LOW)
    LP = lr4(fl, fsiso, 'LP')                 # DSP 에서는 Fsiso LR4 LP(IIR)
    problems = []
    for i, sup in design.supports.items():
        if not sup:                            # FIR 가드가 이 입력의 MIMO 를 끈 경우(09-26 맥 시뮬: 빈 경로로 워커가 죽었다)
            continue
        roles = list(sup)
        X, WL, WE, G = [], [], [], []
        for p in positions:
            x = to_low(engine.input_response(i, H, p), f, n)
            onset = int(np.argmax(np.abs(x) > 0.1 * np.max(np.abs(x))))
            wl = np.zeros(n)
            wl[onset + int(EARLY_MS / 1000 * FS_LOW):] = 1.0
            X.append(x)
            WL.append(wl)
            WE.append(1.0 - wl)
            G.append([np.fft.rfft(to_low(support_path(engine, design, j, H, p), f, n), nf) * LP for j in roles])
        problems.append({'input': i, 'roles': roles, 'K': K, 'fl': fl, 'LP': LP, 'nf': nf, 'n': n,
                         'X': np.array(X), 'WL': np.array(WL), 'WE': np.array(WE), 'G': np.array(G),
                         'fir': {j: sup[j] for j in roles}, 'ranges': {j: design.ranges[j] for j in roles},
                         'modes': late_modes(X, A, 20.0, fsiso), 'A': A, 'fsiso': fsiso, 'level_db': level_db,
                         'direct': float(np.sum(to_low(engine.chain[i]['F'], f, n) ** 2))})
    n_workers = workers or _workers(len(problems))
    # progress(끝난 입력 수, 전체) — 화면의 'MIMO 계산 중… (k/n)'(10-09 사용자: 얼마나 돌았는지 알 수 없다).
    total = len(problems)
    if n_workers <= 1:
        results = []
        for pb in problems:
            results.append(_solve_problem(pb))
            if progress is not None:
                progress(len(results), total)
    else:
        one_blas_thread_for_workers()
        pool = ProcessPoolExecutor(max_workers=n_workers, initializer=_low_priority)
        _ACTIVE_POOLS.add(pool)
        try:
            from concurrent.futures import as_completed
            futures = [pool.submit(_solve_problem, pb) for pb in problems]
            done = 0
            for _future in as_completed(futures):
                done += 1
                if progress is not None:
                    progress(done, total)
            results = [future.result() for future in futures]       # 입력 순서 그대로(결과는 map 과 같다)
        finally:
            _ACTIVE_POOLS.discard(pool)
            pool.shutdown(wait=False, cancel_futures=True)
    out = {}
    target = 10 ** (level_db / 10)
    for (i, x, pols), pb in zip(results, problems):
        sups = _unpack(x, K, pols, A)
        ss = [s.response(fl) for s in sups]
        lvl = sum(float(np.sum(np.fft.irfft(s * LP, nf)[:n] ** 2)) for s in ss) / max(pb['direct'], 1e-30)
        if lvl > target:                       # 세기 확인 — 넘으면 게인을 같이 낮춘다
            cut = 10 * np.log10(lvl / target)
            for s in sups:
                s.gain_db -= cut
        out[i] = dict(zip(pb['roles'], sups))
        # 안 나빠지게(09-26): 대역 늦은 에너지(> WORSE_DB)나 모드 T60(> T60_WORSE_S)이 나빠지면 먼저 **그 원인 경로만**
        # 하나씩 뺀다(뺐을 때 위반이 가장 줄어드는 경로부터, 최대 경로 수의 절반). 그래도 나쁘면 전체 게인을 3 dB 씩
        # 낮추고(최대 12 dB), 끝내 나쁘면 이 입력의 MIMO 를 끈다. 전체를 한꺼번에 낮추면 효과가 −33 → −26 dB 로 줄었다.
        def violation(sups: dict) -> float:
            ss = [sups[j].response(fl) for j in sups]
            roles_now = list(sups)
            idx = [pb['roles'].index(j) for j in roles_now]
            ys = [pb['X'][p] + (np.fft.irfft(sum(pb['G'][p, r] * ss[k] for k, r in enumerate(idx)), nf)[:n] if ss else 0.0)
                  for p in range(pb['X'].shape[0])]
            band = _worst_band_db({**pb, 'roles': roles_now, 'G': pb['G'][:, idx] if idx else pb['G'][:, :0]}, sups, fl, nf, n) if sups else -99.0
            return max(band - WORSE_DB, worst_t60_change(list(pb['X']), ys) - T60_WORSE_S * WORSE_DB)   # 꼬리 한도도 WORSE_DB 를 따른다

        cur = dict(out[i])
        v = violation(cur)
        removed = []
        while v > 0 and len(removed) < max(1, len(pb['roles']) // 2) and len(cur) > 1:
            trials = {j: violation({k: s_ for k, s_ in cur.items() if k != j}) for j in cur}
            j_best = min(trials, key=trials.get)
            if trials[j_best] >= v:
                break
            removed.append(j_best)
            cur.pop(j_best)
            v = trials[j_best]
        for step in range(5):
            if v <= 0:
                break
            if step == 4:
                cur = {}
                break
            for s_ in cur.values():
                s_.gain_db -= 3.0
            v = violation(cur)
        out[i] = cur
        if removed:
            print(f'[MIMO IIR] {i}: 경로 뺌 {removed}', flush=True)
    return out


def _worst_band_db(pb: dict, sups: dict, fl, nf: int, n: int) -> float:
    """설계 위치 평균, 도착 + 60 ms 뒤 에너지를 1/3 oct 대역(20 Hz–Fsiso)마다 전·후 비교한 가장 나빠진 dB."""
    X, WL, G = pb['X'], pb['WL'], pb['G']
    ss = [sups[j].response(fl) for j in pb['roles']] if sups else []
    edges = 20.0 * 2 ** (np.arange(0, np.log2(pb['fsiso'] / 20.0) + 1e-9, 1 / 3))
    worst = -99.0
    # 모드 단위(09-26: 대역 에너지는 줄어도 모드 하나가 0.8 → 1.6 s 로 길어졌다): 1/3 oct 대역에 더해 늦은 스펙트럼의
    # 센 봉우리 6개 둘레 ±1.5 Hz 도 따로 본다.
    lows, highs = list(edges[:-1]), list(edges[1:])
    for fm in _late_peaks(list(pb['X']), 6):
        lows.append(fm - 1.5)
        highs.append(fm + 1.5)
    edges_pairs = list(zip(lows, highs))
    before = np.zeros(len(edges_pairs))
    after = np.zeros(len(edges_pairs))
    for p in range(X.shape[0]):
        add = np.fft.irfft(sum(G[p, r] * ss[r] for r in range(len(ss))), nf)[:n] if ss else 0.0
        for tag, y in (('b', X[p]), ('a', X[p] + add)):
            Y = np.abs(np.fft.rfft(WL[p] * y, nf)) ** 2
            e = np.array([Y[(fl >= lo) & (fl < hi)].sum() for lo, hi in edges_pairs])
            if tag == 'b':
                before += e
            else:
                after += e
    ok = before > 0
    if ok.any():
        worst = float(np.max(10 * np.log10((after[ok] + 1e-30) / before[ok])))
    return worst


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


def apply_iir(engine, design, iir: dict, role: str, H: dict, pos: int, n: int, fsiso: float = 150.0):
    """위치 pos 에서 입력 role 의 저속 IR (IIR MIMO 적용 후)."""
    f = engine.f
    nf = 2 * n
    fl = np.fft.rfftfreq(nf, 1.0 / FS_LOW)
    LP = lr4(fl, fsiso, 'LP')
    y = to_low(engine.input_response(role, H, pos), f, n)
    for j, s in iir.get(role, {}).items():
        g = np.fft.rfft(to_low(support_path(engine, design, j, H, pos), f, n), nf) * LP
        y = y + np.fft.irfft(g * s.response(fl), nf)[:n]
    return y
