"""Freeze staged and before/after REW export items for four shared engine cases."""
import hashlib

import common
import numpy as np
from engine_cases import cases
from secs import calibration
from secs.rew_mdat.builder import build_mdat_bytes

NAMES = {'dolby', 'room_dba', 'room_manual', 'room_filters'}
NOW_MS = 1792195538802


def item_value(item):
    samples = np.ascontiguousarray(item.samples, dtype='<f4')
    indexes = sorted(set([0, 1, 2, len(samples) // 2, len(samples) - 3,
                          len(samples) - 2, len(samples) - 1] +
                         list(range(0, len(samples), 4096))))
    return {
        'title': item.title,
        'notes': item.notes,
        'sample_rate_hz': item.sample_rate_hz,
        'start_time_s': item.start_time_s,
        'length': len(samples),
        'samples_sha256': hashlib.sha256(samples.tobytes()).hexdigest(),
        'samples': [[i, float(samples[i])] for i in indexes],
    }


def main():
    family = common.Family('export_mdat')
    for case in cases():
        if case.name not in NAMES:
            continue
        run = calibration.compute(case.project, case.irs, case.settings)
        staged = calibration.stage_items(run)
        sources = None
        if case.name == 'room_filters':
            pos = run.engine.main_position
            first_roles = list(run.engine.chain)[:2]
            sources = {pos: {first_roles[0]: {
                'strings': {'outputName': 'Output A', 'sourceFileName': 'Input A'},
                'boxed': {}, 'cal': {}, 'date': NOW_MS - 1000, 'format': 'Original A', 'notes': 'Source A',
            }, first_roles[1]: {
                'strings': {'outputName': 'Output B'}, 'boxed': {}, 'cal': {}, 'date': 0, 'notes': '',
            }}}
        before_after = calibration.before_after_items(run, sources=sources)
        uuids = iter(range(1, len(staged) + len(before_after) + 1))
        import time
        import uuid
        original = uuid.uuid4
        original_time = time.time
        uuid.uuid4 = lambda: uuid.UUID(int=next(uuids))
        time.time = lambda: NOW_MS / 1000.0
        try:
            staged_bytes = build_mdat_bytes(staged)
            before_after_bytes = build_mdat_bytes(before_after)
        finally:
            uuid.uuid4 = original
            time.time = original_time
        outputs = {
            'label': case.settings.label(),
            'rc_pieces': calibration.rc_pieces(run),
            'layout_lines': calibration.layout_lines(run),
            'sub_values_text': calibration.sub_values_text(run),
            'staged_filename': f'SECS 보정 설계 {case.settings.label()}.mdat',
            'before_after_filename': f'SECS 보정 전후 {case.settings.label()}.mdat',
            'staged': [item_value(item) for item in staged],
            'before_after': [item_value(item) for item in before_after],
            'staged_mdat_sha256': hashlib.sha256(staged_bytes).hexdigest(),
            'staged_mdat_length': len(staged_bytes),
            'before_after_mdat_sha256': hashlib.sha256(before_after_bytes).hexdigest(),
            'before_after_mdat_length': len(before_after_bytes),
        }
        family.save(case.name, inputs={'now_ms': NOW_MS, 'sources': sources}, outputs=outputs)
    family.finish()
    assert sum(family.written.values()) <= 4_000_000


if __name__ == '__main__':
    main()
