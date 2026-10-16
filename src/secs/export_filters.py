# -*- coding: utf-8 -*-
"""새 엔진 결과 → 필터 계획(채널마다 거는 필터의 순서와 값) → 텍스트 · Equalizer APO 설정(2026-10-15).

누가 부르나: 내보내기 화면의 '필터 내보내기' 카드(`secs.calibration.export_filter_text` · `export_eqapo`).
신호 흐름은 카밀라 내보내기(`secs.export_camilladsp.make_config`)와 같다 — 같은 결과 객체에서 같은 순서로 뽑는다:
  입력 k: PEQ(입력 보정) → FIR(입력 보정) ─┬─ 메인 경로: 게인 → XO PEQ → LR4 HP → 딜레이 → 메인 출력 k
                                           └─ 서브 센드: LR4 LP ─┐
  입력 LFE(서브가 있으면): +10 dB → LR4 LP 120 Hz ─ 서브 센드 ─┐
  서브 버스: Σ 센드 × (서브마다) 게인·극성 → 버스 PEQ → 고정 대역 필터·PEQ → 올패스 → 딜레이 → 서브 출력
  MIMO 서포트 경로: 입력 원신호(입력 보정 전) → 경로 필터 → 서포트 스피커 출력에 더함.
`plan_responses` 는 이 계획의 전기 응답(입력 → 출력)을 엔진과 같은 응답 함수로 계산한다 — 테스트가 엔진 최종 응답과
대조하고(`tests/test_export_filters.py`), EqAPO 머리 여유(Preamp)도 이것으로 정한다.

텍스트(사람이 아무 DSP 에나 옮겨 넣을 값, 한국어 UTF-8 BOM) = `SECS 필터 <모드>.txt` + FIR wav.
EqAPO(10-15 사용자: "입출력만 설정하면 바로 쓸 수 있게 설정 파일째로") = 내보낸 폴더 안의 것을 EqAPO 의 config 폴더에
그대로 복사하면 끝:
  config.txt          — 맨 위 `Eval:` 두 줄(입력 · 출력 장치 채널 번호, 0 = 안 씀)만 사람이 고친다. 그 아래: 머리 여유
                        Preamp · 입력 복사 · `Include: <하위 폴더>\\secs_<모드>.txt` · 출력 복사.
  <하위 폴더>/        — 필터 본체(고치지 않음) + FIR wav(Convolution 경로는 그 파일 기준이라 파일 이름만).
  EqAPO 적용 방법.txt — 한국어 안내(번호표 포함).
EqAPO 는 채널 하나에 한 신호만 들 수 있어 메인 · 센드 · 서브 버스 · MIMO 갈래를 가상 채널(`VML` · `VSL` · `VBSW1` ·
`VP1` …, 장치에 없는 이름)로 나눠 처리하고 맨 끝 Copy 로 장치 채널에 낸다. EqAPO 1.4.2 Benchmark 로 확인한 두 함정(10-15):
① `Channel:` 은 이름을 대문자로 바꿔 찾는데 Copy 는 적힌 그대로 만든다 → 가상 채널 이름은 **모두 대문자**(전에는 `vmL` 이라
`Invalid channel position VML` — 필터가 하나도 안 걸렸다). ② 장치에 없는 채널 번호를 Copy 원본으로 쓰면 **상수 1(직류)** 이
된다 → 입력은 0 으로 비운 뒤 `If:` 로 장치에 있는 번호만 복사하고, 출력도 `If:` 로 거른다(전에는 8채널 장치에서 탑 9–12 가
직류가 되어 서브 출력으로 나갔다). 이름은 'V' 로 시작해 EqAPO 표준 이름(L R C LFE RL RR RC SL SR)과 겹치지 않는다.
EqAPO 파일은 ASCII 만 쓴다(주석 영어) — 한국어 안내는 따로 둔 txt 에.
"""
from __future__ import annotations

import time
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np

from secs.export_camilladsp import (LFE_GAIN_DB, LFE_LP_HZ, SAMPLE_RATE, fir_to_48k, has_lfe, mimo_paths,
                                    write_wav)

#: 대역 필터 종류 → 2차 섹션 Q 들(차례로 곱한다). LR 2N = 버터워스 N 의 제곱, LR12 = 2차 Q 0.5 하나.
SECTION_Q = {'LR12': (0.5,), 'LR24': (0.70711, 0.70711), 'LR48': (0.5412, 1.30656, 0.5412, 1.30656),
             'BW12': (0.70711,), 'BW24': (0.5412, 1.30656)}

#: 윈도우 스피커 위치(WAVEFORMATEXTENSIBLE 채널 마스크 비트 순서) — 장치 채널 번호는 있는 위치를 이 순서로 센다.
#: FL FR FC LFE BL BR FLC FRC BC SL SR TC TFL TFC TFR TBL TBC TBR. 맞는 자리가 없는 역할(Tml · 하이트 · 두 번째 서브 …)은
#: 그 뒤 번호를 차례로 받는다 — 장치 구성이 다르면 사람이 config.txt 의 번호만 고친다.
WINDOWS_POSITION = {'L': 0, 'R': 1, 'C': 2, 'LFE': 3, 'Lb': 4, 'Rb': 5, 'Lw': 6, 'Rw': 7,
                    'Ls': 9, 'Rs': 10, 'Tfl': 12, 'Tfr': 14, 'Tbl': 15, 'Tbr': 17}
WINDOWS_NAMES = {0: 'FL', 1: 'FR', 2: 'FC', 3: 'LFE', 4: 'BL', 5: 'BR', 6: 'FLC', 7: 'FRC', 8: 'BC', 9: 'SL', 10: 'SR',
                 11: 'TC', 12: 'TFL', 13: 'TFC', 14: 'TFR', 15: 'TBL', 16: 'TBC', 17: 'TBR'}

EQAPO_GUIDE = 'EqAPO 적용 방법.txt'


# ──────────────────────────── 계획 ────────────────────────────
# 필터 한 칸 = 튜플: ('peq', f, q, dB) · ('fir', 이름, 48 kHz 탭) · ('gain', dB) · ('invert',) · ('hp'|'lp', f, 종류)
# · ('ap', f, q) · ('delay', ms). 고정 서브 필터는 ('hp'|'lp'|'peq', …, 'fixed') 처럼 끝에 'fixed' 를 단다(대조 테스트가 뺀다).
@dataclass
class SubFeed:
    sources: list                     # 이 서브 버스에 더하는 입력(센드가 있는 메인 역할 · 'LFE')
    gain_db: float
    polarity: int
    ops: list


@dataclass
class MimoFeed:
    input: str
    support: str
    ops: list


@dataclass
class FilterPlan:
    inputs: list                      # 메인 역할 + ('LFE')
    outputs: list                     # 메인 역할 + 서브 역할
    input_ops: dict = field(default_factory=dict)
    main_ops: dict = field(default_factory=dict)
    send_ops: dict = field(default_factory=dict)      # XO 가 있는 메인만
    lfe_ops: list = field(default_factory=list)
    subs: dict = field(default_factory=dict)          # 서브 역할 → SubFeed
    mimo: list = field(default_factory=list)          # [MimoFeed]

    def firs(self) -> dict:
        """FIR 이름 → 48 kHz 탭(계획에 나오는 순서)."""
        out = {}
        for ops in (*self.input_ops.values(), *(m.ops for m in self.mimo)):
            for op in ops:
                if op[0] == 'fir':
                    out[op[1]] = op[2]
        return out


def make_plan(eng, res, mimo=None, iir: dict | None = None, fsiso: float = 150.0,
              lfe_gain_db: float | None = LFE_GAIN_DB, lfe_lp_hz: float = LFE_LP_HZ) -> FilterPlan:
    """엔진 결과 → `FilterPlan`. 인자 뜻은 `export_camilladsp.make_config` 와 같다(MIMO: IIR 이면 `iir`, FIR 이면 None)."""
    mains, subs = list(res.channels), list(res.subs)
    lfe = has_lfe(res, lfe_gain_db)
    plan = FilterPlan(inputs=mains + (['LFE'] if lfe else []), outputs=mains + subs)
    for role in mains:
        c = res.channels[role]
        ops = [('peq', float(f0), float(q), float(g)) for f0, q, g in c.peq]
        fir = np.asarray(c.fir, float)
        if fir.size > 1:                                # 최소 지연 모드(IIR 만)는 FIR 이 [1.0] — 넣지 않는다
            ops.append(('fir', f'{role}_fir', fir))
        plan.input_ops[role] = ops
        main = [('gain', float(c.gain_db))] + [('peq', float(f0), float(q), float(g)) for f0, q, g in eng.top_peq.get(role, [])]
        if c.xo_hz:
            main.append(('hp', float(c.xo_hz), 'LR24'))
            plan.send_ops[role] = [('lp', float(c.xo_hz), 'LR24')]
        main.append(('delay', float(c.delay_ms)))
        plan.main_ops[role] = main
    if lfe:
        plan.lfe_ops = [('gain', float(lfe_gain_db)), ('lp', float(lfe_lp_hz), 'LR24')]

    def fixed(role) -> list:
        a = res.subs.get(role)
        if a is None:
            return []
        ops = []
        for kind, hz, typ in (('hp', getattr(a, 'hp_hz', 0.0), getattr(a, 'hp_type', 'LR24')),
                              ('lp', getattr(a, 'lp_hz', 0.0), getattr(a, 'lp_type', 'LR24'))):
            if hz:
                ops.append((kind, float(hz), typ, 'fixed'))
        ops += [('peq', float(f0), float(q), float(g), 'fixed') for f0, q, g in (getattr(a, 'peq', []) or [])]
        return ops

    sources = [r for r in mains if res.channels[r].xo_hz] + (['LFE'] if lfe else [])
    for role, a in res.subs.items():
        ops = [('peq', float(f0), float(q), float(g)) for f0, q, g in eng.sub_bus_peq] + fixed(role)
        ops += [('ap', float(f0), float(q)) for f0, q in a.allpass] + [('delay', float(a.delay_ms))]
        plan.subs[role] = SubFeed(list(sources), float(a.gain_db), 1 if a.polarity > 0 else -1, ops)

    paths = mimo_paths(eng, mimo, iir) if mimo is not None and (iir or iir is None) else []
    for i, j, sp, low, d_ms in paths:
        if isinstance(sp, np.ndarray):                  # FIR MIMO: 지연 → HP(하한) → FIR(Fsiso 저역 통과 포함)
            ops = [('delay', d_ms), ('hp', low, 'LR24'), ('fir', f'mimo_{i}_{j}', fir_to_48k(sp, getattr(mimo, 'fs_low', 600)))]
        else:                                           # IIR MIMO: 게인·극성 → 지연 → PEQ → 올패스 → HP(하한) → LP(Fsiso)
            ops = [('gain', float(sp.gain_db))] + ([('invert',)] if sp.polarity < 0 else [])
            ops += [('delay', float(sp.delay_ms) + d_ms)] + [('peq', float(f0), float(q), float(g)) for f0, q, g in sp.peq]
            ops += [('ap', float(f0), float(q)) for f0, q in sp.allpass] + [('hp', low, 'LR24'), ('lp', float(fsiso), 'LR24')]
        plan.mimo.append(MimoFeed(i, j, ops + fixed(j)))
    return plan


def is_fixed(op) -> bool:
    """고정 서브 필터 칸인가(끝에 'fixed' — FIR 칸은 끝이 탭 배열이라 문자열인지부터 본다)."""
    return isinstance(op[-1], str) and op[-1] == 'fixed'


# ──────────────────────────── 응답(대조 · 머리 여유) ────────────────────────────
def op_response(op, f: np.ndarray, skip_fixed: bool = False) -> np.ndarray | float:
    """필터 한 칸의 복소 응답 — 엔진과 같은 함수(PEQ = RBJ 디지털, HP·LP·올패스 = 아날로그 원형).
    FIR 은 `f` 가 `rfftfreq(n, 1/48000)` 격자라고 본다."""
    from secs.correction_engine import allpass2, band_filter, peaking_response

    kind = op[0]
    if skip_fixed and is_fixed(op):
        return 1.0
    if kind == 'peq':
        return peaking_response(f, op[1], op[2], op[3])
    if kind == 'fir':
        return np.fft.rfft(op[2], 2 * (f.size - 1))
    if kind == 'gain':
        return 10 ** (op[1] / 20)
    if kind == 'invert':
        return -1.0
    if kind in ('hp', 'lp'):
        return band_filter(f, op[1], kind.upper(), op[2])
    if kind == 'ap':
        return allpass2(f, op[1], op[2])
    if kind == 'delay':
        return np.exp(-2j * np.pi * f * op[1] / 1000.0)
    raise ValueError(kind)


def chain(ops, f, skip_fixed: bool = False) -> np.ndarray:
    out = np.ones(f.size, complex)
    for op in ops:
        out = out * op_response(op, f, skip_fixed)
    return out


def plan_responses(plan: FilterPlan, f: np.ndarray, source: str, skip_fixed: bool = False) -> dict:
    """입력 `source`(메인 역할 또는 'LFE') → 출력마다 전기 응답(복소). 경로가 없는 출력은 빠진다."""
    out: dict = {}

    def add(role, h):
        out[role] = out.get(role, 0) + h

    if source == 'LFE':
        pre, send = chain(plan.lfe_ops, f), 1.0
    else:
        pre = chain(plan.input_ops[source], f)
        add(source, pre * chain(plan.main_ops[source], f))
        send = chain(plan.send_ops[source], f) if source in plan.send_ops else None
    for role, feed in plan.subs.items():
        if source in feed.sources and send is not None:
            add(role, pre * send * feed.polarity * 10 ** (feed.gain_db / 20) * chain(feed.ops, f, skip_fixed))
    for m in plan.mimo:
        if m.input == source:
            add(m.support, chain(m.ops, f, skip_fixed))
    return out


def headroom_db(plan: FilterPlan) -> float:
    """메인 입력 하나가 출력 하나에서 가장 크게 올라가는 dB(20 Hz–20 kHz) — 0 아래면 0. EqAPO Preamp 로 미리 낮춘다.
    LFE 는 뺀다: 돌비 표준 +10 dB 까지 넣으면 LFE 를 안 쓰는 스테레오 재생도 늘 그만큼 작아진다(안내문에 따로 적는다)."""
    taps = max([np.asarray(t).size for t in plan.firs().values()] or [1])
    n = max(1 << 16, 1 << int(np.ceil(np.log2(max(taps, 2)))))
    f = np.fft.rfftfreq(n, 1.0 / SAMPLE_RATE)
    band = (f >= 20.0) & (f <= 20000.0)
    peak = 0.0
    for source in [r for r in plan.inputs if r != 'LFE']:
        for h in plan_responses(plan, f, source).values():
            peak = max(peak, float(np.max(np.abs(np.asarray(h) * np.ones(f.size))[band])))
    return max(0.0, 20 * np.log10(peak)) if peak > 0 else 0.0


# ──────────────────────────── 파일 이름 · 번호 ────────────────────────────
def safe_name(name: str, default: str = 'secs_calibration') -> str:
    return ''.join(ch if (ch.isascii() and (ch.isalnum() or ch in '-_.')) else '_' for ch in name) or default


def write_firs(plan: FilterPlan, folder: Path, prefix: str) -> dict:
    """FIR wav(48 kHz float32 모노)를 쓰고 이름 → 파일 이름을 돌려준다."""
    names = {}
    for stem, taps in plan.firs().items():
        folder.mkdir(parents=True, exist_ok=True)
        names[stem] = f'{prefix}{stem}.wav'
        write_wav(folder / names[stem], taps)
    return names


def windows_channels(plan: FilterPlan) -> tuple[dict, dict]:
    """(입력 → 장치 채널 번호, 출력 → 장치 채널 번호), 1부터. 메인은 같은 번호로 들어와 같은 번호로 나간다(EqAPO 는
    장치 채널을 제자리에서 고친다). 첫 서브와 LFE 입력은 LFE 자리. 자리가 없는 출력은 맨 뒤 번호를 차례로."""
    subs = [r for r in plan.outputs if r in plan.subs]
    pos = {}
    for role in plan.outputs:
        if role in plan.subs:
            if role == subs[0]:
                pos[role] = WINDOWS_POSITION['LFE']
        elif role in WINDOWS_POSITION:
            pos[role] = WINDOWS_POSITION[role]
    used = sorted(set(pos.values()) | ({WINDOWS_POSITION['LFE']} if 'LFE' in plan.inputs else set()))
    number = {p: k + 1 for k, p in enumerate(used)}
    out_num = {r: number[pos[r]] for r in pos}
    nxt = len(used)
    for role in plan.outputs:
        if role not in out_num:
            nxt += 1
            out_num[role] = nxt
    in_num = {r: (number[WINDOWS_POSITION['LFE']] if r == 'LFE' else out_num[r]) for r in plan.inputs}
    return in_num, out_num


def windows_name(role: str, plan: FilterPlan) -> str:
    """번호표에 쓰는 윈도우 스피커 이름(자리가 없으면 '-')."""
    subs = [r for r in plan.outputs if r in plan.subs]
    if role == 'LFE' or (subs and role == subs[0]):
        return 'LFE'
    return WINDOWS_NAMES.get(WINDOWS_POSITION.get(role, -1), '-') if role not in plan.subs else '-'


# ──────────────────────────── EqAPO ────────────────────────────
def _v(kind: str, role: str) -> str:
    """가상 채널 이름 — 모두 대문자(모듈 독스트링 ①)."""
    return {'main': 'VM', 'send': 'VS', 'sub': 'VB'}[kind] + role.upper()


LFE_V = 'VLFE'


def _vp(k: int) -> str:
    return f'VP{k + 1}'


def _vin(role: str) -> str:
    return LFE_V if role == 'LFE' else _v('main', role)


def _eqapo_ops(ops, channel: str, firs: dict) -> list[str]:
    lines = []
    for op in ops:
        kind = op[0]
        if kind == 'peq':
            lines.append(f'Filter: ON PK Fc {op[1]:.2f} Hz Gain {op[3]:.2f} dB Q {op[2]:.4f}')
        elif kind == 'fir':
            lines.append(f'Convolution: {firs[op[1]]}')
        elif kind == 'gain':
            if abs(op[1]) >= 0.005:
                lines.append(f'Preamp: {op[1]:.2f} dB')
        elif kind == 'invert':
            lines.append(f'Copy: {channel}=-1.0*{channel}')
        elif kind in ('hp', 'lp'):
            lines += [f'Filter: ON {kind.upper()}Q Fc {op[1]:.2f} Hz Q {q:.4f}' for q in SECTION_Q[op[2]]]
        elif kind == 'ap':
            lines.append(f'Filter: ON AP Fc {op[1]:.2f} Hz Q {op[2]:.4f}')
        elif kind == 'delay':
            if op[1] > 0:
                lines.append(f'Delay: {op[1]:.3f} ms')
        else:
            raise ValueError(kind)
    return lines


def eqapo_filters(plan: FilterPlan, firs: dict, note: str = '') -> str:
    """필터 본체(가상 채널 VM*/VLFE 를 받아 VM*/VB*/VP* 에 결과를 남긴다) — config.txt 가 Include 한다."""
    L = ['# SECS 2 calibration filters for Equalizer APO (generated - do not edit).',
         '# Included by config.txt between the input Copy and the output Copy.']
    if note:
        L.append('# ' + note.encode('ascii', 'ignore').decode('ascii'))
    if plan.mimo:                                       # MIMO 갈래는 입력 보정 전 원신호
        L += ['', '# MIMO support paths: copy of the raw input',
              'Copy: ' + ' '.join(f'{_vp(k)}={_v("main", m.input)}' for k, m in enumerate(plan.mimo))]
    L += ['', '# 1) input correction (PEQ -> FIR)']
    for role in plan.input_ops:
        body = _eqapo_ops(plan.input_ops[role], _v('main', role), firs)
        if body:
            L += [f'Channel: {_v("main", role)}'] + body
    if plan.send_ops:
        L += ['', '# 2) sub sends (LR4 lowpass at each crossover)',
              'Copy: ' + ' '.join(f'{_v("send", r)}={_v("main", r)}' for r in plan.send_ops)]
        for role, ops in plan.send_ops.items():
            L += [f'Channel: {_v("send", role)}'] + _eqapo_ops(ops, _v('send', role), firs)
    L += ['', '# 3) main paths (gain -> XO PEQ -> LR4 highpass -> delay)']
    for role, ops in plan.main_ops.items():
        body = _eqapo_ops(ops, _v('main', role), firs)
        if body:
            L += [f'Channel: {_v("main", role)}'] + body
    if plan.lfe_ops:
        L += ['', '# 4) LFE input (+10 dB, LR4 lowpass) -> every sub bus', f'Channel: {LFE_V}'] + _eqapo_ops(plan.lfe_ops, LFE_V, firs)
    if plan.subs:
        L += ['', '# 5) sub buses: sum of sends x gain/polarity -> bus PEQ -> fixed filters -> allpass -> delay']
        for role, feed in plan.subs.items():
            k = feed.polarity * 10 ** (feed.gain_db / 20)
            srcs = '+'.join(f'{k:.6f}*{LFE_V if s == "LFE" else _v("send", s)}' for s in feed.sources) or '0.0'
            L += [f'# sub {role}: gain {feed.gain_db:+.2f} dB, polarity {"normal" if feed.polarity > 0 else "inverted"}',
                  f'Copy: {_v("sub", role)}={srcs}', f'Channel: {_v("sub", role)}'] + _eqapo_ops(feed.ops, _v('sub', role), firs)
    if plan.mimo:
        L += ['', '# 6) MIMO support paths']
        for k, m in enumerate(plan.mimo):
            L += [f'# path {k + 1}: input {m.input} -> speaker {m.support}', f'Channel: {_vp(k)}'] + _eqapo_ops(m.ops, _vp(k), firs)
    L += ['', 'Channel: all']
    return '\n'.join(L) + '\n'


def eqapo_config(plan: FilterPlan, include: str, preamp_db: float, in_num: dict, out_num: dict,
                 latency_ms: float | None = None) -> str:
    """config.txt — 사람이 고치는 것은 맨 위 `Eval:` 두 줄의 번호뿐(0 = 안 씀).
    입력: 가상 채널을 0 으로 비운 뒤, 번호가 1 이상이고 장치에 있을 때만 복사(없는 번호가 직류가 되지 않게 — 독스트링 ②).
    출력: 입력으로 쓴 장치 채널을 먼저 비우고(원신호가 새지 않게) 출력마다 같은 조건으로 쓴다."""
    def out_src(role):
        own = _v('sub', role) if role in plan.subs else _v('main', role)
        return '+'.join([own] + [_vp(k) for k, m in enumerate(plan.mimo) if m.support == role])

    def guarded(var: str, count: str, line: str) -> list[str]:
        return [f'If: {var} >= 1 && {var} <= {count}', f'  {line}', 'EndIf:']

    rows = [f'#   {r:6s} input {in_num[r]:2d}' + (f'   output {out_num[r]:2d}' if r in out_num else '')
            for r in plan.inputs] + [f'#   {r:6s}            output {out_num[r]:2d}' for r in plan.outputs if r not in plan.inputs]
    L = ['# SECS 2 calibration - Equalizer APO config (generated)',
         '# Copy everything in this folder into the Equalizer APO config folder',
         '# (C:\\Program Files\\EqualizerAPO\\config) - keep the old config.txt somewhere first.',
         '# Device format: 48000 Hz (the FIR wav files are 48 kHz). Speaker setup: enough channels for the numbers below.',
         '#',
         '# Default channel numbers (1 = first channel of the device, Windows order FL FR FC LFE BL BR SL SR ...):'] + rows
    if latency_ms is not None:
        L.append(f'# Latency of the filters: {float(latency_ms):.1f} ms (video: delay the picture or set the player audio offset).')
    L += ['',
          '# ==== EDIT ONLY THESE TWO LINES: device channel number of each SECS input / output (0 = not used) ====',
          'Eval: ' + '; '.join(f'in{r} = {in_num[r]}' for r in plan.inputs),
          'Eval: ' + '; '.join(f'out{r} = {out_num[r]}' for r in plan.outputs),
          '# ==== do not edit below ====',
          '', '# Headroom for the largest boost of the correction (avoids clipping).', 'Channel: all',
          f'Preamp: {-preamp_db:.1f} dB' if preamp_db > 0 else 'Preamp: 0.0 dB',
          '', '# Inputs: start silent, then copy each device channel that exists (a missing channel stays silent).',
          'Copy: ' + ' '.join(f'{_vin(r)}=0.0' for r in plan.inputs)]
    for r in plan.inputs:
        L += guarded(f'in{r}', 'inputChannelCount', f'Copy: {_vin(r)}=`in{r}`')
    L += ['', f'Include: {include}', '',
          '# Outputs: clear the input channels first (no raw signal leaks), then write each SECS output.']
    for r in plan.inputs:
        L += guarded(f'in{r}', 'outputChannelCount', f'Copy: `in{r}`=0.0')
    for r in plan.outputs:
        L += guarded(f'out{r}', 'outputChannelCount', f'Copy: `out{r}`={out_src(r)}')
    L += ['Channel: all']
    return '\n'.join(L) + '\n'


def eqapo_guide(plan: FilterPlan, in_num: dict, out_num: dict, sub_dir: str, mode_label: str,
                latency_ms: float | None, preamp_db: float) -> str:
    """한국어 안내(EqAPO 적용 방법.txt)."""
    L = [f'SECS 2 보정 → Equalizer APO ({mode_label})', '',
         '■ 적용 순서',
         '1. Equalizer APO 를 설치하고 Configurator 에서 쓸 출력 장치(오디오 인터페이스)에 체크한 뒤 재부팅합니다.',
         '2. 윈도우 소리 설정에서 그 장치의 형식을 48000 Hz 로, 스피커 구성을 아래 번호가 다 들어가는 채널 수로 맞춥니다.',
         '   (FIR wav 가 48 kHz 라 샘플레이트가 다르면 FIR 이 빠집니다.)',
         '3. C:\\Program Files\\EqualizerAPO\\config 의 config.txt 를 다른 이름으로 보관합니다(예: config_old.txt).',
         f'4. 이 폴더 안의 것(config.txt 와 {sub_dir} 폴더)을 그대로 C:\\Program Files\\EqualizerAPO\\config 에 복사합니다.',
         '5. 끝 — EqAPO 는 저장하면 바로 다시 읽습니다. Editor 를 열어 config.txt 에 빨간 줄(오류)이 없는지 봅니다.',
         '',
         '■ 채널 번호(장치 채널, 1부터 — 윈도우 순서 FL FR FC LFE BL BR SL SR …)',
         '   역할    입력  출력  윈도우 이름']
    for r in plan.inputs + [r for r in plan.outputs if r not in plan.inputs]:
        a = f'{in_num[r]:4d}' if r in in_num else '   -'
        b = f'{out_num[r]:4d}' if r in out_num else '   -'
        L.append(f'   {r:6s} {a}  {b}  {windows_name(r, plan)}')
    L += ['   배선이 다르면 config.txt 맨 위 Eval 두 줄의 숫자만 고칩니다(0 = 안 씀, 장치에 없는 번호는 저절로 건너뜀):',
          '   · 입력: "Eval: inL = 1; …" — 그 소리가 들어오는 장치 채널',
          '   · 출력: "Eval: outL = 1; …" — 그 스피커가 꽂힌 장치 채널',
          '   필터 본체(' + sub_dir + ' 폴더 안)는 고치지 않습니다.', '',
          '■ 알아 둘 것',
          f'· 머리 여유: 보정의 가장 큰 부스트만큼 Preamp 를 {preamp_db:.1f} dB 낮춰 두었습니다(클리핑 방지). '
          '앰프·인터페이스 볼륨으로 레벨을 다시 맞춥니다.',
          '· 기존 EQ·룸 보정(다른 config 파일, 플레이어 EQ, 장치 드라이버의 베이스 관리·업믹스)은 끕니다 — 겹치면 보정이 틀어집니다.']
    if latency_ms is not None:
        L.append(f'· 지연: 필터 {float(latency_ms):.1f} ms. 영상은 플레이어의 오디오 지연으로 맞춥니다.')
    if plan.lfe_ops:
        L.append('· LFE 입력은 +10 dB, 120 Hz 로우패스 뒤 모든 서브로 갑니다(돌비 재생 표준). 머리 여유에는 넣지 않았습니다 — '
                 '영화에서 서브가 찢어지면 config.txt 의 Preamp 를 더 낮춥니다.')
    return '\n'.join(L) + '\n'


def write_eqapo(plan: FilterPlan, out: Path, name: str, mode_label: str = '', note: str = '',
                latency_ms: float | None = None) -> list[Path]:
    """`out/config.txt` + `out/<name>/<name>.txt` + `out/<name>/<name>_*.wav` + 안내. 쓴 파일(config.txt 맨 앞)."""
    out = Path(out)
    safe = safe_name(name)
    sub_dir = out / safe
    sub_dir.mkdir(parents=True, exist_ok=True)
    firs = write_firs(plan, sub_dir, safe + '_')
    (sub_dir / f'{safe}.txt').write_text(eqapo_filters(plan, firs, note), encoding='ascii')
    in_num, out_num = windows_channels(plan)
    preamp = float(np.ceil(headroom_db(plan) * 2) / 2)          # 0.5 dB 단위로 올림
    config = out / 'config.txt'
    config.write_text(eqapo_config(plan, f'{safe}\\{safe}.txt', preamp, in_num, out_num, latency_ms), encoding='ascii')
    guide = out / EQAPO_GUIDE
    guide.write_text(eqapo_guide(plan, in_num, out_num, safe, mode_label, latency_ms, preamp), encoding='utf-8-sig')
    return [config, sub_dir / f'{safe}.txt', guide] + [sub_dir / n for n in firs.values()]


# ──────────────────────────── 텍스트 ────────────────────────────
def _db(value: float) -> str:
    """'+1.20' 꼴 — 반올림해 0 이면 '+0.00'('-0.00' 이 안 나오게)."""
    return f'{round(float(value), 2) + 0.0:+.2f}'


def _text_ops(ops, firs: dict) -> list[str]:
    lines = []
    peq = 0
    for op in ops:
        kind = op[0]
        tag = ' (고정)' if is_fixed(op) else ''
        if kind == 'peq':
            peq += 1
            lines.append(f'  PEQ {peq:<3d}  {op[1]:9.2f} Hz   Q {op[2]:7.3f}   {_db(op[3]):>7s} dB{tag}')
        elif kind == 'fir':
            lines.append(f'  FIR      {firs[op[1]]} ({np.asarray(op[2]).size:,} 탭, 48 kHz)')
        elif kind == 'gain':
            lines.append(f'  게인     {_db(op[1])} dB')
        elif kind == 'invert':
            lines.append('  극성     반전')
        elif kind in ('hp', 'lp'):
            lines.append(f'  {kind.upper()}       {op[2][:2]} {op[2][2:]}   {op[1]:9.2f} Hz{tag}')
        elif kind == 'ap':
            lines.append(f'  올패스   {op[1]:9.2f} Hz   Q {op[2]:7.3f}   (2차)')
        elif kind == 'delay':
            lines.append(f'  딜레이   {op[1]:.3f} ms ({int(round(op[1] * SAMPLE_RATE / 1000))} 샘플 @ 48 kHz)')
    return lines or ['  (없음)']


def filter_text(plan: FilterPlan, firs: dict, title: str, info: list[str]) -> str:
    """사람이 DSP 에 옮겨 넣을 값(한국어)."""
    L = [title] + info + ['', '필터는 위에서 아래 순서로 겁니다.', '· PEQ = 피킹(RBJ 바이쿼드): 중심 주파수 · Q · 게인']
    if plan.send_ops or plan.subs:
        L += ['· HP · LP: LR 24 = 링크위츠-라일리 24 dB/oct(버터워스 2차 Q 0.7071 두 개), LR 12 = 2차 Q 0.5 하나, '
              'LR 48 = 버터워스 4차 두 번, BW = 버터워스',
              '· 올패스 = 2차 올패스: 중심 주파수 · Q']
    L += ['· FIR = 같은 폴더의 wav(48 kHz, 32비트 float 모노)', '', '[신호 흐름]']
    if plan.send_ops:
        L += ['  입력 → 입력 보정(PEQ → FIR) ─┬─ 메인 경로(게인 → XO PEQ → HP → 딜레이) → 메인 출력',
              '                                └─ 서브 센드(LP) → 서브 버스']
    else:                                               # RC 만(10-15 — 텍스트 내보내기의 범위)
        L += ['  채널마다: 입력 → PEQ → FIR → 게인 → 딜레이 → 출력']
    if plan.lfe_ops:
        L.append('  LFE 입력 → 게인 → LP → 서브 버스')
    if plan.subs:
        L.append('  서브 버스: 센드 합 × 서브별 게인 · 극성 → 버스 PEQ → 고정 필터 → 올패스 → 딜레이 → 서브 출력')
    if plan.mimo:
        L.append('  MIMO 서포트: 입력 원신호(입력 보정 전) → 경로 필터 → 서포트 스피커 출력에 더함')
    rc_only = not (plan.send_ops or plan.subs or plan.mimo)
    for role in plan.input_ops:
        if rc_only:                                     # 10-15 사용자: '입력 보정 · 메인 경로' 구분이 어색 — 한 목록으로
            L += ['', f'== {role} =='] + _text_ops(plan.input_ops[role] + plan.main_ops[role], firs)
            continue
        L += ['', f'== 메인 {role} ==', '입력 보정'] + _text_ops(plan.input_ops[role], firs)
        L += ['메인 경로'] + _text_ops(plan.main_ops[role], firs)
        if role in plan.send_ops:
            L += ['서브 센드'] + _text_ops(plan.send_ops[role], firs)
    if plan.lfe_ops:
        L += ['', '== LFE 입력 =='] + _text_ops(plan.lfe_ops, firs)
    for role, feed in plan.subs.items():
        src = ' · '.join(s if s == 'LFE' else f'{s} 센드' for s in feed.sources) or '(없음 — 무음)'
        L += ['', f'== 서브 {role} ==', f'  입력     {src} 의 합',
              f'  게인     {_db(feed.gain_db)} dB', f'  극성     {"정상" if feed.polarity > 0 else "반전"}']
        L += _text_ops(feed.ops, firs)
    for k, m in enumerate(plan.mimo):
        L += ['', f'== MIMO 서포트 {k + 1}: 입력 {m.input} → 스피커 {m.support} 출력에 더함 =='] + _text_ops(m.ops, firs)
    return '\n'.join(L) + '\n'


def write_text(plan: FilterPlan, out: Path, file_name: str, wav_prefix: str, title: str, info: list[str]) -> list[Path]:
    """`out/<file_name>` (UTF-8 BOM — 메모장이 바로 연다) + FIR wav. 쓴 파일(텍스트 맨 앞)."""
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    firs = write_firs(plan, out, safe_name(wav_prefix) + '_')
    path = out / file_name
    path.write_text(filter_text(plan, firs, title, info), encoding='utf-8-sig')
    return [path] + [out / n for n in firs.values()]


def stamp() -> str:
    return time.strftime('%Y-%m-%d %H:%M')


__all__ = ['FilterPlan', 'SubFeed', 'MimoFeed', 'make_plan', 'plan_responses', 'headroom_db', 'windows_channels',
           'eqapo_filters', 'eqapo_config', 'eqapo_guide', 'write_eqapo', 'filter_text', 'write_text', 'SECTION_Q',
           'EQAPO_GUIDE']
