"""Frozen shared engine inputs; all later engine exporters import cases()."""
from __future__ import annotations

import copy
import json
import sys
from dataclasses import dataclass
from functools import lru_cache

import common
import numpy as np
from secs import calibration as cal
from secs.model import DspProfile, Project, ordered_speakers

sys.path.insert(0, str(common.ROOT / "tests"))
import synth
from room_model import ModalRoom, POSITIONS


@dataclass
class Case:
    name: str
    project: Project
    irs: dict
    settings: cal.CalibrationSettings

    def engine(self):
        """Exactly the construction entry point used by calibration.compute."""
        return cal.make_engine(self.project, self.irs, self.settings)[0]


def _project(preset, subs):
    p = Project.from_preset(preset)
    p.dsp = DspProfile.presets()['camilla']
    p.config.set_sub_count(subs)
    mains = [s for s in p.config.speakers if not s.role.startswith('SW')]
    sub = [s for s in p.config.speakers if s.role.startswith('SW')]
    at = next((i for i, s in enumerate(mains) if s.role in ('Ls', 'Lb', 'Tfl')), len(mains))
    p.config.speakers = mains[:at] + sub + mains[at:]
    p.config.renumber()
    p.config.preset = preset
    p.extra['array'] = 1 if preset == '7.1.4' else 4
    p.curtain = (20.0, 20000.0)
    p.bc['phase'] = 'mixed'
    return p


def _dolby():
    p = _project('7.1.4', 3)
    with np.load(common.SRC / 'secs/rewapi/sample_dolby_714.npz') as data:
        meta = json.loads(str(data['meta_json']))
        table = {r: (np.asarray(data[f'ir_{r}'], dtype=np.float32), meta['sample_rate'], float(t))
                 for r, t in zip(meta['roles'], data['t0'])}
    # step5_measure.plan explicitly moves subs to the end of measurement order.
    return p, {1: {s.role: table[s.role] for s in ordered_speakers(p.config.speakers, subs_last=True)}}


def _room():
    # One second at 48 kHz, with actual position-dependent modal responses.
    room = ModalRoom(sample_rate=48000, n_fft=48000)
    sources = {'L': (1.6, 3.9, 1.0), 'R': (4.6, 3.9, 1.0), 'C': (3.1, 3.9, 1.0),
               'SW1': (1.2, 0.5, 0.4), 'SW2': (5.0, 0.5, 0.4),
               'SW3': (1.2, 3.9, 0.4), 'SW4': (5.0, 3.9, 0.4)}
    irs = {}
    for pos, receiver in enumerate(POSITIONS[:4], 1):
        irs[pos] = {}
        for i, (role, source) in enumerate(sources.items()):
            # The upstream synth gives deterministic broadband excitation and
            # sub roll-off; the modal room gives distinct spatial responses.
            excitation = (synth.main_ir(seed=synth.SEED + i) if i < 3
                          else synth.sub_ir(seed=synth.SEED + i))
            response = np.fft.irfft(np.fft.rfft(excitation) * room.transfer(source, receiver), 48000)
            irs[pos][role] = (response.astype(np.float32), 48000, (pos - 1) * 0.00013 + i * 0.00107 - 0.0002)
    return irs, float(np.prod(room.dims))


@lru_cache(maxsize=1)
def _cases():
    p, dolby = _dolby()
    out = [Case('dolby', p, dolby, cal.CalibrationSettings())]
    for mode in ('zero', 'linear', 'ultra'):
        out.append(Case(f'dolby_{mode}', p, dolby, cal.CalibrationSettings(mode=mode)))
    infra = cal.CalibrationSettings()
    for role in ('SW1', 'SW2'):
        infra.set_sub(role, hp_hz=25.0, hp_type='LR24')
    infra.set_sub('SW3', lp_hz=25.0, lp_type='LR24', hp_hz=0.0)
    out.append(Case('dolby_infra', p, dolby, infra))
    room, volume = _room()

    def add(name, nsubs):
        project = _project('5.1', nsubs)
        project.config.speakers = [s for s in project.config.speakers if s.role not in ('Ls', 'Rs')]
        project.config.renumber()
        project.config.preset = 'custom'
        roles = {s.role for s in project.config.speakers}
        irs = {pos: {r: v for r, v in rs.items() if r in roles} for pos, rs in room.items()}
        settings = cal.CalibrationSettings(room_volume_m3=volume)
        out.append(Case(name, project, irs, settings))
        return settings

    add('room', 2)
    for kind in ('dba', 'multi'):
        s = add(f'room_{kind}', 4)
        for i in range(1, 5):
            s.set_sub(f'SW{i}', mode='bundle', bundle='A', dba_row='front' if i <= 2 else 'rear')
        s.set_layout('A', kind, rear_trim_ms=1.25)
    s = add('room_manual', 3)
    s.set_sub('SW1', mode='fixed', gain_db=-2.5, delay_ms=6.0, polarity=-1,
              allpass=[[65.0, 0.8]])
    s.set_sub('SW2', mode='bundle', bundle='A', gain_db=-1.5, delay_ms=2.0)
    s.set_sub('SW3', mode='bundle', bundle='A', gain_db=1.0, delay_ms=4.5, polarity=-1)
    s.set_layout('A', 'values')
    s.sub_main_delay_ms = 8.0
    s = add('room_filters', 2)
    s.set_sub('SW1', hp_hz=18.0, hp_type='BW12', lp_hz=160.0, lp_type='LR48',
              peq=[[43.0, 2.0, -4.0], [85.0, 1.1, 2.5]])
    s.set_sub('SW2', hp_hz=22.0, hp_type='LR12', lp_hz=180.0, lp_type='BW24',
              peq=[[67.0, 3.0, -3.0]])
    s.xo_peq_lo, s.xo_peq_hi = 35.0, 270.0
    s.boost_max_db, s.infra_boost_max_db = 4.0, 8.0
    return out


def big_case() -> Case:
    """P18 only: six seats, the upstream 7.1.4 roles and two subwoofers.

    Kept outside cases(): existing golden exporters must not gain a case.
    The sixth seat extends the upstream five-seat cross towards the rear left.
    """
    project = _project('7.1.4', 2)
    project.extra['array'] = 6
    room = ModalRoom(sample_rate=48000, n_fft=48000)
    sources = {
        'L': (1.6, 3.9, 1.0), 'R': (4.6, 3.9, 1.0), 'C': (3.1, 3.9, 1.0),
        'Ls': (0.4, 2.2, 1.0), 'Rs': (5.8, 2.2, 1.0),
        'Lb': (1.6, 0.5, 1.0), 'Rb': (4.6, 0.5, 1.0),
        'Tfl': (1.6, 3.4, 2.5), 'Tfr': (4.6, 3.4, 2.5),
        'Tbl': (1.6, 0.9, 2.5), 'Tbr': (4.6, 0.9, 2.5),
        'SW1': (1.2, 0.5, 0.4), 'SW2': (5.0, 0.5, 0.4),
    }
    roles = [s.role for s in ordered_speakers(project.config.speakers, subs_last=True)]
    assert set(roles) == set(sources)
    receivers = (*POSITIONS, (2.6, 2.4, 1.15))
    irs = {}
    for pos, receiver in enumerate(receivers, 1):
        irs[pos] = {}
        for i, role in enumerate(roles):
            excitation = (synth.sub_ir(seed=synth.SEED + i) if role.startswith('SW')
                          else synth.main_ir(seed=synth.SEED + i))
            response = np.fft.irfft(np.fft.rfft(excitation)
                                    * room.transfer(sources[role], receiver), 48000)
            irs[pos][role] = (response.astype(np.float32), 48000,
                             (pos - 1) * 0.00013 + i * 0.00107 - 0.0002)
    return Case('big', project, irs,
                cal.CalibrationSettings(room_volume_m3=float(np.prod(room.dims)), mimo_on=True))


def cases() -> list[Case]:
    # Exporters/optimisers may mutate settings and engines, never the shared set.
    return copy.deepcopy(_cases())
