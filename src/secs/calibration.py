# -*- coding: utf-8 -*-
"""보정 설계(옛 6·7단계를 하나로, 2026-09-26) 계산 묶음 — 새 엔진과 MIMO 를 위저드 화면에 잇는다. Qt 없음.

누가 부르나: 위저드 '보정 설계' 화면(`secs.wizard.step_calibration`, 워커 스레드에서), 내보내기 화면
(`secs.wizard.step8_mdat`), `tests/test_calibration_step.py`·`tests/test_step8_mdat.py`.
입력: `Project`(구성·타겟·XO·`extra['calibration']` 설정) + 위치별 IR(`rewapi.session.load_project_irs` 모양: pos → role →
(ir, sr) 또는 (ir, sr, t0) — `CorrectionEngine` 이 둘 다 받는다). 출력:
  * `compute()` → `CalibrationRun`: `CorrectionEngine`(계산 끝난 것)·`CorrectionResult`·채널별 지표(`secs.scorecard.metrics`,
    보정 전·후)·표시 곡선(1/24 oct SPL·GD, 로그 격자)·풀린 RC 상한·전체 지연.
  * `compute_mimo()` → `MimoRun`: MIMO 서포트(`mimo.design_mimo` → `mimo_iir.design_mimo_iir`, 모든 모드 IIR —
    09-26 사용자: 저사양은 IIR 만, 최소 지연도 IIR, 최고사양은 IIR 이 기본)와 적용 곡선.
  * `export_mdat()`(REW 단계별 mdat — 이름에 '측정' 을 쓰지 않는다, 모두 계산값) · `export_camilladsp()` ·
    `export_filter_text()`(필터 값 텍스트) · `export_eqapo()`(EqAPO 설정째로 — `secs.export_filters`, 10-15).
  * `export_before_after_mdat()`(내보내기 단계 — 채널마다 보정 전 = 원 측정, 보정 후 = 선택한 모드의 최종 응답).
  * `rc_pieces()` — RC 상한이 어떻게 정해졌는지(엔진 노트 + 룸 볼륨), 보정 설계·내보내기 화면이 같이 쓴다.
  * `sub_combine()` — BA 보기: 서브별 정렬 전·후, 서브 합, 평탄도·좌석 편차, 정렬 값(서브 2개 이상).
  * BA 수동 설정(10-05 요구 대장 R27): `CalibrationSettings.subs_manual`(서브마다 자동/고정/묶음 · 값 · MIMO 서포트)
    → 엔진 `sub_manual`, `take_sub_values()`(마지막 결과 값을 수동 칸으로), 내보내기 노트의 서브 값(`sub_values_text`).
엔진 수학은 여기서 바꾸지 않는다 — 엔진에 넘길 값을 모으고, 결과를 화면이 바로 그릴 모양으로 편다.
"""
from __future__ import annotations

import copy
import os
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path

import numpy as np

from secs import mimo, mimo_iir
from secs.correction_engine import MODES, N_FFT, SR, CorrectionEngine, allpass2, gd_curve, lr4, smooth_power

#: 화면에 내놓는 모드(엔진 키 — 이름은 09-26 사용자: 최소 지연 · 저사양 · 최고사양). '긴 필터' 는 참고용이라 뺀다.
MODE_KEYS: tuple[str, ...] = ('zero', 'mixed', 'linear')
DEFAULT_MODE = 'mixed'
#: 초고지연 모드(09-27 사용자: 최고사양에서 숨은 버튼으로 여는 이스터에그, 지연 30–200 ms). `MODE_KEYS` 에는 없어 평소엔
#: 안 보인다. FIR 길이는 지연에서 정한다 — 엔진 안전장치(여유 지연 ≤ FIR 길이 × 0.7) 안의 가장 짧은 2의 거듭제곱,
#: 최대 16,384탭(09-27 YENA 지연별 시험: 200 ms · 16,384 에서 만족, 사용자).
ULTRA_MODE = 'ultra'
ULTRA_MS_RANGE = (30.0, 200.0)
ULTRA_MS_DEFAULT = 200.0
ULTRA_MAX_TAPS = 16384
DEFAULT_XO_HZ = 80.0
RC_LO_DEFAULT = 20.0
#: 수동 RC 상한의 범위(자동은 엔진이 슈뢰더 주파수, 최대 500 Hz 로 정한다).
RC_HI_RANGE = (100.0, 20000.0)
FSISO_DEFAULT = 150.0
FSISO_RANGE = (50.0, 150.0)             # Fsiso = 서포트 상한 ≤ 150 Hz(음상 보호)
MIMO_MIN_POSITIONS = 3                   # 측정 위치 3곳 미만이면 MIMO 를 고를 수 없다(청취 영역의 모드를 가늠할 위치가 모자라다)
#: IIR 맞춤 워커 프로세스 수 = 논리 코어 절반(최소 2). 입력마다 따로 푸는 문제라 수를 늘려도 결과는 같다.
#: 10-09: 2 개로는 돌비룸(입력 11) 이 맥에서 21.6분 — 사용자 요청으로 늘림. 워커마다 BLAS 1스레드(`one_blas_thread_for_workers`)라
#: 코어 절반만 쓴다(09-25 먹통 교훈). 10-09 맥 시험: 피킹·올패스 해석적 미분은 1.9 % 만 빨라지고 입력별 늦은 에너지가 달라져(Rb −7.08 → −5.42 dB) 버렸다.
MIMO_WORKERS = max(2, (os.cpu_count() or 4) // 2)
#: 입력당 MIMO 서포트(갈래) 수 상한(10-05 R28 — 테시라 DSP 예산: 2×8 딜레이 매트릭스를 입력 둘이 나눠 써 입력당 4갈래가 기준,
#: `보고서/01 안내서/2026-10-05 테시라 오브젝트 배치도/테시라 최종 배치도 (DSP 예산) 2026-10-05.html`). 0 = 제한 없음(기본).
#: 넘으면 엔진(`mimo.design_mimo(max_supports=)`)이 혼자 풀 때 늦은 에너지를 가장 많이 줄이는 순서로 고른다.
MIMO_MAX_SUPPORTS_RANGE = (0, 12)
#: 그룹 XO 범위(Hz)와 추천 후보. 09-27 외부 실측 XO 비교(60/80/100/120): 평탄도는 80–100, GD·울림은 120 이 좋았다 —
#: 그룹마다 달랐고, RC 앞 합산만 보는 빠른 점수는 엔진 전체 순위와 안 맞아(`tools/xo_proxy_check.py`) 후보마다 엔진을 돌린다.
XO_RANGE = (40.0, 200.0)
XO_CANDIDATES: tuple[float, ...] = (60.0, 80.0, 100.0, 120.0)

#: 표시 격자(로그). SPL 은 가청 전 대역, GD는 보정이 일하는 저·중역.
SPL_GRID = np.geomspace(5.0, 20000.0, 1390)      # 화면 곡선(10-09 사용자: X축 5 Hz 까지)
GD_GRID = np.geomspace(5.0, 5000.0, 1070)
DISPLAY_FRAC = 24.0                     # 1/24 oct(REW 로 볼 때와 같게 — 09-26 사용자가 1/24 로 본다)

#: BA 수동 설정(R27) — 방식 · 묶음 이름 · 값 범위. 고정 딜레이는 DSP 출력 값이라 0 이상.
SUB_MODES: tuple[str, ...] = ('auto', 'fixed', 'bundle')
SUB_BUNDLES: tuple[str, ...] = ('A', 'B')
SUB_GAIN_RANGE = (-60.0, 20.0)
SUB_DELAY_RANGE = (0.0, 500.0)
SUB_AP_F_RANGE = (10.0, 500.0)
SUB_AP_Q_RANGE = (0.1, 20.0)
SUB_MAIN_DELAY_RANGE = (0.0, 100.0)
#: 서브 고정 출력 필터(R27 10-05 추가 — INFRA 로우패스 등): LR4 HP·LP 주파수(0 = 없음) · PEQ 밴드 수와 범위.
SUB_FILTER_RANGE = (10.0, 300.0)
SUB_PEQ_BANDS = 3
SUB_PEQ_GAIN_RANGE = (-30.0, 12.0)
#: 대역 필터 종류(엔진 `BAND_TYPES` 키) — 화면 순서, 첫 것이 기본(10-05 리드: LR 24 기본).
SUB_BAND_TYPES: tuple[str, ...] = ('LR24', 'LR12', 'LR48', 'BW12', 'BW24')
#: 화면에서 '끔' 이던 대역 필터의 종류를 고르면 넣는 주파수(Hz) — 켠 것이 보이게(사람이 이어서 고친다).
SUB_FILTER_DEFAULT_HZ = {'hp': 20.0, 'lp': 80.0}
#: 2-way 서브 저역 레벨 맞춤 상한(dB) 범위와 기본(끝까지 맞춤 — 10-09 사용자 결정).
INFRA_BOOST_RANGE = (0.0, 24.0)
INFRA_BOOST_DEFAULT = 24.0
#: XO PEQ 대역 칸 범위(Hz, 0 = 자동).
XO_PEQ_RANGE = (0.0, 1000.0)
#: 부스트 상한 칸(dB) — 기본 10 = 엔진 기본(`correction_engine.MAX_BOOST_DB`)이라 전과 같은 답.
BOOST_RANGE = (0.0, 10.0)
BOOST_DEFAULT = 10.0
#: 묶음 레이아웃(10-09 R30): 'values' = 넣은 상대값 고정(R27 그대로, 옛 .secs 의 묶음) · 'dba' · 'multi'(위치기반 멀티서브).
#: DBA·멀티는 2단 합성(엔진 `bundle_layouts`) — 묶음 서브는 필터 완전히 같고 레벨만 서브별 자동, DBA 뒤 줄은 반전·전파 지연 자동.
SUB_LAYOUTS: tuple[str, ...] = ('values', 'dba', 'multi')
SUB_LAYOUT_NEW = 'values'                  # 빈 묶음에 처음 서브를 넣을 때의 레이아웃 — R27 그대로(레이아웃 칸에서 DBA·멀티를 고른다)
DBA_ROWS: tuple[str, ...] = ('front', 'rear')
DBA_TRIM_RANGE = (-20.0, 20.0)             # DBA 뒤 지연 사람 보정(ms, 자동 값에 더함)
#: 묶음 안에서 같이 맞추는 고정 출력 필터 칸 — 레이아웃 묶음은 한 서브를 고치면 묶음 전부에 옮긴다(필터 완전히 같음).
SUB_FILTER_KEYS: tuple[str, ...] = ('hp_hz', 'hp_type', 'lp_hz', 'lp_type', 'peq')


class CalibrationError(RuntimeError):
    """계산을 시작할 수 없다(측정 없음 등) — 화면에 그대로 보여 줄 한국어 문구."""


# ──────────────────────────── 설정 ────────────────────────────
@dataclass
class CalibrationSettings:
    """화면 값 — `project.extra['calibration']` 에 그대로 저장한다(.secs 에 남는다)."""

    mode: str = DEFAULT_MODE
    rc_lo: float = RC_LO_DEFAULT
    rc_hi: float | None = None             # None = 자동(슈뢰더, 최대 500 Hz)
    room_volume_m3: float | None = None    # 없으면 슈뢰더를 못 구해 자동 상한 = 500 Hz
    #: 방 치수(가로·세로·높이 m) — 셋 다 있으면 부피 = 곱(화면이 계산해 `room_volume_m3` 에 넣는다). 부피를 직접
    #: 고치면 치수는 지운다(맞지 않는 치수가 남지 않게). 엔진은 부피만 쓴다(2026-09-26).
    room_dims_m: list | None = None
    mimo_on: bool = False
    fsiso: float = FSISO_DEFAULT
    mimo_low: float | None = None           # None = 자동(스피커마다 서포트 대역 판정)
    mimo_max_supports: int = 0              # 입력당 서포트 상한(0 = 제한 없음, `MIMO_MAX_SUPPORTS_RANGE`)
    #: 서포트를 FIR 로(10-12 사용자: 'FIR 사용' 체크, 서포트당 `mimo.FIR_TAPS_48K` 탭). 끄면 IIR 맞춤(PEQ 9 + 올패스 3).
    mimo_fir: bool = True                # 10-15 사용자: MIMO 는 FIR 기본(새 프로젝트)
    #: 그룹 → XO(Hz). 비어 있는 그룹은 `project.xo` → 80 Hz. [XO 추천 계산] 이 채우고 사용자가 고칠 수 있다(09-27).
    xo: dict = field(default_factory=dict)
    #: 마지막 추천 결과(그룹 → Hz) — 화면이 '추천 90 Hz' 로 보여 준다. 엔진은 쓰지 않는다.
    xo_rec: dict = field(default_factory=dict)
    #: 초고지연 모드의 전체 지연 한도(ms, `ULTRA_MS_RANGE`). 다른 모드에서는 쓰지 않는다.
    ultra_ms: float = ULTRA_MS_DEFAULT
    #: BA 수동 설정(10-05 요구 대장 R27): 역할 → `sub_entry()` 모양 {'mode': 자동/고정/묶음, 'bundle', 'gain_db',
    #: 'delay_ms', 'polarity', 'allpass': [[f, q]] ≤ 2, 'mimo_support', 'hp_hz', 'hp_type', 'lp_hz', 'lp_type',
    #: 'peq': [[f, q, dB]] ≤ 3}.
    #: 고정 출력 필터(hp·lp·peq)는 방식과 상관없이 걸린다. 없는 역할 = 자동·MIMO 서포트 씀·필터 없음(옛 .secs 그대로).
    subs_manual: dict = field(default_factory=dict)
    #: 고정 서브가 있을 때 메인 출력에 더하는 지연(ms) — 고정 값은 DSP 출력 딜레이 그대로라 엔진이 정하지 않는다.
    #: [결과 값 가져오기](`take_sub_values`)가 결과의 메인 추가 지연으로 채운다.
    sub_main_delay_ms: float = 0.0
    #: 묶음 레이아웃(10-09 R30): 묶음 이름 → {'kind': `SUB_LAYOUTS`, 'rear_trim_ms': DBA 뒤 지연 보정}. 없는 묶음 = 'values'
    #: (R27 상대값 고정 — 옛 .secs 그대로). 서브마다 DBA 줄은 `subs_manual[역할]['dba_row']`('front'|'rear').
    bundle_layouts: dict = field(default_factory=dict)
    #: 2-way 서브(인프라 XO, 10-09): 저역 서브 게인을 대역 합 레벨에 맞춰 올리는 상한(dB) — 기본은 끝까지 맞춤, 앰프·유닛
    #: 헤드룸에 맞춰 줄인다(사용자 결정 10-09).
    infra_boost_max_db: float = INFRA_BOOST_DEFAULT
    #: 메인 XO PEQ(10-09 사용자: 따로 정하게) — 켜기 · 하한 · 상한 Hz(0 = 자동: XO ½ … XO × 4, RC 상한으로 자르지 않음 —
    #: 10-09 사용자 결정 '평탄 우선', `correction_engine.xo_peq_band`). RC 상한 위를 막으려면 상한 칸에 RC 상한 값.
    xo_peq_on: bool = True
    #: 부스트 상한(dB, 10-09 사용자) — RC · XO PEQ · 서브 버스 PEQ 의 올림이 모두 이 안(엔진 기본 RC 10 · XO/버스 6 dB 와 작은 쪽).
    boost_max_db: float = BOOST_DEFAULT
    xo_peq_lo: float = 0.0
    xo_peq_hi: float = 0.0

    def __setstate__(self, state: dict) -> None:
        """옛 계산 캐시(pickle)의 설정 — 1.2.0 전에는 `mimo_fir` 가 없었고 그때 MIMO 는 IIR 뿐 → 끔(10-15 기본 FIR 과 구별)."""
        self.__dict__.update(state)
        self.__dict__.setdefault('mimo_fir', False)

    @classmethod
    def from_project(cls, project) -> 'CalibrationSettings':
        raw = (getattr(project, 'extra', None) or {}).get('calibration') or {}
        out = cls()
        if not isinstance(raw, dict):
            return out
        if raw.get('mode') in MODE_KEYS + (ULTRA_MODE,):
            out.mode = raw['mode']

        def num(key, lo, hi):
            value = raw.get(key)
            try:
                value = float(value)
            except (TypeError, ValueError):
                return None
            return float(min(max(value, lo), hi)) if np.isfinite(value) else None

        out.rc_lo = num('rc_lo', 10.0, 200.0) or RC_LO_DEFAULT
        out.rc_hi = num('rc_hi', *RC_HI_RANGE)
        out.room_volume_m3 = num('room_volume_m3', 5.0, 100000.0)
        out.room_dims_m = room_dims(raw.get('room_dims_m'))
        out.mimo_on = bool(raw.get('mimo_on', False))
        out.fsiso = num('fsiso', *FSISO_RANGE) or FSISO_DEFAULT
        out.mimo_low = num('mimo_low', 15.0, FSISO_RANGE[1])
        out.mimo_max_supports = int(round(num('mimo_max_supports', *MIMO_MAX_SUPPORTS_RANGE) or 0))
        # 키가 없는 보정 설정 = 1.2.0 전 파일(그때는 IIR 뿐) → 끔. 보정 설정이 아예 없는 새 프로젝트는 기본값(FIR).
        out.mimo_fir = bool(raw.get('mimo_fir', False if raw else cls.mimo_fir))
        out.xo = _xo_map(raw.get('xo'))
        out.xo_rec = _xo_map(raw.get('xo_rec'))
        out.ultra_ms = num('ultra_ms', *ULTRA_MS_RANGE) or ULTRA_MS_DEFAULT
        manual = raw.get('subs_manual')
        if isinstance(manual, dict):
            out.subs_manual = {str(r): sub_entry(v) for r, v in manual.items() if isinstance(v, dict)}
        out.sub_main_delay_ms = num('sub_main_delay_ms', *SUB_MAIN_DELAY_RANGE) or 0.0
        out.xo_peq_on = bool(raw.get('xo_peq_on', True))
        b = num('boost_max_db', *BOOST_RANGE)
        out.boost_max_db = BOOST_DEFAULT if b is None else b
        out.xo_peq_lo = num('xo_peq_lo', *XO_PEQ_RANGE) or 0.0
        out.xo_peq_hi = num('xo_peq_hi', *XO_PEQ_RANGE) or 0.0
        boost = num('infra_boost_max_db', *INFRA_BOOST_RANGE)
        out.infra_boost_max_db = INFRA_BOOST_DEFAULT if boost is None else boost
        layouts = raw.get('bundle_layouts')
        if isinstance(layouts, dict):
            out.bundle_layouts = {str(b): bundle_layout_entry(v) for b, v in layouts.items()
                                  if str(b) in SUB_BUNDLES and isinstance(v, dict)}
        return out

    def store(self, project) -> None:
        project.extra['calibration'] = asdict(self)

    def engine_key(self) -> tuple:
        """이 값이 바뀌면 보정 결과가 낡는다(MIMO 값은 따로)."""
        return (self.mode, float(self.rc_lo), self.rc_hi, self.room_volume_m3, tuple(sorted(self.xo.items())),
                float(self.ultra_ms) if self.mode == ULTRA_MODE else None, self.manual_key(),
                float(self.infra_boost_max_db), bool(self.xo_peq_on), float(self.xo_peq_lo), float(self.xo_peq_hi),
                float(self.boost_max_db))

    def manual_key(self) -> tuple:
        """서브 수동 설정 중 보정 결과를 바꾸는 부분 — 자동 서브의 칸 값·MIMO 서포트 체크는 빠진다(체크는 `mimo_key`)."""
        rows = []
        for role, e in sorted(self.subs_manual.items()):
            filters = (e['hp_hz'], e['hp_type'] if e['hp_hz'] else '', e['lp_hz'], e['lp_type'] if e['lp_hz'] else '',
                       tuple(tuple(b) for b in e['peq']))
            if e['mode'] == 'auto':
                if filters != (0.0, '', 0.0, '', ()):
                    rows.append((role, 'auto', filters))          # 자동 서브도 고정 출력 필터는 결과를 바꾼다
                continue
            layout = self.layout(e['bundle']) if e['mode'] == 'bundle' else 'values'
            if layout != 'values':
                # 레이아웃 묶음(R30): 칸 값(게인·딜레이·극성·올패스)은 안 쓰고 레이아웃·DBA 줄·뒤 지연 보정이 결과를 바꾼다
                rows.append((role, 'bundle', e['bundle'], layout, e['dba_row'] if layout == 'dba' else '',
                             float(self.bundle_layouts[e['bundle']]['rear_trim_ms']) if layout == 'dba' else 0.0, filters))
                continue
            rows.append((role, e['mode'], e['bundle'] if e['mode'] == 'bundle' else '', float(e['gain_db']),
                         float(e['delay_ms']), int(e['polarity']), tuple(tuple(a) for a in e['allpass']), filters))
        fixed = any(e['mode'] == 'fixed' for e in self.subs_manual.values())
        return tuple(rows), (float(self.sub_main_delay_ms) if fixed else None)

    # ── 묶음 레이아웃(R30) ──
    def layout(self, bundle: str) -> str:
        """묶음의 레이아웃('values' · 'dba' · 'multi') — 없으면 'values'(R27 상대값 고정)."""
        return (self.bundle_layouts.get(bundle) or {}).get('kind', 'values')

    def bundle_members(self, bundle: str) -> list[str]:
        return [r for r, e in self.subs_manual.items() if e['mode'] == 'bundle' and e['bundle'] == bundle]

    def set_layout(self, bundle: str, kind: str | None = None, rear_trim_ms: float | None = None) -> None:
        """묶음 레이아웃·DBA 뒤 지연 보정을 바꾼다. 'values' 가 아닌 레이아웃으로 바꾸면 묶음 서브들의 고정 출력 필터를 첫
        서브 것으로 맞춘다(필터 완전히 같음 — 엔진도 같은 규칙)."""
        e = dict(self.bundle_layouts.get(bundle) or bundle_layout_entry({}))
        if kind is not None:
            e['kind'] = kind
        if rear_trim_ms is not None:
            e['rear_trim_ms'] = rear_trim_ms
        self.bundle_layouts[bundle] = bundle_layout_entry(e)
        members = self.bundle_members(bundle)
        if self.layout(bundle) != 'values' and members:
            self._share_filters(members[0])

    def _share_filters(self, role: str) -> None:
        """레이아웃 묶음 안: role 의 고정 출력 필터를 같은 묶음 서브 전부에 옮겨 적는다."""
        e = self.subs_manual.get(role)
        if not e or e['mode'] != 'bundle' or self.layout(e['bundle']) == 'values':
            return
        for other in self.bundle_members(e['bundle']):
            if other != role:
                self.subs_manual[other] = sub_entry({**self.subs_manual[other],
                                                     **{k: copy.deepcopy(e[k]) for k in SUB_FILTER_KEYS}})

    def sub(self, role: str) -> dict:
        """서브 하나의 수동 설정(없으면 기본 = 자동). 고칠 때는 `set_sub`."""
        return dict(self.subs_manual.get(role) or sub_entry({}))

    def set_sub(self, role: str, **values) -> None:
        before = self.sub(role)
        joining = values.get('mode') == 'bundle' and (before['mode'] != 'bundle'
                                                      or before['bundle'] != values.get('bundle', before['bundle']))
        self.subs_manual[role] = sub_entry({**before, **values})
        e = self.subs_manual[role]
        if joining:
            # 묶음에 들어올 때(R30): 빈 묶음이면 새 레이아웃(`SUB_LAYOUT_NEW`), 레이아웃 묶음이면 먼저 있던 서브의 필터를 받는다
            others = [r for r in self.bundle_members(e['bundle']) if r != role]
            if not others and e['bundle'] not in self.bundle_layouts:
                self.bundle_layouts[e['bundle']] = bundle_layout_entry({'kind': SUB_LAYOUT_NEW})
            elif others:
                self._share_filters(others[0])
                return
        if any(k in values for k in SUB_FILTER_KEYS):
            self._share_filters(role)

    def excluded_supports(self) -> tuple:
        """MIMO 서포트로 쓰지 않을 서브(체크를 끈 것)."""
        return tuple(sorted(r for r, e in self.subs_manual.items() if not e['mimo_support']))

    def take_sub_values(self, run: 'CalibrationRun') -> None:
        """[결과 값 가져오기]: 마지막 결과의 서브 출력 값(게인·딜레이·극성·올패스)을 모든 서브의 수동 칸에, 그 결과의 메인
        추가 지연을 `sub_main_delay_ms` 에 넣는다(방식·MIMO 체크는 그대로). 반올림하지 않는다 — 전부 고정으로 바꾸고 다시
        계산하면 자동 결과와 같은 결과가 나와야 한다(R27 고정 경로 검증)."""
        for role, s in run.result.subs.items():
            self.set_sub(role, gain_db=float(s.gain_db), delay_ms=float(s.delay_ms), polarity=int(s.polarity),
                         allpass=[[float(a), float(b)] for a, b in s.allpass])
        self.sub_main_delay_ms = float(getattr(run.engine, 'main_extra_ms', 0.0))

    def params(self) -> dict:
        """엔진에 넘길 모드 값(`MODES` 한 줄 — 초고지연이면 지연 한도로 만든다)."""
        return mode_params(self.mode, self.ultra_ms)

    def label(self) -> str:
        """결과·파일 이름에 쓰는 모드 이름(초고지연이면 지연까지 — 지연마다 파일이 겹치지 않게)."""
        return mode_label(self.mode, self.ultra_ms)

    def mimo_key(self) -> tuple:
        return (float(self.fsiso), self.mimo_low, self.excluded_supports(), int(self.mimo_max_supports), bool(self.mimo_fir))


def sub_entry(value: dict) -> dict:
    """서브 수동 설정 한 줄을 정리한다(.secs 에서 읽은 값·화면 값 모두) — 모르는 방식은 자동, 값은 범위로 자르고,
    올패스는 (f, Q) 두 개까지(f 가 0 이면 없음)."""
    def num(key, lo, hi, default):
        try:
            v = float(value.get(key, default))
        except (TypeError, ValueError):
            return default
        return float(min(max(v, lo), hi)) if np.isfinite(v) else default

    allpass = []
    for item in value.get('allpass') or []:
        try:
            f0, q = float(item[0]), float(item[1])
        except (TypeError, ValueError, IndexError):
            continue
        if np.isfinite(f0) and np.isfinite(q) and f0 > 0:
            allpass.append([float(min(max(f0, SUB_AP_F_RANGE[0]), SUB_AP_F_RANGE[1])),
                            float(min(max(q, SUB_AP_Q_RANGE[0]), SUB_AP_Q_RANGE[1]))])
    peq = []
    for item in value.get('peq') or []:
        try:
            f0, q, g = float(item[0]), float(item[1]), float(item[2])
        except (TypeError, ValueError, IndexError):
            continue
        # 0 dB 밴드도 남긴다(칸에 f 를 먼저 넣고 게인을 나중에 넣는 동안 지워지지 않게) — 엔진은 0 dB 밴드를 건너뛴다
        if np.isfinite(f0) and np.isfinite(q) and np.isfinite(g) and f0 > 0:
            peq.append([float(min(max(f0, SUB_AP_F_RANGE[0]), SUB_AP_F_RANGE[1])),
                        float(min(max(q, SUB_AP_Q_RANGE[0]), SUB_AP_Q_RANGE[1])),
                        float(min(max(g, SUB_PEQ_GAIN_RANGE[0]), SUB_PEQ_GAIN_RANGE[1]))])

    def corner(key):
        v = num(key, 0.0, SUB_FILTER_RANGE[1], 0.0)
        return 0.0 if v <= 0 else max(v, SUB_FILTER_RANGE[0])

    bundle = str(value.get('bundle') or SUB_BUNDLES[0])
    return {'mode': value.get('mode') if value.get('mode') in SUB_MODES else 'auto',
            'bundle': bundle if bundle in SUB_BUNDLES else SUB_BUNDLES[0],
            'gain_db': num('gain_db', *SUB_GAIN_RANGE, 0.0), 'delay_ms': num('delay_ms', *SUB_DELAY_RANGE, 0.0),
            'polarity': -1 if num('polarity', -1.0, 1.0, 1.0) < 0 else 1, 'allpass': allpass[:2],
            'mimo_support': bool(value.get('mimo_support', True)), 'hp_hz': corner('hp_hz'), 'lp_hz': corner('lp_hz'),
            'peq': peq[:SUB_PEQ_BANDS],
            'hp_type': value.get('hp_type') if value.get('hp_type') in SUB_BAND_TYPES else SUB_BAND_TYPES[0],
            'lp_type': value.get('lp_type') if value.get('lp_type') in SUB_BAND_TYPES else SUB_BAND_TYPES[0],
            'dba_row': value.get('dba_row') if value.get('dba_row') in DBA_ROWS else DBA_ROWS[0]}


def bundle_layout_entry(value: dict) -> dict:
    """묶음 레이아웃 한 줄 정리(R30) — 모르는 종류는 'values', 보정은 `DBA_TRIM_RANGE` 로 자른다."""
    try:
        trim = float(value.get('rear_trim_ms', 0.0))
    except (TypeError, ValueError):
        trim = 0.0
    trim = float(min(max(trim, DBA_TRIM_RANGE[0]), DBA_TRIM_RANGE[1])) if np.isfinite(trim) else 0.0
    return {'kind': value.get('kind') if value.get('kind') in SUB_LAYOUTS else 'values', 'rear_trim_ms': trim}


def sub_has_filters(entry: dict) -> bool:
    """고정 출력 필터(HP·LP·PEQ)가 하나라도 있나."""
    return bool(entry.get('hp_hz') or entry.get('lp_hz') or any(b[2] for b in entry.get('peq') or []))


ROOM_DIM_RANGE = (0.5, 100.0)            # 방 치수 한 변(m)


def _xo_map(value) -> dict:
    """{그룹: Hz} — 범위(`XO_RANGE`) 밖·숫자 아님은 버린다(옛 파일·손상된 값)."""
    out = {}
    if isinstance(value, dict):
        for group, hz in value.items():
            try:
                hz = float(hz)
            except (TypeError, ValueError):
                continue
            if np.isfinite(hz):
                out[str(group)] = float(min(max(hz, XO_RANGE[0]), XO_RANGE[1]))
    return out


def room_dims(value) -> list[float] | None:
    """[가로, 세로, 높이](m) 세 값이 모두 범위 안이면 그 목록, 아니면 None(옛 파일·빈 칸·잘못된 값)."""
    if not isinstance(value, (list, tuple)) or len(value) != 3:
        return None
    try:
        dims = [float(v) for v in value]
    except (TypeError, ValueError):
        return None
    if not all(np.isfinite(d) and ROOM_DIM_RANGE[0] <= d <= ROOM_DIM_RANGE[1] for d in dims):
        return None
    return dims


def room_volume(dims) -> float | None:
    """치수 → 부피(m³). 치수가 온전하지 않으면 None."""
    dims = room_dims(dims)
    return float(dims[0] * dims[1] * dims[2]) if dims else None


def ultra_params(ms: float) -> dict:
    """초고지연 모드의 엔진 값 — 지연 한도 `ms`(30–200)에서 여유 지연 = 한도 − 2 ms, FIR = 그걸 담는 탭 수."""
    ms = float(min(max(float(ms), ULTRA_MS_RANGE[0]), ULTRA_MS_RANGE[1]))
    lift = ms - 2.0
    taps = 2048
    while taps < ULTRA_MAX_TAPS and 0.7 * taps / SR * 1000.0 < lift:
        taps *= 2
    return {'label': '초고지연 모드', 'taps': taps, 'lift_ms': lift, 'max_latency_ms': ms, 'mimo': 'iir', 'phase_frac': 24.0}


def mode_params(mode: str, ultra_ms: float = ULTRA_MS_DEFAULT) -> dict:
    return ultra_params(ultra_ms) if mode == ULTRA_MODE else MODES[mode]


def mode_spec(mode: str, ultra_ms: float = ULTRA_MS_DEFAULT) -> str:
    """모드 한 줄 사양(엔진 `MODES` 에서 — 탭·지연을 바꾸면 따라온다)."""
    m = mode_params(mode, ultra_ms)
    mimo = 'IIR' if m.get('mimo') == 'iir' else 'IIR/FIR'
    if int(m.get('taps', 0)) <= 0:
        return f'RC IIR 전용 · MIMO {mimo} · 필터 지연 {float(m["max_latency_ms"]):g} ms'
    return f'RC IIR + FIR {int(m["taps"]):,} · MIMO {mimo} · 지연 {float(m["max_latency_ms"]):g} ms 이내'


def mode_label(mode: str, ultra_ms: float | None = None) -> str:
    """모드 이름 — `ultra_ms` 를 주면 초고지연 모드는 '초고지연 모드 120 ms'(결과·파일 이름용)."""
    name = str(mode_params(mode)['label'])
    if mode == ULTRA_MODE and ultra_ms is not None:
        name += f' {float(min(max(float(ultra_ms), ULTRA_MS_RANGE[0]), ULTRA_MS_RANGE[1])):g} ms'
    return name


# ──────────────────────────── 측정 · MIMO 조건 ────────────────────────────
def measured_positions(project) -> list[int]:
    """IR 이 하나라도 기록된 측정 위치 번호(오름차순)."""
    out = set()
    for record in list(getattr(project, 'measurements', None) or []):
        if not isinstance(record, dict) or not isinstance(record.get('ir'), dict) or not record['ir']:
            continue
        try:
            out.add(int(record.get('pos')))
        except (TypeError, ValueError):
            continue
    return sorted(out)


def measurement_signature(project) -> tuple:
    """측정 기록의 지문(위치·역할·시간 원점) — 바뀌면 보정 결과가 낡는다(파일 경로는 저장할 때 바뀌어 뺀다)."""
    out = []
    for record in list(getattr(project, 'measurements', None) or []):
        if not isinstance(record, dict):
            continue
        t0 = record.get('t0') if isinstance(record.get('t0'), dict) else {}
        ir = record.get('ir') if isinstance(record.get('ir'), dict) else {}
        out.append((str(record.get('pos')), tuple(sorted(map(str, ir))),
                    tuple(sorted((str(k), repr(v)) for k, v in t0.items()))))
    return tuple(sorted(out))


def mimo_availability(count: int) -> tuple[bool, str]:
    """(MIMO 를 쓸 수 있나, 못 쓰면 이유)."""
    if count < MIMO_MIN_POSITIONS:
        return False, f'측정 위치 {count}곳 — MIMO 는 {MIMO_MIN_POSITIONS}곳 이상 필요'
    return True, ''


def engine_inputs(project, irs: dict, xo: dict | None = None) -> dict:
    """`CorrectionEngine` 인자 모음. 바이앰프 대역 측정(`'L:LF'`)은 아직 한 IR 로 합칠 규칙이 없어 뺀다(노트에 남김).
    그룹·메인·서브는 기준 위치(가장 작은 위치 번호)에 측정이 있는 역할만."""
    clean = {int(pos): {str(r): v for r, v in (roles or {}).items() if ':' not in str(r)}
             for pos, roles in (irs or {}).items()}
    clean = {pos: roles for pos, roles in clean.items() if roles}
    if not clean:
        raise CalibrationError('측정된 IR 이 없습니다 — 5단계에서 측정하세요')
    notes = []
    skipped = sorted({str(r) for roles in (irs or {}).values() for r in (roles or {}) if ':' in str(r)})
    if skipped:
        notes.append('바이앰프 대역 측정 제외: ' + ', '.join(skipped))
    main_pos = min(clean)
    have = set(clean[main_pos])
    groups: dict[str, list[str]] = {}
    for group, members in project.config.groups().items():
        roles = [str(getattr(s, 'role', s)) for s in members]
        roles = [r for r in roles if r in have]
        if roles:
            groups[str(group)] = roles
    subs = list(groups.get('sub', []))
    mains = [r for g, rs in groups.items() if g != 'sub' for r in rs]
    if not mains:
        raise CalibrationError('측정된 메인 채널이 없습니다 — 5단계에서 측정하세요')
    # 그룹 XO: 보정 설계에서 고른 값(`CalibrationSettings.xo`) → 옛 7단계 값(`project.xo`) → 80 Hz
    xo = {g: float((xo or {}).get(g) or (project.xo or {}).get(g) or DEFAULT_XO_HZ) for g in groups if g != 'sub'}
    keep = set(mains) | set(subs)
    clean = {pos: {r: v for r, v in roles.items() if r in keep} for pos, roles in clean.items()}
    return {'irs': clean, 'mains': mains, 'subs': subs, 'groups': groups, 'xo': xo,
            'main_position': main_pos, 'notes': notes}


def target_key(project) -> tuple:
    """타겟 커브의 지문(제어점) — 보정 설계 화면에서 타겟을 바꾸면 결과가 낡았다고 보이게(10-09, 자동 계산은 안 한다)."""
    return tuple((float(f), float(db)) for f, db in sorted(project.target.points))


class _TargetFn:
    """타겟(dB) 함수 — 피클되게 람다 대신 클래스(10-09: 계산 결과를 파일로 남긴다)."""

    def __init__(self, target) -> None:
        self.target = target

    def __call__(self, f):
        return np.asarray(self.target.sample(np.asarray(f, float)), float)


def target_fn(project):
    """타겟(dB) 함수 — 계산 도중 화면에서 타겟을 바꿔도 이 계산은 시작할 때 값으로 끝나게 복사해 둔다."""
    return _TargetFn(copy.deepcopy(project.target))


# ──────────────────────────── 표시 곡선 ────────────────────────────
def spl_db(H: np.ndarray, f: np.ndarray, grid: np.ndarray = SPL_GRID, frac: float = DISPLAY_FRAC) -> np.ndarray:
    """1/frac oct 파워 스무딩 SPL(dB) 을 로그 격자에."""
    power = smooth_power(f, np.abs(H) ** 2, frac)
    return np.interp(grid, f, 10 * np.log10(power + 1e-30))


def gd_ms(H: np.ndarray, f: np.ndarray, grid: np.ndarray = GD_GRID) -> np.ndarray:
    """1/24 oct 복소 스무딩 GD(ms, IR 피크 지연을 뺀 값 — `correction_engine.gd_curve`)."""
    return gd_curve(H, f, grid, DISPLAY_FRAC)


# ──────────────────────────── 보정 계산 ────────────────────────────
@dataclass
class CalibrationRun:
    settings: CalibrationSettings
    engine: CorrectionEngine
    result: object                      # CorrectionResult
    metrics: dict                       # role → {'before': 지표, 'after': 지표}
    curves: dict                        # role → {'spl_before', 'spl_after', 'gd_before', 'gd_after'}
    target: np.ndarray                  # SPL_GRID 위 타겟(레벨 기준 포함)
    rc_hi: float
    rc_auto: bool
    schroeder_hz: float | None
    latency_ms: float
    elapsed_s: float = 0.0
    notes: list = field(default_factory=list)
    signature: tuple = ()               # 계산에 쓴 측정의 지문(`measurement_signature`)
    rc_note: str = ''                   # 엔진 노트의 'RC 상한 … Hz (…)' 한 줄(상한이 어떻게 정해졌나)
    target_key: tuple | None = None     # 계산에 쓴 타겟 제어점(`target_key`) — 다르면 결과가 낡았다(None = 비교 안 함)

    @property
    def roles(self) -> list[str]:
        return list(self.result.channels)

    def filters(self, role: str) -> dict:
        """채널 보정 필터 요약 — PEQ 밴드 수, FIR 탭 수(0 = FIR 없음), XO."""
        ch = self.result.channels[role]
        taps = int(np.asarray(ch.fir).size)
        return {'peq': len(ch.peq), 'fir_taps': taps if taps > 1 else 0, 'xo_hz': ch.xo_hz,
                'gain_db': float(ch.gain_db), 'delay_ms': float(ch.delay_ms)}


def make_engine(project, irs: dict, settings: CalibrationSettings) -> tuple[CorrectionEngine, dict]:
    inputs = engine_inputs(project, irs, settings.xo)
    m = settings.params()
    engine = CorrectionEngine(inputs['irs'], inputs['mains'], inputs['subs'], inputs['groups'], target_fn(project),
                         xo=inputs['xo'], main_position=inputs['main_position'], taps=int(m['taps']),
                         lift_ms=float(m['lift_ms']), peq_bands=m.get('peq_bands'), phase_frac=float(m.get('phase_frac', 6.0)),
                         max_latency_ms=float(m['max_latency_ms']),
                         rc_range=(float(settings.rc_lo), settings.rc_hi),
                         room_volume_m3=settings.room_volume_m3,
                         sub_manual={r: e for r, e in settings.subs_manual.items()
                                     if e['mode'] != 'auto' or sub_has_filters(e)},
                         sub_main_delay_ms=float(settings.sub_main_delay_ms),
                         bundle_layouts={b: v for b, v in settings.bundle_layouts.items() if v['kind'] != 'values'})
    engine.infra_boost_max_db = float(settings.infra_boost_max_db)
    engine.max_boost_db = min(float(engine.max_boost_db), float(settings.boost_max_db))
    engine.xo_peq_on, engine.xo_peq_lo, engine.xo_peq_hi = (bool(settings.xo_peq_on), float(settings.xo_peq_lo),
                                                            float(settings.xo_peq_hi))
    return engine, inputs


def compute(project, irs: dict, settings: CalibrationSettings, progress=None, subs_only: bool = False) -> CalibrationRun:
    """보정 한 번(수십 초~수 분 — 화면은 워커 스레드에서 부른다). progress(끝난 단계, 전체) = BA + 메인 채널마다."""
    started = time.perf_counter()
    engine, inputs = make_engine(project, irs, settings)
    engine.progress = progress
    try:
        res = engine.compute(subs_only=subs_only)
    finally:
        engine.progress = None              # 람다가 남으면 결과를 파일로 못 남긴다(피클)
    f = res.f
    tgt = engine.target_db_fn(np.maximum(f, 1.0))
    from secs.scorecard import metrics as score

    metrics, curves = {}, {}
    for role in res.channels:
        metrics[role] = {'before': score(res.before[role], f, tgt), 'after': score(res.after[role], f, tgt)}
        curves[role] = {'spl_before': spl_db(res.before[role], f), 'spl_after': spl_db(res.after[role], f),
                        'gd_before': gd_ms(res.before[role], f), 'gd_after': gd_ms(res.after[role], f)}
    auto = settings.rc_hi is None
    schroeder = engine.schroeder_hz() if auto else None
    target = np.interp(SPL_GRID, f, res.target_db)
    return CalibrationRun(settings=copy.deepcopy(settings), engine=engine, result=res, metrics=metrics,
                          curves=curves, target=target, rc_hi=float(engine.rc_hi), rc_auto=auto,
                          schroeder_hz=schroeder, latency_ms=float(getattr(engine, 'latency_ms', 0.0)),
                          elapsed_s=time.perf_counter() - started, notes=list(inputs['notes']),
                          signature=measurement_signature(project), target_key=target_key(project),
                          rc_note=next((str(n) for n in getattr(engine, 'notes', []) if str(n).startswith('RC 상한')), ''))


def rc_pieces(run: CalibrationRun) -> list[str]:
    """RC 상한이 어떻게 정해졌는지 — 엔진 노트 그대로('RC 상한 312 Hz (슈뢰더 312 Hz)') + 자동이면 무엇으로 정했는지
    (룸 볼륨). 화면이 조각마다 옮겨 ' · ' 로 잇는다. 엔진 노트가 없으면(옛 엔진) 풀린 값으로 같은 뜻을 만든다."""
    hz = f'{run.rc_hi:.0f}'
    head = run.rc_note or (f'RC 상한 {hz} Hz (수동)' if not run.rc_auto else f'RC 상한 {hz} Hz (자동)')
    if not run.rc_auto:
        return [head]
    volume = run.settings.room_volume_m3
    if not volume:
        return [head, '자동: 룸 볼륨 없음']
    tail = [f'자동: 룸 볼륨 {volume:.0f} m³']
    if run.schroeder_hz is not None and run.schroeder_hz > run.rc_hi + 0.5:
        tail.append(f'자동 상한 최대 {run.rc_hi:.0f} Hz')
    return [head] + tail


# ──────────────────────────── XO 추천 · 미리보기 ────────────────────────────
#: XO 보기 격자(로그 15–500 Hz)와 스무딩(1/12 oct — 메인·서브가 겹치는 모양을 볼 만큼).
XO_GRID = np.geomspace(5.0, 500.0, 620)
XO_FRAC = 12.0
#: 추천 점수 = 위치 가중 ±1 dB 비율(%, 20–200 Hz) − GD_W × GD 폭(ms, 20–100 Hz) − LATE_W × 늦은 에너지(dB, 후보 중 가장
#: 낮은 것 대비). 평탄이 먼저(요구 대장 제품 목표 순서), GD·울림은 평탄이 비슷할 때 가른다.
XO_GD_W = 0.5
XO_LATE_W = 1.0


def xo_groups(project, irs: dict) -> list[str]:
    """XO 가 있는 그룹(서브가 있을 때 메인 그룹 전부). 서브가 없으면 빈 목록."""
    inputs = engine_inputs(project, irs)
    if not inputs['subs']:
        return []
    return [g for g in inputs['groups'] if g != 'sub']


def _late_db(H: np.ndarray, f: np.ndarray) -> float:
    """20–150 Hz 늦은 에너지: 도착 뒤 150–400 ms / 0–50 ms(dB). 대역은 인과 버터워스 4차(10-05 사용자 결정) — 전의 벽돌 대역
    (주파수에서 잘라 내기)은 sinc 꼬리가 1/t 로만 줄어 방 없는 평탄 응답도 −27 dB 가 나왔고, 실측값(−22~−27 dB)이 그 바닥
    바로 위라 개선·악화를 못 가렸다(버터워스 바닥 −58 dB, `보고서/02 새 엔진/2026-10-05 감쇠 악화`). 옛 도구들
    (`tools/raw_test_external.late_db` 등)은 벽돌 대역 그대로다."""
    from scipy.signal import butter, sosfreqz

    sos = butter(4, [20.0, 150.0], btype='bandpass', fs=SR, output='sos')
    ir = np.fft.irfft(H * sosfreqz(sos, worN=f, fs=SR)[1])
    k = int(np.argmax(np.abs(ir) > 0.1 * np.max(np.abs(ir))))
    e0 = np.sum(ir[k:k + int(0.05 * SR)] ** 2)
    e1 = np.sum(ir[k + int(0.15 * SR):k + int(0.4 * SR)] ** 2)
    return float(10 * np.log10(e1 / (e0 + 1e-30) + 1e-30))


def xo_scores(run: CalibrationRun) -> dict:
    """그룹 → {'flat': 위치 가중 ±1 dB %(20–200), 'gd': 청취 위치 GD 폭 ms(20–100), 'late': 늦은 에너지 dB} (그룹 채널 평균)."""
    from secs.scorecard import metrics

    engine = run.engine
    f = engine.f
    t = engine.target_db_fn(np.maximum(f, 1.0))
    Hw, Hl = engine.long_spectra(window_ms=500.0), engine.long_spectra()
    w = engine._weights()
    pos0 = engine.main_position
    out = {}
    for group, roles in engine.groups.items():
        if group == 'sub':
            continue
        flat, gd, late = [], [], []
        for role in roles:
            if role not in engine.chain:
                continue
            ps = [p for p in engine.positions if role in Hw[p]]
            flat.append(sum(w[p] * metrics(engine.stage_responses(role, Hw, p)['final'], f, t)['in1_20'] for p in ps)
                        / sum(w[p] for p in ps))
            gd.append(metrics(engine.stage_responses(role, Hw, pos0)['final'], f, t)['gd_20'])
            late.append(_late_db(engine.stage_responses(role, Hl, pos0)['final'], f))
        if flat:
            out[group] = {'flat': float(np.mean(flat)), 'gd': float(np.mean(gd)), 'late': float(np.mean(late))}
    return out


@dataclass
class XoRecommendation:
    candidates: tuple
    table: dict                         # group → {fc: {'flat', 'gd', 'late', 'score'}}
    best: dict                          # group → fc
    elapsed_s: float = 0.0


def pick_xo(table: dict) -> dict:
    """후보 표 → 그룹마다 점수가 가장 높은 XO(점수를 표에 적어 둔다)."""
    best = {}
    for group, rows in table.items():
        low = min(r['late'] for r in rows.values())
        for r in rows.values():
            r['score'] = r['flat'] - XO_GD_W * r['gd'] - XO_LATE_W * (r['late'] - low)
        best[group] = float(max(rows, key=lambda fc: (rows[fc]['score'], -fc)))
    return best


def _xo_trial(job: tuple) -> tuple:
    """XO 후보 하나 = 보정 한 번 + 점수. 워커 프로세스(spawn)에서 돌아 피클 가능한 최상위 함수여야 한다.
    job = (project, irs, settings, groups, fc). 직렬도 이 함수를 그대로 불러 결과가 같다."""
    project, irs, settings, groups, fc = job
    trial = copy.deepcopy(settings)
    trial.xo = {g: float(fc) for g in groups}
    return float(fc), xo_scores(compute(project, irs, trial))


def recommend_xo(project, irs: dict, settings: CalibrationSettings, candidates=XO_CANDIDATES, progress=None,
                 workers: int | None = None) -> XoRecommendation:
    """후보 XO 마다 보정을 끝까지 계산해 그룹별로 고른다(후보 수 × 보정 한 번 — 무겁다, 명시 버튼으로만).
    그룹은 서로 서브 버스 지연 하나로만 얽혀 있어 후보마다 모든 그룹에 같은 값을 걸고 그룹별로 따로 고른다.

    후보는 서로 독립이라 워커 프로세스로 동시에 푼다(10-09 사용자: "너무 오래 걸려, 멀티코어 못 해?") — 코어 절반 상한·
    낮은 우선순위(`mimo_iir._workers`/`_low_priority`), spawn 컨텍스트(얼린 exe 는 run_secs.py 의 freeze_support).
    풀은 `mimo_iir._ACTIVE_POOLS` 에 넣어 [계산 취소]·창 닫기의 `abort_workers()` 가 프로세스를 바로 끈다(10-09 사용자:
    프로그램을 꺼도 계속 연산해 강제 종료했다). workers=1 이면 직렬 — 엔진을 몽키패치한 테스트용(워커에는 패치가 안 간다).
    progress(k, n) 는 후보가 끝날 때마다(끝난 순서로) 불린다. 표는 끝난 순서와 상관없이 후보 순서로 모아 직렬과 같다."""
    from secs import mimo_iir

    started = time.perf_counter()
    groups = xo_groups(project, irs)
    if not groups:
        raise CalibrationError('서브가 없어 XO 가 없습니다')
    cands = [float(c) for c in candidates]
    jobs = [(project, irs, settings, groups, fc) for fc in cands]
    n_workers = int(workers) if workers else mimo_iir._workers(len(jobs))
    results: dict = {}
    if n_workers <= 1 or len(jobs) <= 1:
        for k, job in enumerate(jobs):
            fc, scores = _xo_trial(job)
            results[fc] = scores
            if progress is not None:
                progress(k + 1, len(jobs))
    else:
        import multiprocessing
        from concurrent.futures import ProcessPoolExecutor, as_completed

        from secs.mimo_iir import one_blas_thread_for_workers
        one_blas_thread_for_workers()
        pool = ProcessPoolExecutor(max_workers=n_workers, mp_context=multiprocessing.get_context('spawn'),
                                   initializer=mimo_iir._low_priority)
        mimo_iir._ACTIVE_POOLS.add(pool)
        try:
            futures = [pool.submit(_xo_trial, job) for job in jobs]
            for k, fut in enumerate(as_completed(futures)):
                fc, scores = fut.result()
                results[fc] = scores
                if progress is not None:
                    progress(k + 1, len(jobs))
        finally:
            mimo_iir._ACTIVE_POOLS.discard(pool)
            pool.shutdown(wait=False, cancel_futures=True)
    table: dict = {g: {} for g in groups}
    for fc in cands:                                 # 후보 순서대로(직렬과 같은 dict 순서)
        for g in groups:
            if g in results[fc]:
                table[g][fc] = results[fc][g]
    table = {g: rows for g, rows in table.items() if rows}
    return XoRecommendation(candidates=tuple(cands), table=table, best=pick_xo(table),
                            elapsed_s=time.perf_counter() - started)


def xo_preview(run: CalibrationRun, group: str, fc: float, corrected: bool = True, role: str | None = None) -> dict:
    """XO 보기 곡선(청취 위치, `XO_GRID`, 1/12 oct dB): 'main' = 그룹 메인 평균(전기 HP), 'sub' = 서브 버스(BA 정렬·버스
    지연·극성, 전기 LP), 'sum' = 채널마다 둘의 합의 평균, 'target' = 타겟(레벨 기준 포함). 마지막 보정 계산의 BA 를
    그대로 쓰고 XO 만 바꾼다(끌 때마다 0.2 s 안 — 요구 대장 R7). 버스 지연 재탐색·1차 XO PEQ 는 [계산] 때.

    `corrected`(기본, 10-09 사용자: "XO 가 아무 보정도 없는 상태에서 되는 것 같다 — 보이는 건 보정이 적용된 게 보였으면"):
    메인에 탑 1차 PEQ(`chain['top_P']`)와 채널 입력 보정(`chain['F']` = PEQ + FIR)을, 서브 몫에도 그 채널의 입력 보정을
    곱한다(`CorrectionEngine.stage_responses` 의 'final' 과 같은 길 — 입력 보정은 메인+서브 합에 걸린다). 보정 필터는 계산 때의
    XO 로 설계된 것이라, XO 를 끌면 필터는 그대로 두고 XO 만 바꾼 모습이다. 필터 응답은 처음 한 번 곱해 두고 끌 때는
    HP·LP 만 곱한다(0.2 s 규칙 유지)."""
    engine = run.engine
    f = engine.f
    pos = engine.main_position
    cache = run.__dict__.setdefault('_xo_cache', {})
    if 'sub' not in cache:
        st = engine.sub_state
        # 서브 합은 엔진이 합에 쓴 `engine.H` 로 만든다(원 측정 H0 아님) — R30 레이아웃 묶음 PEQ 는 레이아웃 단계에서
        # 서브 스펙트럼(`engine.H`)에 곱해지고 H0 에는 없다. 10-09 QA: H0 로 그려 돌비 실측(묶음 A 멀티서브, 초고지연, XO 120)
        # 에서 합이 97 Hz 근처 SPL 탭 보정 후보다 +17 dB 솟아 보였다. 메인은 H0 × 탑 PEQ = engine.H 라 그대로 둔다.
        ssum = engine._sub_sum(st['params'], np.arange(f.size), [pos])[pos] if engine.subs else np.zeros(f.size, complex)
        if st.get('bus_P') is not None:
            ssum = ssum * st['bus_P']
        cache['sub'] = ssum
        cache['mains'] = {r: engine.H0[pos][r] * engine._delay(engine.align_delay_ms[r]) * 10 ** (engine.chain[r]['gain_db'] / 20)
                          for r in engine.chain if r in engine.H0[pos]}
    if corrected and 'mains_corr' not in cache:
        one = np.ones(f.size)
        cache['F'] = {r: np.asarray(engine.chain[r].get('F', one)) for r in cache['mains']}
        cache['mains_corr'] = {r: m * (engine.chain[r]['top_P'] if engine.chain[r].get('top_P') is not None else one)
                               * cache['F'][r] for r, m in cache['mains'].items()}
    roles = [r for r in engine.groups.get(group, []) if r in cache['mains']]
    if role in roles:                       # 채널 하나만(10-09 사용자: XO 탭에서 채널 선택이 무의미했다) — 없으면 그룹 평균
        roles = [role]
    hp, lp = lr4(f, float(fc), 'HP'), lr4(f, float(fc), 'LP')
    sub = cache['sub'] * lp
    if corrected:
        mains = [cache['mains_corr'][r] * hp for r in roles]
        subs = [sub * cache['F'][r] for r in roles]
    else:
        mains = [cache['mains'][r] * hp for r in roles]
        subs = [sub for _r in roles]

    def db(power):
        return np.interp(XO_GRID, f, 10 * np.log10(smooth_power(f, power, XO_FRAC) + 1e-30))

    main_p = np.mean([np.abs(m) ** 2 for m in mains], axis=0) if mains else np.zeros(f.size)
    sub_p = np.mean([np.abs(s) ** 2 for s in subs], axis=0) if subs else np.abs(sub) ** 2
    sum_p = np.mean([np.abs(m + s) ** 2 for m, s in zip(mains, subs)], axis=0) if mains else np.abs(sub) ** 2
    return {'grid': XO_GRID, 'main': db(main_p), 'sub': db(sub_p), 'sum': db(sum_p),
            'target': engine.target_db_fn(XO_GRID) + float(getattr(engine, 'ref_level', 0.0))}


# ──────────────────────────── BA 보기 ────────────────────────────
#: BA 보기 격자(로그 15–300 Hz)·곡선 스무딩(XO 보기와 같은 1/12 oct)·숫자 스무딩(1/6 oct — 좁은 노치가
#: 평탄도를 혼자 정하지 않게).
SUB_GRID = np.geomspace(5.0, 300.0, 540)
SUB_METRIC_FRAC = 6.0
SUB_RAW = (0.0, 0.0, 1, [])              # 정렬 전 = DSP 없이 그대로(게인 0 dB · 딜레이 0 · 정상 · 올패스 없음)


def _sub_out(engine: CorrectionEngine, role: str, param, H: np.ndarray) -> np.ndarray:
    """서브 하나의 출력 응답 — `CorrectionEngine._sub_sum` 의 한 항(게인·딜레이·극성·올패스)과 같은 식."""
    g, d, pol, aps = param
    f = engine.f
    out = H * (10 ** (g / 20)) * pol * np.exp(-2j * np.pi * f * d / 1000)
    for f0, q in aps:
        out = out * allpass2(f, f0, q)
    return out


def sub_band(engine: CorrectionEngine) -> tuple[float, float]:
    """BA 를 맞춘 대역 — 엔진 `compute()` 가 `_align_subs` 에 주는 상한(메인 XO 최댓값 × 1.5, XO 없으면 100 Hz × 1.5)."""
    xos = [float(v) for g, v in engine.xo.items() if g != 'sub' and v]
    return 20.0, max(xos, default=100.0) * 1.5


def sub_combine(run: CalibrationRun) -> dict | None:
    """BA 보기(요구 대장 R9) — 서브가 2개 이상일 때만, 아니면 None.

    마지막 보정 계산의 BA 를 그대로 쓴다(엔진 수학은 다시 풀지 않는다): 정렬 값 = `CorrectionResult.subs`(시트·내보내기와
    같은 값), 곡선 = 엔진이 합에 쓴 매개변수 `engine.sub_state['params']`(서브 버스 게인·지연·극성까지 든 최종값) 를
    원 측정 `engine.H0` 에 건 것 × 서브 버스 PEQ(`sub_state['bus_P']`, 10-09). 정렬 전 = 같은 측정을 DSP 없이 그대로 더한 것(`SUB_RAW`).
    서브 수동 설정(R27)의 고정·묶음 서브도 같은 매개변수 자리로 들어오니 그대로 그린다 — 'mode' = `SubAlign.mode`.

    돌려주는 것: 'subs'(역할 순서) · 'grid'(`SUB_GRID`) · 'before'/'after'(역할 → 청취 위치 1/12 oct dB) ·
    'sum_before'/'sum_after'(서브 합 dB) · 'band'(평탄도를 잰 대역 Hz) · 'flat'(정렬 전, 후 — 서브 합 1/6 oct dB 의
    대역 안 표준편차, 청취 위치) · 'seat'(정렬 전, 후 — 위치 사이 표준편차의 대역 평균 dB, 위치가 1곳이면 None) ·
    'positions'(모든 서브를 잰 위치 수) · 'values'(역할마다 {'role', 'gain_db', 'delay_ms', 'polarity', 'allpass', 'mode'}).
    """
    engine = run.engine
    subs = list(engine.subs)
    if len(subs) < 2:
        return None
    cached = run.__dict__.get('_sub_combine')
    if cached is not None:
        return cached
    f = engine.f
    pos0 = engine.main_position
    params = dict(zip(subs, engine.sub_state['params']))
    positions = [p for p in engine.positions if all(r in engine.H0[p] for r in subs)]
    # 정렬 후 = 엔진 스펙트럼(서브는 `H0` + R30 묶음 PEQ — 레이아웃 묶음이 없으면 `H0` 와 같은 값) × 서브 버스 PEQ(10-09:
    # 빠져 있어 화면의 '정렬 후'·서브 합이 실제로 나가는 서브와 달랐다 — 버스 PEQ 는 모든 서브에 같이 걸리니 서브마다 곱한다)
    bus_P = engine.sub_state.get('bus_P')
    each = {state: {p: {r: _sub_out(engine, r, SUB_RAW if state == 'before' else params[r],
                                    (engine.H0 if state == 'before' else engine.H)[p][r])
                                * (bus_P if state == 'after' and bus_P is not None else 1.0)
                        for r in subs} for p in positions}
            for state in ('before', 'after')}
    sums = {state: {p: sum(each[state][p].values()) for p in positions} for state in each}

    def curve(H):
        return np.interp(SUB_GRID, f, 10 * np.log10(smooth_power(f, np.abs(H) ** 2, XO_FRAC) + 1e-30))

    lo, hi = sub_band(engine)
    grid = np.geomspace(lo, hi, 200)

    def level(H):
        return np.interp(grid, f, 10 * np.log10(smooth_power(f, np.abs(H) ** 2, SUB_METRIC_FRAC) + 1e-30))

    flat = tuple(float(np.std(level(sums[state][pos0]))) for state in ('before', 'after'))
    seat = None
    if len(positions) > 1:
        seat = tuple(float(np.mean(np.std([level(sums[state][p]) for p in positions], axis=0)))
                     for state in ('before', 'after'))
    values = [{'role': r, 'gain_db': float(s.gain_db), 'delay_ms': float(s.delay_ms), 'polarity': int(s.polarity),
               'allpass': [(float(a), float(b)) for a, b in s.allpass], 'mode': s.mode, 'peq': list(s.peq)}
              for r, s in ((r, run.result.subs[r]) for r in subs)]
    out = {'subs': subs, 'grid': SUB_GRID,
           'before': {r: curve(each['before'][pos0][r]) for r in subs},
           'after': {r: curve(each['after'][pos0][r]) for r in subs},
           'sum_before': curve(sums['before'][pos0]), 'sum_after': curve(sums['after'][pos0]),
           'band': (lo, hi), 'flat': flat, 'seat': seat, 'positions': len(positions), 'values': values,
           'layouts': layout_lines(run)}
    run.__dict__['_sub_combine'] = out
    return out


# ──────────────────────────── MIMO ────────────────────────────
@dataclass
class MimoRun:
    fsiso: float
    f_low: float | None
    design: object                      # mimo.MimoResult — 서포트 고르기 · 대역 · 이상 FIR
    iir: dict                           # input → {support role: IirSupport, FIR 이면 mimo.FirSupport}
    final: dict                         # role → 기준 위치 최종 응답(긴 창, MIMO 전)
    applied: dict                       # role → 같은 것 + IIR 서포트(MIMO 적용)
    curves: dict                        # role → {'spl_mimo': SPL_GRID}
    elapsed_s: float = 0.0
    notes: list = field(default_factory=list)
    exclude: tuple = ()                 # 서포트로 안 쓴 서브(R27 — 서브별 'MIMO 서포트로 쓰기' 끔)
    max_supports: int = 0               # 입력당 서포트 상한(0 = 제한 없음, R28)
    fir: bool = False                   # 서포트가 FIR(10-12) — 옛 캐시에는 없다(`mimo_kind` 가 getattr 로)

    def supports(self, role: str) -> int:
        return len(self.iir.get(role) or {})

    def key(self) -> tuple:
        """이 MIMO 를 계산한 설정(`CalibrationSettings.mimo_key` 와 견준다)."""
        return (self.fsiso, self.f_low, tuple(self.exclude), int(self.max_supports), bool(getattr(self, 'fir', False)))


def compute_mimo(run: CalibrationRun, fsiso: float = FSISO_DEFAULT, f_low: float | None = None,
                workers: int = MIMO_WORKERS, exclude=(), max_supports: int = 0, progress=None, fir: bool = False) -> MimoRun:
    """MIMO(무겁다 — 명시적 버튼으로만). 위치 3곳 미만이면 `CalibrationError`. exclude = 서포트로 안 쓸 서브,
    max_supports = 입력당 서포트 상한(0 = 제한 없음 — 엔진에 None 을 넘겨 전과 같다). fir = 서포트를 FIR 그대로(10-12):
    `mimo.FIR_TAPS_LOW` 길이로 풀고 IIR 맞춤을 건너뛴다 — Fsiso 저역 통과가 FIR 안에 있어 LR4 LP 를 따로 곱하지 않는다."""
    started = time.perf_counter()
    engine = run.engine
    ok, reason = mimo_availability(len(engine.positions))
    if not ok:
        raise CalibrationError(reason)
    exclude = tuple(sorted(exclude or ()))
    max_supports = int(max_supports or 0)
    design = mimo.design_mimo(engine, fsiso=float(fsiso), positions=list(engine.positions), f_low=f_low, exclude=exclude,
                               max_supports=max_supports or None, taps_low=mimo.FIR_TAPS_LOW if fir else None)
    if max_supports:
        design.notes.append(f'입력당 서포트 ≤ {max_supports}')
    if fir:
        iir = {i: {j: mimo.FirSupport(np.asarray(s, float), design.fs_low) for j, s in sups.items()}
               for i, sups in design.supports.items() if sups}
        total = sum(sp.taps_48k for sups in iir.values() for sp in sups.values())
        design.notes.append(f'FIR 서포트 {sum(len(v) for v in iir.values())}개 · 총 {total:,}탭(48 kHz)')
        if progress is not None:
            progress(1, 1)
    else:
        iir = (mimo_iir.design_mimo_iir(engine, design, list(engine.positions), fsiso=float(fsiso), workers=workers,
                                        progress=progress)
               if design.supports else {})
    H = engine.long_spectra()
    f = engine.f
    pos = engine.main_position
    lp = 1.0 if fir else lr4(f, float(fsiso), 'LP')     # FIR 은 저역 통과가 안에 있다
    final, applied, curves = {}, {}, {}
    for role in engine.chain:
        base = engine.stage_responses(role, H, pos)['final']
        extra = np.zeros(f.size, complex)
        for j, sp in (iir.get(role) or {}).items():
            extra += mimo.support_path(engine, design, j, H, pos) * lp * sp.response(f)
        final[role] = base
        applied[role] = base + extra
        curves[role] = {'spl_mimo': spl_db(applied[role], f)}
    return MimoRun(fsiso=float(fsiso), f_low=f_low, design=design, iir=iir, final=final, applied=applied, curves=curves,
                  elapsed_s=time.perf_counter() - started, notes=list(getattr(design, 'notes', []) or []), exclude=exclude,
                  max_supports=max_supports, fir=bool(fir))


def mimo_kind(mimo_run: MimoRun) -> str:
    """서포트 필터 종류 글자 — 'FIR' 또는 'IIR'(옛 캐시는 IIR)."""
    return 'FIR' if getattr(mimo_run, 'fir', False) else 'IIR'


def mimo_fir_budget_text(mimo_run: MimoRun, budget: int) -> str:
    """FIR 서포트 총 탭과 DSP FIR 예산(`DspProfile.fir_budget`) 비교 한 줄 — IIR 이거나 예산을 모르면 ''."""
    if mimo_kind(mimo_run) != 'FIR' or not budget:
        return ''
    total = sum(sp.taps_48k for sups in mimo_run.iir.values() for sp in sups.values())
    return f'FIR 서포트 총 {total:,}탭 — DSP FIR 예산 {int(budget):,}탭 ' + ('안' if total <= budget else '초과')


def mimo_cap_text(mimo_run: MimoRun) -> str:
    """결과·노트에 붙일 입력당 상한 글자(' · 입력당 서포트 ≤ 4', 제한 없으면 '')."""
    return f' · 입력당 서포트 ≤ {mimo_run.max_supports}' if getattr(mimo_run, 'max_supports', 0) else ''


# ──────────────────────────── 내보내기 ────────────────────────────
def _chain_text(run: CalibrationRun) -> str:
    m = run.settings.params()
    peq = m.get('peq_bands') or 16
    fir = f' + FIR {int(m["taps"]):,}' if int(m['taps']) > 0 else ''
    return f'PEQ {peq}{fir}'


#: 레이아웃 묶음 안 자리 글자(R30) — `SubAlign.mode` 'bundle:A:<자리>' 의 셋째 조각.
LAYOUT_PLACE_TEXT = {'dba-front': 'DBA 앞', 'dba-rear': 'DBA 뒤', 'multi': '멀티'}


def sub_mode_text(mode: str) -> str:
    """`SubAlign.mode` → 화면·노트 글자(자동 · 고정 · 묶음 A · 묶음 A DBA 뒤 · 묶음 A 멀티).
    한 덩어리로 돌려준다 — 영어는 '묶음 {0} DBA 뒤' 꼴 패턴(`en_g8`)."""
    if mode == 'fixed':
        return '고정'
    if mode.startswith('bundle:'):
        parts = mode.split(':')
        if len(parts) > 2 and parts[2] in LAYOUT_PLACE_TEXT:
            return f'Group {parts[1]} {LAYOUT_PLACE_TEXT[parts[2]]}'
        return f'Group {parts[1]}'
    return '자동'


def band_type_text(typ: str) -> str:
    """대역 필터 종류 글자 — 'LR24' → 'LR 24'(dB/oct)."""
    return f'{typ[:2]} {typ[2:]}'


#: 서브 출력 게인이 이보다 크면 경고(10-12: 묶음 서브에 LP 25 Hz 가 잘못 남아 BA 가 막힌 대역을 +51 dB 로 채웠다).
SUB_GAIN_WARN_DB = 12.0


def sub_filter_warnings(settings: 'CalibrationSettings', roles) -> list[str]:
    """설정만 보고 내는 경고 — 서브의 고정 LP 가 HP 의 2 배보다 낮으면(통과 대역이 한 옥타브도 안 됨) 그 서브는 거의 소리를
    못 낸다. 화면이 BA 설정 카드와 BA 결과에 빨간 글자로 보인다(10-12 사용자: 경고 기능)."""
    out = []
    for r in roles:
        e = settings.sub(r)
        if e['hp_hz'] and e['lp_hz'] and e['lp_hz'] < 2.0 * e['hp_hz']:
            out.append(f"{r}: HP {e['hp_hz']:g} Hz, LP {e['lp_hz']:g} Hz — 통과 대역이 거의 없습니다")
    return out


def sub_gain_warnings(run: 'CalibrationRun') -> list[str]:
    """계산 결과의 서브 출력 게인이 `SUB_GAIN_WARN_DB` 를 넘으면 경고 — 막힌 대역을 게인으로 채운 것일 수 있다."""
    return [f'{r} 게인 {s.gain_db:+.1f} dB — 너무 큽니다. 서브 필터(HP, LP) 설정을 확인하세요'
            for r, s in run.result.subs.items() if s.gain_db > SUB_GAIN_WARN_DB]


def sub_filter_text(s) -> str:
    """서브 하나의 대역 필터·고정 PEQ 글자('HP 15 Hz BW 12 LP 25 Hz LR 24 PEQ 30 Hz Q 2.00 -3.0 dB', 없으면 '') — 화면 표 아래 줄·노트."""
    return ' '.join([f'HP {s.hp_hz:g} Hz {band_type_text(s.hp_type)}'] * bool(s.hp_hz)
                    + [f'LP {s.lp_hz:g} Hz {band_type_text(s.lp_type)}'] * bool(s.lp_hz)
                    + [f'PEQ {a:.0f} Hz Q {b:.2f} {c:+.1f} dB' for a, b, c in s.peq])


def sub_value_text(s) -> str:
    """서브 하나의 DSP 값 한 조각(mdat 노트용 — 화면 표는 칸마다 따로)."""
    ap = ''.join(f' AP {a:.0f} Hz Q {b:.2f}' for a, b in s.allpass)
    fixed = sub_filter_text(s)
    return (f'{s.role} {sub_mode_text(s.mode)} {s.gain_db:+.2f} dB {s.delay_ms:.2f} ms '
            f'{"정상" if s.polarity > 0 else "반전"}{ap}' + (f' {fixed}' if fixed else ''))


def sub_values_text(run: CalibrationRun) -> str:
    """모든 서브의 DSP 값(게인·딜레이·극성·올패스·방식) — mdat 노트에 붙인다(R27: 고정 값이 그대로 보이게). 레이아웃 묶음(R30)이
    있으면 묶음 줄(`layout_lines`)을 앞에 붙인다."""
    return ' · '.join(layout_lines(run) + [sub_value_text(s) for s in run.result.subs.values()])


def layout_lines(run: CalibrationRun) -> list[str]:
    """레이아웃 묶음(R30) 한 줄씩 — '묶음 A DBA: 앞 SW1·SW2, 뒤 SW3 반전 +12.25 ms(자동 12.25 보정 +0.00), 같은 필터 PEQ 6'.
    화면 BA 탭·mdat 노트가 같이 쓴다. 레이아웃 묶음이 없으면 []. 한 줄 안에는 ' · ' 를 쓰지 않는다(번역 조각 경계)."""
    out = []
    for name, info in sorted((getattr(run.engine, 'layout_info', None) or {}).items()):
        if info['kind'] == 'dba':
            out.append(f"Group {name} DBA: 앞 {'·'.join(info['front'])}, 뒤 {'·'.join(info['rear'])} 반전 "
                       f"+{info['rear_ms']:.2f} ms(자동 {info['auto_ms']:.2f} 보정 {info['trim_ms']:+.2f}), 같은 필터 PEQ {len(info['peq'])}")
        else:
            out.append(f"Group {name} 멀티서브: {'·'.join(info['members'])}, 같은 필터 PEQ {len(info['peq'])}")
    return out


def stage_items(run: CalibrationRun, mimo_run: MimoRun | None = None, label: str = '') -> list:
    """채널마다 단계별 응답(청취 위치 = 기준 위치, 2 s 창, 모두 계산값) — `tools/export_stages_mdat.py` 의 순서:
    1 보정 전 메인 · 1 보정 전 서브 · 2 XO 타겟 메인 · 2 XO 타겟 서브 · 3 합 후 보정 · 4 MIMO 적용(IIR, 계산했으면).
    이름에 '측정' 을 쓰지 않는다(요구 대장 R18 — 계산값을 측정으로 부르지 않는다)."""
    from secs.rew_mdat.builder import MeasurementInput

    engine = run.engine
    H = engine.long_spectra()
    base = float(min(engine.arrival.values()))
    pos = engine.main_position
    subs = [s for s in engine.subs if s in H[pos]]
    label = label or run.settings.label()
    items = []

    def ir(Hx):
        return np.fft.irfft(Hx, N_FFT).astype(np.float32)

    # 필터 지연(FIR 여유 지연 + 메인 추가)이 든 단계는 시작 시각을 그만큼 당긴다 — REW 에서 피크가 원 응답과 같은 자리에
    # 와서 초과위상이 지연 없이 보인다(09-27 사용자: 28 ms 가 초과위상에 그대로 보였다).
    shifted = base - float(run.latency_ms) / 1000.0

    for seq, role in enumerate(engine.chain, start=1):
        st = engine.stage_responses(role, H, pos)
        xo = engine.chain[role]['xo']

        def add(stage, name, Hx, note, t0=base):
            items.append(MeasurementInput(f'{seq:02d}_{role}_{stage} {name}',
                                          f'SECS 보정 설계 · {label} · 청취 위치 {pos} · 계산값 — {note}', SR, ir(Hx), t0))

        add(1, '보정 전 메인', H[pos][role], '메인 스피커 원 응답(2 s 창, 필터 없음)')
        if subs:
            add(1, '보정 전 서브', sum(H[pos][s] for s in subs),
                '서브 원 응답의 단순 합(정렬 없음' + (' — 서브 고정 출력 필터만)' if engine.sub_filters else '·필터 없음)'))
        if xo:
            add(2, 'XO 타겟 메인', st['top_xo'], f'1차: 게인·딜레이·출력 PEQ(음향 XO 타겟)·전기 HP {xo:g} Hz')
        if 'sub_xo' in st and xo:
            add(2, 'XO 타겟 서브', st['sub_xo'], '1차: BA(게인·지연·극성)·서브 버스 PEQ·전기 LP — '
                + sub_values_text(run))
        add(3, '합 후 보정', st['final'],
            f'2차: 탑+서브 합에 입력 보정({_chain_text(run)}) · 전체 지연 {run.latency_ms:.1f} ms(시작 시각에서 뺌) · RC 상한 {run.rc_hi:.0f} Hz',
            shifted)
        if mimo_run is not None and mimo_run.iir.get(role):
            kind = mimo_kind(mimo_run)
            if kind == 'FIR':
                desc = '; '.join(f'{j} FIR {sp.taps_48k:,}탭' for j, sp in mimo_run.iir[role].items())
            else:
                desc = '; '.join(f'{j} {sp.gain_db:.1f} dB {sp.delay_ms:.1f} ms {"+" if sp.polarity > 0 else "-"} '
                                 f'PEQ {len(sp.peq)} AP {len(sp.allpass)}' for j, sp in mimo_run.iir[role].items())
            add(4, f'MIMO 적용({kind})', mimo_run.applied[role],
                f'MIMO {kind} 서포트 {len(mimo_run.iir[role])}개(Fsiso {mimo_run.fsiso:g} Hz{mimo_cap_text(mimo_run)}) — {desc}', shifted)
    return items


# ──────────────────────────── 계산 결과 저장 · 복원(10-09) ────────────────────────────
def run_cache_path(project_path) -> Path | None:
    """계산 결과 캐시 = `<프로젝트>.irs/calibration_cache.pkl`(측정 IR 과 한 폴더 — 같이 옮겨진다). 10-09 사용자: 불러오기 하니
    연산이 다 풀려 있다(초고지연 계산 몇 분 · MIMO 십여 분을 다시 해야 했다)."""
    if not project_path:
        return None
    return Path(project_path).with_suffix('.irs') / 'calibration_cache.pkl'


def save_run_cache(project_path, run, mimo_run=None) -> Path | None:
    import pickle
    path = run_cache_path(project_path)
    if path is None or run is None:
        return None
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix('.tmp')
    with open(tmp, 'wb') as fh:
        pickle.dump({'version': RUN_CACHE_VERSION, 'run': run, 'mimo_run': mimo_run}, fh, protocol=pickle.HIGHEST_PROTOCOL)
    tmp.replace(path)
    return path


#: 계산 결과 캐시 판. 2(10-10) = 기능 이름 정리 뒤(모듈 `correction_engine` · `mimo` · `mimo_iir`, `MimoRun`, 키 `mimo_run`).
#: 판 1 캐시는 `_cache_unpickler` 가 옛 모듈 · 클래스를, `_migrate_run_names` 가 옛 속성 · 설정 키를 옮겨 그대로 연다.
RUN_CACHE_VERSION = 2
#: 판 1 캐시에서 이름이 바뀐 클래스(클래스 이름 → 지금 모듈, 이름).
_LEGACY_CLASSES = {'ArtRun': ('secs.calibration', 'MimoRun'), 'ArtResult': ('secs.mimo', 'MimoResult')}
#: 판 1 캐시의 옛 모듈(10-10 에 이름이 바뀌어 지금은 없다)의 클래스 · 함수를 찾을 지금 모듈들. 같은 이름이 없으면 끝 이름으로
#: (`…Engine` → `CorrectionEngine`, `…Result` → `CorrectionResult` — 옛 엔진 모듈의 두 클래스).
_RENAMED_MODULES = ('secs.correction_engine', 'secs.mimo', 'secs.mimo_iir')
_LEGACY_SUFFIXES = {'Engine': 'CorrectionEngine', 'Result': 'CorrectionResult'}


def _cache_unpickler(fh):
    import importlib
    import importlib.util
    import pickle

    class _CacheUnpickler(pickle.Unpickler):
        def find_class(self, module, name):
            if name in _LEGACY_CLASSES:
                module, name = _LEGACY_CLASSES[name]
            elif module.startswith('secs.') and importlib.util.find_spec(module) is None:
                for new in _RENAMED_MODULES:
                    if hasattr(importlib.import_module(new), name):
                        module = new
                        break
                else:
                    module = _RENAMED_MODULES[0]
                    name = next((v for k, v in _LEGACY_SUFFIXES.items() if name.endswith(k)), name)
            return super().find_class(module, name)

    return _CacheUnpickler(fh)


def demo_run_path() -> Path:
    """튜토리얼 데모 방(7.1.4 + 실측 서브 3) 보정 결과 파일 — `tools/make_sample_calibration.py` 가 만든다. exe 에는
    `SECS.spec` 의 datas 로 들어간다(`secs/rewapi/` 옆, 샘플 IR `sample_dolby_714.npz` 와 같은 자리)."""
    return Path(__file__).resolve().parent / 'rewapi' / 'sample_calibration.pkl.gz'


_DEMO_RUN: list = []


def _load_demo() -> dict:
    if not _DEMO_RUN:
        data = {}
        try:
            import gzip
            with gzip.open(demo_run_path(), 'rb') as fh:
                loaded = _cache_unpickler(fh).load()
            if loaded.get('version') == RUN_CACHE_VERSION:
                data = loaded
        except Exception:  # noqa: BLE001 - 없거나 깨진 파일
            data = {}
        _DEMO_RUN.append(data)
    return _DEMO_RUN[0]


def load_demo_subs_run():
    """튜토리얼 [서브만 계산] 이 보여 줄 결과 — 인프라 서브 퀘스트대로 SW3 을 인프라로 정한 뒤의 BA(`subs_only`)."""
    return _load_demo().get('subs_run')


def load_demo_run():
    """튜토리얼이 보여 줄 데모 보정 결과(`CalibrationRun`) — 튜토리얼은 계산하지 않고 이것만 보여 준다(10-11 사용자: "연산하는
    척만 하라니까" — 서브 3 데모의 미리 계산이 길어 [계산] 이 멈춘 것처럼 보였다). 없거나 못 읽으면 None(그때만 실제 계산).
    엔진이 바뀌면 `tools/make_sample_calibration.py` 로 다시 만든다(`tests/test_demo_run.py` 가 지킨다)."""
    return _load_demo().get('run')


def load_run_cache(project_path):
    """(run, mimo_run) 또는 None. 깨졌거나 판이 달라 못 읽으면 None(다시 계산하면 된다). 판 1(10-10 기능 이름 정리 전)
    캐시도 옛 이름을 옮겨 연다."""
    path = run_cache_path(project_path)
    if path is None or not path.exists():
        return None
    try:
        with open(path, 'rb') as fh:
            data = _cache_unpickler(fh).load()
        run = data['run']
        mimo_run = data['mimo_run'] if 'mimo_run' in data else data.get('art_run')
        _migrate_run_names(run, mimo_run)
        _migrate_run_groups(run)
        return run, mimo_run
    except Exception:  # noqa: BLE001 - 옛 판의 캐시 · 깨진 파일
        return None


def _migrate_run_names(run, mimo_run) -> None:
    """판 1 캐시의 옛 속성 이름(10-10 기능 이름 정리 전): 계산에 쓴 설정 · 엔진의 `art_*` 칸 · 서브마다 `art_support`
    (`model.LEGACY_CALIBRATION_KEYS` · `LEGACY_SUB_KEYS` — `.secs` 이주와 같은 표), `MimoRun` 의 `art`(→ `design`)와
    곡선 키 `spl_art`(→ `spl_mimo`). 안 옮기면 설정 비교(`mimo_key`)가 KeyError 를 내거나 결과가 늘 낡음으로 보인다."""
    from secs.model import LEGACY_CALIBRATION_KEYS, LEGACY_SUB_KEYS, rename_keys

    for holder in (getattr(run, 'settings', None), getattr(run, 'engine', None)):
        state = getattr(holder, '__dict__', None)
        if not isinstance(state, dict):
            continue
        for old, new in LEGACY_CALIBRATION_KEYS.items():
            if old in state:
                value = state.pop(old)
                state.setdefault(new, value)
        for key in ('subs_manual', 'sub_manual'):
            if isinstance(state.get(key), dict):
                state[key] = {r: rename_keys(e, LEGACY_SUB_KEYS) for r, e in state[key].items()}
    state = getattr(mimo_run, '__dict__', None)
    if isinstance(state, dict):
        if 'art' in state:
            state.setdefault('design', state.pop('art'))
        if isinstance(state.get('curves'), dict):
            state['curves'] = {r: rename_keys(c, {'spl_art': 'spl_mimo'}) for r, c in state['curves'].items()}


def _migrate_run_groups(run) -> None:
    """10-10 탑 · 하이트 분리 전의 캐시: 엔진 그룹 '하이트' 가 탑(T*)만 담고 있으면 '탑' 으로 옮긴다(엔진 그룹 · 엔진 XO ·
    결과 XO · 계산에 쓴 설정의 XO). 안 옮기면 화면의 XO 그룹('탑')이 결과에 없어 XO 보기가 비고, `.secs` 쪽만 옮겨진
    설정과 키가 달라 결과가 낡음으로 보인다(`Project._migrate_legacy` 4번과 짝)."""
    from secs.model import LEGACY_TOP_GROUP, TOP_GROUP, migrate_group_keys, role_group

    engine = getattr(run, 'engine', None)
    groups = getattr(engine, 'groups', None)
    if not isinstance(groups, dict) or LEGACY_TOP_GROUP not in groups:
        return
    if not all(role_group(r) == TOP_GROUP for r in groups[LEGACY_TOP_GROUP]):
        return                                      # 새 판의 진짜 하이트(H*)
    engine.groups = migrate_group_keys(groups)
    engine.xo = migrate_group_keys(getattr(engine, 'xo', {}))
    result = getattr(run, 'result', None)
    if isinstance(getattr(result, 'xo', None), dict):
        result.xo = migrate_group_keys(result.xo)
    settings = getattr(run, 'settings', None)
    if settings is not None:
        settings.xo = migrate_group_keys(settings.xo)
        settings.xo_rec = migrate_group_keys(settings.xo_rec)


def project_spl_offset(project) -> float | None:
    """측정 기록의 REW SPL 오프셋 중앙값(dB) — 계산값 mdat 의 레벨 기준(없으면 None = 템플릿 120 dB). 10-09."""
    values = [float(v) for r in (getattr(project, 'measurements', None) or []) if isinstance(r, dict)
              for v in (r.get('spl_offset') or {}).values() if v is not None]
    return float(np.median(values)) if values else None


def export_mdat(run: CalibrationRun, folder, mimo_run: MimoRun | None = None, spl_offset_db: float | None = None) -> Path:
    """REW 로 열 단계별 mdat 하나(`<folder>/SECS 보정 설계 <모드>.mdat`)."""
    from secs.rew_mdat.builder import build_mdat

    label = run.settings.label()
    return build_mdat(stage_items(run, mimo_run, label), Path(folder) / f'SECS 보정 설계 {label}.mdat',
                      spl_offset_db=spl_offset_db)


def before_after_items(run: CalibrationRun, mimo_run: MimoRun | None = None, sources: dict | None = None) -> list:
    """내보내기 단계(8)의 REW 항목 — 입력 채널마다 `<순번>_<역할>_1 보정 전(원 측정)`(기준 위치 실측 IR 그대로, 시간
    원점 = 측정 t0) · `_2 보정 후 <모드>`(`stage_responses(...)['final']`: 채널 보정 필터 + 서브 합, 2 s 창, 계산값) ·
    MIMO 를 계산했으면 `_3 보정 후 + MIMO(IIR)`. 서브는 원 측정만 채널 뒤에 잇는다(서브 혼자의 '보정 후' 는 없다 —
    서브는 각 입력의 합 안에 들어 있다). 계산값의 시간 원점은 엔진의 가장 이른 도착에서 필터 지연(`run.latency_ms`)을 뺀 것 —
    그래서 REW 에서 전·후의 피크가 같은 자리에 오고 초과위상에 필터 지연이 섞이지 않는다(09-27). 원 측정만 '측정' 이라 부른다(요구 대장 R18)."""
    from secs.rew_mdat.builder import MeasurementInput

    engine = run.engine
    pos = engine.main_position
    raw = engine.irs.get(pos, {})
    H = engine.long_spectra()
    base = float(min(engine.arrival.values()))
    shifted = base - float(run.latency_ms) / 1000.0      # 필터 지연을 시작 시각에서 뺀다(stage_items 와 같은 이유)
    label = run.settings.label()
    items = []
    # 원 측정 환경(마이크 캘 · 장치) — 보정 전은 그대로, 보정 후는 같은 환경 + 'SECS2 계산값' 표시(10-17 사용자).
    env = (sources or {}).get(pos, {})

    def measured(seq: int, role: str, note: str = '') -> None:
        ir, sr, *rest = raw[role]
        items.append(MeasurementInput(f'{seq:02d}_{role}_1 보정 전(원 측정)',
                                      f'SECS · 청취 위치 {pos} · 원 측정 그대로(필터 없음)' + note, int(sr),
                                      np.asarray(ir, np.float32), float(rest[0]) if rest else 0.0,
                                      source_info=env.get(role)))

    seq = 0
    for role in engine.chain:
        seq += 1
        if role in raw:
            measured(seq, role)
        final = engine.stage_responses(role, H, pos)['final']
        info = run.filters(role)
        fir = f'FIR {info["fir_taps"]:,}탭' if info['fir_taps'] else 'FIR 없음'
        items.append(MeasurementInput(
            f'{seq:02d}_{role}_2 보정 후 {label}',
            f'SECS 보정 설계 · {label} · 청취 위치 {pos} · 계산값 — 채널 보정(PEQ {info["peq"]}개 · {fir}) + 서브 합, '
            f'전체 지연 {run.latency_ms:.1f} ms(시작 시각에서 뺌) · ' + ' · '.join(rc_pieces(run))
            + (f' · 서브: {sub_values_text(run)}' if run.result.subs and run.result.channels[role].xo_hz else ''),
            SR, np.fft.irfft(final, N_FFT).astype(np.float32), shifted, source_info=env.get(role), computed=True))
        if mimo_run is not None and mimo_run.iir.get(role) and role in mimo_run.applied:
            items.append(MeasurementInput(
                f'{seq:02d}_{role}_3 보정 후 + MIMO({mimo_kind(mimo_run)})',
                f'SECS 보정 설계 · {label} · 청취 위치 {pos} · 계산값 — MIMO {mimo_kind(mimo_run)} 서포트 {len(mimo_run.iir[role])}개 '
                f'(Fsiso {mimo_run.fsiso:g} Hz{mimo_cap_text(mimo_run)})', SR, np.fft.irfft(mimo_run.applied[role], N_FFT).astype(np.float32), shifted))
    for role in engine.subs:
        if role in raw:
            seq += 1
            s = run.result.subs.get(role)
            measured(seq, role, f' · DSP 값: {sub_value_text(s)}' if s is not None else '')
    return items


def export_before_after_mdat(run: CalibrationRun, folder, mimo_run: MimoRun | None = None,
                             spl_offset_db: float | None = None, sources: dict | None = None) -> Path:
    """보정 전·후 mdat 하나(`<folder>/SECS 보정 전후 <모드>.mdat`) — 내보내기 단계가 부른다."""
    from secs.rew_mdat.builder import build_mdat

    return build_mdat(before_after_items(run, mimo_run, sources), Path(folder) / f'SECS 보정 전후 {run.settings.label()}.mdat',
                      spl_offset_db=spl_offset_db)


def export_camilladsp(run: CalibrationRun, folder, mimo_run: MimoRun | None = None) -> list[Path]:
    """카밀라 DSP v4 설정(`configs/secs_<모드>.yml`, ASCII) + FIR wav(`coeffs/`) — CamillaGUI 폴더 모양(10-10).
    MIMO(IIR)를 계산했으면 서포트 경로도 넣는다(09-26 — 카밀라로 돌린 출력을 엔진 체인과 대조해 확인했다)."""
    from secs.export_camilladsp import mimo_paths, write_config

    fir = mimo_run is not None and mimo_kind(mimo_run) == 'FIR'
    iir = None if fir else (mimo_run.iir if mimo_run is not None else None)    # FIR 은 `design.supports` 로 Conv wav(이미 있는 길)
    n_mimo = len(mimo_paths(run.engine, mimo_run.design, iir)) if mimo_run is not None and mimo_run.iir else 0
    # 장치 · 라우팅 · 지연 안내는 내보내기가 머리 주석 끝에 따로 쓴다(`export_camilladsp.howto_lines`, 10-10).
    note = (f'mode {run.settings.mode}, total latency {run.latency_ms:.1f} ms, RC upper {run.rc_hi:.0f} Hz; '
            + (f'MIMO {mimo_kind(mimo_run)} support paths {n_mimo} (Fsiso {mimo_run.fsiso:g} Hz'
               + (f', max {mimo_run.max_supports} per input' if getattr(mimo_run, 'max_supports', 0) else '') + ')' if n_mimo else 'no MIMO'))
    kw = {'mimo': mimo_run.design, 'iir': iir, 'fsiso': mimo_run.fsiso} if n_mimo else {}
    mode = run.settings.mode + (f'{run.settings.ultra_ms:g}' if run.settings.mode == ULTRA_MODE else '')
    return write_config(run.engine, run.result, Path(folder), name=f'secs_{mode}', note=note,
                        latency_ms=float(run.latency_ms), mode=mode, **kw)


#: 필터 내보내기 범위(10-15 사용자): 텍스트 = RC 만(BA · MIMO 없음 — 사람이 옮겨 넣는 값), EqAPO = RC · BA(MIMO 는 경로가
#: 수십~백여 개라 윈도우 오디오 엔진 안에서 돌기에 무겁다), 카밀라 = 전부.
FILTER_EXPORT_SCOPE = {'camilla': 'RC · BA · MIMO', 'text': 'RC', 'eqapo': 'RC · BA'}


def filter_export_block(run: CalibrationRun, mimo_run: MimoRun | None, kind: str) -> str:
    """이 결과를 `kind` 로 낼 수 없으면 그 이유(화면 문구), 되면 ''."""
    if kind == 'text':
        if run.result.subs:
            return '텍스트는 RC 만 적용한 결과에서만 — 서브(BA)가 있습니다'
        if mimo_run is not None:
            return '텍스트는 RC 만 적용한 결과에서만 — MIMO 를 끄면 낼 수 있습니다'
    if kind == 'eqapo' and mimo_run is not None:
        return 'EqAPO 는 RC · BA 까지 — MIMO 를 끄면 낼 수 있습니다'
    return ''


def _filter_plan(run: CalibrationRun, mimo_run: MimoRun | None):
    """(필터 계획, 모드 이름(ASCII), MIMO 갈래 수) — 텍스트 · EqAPO 내보내기가 같이 쓴다(카밀라와 같은 MIMO 규칙)."""
    from secs.export_filters import make_plan

    fir = mimo_run is not None and mimo_kind(mimo_run) == 'FIR'
    iir = None if fir else (mimo_run.iir if mimo_run is not None else None)
    kw = {'mimo': mimo_run.design, 'iir': iir, 'fsiso': mimo_run.fsiso} if mimo_run is not None and mimo_run.iir else {}
    plan = make_plan(run.engine, run.result, **kw)
    mode = run.settings.mode + (f'{run.settings.ultra_ms:g}' if run.settings.mode == ULTRA_MODE else '')
    return plan, mode, len(plan.mimo)


def export_filter_text(run: CalibrationRun, folder, mimo_run: MimoRun | None = None) -> list[Path]:
    """필터 값 텍스트(`SECS 필터 <모드>.txt`, 한국어 — DSP 에 직접 입력할 값) + FIR wav(10-15). RC 만(`filter_export_block`)."""
    from secs.export_filters import stamp, write_text

    if block := filter_export_block(run, mimo_run, 'text'):
        raise CalibrationError(block)

    plan, mode, n_mimo = _filter_plan(run, mimo_run)
    info = [f'만든 때 {stamp()} · 전체 지연 {run.latency_ms:.1f} ms · ' + ' · '.join(rc_pieces(run)),
            '샘플레이트 48000 Hz(FIR wav · 딜레이 샘플 수 기준)'
            + (f' · MIMO {mimo_kind(mimo_run)} 서포트 {n_mimo}개(Fsiso {mimo_run.fsiso:g} Hz)' if n_mimo else '')]
    return write_text(plan, Path(folder), f'SECS 필터 {run.settings.label()}.txt', f'secs_{mode}',
                      f'SECS 2 보정 필터 — {run.settings.label()}', info)


def export_eqapo(run: CalibrationRun, folder, mimo_run: MimoRun | None = None) -> list[Path]:
    """Equalizer APO 설정째로(10-15 사용자: "입출력만 설정하면 바로 쓸 수 있게") — `config.txt`(입출력 번호 · Include)
    + `secs_<모드>/`(필터 본체 · FIR wav) + 한국어 안내. 폴더 안의 것을 EqAPO config 폴더에 복사하면 된다."""
    from secs.export_filters import write_eqapo

    if block := filter_export_block(run, mimo_run, 'eqapo'):
        raise CalibrationError(block)
    plan, mode, n_mimo = _filter_plan(run, mimo_run)
    note = (f'mode {mode}, total latency {run.latency_ms:.1f} ms, RC upper {run.rc_hi:.0f} Hz; '
            + (f'MIMO {mimo_kind(mimo_run)} support paths {n_mimo} (Fsiso {mimo_run.fsiso:g} Hz)' if n_mimo else 'no MIMO'))
    return write_eqapo(plan, Path(folder), f'secs_{mode}', mode_label=run.settings.label(), note=note,
                       latency_ms=float(run.latency_ms))


def estimate_seconds(project, mode: str = DEFAULT_MODE) -> float:
    """보정 계산 예상 초(구성으로 어림) — 09-26 합성 2.1(메인 2·서브 1) 최소 지연 42 s·저사양 70 s(단일 스레드,
    대부분 입력 PEQ 맞춤), 서브가 둘 이상이면 BA DE 가 수십 초 더."""
    try:
        roles = [s.role for s in project.config.speakers]
    except Exception:  # noqa: BLE001 - 구성이 비면 기본값
        roles = ['L', 'R']
    from secs.model import role_kind

    mains = sum(1 for r in roles if role_kind(r) != 'sub')
    subs = len(roles) - mains
    # 10-09 이 PC(Ryzen 5 5600X) 실측: 돌비룸 실측 2026-10-05.secs — 14채널(메인 11 · 서브 3) · 9위치 · mixed → 81.0 s
    # (전의 식은 412 s 로 다섯 배 넘게 컸다 — 사용자 "예상과 실제가 크게 다르다"). 메인당 6.5 s(9위치 기준, 위치가 적으면
    # 덜) + 서브 둘 이상이면 결합 DE 10 s. zero 는 전의 비(20/32)를 그대로 둔다(따로 재지 않음).
    try:
        positions = max(1, len(measured_positions(project)))
    except Exception:  # noqa: BLE001
        positions = 9
    per = (6.5 if mode != 'zero' else 4.1) * (0.4 + 0.6 * min(positions, 9) / 9)
    return max(5.0, per * max(mains, 1) + (10.0 if subs > 1 else 0.0))


def estimate_subs_seconds(project) -> float:
    """[서브만 계산](BA 까지만) 예상 초 — 10-12 맥 실측(돌비룸 2026-10-09 · 서브 3 · 9위치) 11.8 s × 이 PC 1.5 배 ≈ 18 s.
    위치 수에 비례로 어림(보정 계산 어림과 같은 꼴), 서브가 하나면 정렬할 것이 없어 짧다."""
    from secs.model import role_kind
    try:
        subs = sum(1 for s in project.config.speakers if role_kind(s.role) == 'sub')
    except Exception:  # noqa: BLE001 - 구성이 비면 기본값
        subs = 2
    try:
        positions = max(1, len(measured_positions(project)))
    except Exception:  # noqa: BLE001
        positions = 9
    return max(3.0, SUBS_ONLY_S * (0.4 + 0.6 * min(positions, 9) / 9) * (1.0 if subs > 1 else 0.3))


#: [서브만 계산] 어림의 기준(9위치 · 서브 둘 이상, 이 PC) — 위 독스트링.
SUBS_ONLY_S = 18.0


def estimate_xo_seconds(project, mode: str = DEFAULT_MODE, candidates: int = len(XO_CANDIDATES)) -> float:
    """XO 추천 예상 초 — 후보마다 보정 한 번을 워커(코어 절반)로 나눠 돈다. 동시에 돌면 한 후보가 조금 느려진다:
    10-09 맥미니 실측(돌비룸 14ch·9위치·mixed, 후보 4) 직렬 138.5 s(후보당 34 s) vs 병렬 38.5 s(후보당 37 s, 1.1 배) —
    결과 표·추천은 비트 단위로 같았다. 화면은 후보 하나가 끝나면 이 어림 대신 실제 속도로 남은 시간을 센다."""
    from secs.mimo_iir import _workers

    rounds = -(-int(candidates) // _workers(int(candidates)))
    return estimate_seconds(project, mode) * rounds * (1.1 if rounds < candidates else 1.0)


def estimate_mimo_seconds(project, positions: int | None = None, fir: bool = False) -> float:
    """MIMO 계산 예상 초 = 서포트 고르기 + 입력(메인 채널)마다 IIR 맞춤 ÷ 워커 `MIMO_WORKERS` 개(라운드 단위).
    FIR 서포트(`fir`)는 IIR 맞춤이 없어 고르기만.
    10-09 맥 실측(돌비룸 2026-10-09 · 입력 11 · 9위치 · 워커 2): 고르기 25 s + IIR 1,299 s → 입력 하나 ≈ 236 s(맥).
    이 PC 는 맥보다 약 1.5배 느리다(CLAUDE.md) → 입력 하나 MIMO_INPUT_S. 위치 수에 비례로 어림."""
    import math
    from secs.model import role_kind
    try:
        inputs = sum(1 for sp in project.config.speakers if role_kind(sp.role) != 'sub')
    except Exception:  # noqa: BLE001
        inputs = 2
    if positions is None:
        try:
            positions = len(measured_positions(project))
        except Exception:  # noqa: BLE001
            positions = 9
    scale = max(positions, 3) / 9
    rounds = 0 if fir else math.ceil(max(inputs, 1) / max(MIMO_WORKERS, 1))
    return max(10.0, (MIMO_PICK_S + rounds * MIMO_INPUT_S) * scale)


#: 서포트 고르기(FIR 단계) 초와 입력 하나의 IIR 맞춤 초(이 PC 어림 = 맥 실측 × 1.5, 10-09).
MIMO_PICK_S = 40.0
MIMO_INPUT_S = 355.0


__all__ = ['MODE_KEYS', 'DEFAULT_MODE', 'ULTRA_MODE', 'ULTRA_MS_RANGE', 'ultra_params', 'mode_params', 'CalibrationSettings', 'CalibrationRun', 'MimoRun', 'CalibrationError',
           'mode_spec', 'mode_label', 'measured_positions', 'mimo_availability', 'engine_inputs', 'compute',
           'compute_mimo', 'mimo_kind', 'mimo_fir_budget_text', 'stage_items', 'export_mdat', 'export_camilladsp', 'export_filter_text', 'export_eqapo', 'filter_export_block', 'FILTER_EXPORT_SCOPE', 'estimate_seconds', 'estimate_subs_seconds', 'estimate_xo_seconds', 'estimate_mimo_seconds',
           'before_after_items', 'export_before_after_mdat', 'rc_pieces', 'room_dims', 'room_volume',
           'SPL_GRID', 'GD_GRID', 'MIMO_MIN_POSITIONS', 'FSISO_RANGE', 'RC_HI_RANGE', 'ROOM_DIM_RANGE',
           'XO_RANGE', 'XO_CANDIDATES', 'XO_GRID', 'XoRecommendation', 'xo_groups', 'xo_scores', 'pick_xo',
           'recommend_xo', 'xo_preview', 'SUB_MODES', 'SUB_BUNDLES', 'sub_entry', 'sub_has_filters', 'MIMO_MAX_SUPPORTS_RANGE', 'mimo_cap_text', 'SUB_GAIN_WARN_DB', 'sub_filter_warnings', 'sub_gain_warnings', 'SUB_BAND_TYPES', 'band_type_text', 'sub_filter_text', 'sub_mode_text', 'sub_value_text',
           'sub_values_text', 'SUB_LAYOUTS', 'DBA_ROWS', 'bundle_layout_entry', 'layout_lines']
