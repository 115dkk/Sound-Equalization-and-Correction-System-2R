# -*- coding: utf-8 -*-
"""새 엔진(`secs.correction_engine`) 결과 → CamillaDSP v4 설정(YAML) + FIR wav(2026-09-25 도구에서 옮김, 09-26).

누가 부르나: 보정 설계 · 내보내기 화면(`secs.calibration.export_camilladsp`), `tools/export_camilladsp.py`(돌비 3서브 명령),
카밀라 출력 대조 도구. 입력: `CorrectionEngine`(계산 끝난 것)·`CorrectionResult`·출력 폴더.
출력: 설정 문자열(`make_config`) / 파일(`write_config`: `configs/<이름>.yml` + `coeffs/<이름>_<role>_fir.wav`).

폴더 모양 = CamillaGUI 의 카밀라 폴더(2026-10-10 사용자: "다른 사람이 윈도우나 맥에서 카밀라 GUI 로 설정할 텐데 뭘 어떻게
넣어야 하는지"): 설정은 `configs/`, FIR wav 는 `coeffs/` — GUI 의 Files 탭에서 설정은 Configs 로, wav 는 Filters(계수)로
올리면 그대로 열린다. FIR 경로는 **설정 파일 기준 상대 경로** `../coeffs/<파일>`: 이 PC 의 CamillaDSP 4.1.3 으로 확인했다 —
Conv `filename` 의 상대 경로는 설정 파일 폴더에서 먼저 찾고(없을 때만 작업 폴더), GUI 백엔드도 같은 기준(config_dir)으로
절대 경로를 만든다. 그래서 내보낸 폴더를 통째로 옮겨도, GUI 의 configs · coeffs 에 나눠 넣어도 열린다. wav 이름 앞에 설정
이름을 붙인다 — GUI 의 coeffs 폴더 하나를 여러 설정이 같이 써서, 모드를 둘 올리면 `L_fir.wav` 가 서로 덮였다.
라우팅: 파이프라인 맨 앞 `route_in`(캡처 채널 → SECS 입력) · 맨 끝 `route_out`(SECS 출력 → 재생 채널) 믹서, 기본 1:1 —
GUI Mixers 탭에서 번호만 바꾸면 인터페이스 배선에 맞출 수 있다(신호는 1:1 이라 전과 같다). 머리 주석에 순서표 · 장치 · 지연 안내.
(시험용 파일 장치 `in.raw` · `out.raw` 는 작업 폴더 기준이라 절대 경로 그대로 — 실제로는 GUI 에서 장치를 바꾼다.)

신호 흐름(엔진과 같다):
  입력 k: PEQ(입력 보정) → FIR(입력 보정) ─┬─ 메인 경로: 게인 → 1차 PEQ(XO 타깃) → LR4 HP → 딜레이 → 메인 출력 k
                                           └─ 서브 센드: LR4 LP ─┐
  입력 LFE(맨 뒤, 서브가 있으면, 10-05): +10 dB → LR4 LP 120 Hz ─ 서브 센드 ─┐
  서브 버스: Σ 센드 → (서브마다) 게인·극성 → 서브 버스 PEQ → 고정 대역 필터·고정 PEQ → 올패스 2 → 딜레이 → 서브 출력
  고정 대역 필터·PEQ(10-05 R27 추가, 사람이 넣은 HP·LP(LR 24 기본)·PEQ — 예: INFRA 로우패스)는 엔진이 그 서브 스피커 응답에
  곱해 두고 계산했다 → 그 서브로 가는 MIMO 갈래에도 같은 필터를 넣는다(INFRA 가 자기 LP 위를 돕지 않게). 게인·극성은 버스 믹서
  안(곱이라 순서와 무관), 리드가 정한 순서는 '버스 PEQ → 서브별 대역·PEQ → 게인·극성·딜레이'.
딜레이는 정수 샘플(카밀라 subsample 딜레이는 고역 위상을 37° 까지 틀었다, 09-25).

MIMO 서포트 경로(IIR, 09-26 — `mimo`·`iir` 를 주면): 입력 i 원신호(입력 보정 전)를 경로마다 복사(맨 앞 `mimo_fork` 믹서) →
경로 필터 = 게인·극성 → 지연(서포트 지연 + 서포트 스피커 j 의 정렬 지연) → PEQ 8 → 올패스 3 → LR4 HP(서포트 하한) →
LR4 LP(Fsiso) → 맨 끝 `mimo_merge` 믹서에서 스피커 j 출력에 더한다(서브 버스 필터 뒤 — 경로에 서브 필터가 안 걸리게).
엔진의 `mimo_iir.apply_iir` 와 같은 전기 체인이다(`support_path` 의 HP 는 hp2² = LR4).
FIR MIMO(`iir` 없이 `mimo` 만, 09-26): 경로 필터 = 지연(스피커 j 정렬) → LR4 HP(하한) → Conv(저속 600 Hz FIR 을 48 kHz 로 올린 것 —
Fsiso 저역 통과가 FIR 안에 들어 있다). `mimo.apply_mimo` 와 같은 체인.

파일은 **ASCII 만** 쓴다(09-26): CamillaGUI 서버가 설정을 CP949 로 읽어 한글 경로·주석에서 실패했다(500, 커밋 b436b08).
주석은 영어, 필터 이름은 역할 ID(ASCII), 문자열은 JSON 이스케이프(`\\uXXXX` — YAML 큰따옴표 문자열이 그대로 읽는다)라
폴더 이름에 한글이 있어도 파일 자체는 ASCII 다.
FIR 이 없는 모드(최소 지연 = IIR 만, `fir` 길이 1)는 Conv 필터를 넣지 않는다.
"""
from __future__ import annotations

import json
import os
import struct
from pathlib import Path

import numpy as np

SAMPLE_RATE = 48000
#: CamillaGUI 의 카밀라 폴더와 같은 이름(설정 · 계수 파일). 내보낸 폴더도 이 모양으로 쓴다.
CONFIG_DIR = 'configs'
COEFF_DIR = 'coeffs'
#: 머리 주석의 사용 안내 시작 줄 — `tools/camilla_mac_push.py` 가 맥 설정을 만들 때 이 줄부터는 뺀다(장치 · 배치를 그 도구가 정한다).
HOWTO_MARK = '# ---- HOW TO USE'


def write_wav(path: Path, data, rate: int = SAMPLE_RATE) -> None:
    """float32 모노 wav(표준 IEEE float, 외부 모듈 없이)."""
    d = np.asarray(data, dtype='<f4').tobytes()
    fmt = struct.pack('<HHIIHH', 3, 1, rate, rate * 4, 4, 32)
    body = b'fmt ' + struct.pack('<I', len(fmt)) + fmt + b'data' + struct.pack('<I', len(d)) + d
    Path(path).write_bytes(b'RIFF' + struct.pack('<I', 4 + len(body)) + b'WAVE' + body)


def _abs_posix(path: Path) -> str:
    return str(Path(path).resolve()).replace('\\', '/')


def _rel_posix(path: Path, start: Path) -> str:
    """`start`(설정 파일 폴더) 기준 상대 경로, / 구분 — 윈도우 · 맥 CamillaDSP 둘 다 읽는다."""
    return os.path.relpath(Path(path).resolve(), Path(start).resolve()).replace('\\', '/')


def order_table(inputs: list[str], outputs: list[str]) -> list[str]:
    """번호(0부터) · 입력 · 출력 표 — 머리 주석과 도움말 그림(`tools/make_camilla_help_shots.py`)이 같이 쓴다(ASCII)."""
    rows = ['#    ch  input (capture -> route_in)   output (route_out -> playback)']
    for k in range(max(len(inputs), len(outputs))):
        a = inputs[k] if k < len(inputs) else '-'
        b = outputs[k] if k < len(outputs) else '-'
        rows.append(f'#   {k:3d}  {a:30s}  {b}')
    return rows


def howto_lines(inputs: list[str], outputs: list[str], latency_ms: float | None = None, mode: str = '') -> list[str]:
    """사람이 읽는 사용 안내(영어, ASCII) — CamillaGUI 에 올리는 순서대로. 머리 주석의 맨 끝에 붙는다."""
    # 사실 근거: `문서/조사/CamillaDSP GUI 적용 조사 2026-10-10.md`(4.1.3 + GUI 4.1.0) — 장치 형식 F32(F32_LE 는 파일 장치만,
    # 실측 오류), 클럭이 다른 루프백 → 인터페이스는 rate adjust(윈도우는 AsyncSinc 도), GUI 는 wav 가 없어도 경고만이라 wav 먼저.
    lines = [HOWTO_MARK + ' (CamillaDSP 4.1.x + CamillaGUI 4.1) ----',
             '# 1) Files: this file goes to camilladsp/configs, the wav files to camilladsp/coeffs',
             '#    (same layout as this export). CamillaGUI: Files tab, upload the wav files under Filters FIRST,',
             '#    then this file under Configs. FIR paths are relative to this file (../coeffs/...).',
             '# 2) Devices: capture/playback below are FILE placeholders (in.raw / out.raw) - change them:',
             '#    capture = loopback (Windows: Wasapi "CABLE Output (VB-Audio Virtual Cable)", Mac: CoreAudio',
             '#    "BlackHole 16ch"), playback = your audio interface. format: empty or F32 (F32_LE is for files only).',
             '#    samplerate 48000 (Windows: set both devices\' default format to 48000 Hz too).',
             '#    Different clocks (loopback -> interface): enable_rate_adjust; Windows also resampler AsyncSinc.',
             '#    Capture channels = route_in "in", playback channels = route_out "out".',
             '# 3) Channel numbers start at 0 (interface output 1 = 0). Default routing is 1:1:']
    lines += order_table(inputs, outputs)
    lines += ['#    To move a channel change only the Mixers route_in (source "channel") or route_out ("dest").',
              '#    More interface outputs: raise route_out "out" and the playback channels to the same number.',
              '#    Players send LFE 4th (Windows 7.1: FL FR FC LFE BL BR SL SR) - reorder them in route_in.']
    if latency_ms is not None:
        ms = float(latency_ms)
        lines.append(f'# 4) Latency: filters {ms:.1f} ms{f" (mode {mode})" if mode else ""} + CamillaDSP buffering '
                     '(grows with chunksize).')
        lines.append(f'#    Video: set the player audio delay to about -{ms:.0f} ms (plus the buffering) to keep lip sync.')
    return lines


def file_devices(out: Path, n_in: int, n_out: int) -> tuple[dict, dict]:
    """검증용 파일 장치(RawFile → File, float32) — 실제 장치로 쓸 때는 CamillaGUI 에서 바꾼다."""
    capture = {'type': 'RawFile', 'filename': _abs_posix(Path(out) / 'in.raw'), 'format': 'F32_LE',
               'channels': int(n_in), 'extra_samples': 96000}
    playback = {'type': 'File', 'filename': _abs_posix(Path(out) / 'out.raw'), 'format': 'F32_LE',
                'channels': int(n_out)}
    return capture, playback


def fir_to_48k(s_low: np.ndarray, fs_low: float) -> np.ndarray:
    """저속 FIR(fs_low) → 48 kHz FIR(같은 길이의 시간, 대역 제한 보간). 0.95·fs_low/2 위는 0 으로(코사인 가장자리)."""
    s_low = np.asarray(s_low, float)
    ratio = SAMPLE_RATE / float(fs_low)
    n48 = int(np.ceil(s_low.size * ratio))
    nfft = 1 << int(np.ceil(np.log2(n48 * 2)))
    f = np.fft.rfftfreq(nfft, 1.0 / SAMPLE_RATE)
    edge = fs_low / 2
    Hs = np.exp(-2j * np.pi * np.outer(f[f < edge], np.arange(s_low.size)) / fs_low) @ s_low
    H = np.zeros(f.size, complex)
    H[:Hs.size] = Hs * np.clip((edge - f[:Hs.size]) / (0.1 * edge), 0, 1)
    return np.fft.irfft(H, nfft)[:n48]          # 48 kHz 격자에서 DTFT 가 저속 FIR 과 같다(이득 그대로)


def mimo_paths(eng, mimo, iir: dict | None) -> list:
    """[(입력, 서포트 스피커, 필터, 하한 Hz, 스피커 정렬 지연 ms)] — 필터 = IirSupport(IIR MIMO) 또는
    저속 FIR 배열(FIR MIMO, `iir` 가 None 일 때 `mimo.supports`).
    정렬 지연에는 메인 추가 지연(`main_extra_ms`)을 더한다(10-05): 엔진은 서브를 앞당기느라 메인·서브 딜레이에 모두 이 값을
    더하므로, 갈래도 같이 늦춰야 엔진이 설계한 시간 관계가 지켜진다(전에는 빠져 있어 갈래가 최대 2 ms 일찍 도착했다)."""
    out = []
    extra = float(getattr(eng, 'main_extra_ms', 0.0) or 0.0)

    def d_ms(j):
        return float(eng.align_delay_ms.get(j, 0.0)) + extra

    if iir is None and mimo is not None:
        for i, sups in mimo.supports.items():
            for j, s in sups.items():
                low = (mimo.ranges.get(j) or (50.0, 0))[0]
                out.append((i, j, np.asarray(s, float), float(low), d_ms(j)))
        return out
    for i, sups in (iir or {}).items():
        for j, s in sups.items():
            low = (mimo.ranges.get(j) or (50.0, 0))[0]
            out.append((i, j, s, float(low), d_ms(j)))
    return out


#: LFE 입력(10-05, 사용자: 업믹스를 안 거친 돌비 신호가 그대로 들어올 수 있다) — 서브가 있으면 입력 맨 뒤에 하나.
#: 돌비 재생 표준대로 대역 안 +10 dB, 콘텐츠 대역(120 Hz) 밖은 LR 24 로 자른 뒤 메인 저역 센드와 같은 서브 버스로 —
#: 서브마다 게인 · 극성 · 버스 PEQ · 고정 필터 · 올패스 · 딜레이를 메인 저역과 똑같이 탄다(엔진 lowband 결정 14 와 같은 뜻:
#: 'LFE 는 송출 없이 서브마다 그대로, 서브 정렬은 서브 행 공통값'). LFE 의 MIMO 서포트는 아직 없다.
LFE_GAIN_DB = 10.0
LFE_LP_HZ = 120.0


def has_lfe(res, lfe_gain_db: float | None = LFE_GAIN_DB) -> bool:
    """내보내기에 LFE 입력이 들어가는지 — 서브가 있고 `lfe_gain_db` 가 None 이 아닐 때."""
    return bool(res.subs) and lfe_gain_db is not None


def make_config(eng, res, out: Path, capture: dict, playback: dict, chunksize: int = 1024,
                note: str = '', mimo=None, iir: dict | None = None, fsiso: float = 150.0,
                lfe_gain_db: float | None = LFE_GAIN_DB, lfe_lp_hz: float = LFE_LP_HZ,
                config_dir: Path | None = None, fir_prefix: str = '', latency_ms: float | None = None,
                mode: str = '') -> str:
    """YAML 문자열(ASCII). FIR wav 는 `out/coeffs/<fir_prefix><role>_fir.wav` 에 쓰고, 설정에는 설정 파일이 놓일 폴더
    `config_dir`(기본 `out/configs`) 기준 상대 경로를 넣는다(모듈 독스트링 — 카밀라가 설정 파일 폴더 기준으로 찾는다).
    `note` 는 머리 주석에 한 줄 더 붙일 영어 문구(ASCII 가 아닌 글자는 버린다). `latency_ms` · `mode` 는 사용 안내의 지연 줄.
    서브가 있으면 입력 맨 뒤에 LFE(`lfe_gain_db` 게인 → LR 24 LP `lfe_lp_hz` → 서브 버스). `lfe_gain_db=None` 이면 LFE 없음."""
    mains = list(res.channels)
    subs = list(res.subs)
    lfe = has_lfe(res, lfe_gain_db)
    n_main, n_sub = len(mains), len(subs)
    n_in = n_main + (1 if lfe else 0)                   # LFE 는 입력 n_main(맨 뒤)
    filters, lines_pipe = {}, [('Mixer', 'route_in', None)]
    config_dir = Path(out) / CONFIG_DIR if config_dir is None else Path(config_dir)

    def peaking(name, f0, q, g):
        filters[name] = {'type': 'Biquad', 'parameters': {'type': 'Peaking', 'freq': round(float(f0), 4),
                                                          'q': round(float(q), 5), 'gain': round(float(g), 4)}}

    paths = mimo_paths(eng, mimo, iir) if mimo is not None and (iir or iir is None) else []
    n_p = len(paths)
    n_out = n_main + n_sub
    if paths:
        # MIMO: 입력 원신호를 경로마다 복사(입력 보정 필터보다 먼저)
        fork = {'channels': {'in': n_in, 'out': n_in + n_p},
                'mapping': [{'dest': k, 'sources': [{'channel': k, 'gain': 0, 'inverted': False}]} for k in range(n_in)] +
                           [{'dest': n_in + p, 'sources': [{'channel': mains.index(i), 'gain': 0, 'inverted': False}]}
                            for p, (i, _j, _s, _lo, _d) in enumerate(paths)]}
        lines_pipe.append(('Mixer', 'mimo_fork', None))
    fir_dir = Path(out) / COEFF_DIR

    def fir_file(stem: str) -> Path:
        fir_dir.mkdir(parents=True, exist_ok=True)
        return fir_dir / f'{fir_prefix}{stem}.wav'

    # 1) 입력 보정: PEQ → FIR
    for k, role in enumerate(mains):
        c = res.channels[role]
        names = []
        for b, (f0, q, g) in enumerate(c.peq):
            peaking(f'in_{role}_peq{b}', f0, q, g)
            names.append(f'in_{role}_peq{b}')
        if np.asarray(c.fir).size > 1:                  # 최소 지연 모드(IIR 만)는 FIR 이 [1.0] — 넣지 않는다
            wav = fir_file(f'{role}_fir')
            write_wav(wav, c.fir)
            filters[f'in_{role}_fir'] = {'type': 'Conv', 'parameters': {'type': 'Wav', 'filename': _rel_posix(wav, config_dir)}}
            names.append(f'in_{role}_fir')
        if names:
            lines_pipe.append(('Filter', [k], names))
    if lfe:                                             # LFE: +10 dB → 콘텐츠 대역 LP(입력 보정 없음 — 서브 보정은 버스 뒤에서)
        filters['lfe_gain'] = {'type': 'Gain', 'parameters': {'gain': round(float(lfe_gain_db), 4), 'inverted': False}}
        filters['lfe_lp'] = {'type': 'BiquadCombo', 'parameters': {'type': 'LinkwitzRileyLowpass', 'freq': float(lfe_lp_hz), 'order': 4}}
        lines_pipe.append(('Filter', [n_main], ['lfe_gain', 'lfe_lp']))
    # 2) 나누기: 메인 경로 k, 서브 센드 n_in + k (LFE 는 메인 경로 칸이 남지만 버스에서 안 쓴다)
    split = {'channels': {'in': n_in + n_p, 'out': 2 * n_in + n_p},
             'mapping': [{'dest': k, 'sources': [{'channel': k, 'gain': 0, 'inverted': False}]} for k in range(n_in)] +
                        [{'dest': n_in + k, 'sources': [{'channel': k, 'gain': 0, 'inverted': False}]} for k in range(n_in)] +
                        [{'dest': 2 * n_in + p, 'sources': [{'channel': n_in + p, 'gain': 0, 'inverted': False}]} for p in range(n_p)]}
    lines_pipe.append(('Mixer', 'split', None))
    for k, role in enumerate(mains):
        c = res.channels[role]
        names = []
        filters[f'main_{role}_gain'] = {'type': 'Gain', 'parameters': {'gain': round(float(c.gain_db), 4), 'inverted': False}}
        names.append(f'main_{role}_gain')
        for b, (f0, q, g) in enumerate(eng.top_peq.get(role, [])):
            peaking(f'main_{role}_xopeq{b}', f0, q, g)
            names.append(f'main_{role}_xopeq{b}')
        if c.xo_hz:
            filters[f'main_{role}_hp'] = {'type': 'BiquadCombo', 'parameters': {'type': 'LinkwitzRileyHighpass', 'freq': float(c.xo_hz), 'order': 4}}
            names.append(f'main_{role}_hp')
            filters[f'send_{role}_lp'] = {'type': 'BiquadCombo', 'parameters': {'type': 'LinkwitzRileyLowpass', 'freq': float(c.xo_hz), 'order': 4}}
            lines_pipe.append(('Filter', [n_in + k], [f'send_{role}_lp']))
        filters[f'main_{role}_delay'] = {'type': 'Delay', 'parameters': {'delay': int(round(c.delay_ms * SAMPLE_RATE / 1000)), 'unit': 'samples', 'subsample': False}}
        names.append(f'main_{role}_delay')
        lines_pipe.append(('Filter', [k], names))
    # 3) 버스: 메인 k → 출력 k, 센드 합 → 서브 출력(서브마다 게인·극성)
    bus = {'channels': {'in': 2 * n_in + n_p, 'out': n_out + n_p},
           'mapping': [{'dest': k, 'sources': [{'channel': k, 'gain': 0, 'inverted': False}]} for k in range(n_main)] +
                      [{'dest': n_main + s, 'sources': [{'channel': n_in + k, 'gain': round(float(a.gain_db), 4),
                                                          'inverted': a.polarity < 0} for k in range(n_in)
                                                         if k == n_main or res.channels[mains[k]].xo_hz]}
                       for s, a in enumerate(res.subs.values())]}
    # 센드이 하나도 없는 서브(XO 가 없는 구성)는 카밀라가 빈 sources 를 거부한다 — 매핑에서 뺀다.
    bus['mapping'] = [m for m in bus['mapping'] if m['sources']]
    bus['mapping'] += [{'dest': n_out + p, 'sources': [{'channel': 2 * n_in + p, 'gain': 0, 'inverted': False}]} for p in range(n_p)]
    lines_pipe.append(('Mixer', 'bus', None))
    def fixed_filters(role) -> list:
        """서브 고정 대역 필터·PEQ 이름(정의는 한 번) — 서브 출력과 그 서브로 가는 MIMO 갈래가 같이 쓴다."""
        a = res.subs.get(role)
        if a is None:
            return []
        names = []
        for kind, hz, typ in (('Highpass', getattr(a, 'hp_hz', 0.0), getattr(a, 'hp_type', 'LR24')),
                              ('Lowpass', getattr(a, 'lp_hz', 0.0), getattr(a, 'lp_type', 'LR24'))):
            if hz:
                shape = 'LinkwitzRiley' if typ.startswith('LR') else 'Butterworth'
                name = f'sub_{role}_fixed_{kind[0].lower()}p'
                filters[name] = {'type': 'BiquadCombo', 'parameters': {'type': shape + kind, 'freq': float(hz),
                                                                       'order': int(typ[2:]) // 6}}
                names.append(name)
        for b, (f0, q, g) in enumerate(getattr(a, 'peq', []) or []):
            peaking(f'sub_{role}_fixed_peq{b}', f0, q, g)
            names.append(f'sub_{role}_fixed_peq{b}')
        return names

    for s, (role, a) in enumerate(res.subs.items()):
        names = []
        for b, (f0, q, g) in enumerate(eng.sub_bus_peq):
            peaking(f'sub_{role}_peq{b}', f0, q, g)
            names.append(f'sub_{role}_peq{b}')
        names += fixed_filters(role)
        for b, (f0, q) in enumerate(a.allpass):
            filters[f'sub_{role}_ap{b}'] = {'type': 'Biquad', 'parameters': {'type': 'Allpass', 'freq': round(float(f0), 4), 'q': round(float(q), 5)}}
            names.append(f'sub_{role}_ap{b}')
        filters[f'sub_{role}_delay'] = {'type': 'Delay', 'parameters': {'delay': int(round(a.delay_ms * SAMPLE_RATE / 1000)), 'unit': 'samples', 'subsample': False}}
        names.append(f'sub_{role}_delay')
        lines_pipe.append(('Filter', [n_main + s], names))

    # MIMO 경로 필터 → 합치기
    for p, (i, j, sp, low, d_ms) in enumerate(paths):
        tag = f'mimo_{i}_{j}'
        if isinstance(sp, np.ndarray):                  # FIR MIMO
            wav = fir_file(tag)
            write_wav(wav, fir_to_48k(sp, getattr(mimo, 'fs_low', 600)))
            filters[f'{tag}_delay'] = {'type': 'Delay', 'parameters': {'delay': int(round(d_ms * SAMPLE_RATE / 1000)),
                                                                       'unit': 'samples', 'subsample': False}}
            filters[f'{tag}_hp'] = {'type': 'BiquadCombo', 'parameters': {'type': 'LinkwitzRileyHighpass', 'freq': round(low, 3), 'order': 4}}
            filters[f'{tag}_fir'] = {'type': 'Conv', 'parameters': {'type': 'Wav', 'filename': _rel_posix(wav, config_dir)}}
            lines_pipe.append(('Filter', [n_out + p], [f'{tag}_delay', f'{tag}_hp', f'{tag}_fir'] + fixed_filters(j)))
            continue
        names = [f'{tag}_gain', f'{tag}_delay']
        filters[f'{tag}_gain'] = {'type': 'Gain', 'parameters': {'gain': round(float(sp.gain_db), 4), 'inverted': sp.polarity < 0}}
        filters[f'{tag}_delay'] = {'type': 'Delay', 'parameters': {'delay': int(round((sp.delay_ms + d_ms) * SAMPLE_RATE / 1000)),
                                                                   'unit': 'samples', 'subsample': False}}
        for b, (f0, q, g) in enumerate(sp.peq):
            peaking(f'{tag}_peq{b}', f0, q, g)
            names.append(f'{tag}_peq{b}')
        for b, (f0, q) in enumerate(sp.allpass):
            filters[f'{tag}_ap{b}'] = {'type': 'Biquad', 'parameters': {'type': 'Allpass', 'freq': round(float(f0), 4), 'q': round(float(q), 5)}}
            names.append(f'{tag}_ap{b}')
        filters[f'{tag}_hp'] = {'type': 'BiquadCombo', 'parameters': {'type': 'LinkwitzRileyHighpass', 'freq': round(low, 3), 'order': 4}}
        filters[f'{tag}_lp'] = {'type': 'BiquadCombo', 'parameters': {'type': 'LinkwitzRileyLowpass', 'freq': float(fsiso), 'order': 4}}
        lines_pipe.append(('Filter', [n_out + p], names + [f'{tag}_hp', f'{tag}_lp'] + fixed_filters(j)))
    if paths:
        outs = mains + subs
        merge = {'channels': {'in': n_out + n_p, 'out': n_out},
                 'mapping': [{'dest': o, 'sources': [{'channel': o, 'gain': 0, 'inverted': False}] +
                              [{'channel': n_out + p, 'gain': 0, 'inverted': False}
                               for p, (_i, j, _s, _lo, _d) in enumerate(paths) if outs.index(j) == o]}
                             for o in range(n_out)]}
        lines_pipe.append(('Mixer', 'mimo_merge', None))
    lines_pipe.append(('Mixer', 'route_out', None))
    # 라우팅(10-10): 장치 채널 ↔ SECS 채널, 기본 1:1(신호는 전과 같다). 사람이 GUI Mixers 탭에서 이 둘만 고친다 —
    # route_in 은 source channel(캡처 번호), route_out 은 dest(재생 번호). 장치 채널 수 = route_in in · route_out out.
    one = lambda k: {'dest': k, 'sources': [{'channel': k, 'gain': 0, 'inverted': False}]}    # noqa: E731
    route_in = {'description': 'capture channel -> SECS input (edit source channel to move an input)',
                'channels': {'in': n_in, 'out': n_in}, 'mapping': [one(k) for k in range(n_in)]}
    route_out = {'description': 'SECS output -> playback channel (edit dest to move an output)',
                 'channels': {'in': n_out, 'out': n_out}, 'mapping': [one(k) for k in range(n_out)]}

    def y(v):
        return json.dumps(v, ensure_ascii=True)

    inputs, outputs = mains + (['LFE'] if lfe else []), mains + subs
    head = ['# SECS 2 calibration -> CamillaDSP v4 (generated)',
            f'# inputs {n_in}: {", ".join(inputs)} / outputs {n_main + n_sub}: {", ".join(outputs)}']
    if note:
        head.append('# ' + note.encode('ascii', 'ignore').decode('ascii'))
    if lfe:
        head.append(f'# LFE: gain {lfe_gain_db:+g} dB, LR24 lowpass {lfe_lp_hz:g} Hz -> every sub bus '
                    '(same sub gain/polarity/filters/delay as the bass-managed mains), no MIMO')
    # 서브 출력 값을 사람이 읽게 한 줄씩(10-05 R27 — 고정 서브는 넣은 값이 그대로 나오는지 여기서 바로 본다).
    # 딜레이 필터는 정수 샘플로 반올림되지만 이 줄은 원래 값이다.
    for role, a in res.subs.items():
        ap = ''.join(f', allpass {f0:.1f} Hz Q {q:.3f}' for f0, q in a.allpass)
        fx = ((f', fixed HP {a.hp_hz:g} Hz {getattr(a, "hp_type", "LR24")}' if getattr(a, 'hp_hz', 0.0) else '')
              + (f', fixed LP {a.lp_hz:g} Hz {getattr(a, "lp_type", "LR24")}' if getattr(a, 'lp_hz', 0.0) else '')
              + ''.join(f', fixed PEQ {f0:.1f} Hz Q {q:.3f} {g:+.2f} dB' for f0, q, g in (getattr(a, 'peq', []) or [])))
        head.append(f'# sub {role}: {getattr(a, "mode", "auto")} gain {a.gain_db:+.3f} dB, delay {a.delay_ms:.3f} ms, '
                    f'polarity {"normal" if a.polarity > 0 else "inverted"}{ap}{fx}')
    head += ['#'] + howto_lines(inputs, outputs, latency_ms, mode)
    # CamillaGUI 는 저장할 때 주석을 버린다 — 순서는 Title 탭에 보이는 description 에도 한 줄로 남긴다.
    title = 'SECS 2' + (f' {mode}' if mode else '') + (f' ({float(latency_ms):.1f} ms)' if latency_ms is not None else '')
    desc = ('inputs ' + ', '.join(f'{k} {r}' for k, r in enumerate(inputs)) + ' | outputs '
            + ', '.join(f'{k} {r}' for k, r in enumerate(outputs))
            + ' | numbers start at 0, change routing in Mixers route_in / route_out'
            + ' | devices are file placeholders: set capture = loopback, playback = interface')
    out_lines = head + ['title: ' + y(title), 'description: ' + y(desc),
                        'devices:', f'  samplerate: {SAMPLE_RATE}', f'  chunksize: {int(chunksize)}',
                        '  capture: ' + y(capture), '  playback: ' + y(playback),
                        'filters:']
    for name, spec in filters.items():
        out_lines.append(f'  {name}: ' + y(spec))
    out_lines.append('mixers:')
    out_lines.append('  route_in: ' + y(route_in))
    out_lines.append('  route_out: ' + y(route_out))
    out_lines.append('  split: ' + y(split))
    out_lines.append('  bus: ' + y(bus))
    if paths:
        out_lines.append('  mimo_fork: ' + y(fork))
        out_lines.append('  mimo_merge: ' + y(merge))
    out_lines.append('pipeline:')
    for kind, a, names in lines_pipe:
        if kind == 'Mixer':
            out_lines.append('  - ' + y({'type': 'Mixer', 'name': a}))
        else:
            out_lines.append('  - ' + y({'type': 'Filter', 'channels': a, 'names': names}))
    return '\n'.join(out_lines) + '\n'


def write_config(eng, res, out: Path, name: str = 'secs_calibration', capture: dict | None = None,
                 playback: dict | None = None, note: str = '', mimo=None, iir: dict | None = None,
                 fsiso: float = 150.0, lfe_gain_db: float | None = LFE_GAIN_DB, lfe_lp_hz: float = LFE_LP_HZ,
                 latency_ms: float | None = None, mode: str = '') -> list[Path]:
    """`out/configs/<name>.yml` + `out/coeffs/<name>_*.wav` 를 쓰고 쓴 파일 목록(설정이 맨 앞)을 돌려준다. 장치를 안 주면
    파일 장치(검증용, `out/in.raw` → `out/out.raw`). `name` 은 ASCII 로 바꾼다(ASCII 가 아닌 글자는 '_')."""
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    safe = ''.join(ch if (ch.isascii() and (ch.isalnum() or ch in '-_.')) else '_' for ch in name) or 'secs_calibration'
    n_in = len(res.channels) + (1 if has_lfe(res, lfe_gain_db) else 0)
    default_cap, default_pb = file_devices(out, n_in, len(res.channels) + len(res.subs))
    text = make_config(eng, res, out, capture or default_cap, playback or default_pb, note=note, mimo=mimo, iir=iir, fsiso=fsiso,
                       lfe_gain_db=lfe_gain_db, lfe_lp_hz=lfe_lp_hz, config_dir=out / CONFIG_DIR, fir_prefix=safe + '_',
                       latency_ms=latency_ms, mode=mode)
    if capture is None and not (out / 'in.raw').exists():
        # 파일 장치는 입력 파일이 있어야 `camilladsp -c` 검사를 통과한다 — 무음 한 청크(시험 신호는 검증 도구가 덮어쓴다).
        (out / 'in.raw').write_bytes(np.zeros((1024, n_in), dtype='<f4').tobytes())
    (out / CONFIG_DIR).mkdir(parents=True, exist_ok=True)
    path = out / CONFIG_DIR / f'{safe}.yml'
    path.write_text(text, encoding='ascii')
    return [path] + sorted((out / COEFF_DIR).glob(f'{safe}_*.wav'))


__all__ = ['fir_to_48k', 'mimo_paths', 'write_wav', 'make_config', 'write_config', 'file_devices', 'has_lfe',
           'order_table', 'howto_lines', 'SAMPLE_RATE', 'LFE_GAIN_DB', 'LFE_LP_HZ', 'CONFIG_DIR', 'COEFF_DIR', 'HOWTO_MARK']
