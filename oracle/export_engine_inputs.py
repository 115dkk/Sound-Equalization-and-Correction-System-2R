"""Freeze production make_engine arguments and float32 IRs exactly once."""
import dataclasses
import hashlib
import inspect
from unittest.mock import patch

import common
import numpy as np
from engine_cases import cases
from secs.correction_engine import CorrectionEngine


def main():
    fam = common.Family('engine_inputs')
    blobs = {}
    original = CorrectionEngine.__init__
    signature = inspect.signature(original)
    for case in cases():
        captured = {}

        def record(*args, **kwargs):
            bound = signature.bind(*args, **kwargs)
            bound.apply_defaults()
            captured.update(bound.arguments)
            original(*args, **kwargs)

        with patch.object(CorrectionEngine, '__init__', record):
            engine = case.engine()
        assert {k: captured[k] for k in ('xo_stage', 'sub_allpass', 'gd_cuts', 'bundle_gain')} == {
            'xo_stage': True, 'sub_allpass': 'none', 'gd_cuts': False, 'bundle_gain': 'auto'}
        config = {k: v for k, v in captured.items() if k not in
                  ('self', 'irs', 'target_db_fn', 'xo_stage', 'sub_allpass', 'gd_cuts', 'bundle_gain')}
        config['target'] = captured['target_db_fn'].target.points
        for name in ('max_boost_db', 'infra_boost_max_db', 'xo_peq_on', 'xo_peq_lo', 'xo_peq_hi'):
            config[name] = getattr(engine, name)
        config['xo_peq_lo_hz'] = config.pop('xo_peq_lo')
        config['xo_peq_hi_hz'] = config.pop('xo_peq_hi')
        # Explicit ordered pairs survive readers whose JSON objects sort keys.
        for name in ('groups', 'xo', 'sub_manual', 'bundle_layouts'):
            config[name] = list((config[name] or {}).items())
        measurements = []
        for pos in sorted(captured['irs']):
            roles = []
            for role, (ir, sr, t0) in captured['irs'][pos].items():
                data = np.asarray(ir, dtype='<f4').tobytes()
                digest = hashlib.sha256(data).hexdigest()
                if digest not in blobs:
                    blobs[digest] = fam.blob(f'{case.name}_{pos}_{role}', data, 'f32')
                roles.append([role, {'sample_rate': sr, 't0': t0, 'samples': blobs[digest]}])
            measurements.append([pos, roles])
        fam.save(case.name, inputs={'measurements': measurements, 'config': config,
                 'settings': dataclasses.asdict(case.settings),
                 'project': {'groups': [(g, [s.role for s in rs]) for g, rs in case.project.config.groups().items()],
                             'roles': [s.role for s in case.project.config.speakers],
                             'xo': list(case.project.xo.items()), 'target': case.project.target.points}}, outputs={})
    fam.finish()
    assert sum(fam.written.values()) <= 9_000_000


if __name__ == '__main__':
    main()
