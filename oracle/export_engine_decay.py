"""P08: observe decay calls during real calibration, plus scorecard recipes."""
from __future__ import annotations

import functools

import common
import numpy as np
from engine_cases import cases
from secs import calibration as cal, correction_engine as ce, mimo, scorecard


RECIPES = [
    (0.0, []),
    (7.125, [[42.0, 3.2, 8.0], [130.0, 1.1, -5.0]]),
    (38.75, [[27.0, 12.0, 15.0], [73.0, 7.0, -12.0], [2300.0, 2.0, 4.0]]),
    (-12.5, [[61.0, 18.0, 10.0], [440.0, 4.0, -9.0]]),
]


def main():
    fam = common.Family('engine_decay')
    counters = {}
    current = ''

    bundles = {}

    def save(kind, inputs, outputs):
        # One fixture per kind and case, holding every call (Fixture::calls).
        counters[kind, current] = counters.get((kind, current), 0) + 1
        calls = bundles.setdefault(f'{kind}_{current}', ([], []))
        calls[0].append(inputs)
        calls[1].append(outputs)

    # _long_mode_mask imports these names inside each call, so patch the module.
    original_mode, original_peaks = mimo.mode_t60, mimo._late_peaks

    def mode(ir, rate, center_hz, half_band=3.0):
        result = original_mode(ir, rate, center_hz, half_band)
        key = f'ir_{current}_{counters.get(("mode", current), 0):03}'
        save('mode', dict(ir=fam.f64(key, ir), rate=rate, center_hz=center_hz,
                          half_band=half_band), result)
        return result

    def peaks(xs, count):
        result = original_peaks(xs, count)
        key = f'xs_{current}_{counters.get(("peaks", current), 0):03}'
        save('peaks', dict(xs=[fam.f64(f'{key}_{i}', x) for i, x in enumerate(xs)],
                           count=count), result)
        return result

    original_ba = ce.CorrectionEngine._ba_setup
    rng = np.random.RandomState(20261013)

    @functools.wraps(original_ba)
    def ba(engine, hi_hz):
        setup = original_ba(engine, hi_hz)
        if current not in ('room', 'room_manual', 'dolby', 'dolby_infra'):
            return setup
        closure = dict(zip(setup['late_db'].__code__.co_freevars,
                           (c.cell_contents for c in setup['late_db'].__closure__)))
        key = f'ba_{current}_{counters.get(("ba", current), 0):03}'
        raw = [(0.0, setup['base'][r], 1, []) for r in engine.subs]
        rows = [raw]
        for _ in range(3):
            rows.append([(float(rng.uniform(-8, 0)), setup['base'][r] + float(rng.uniform(-3, 3)),
                          int(rng.choice([-1, 1])), [(float(rng.uniform(20, 150)), float(rng.uniform(.3, 3)))])
                         for r in engine.subs])
        samples = []
        for row_idx, row in enumerate(rows):
            for pos, spec in engine._sub_sum(row, setup['lin'], setup['positions']).items():
                samples.append(dict(row=row_idx, position=pos,
                                    spec=fam.c128(f'{key}_{row_idx}_{pos}', spec),
                                    late_db=setup['late_db'](spec)))
        save('ba', dict(case=current, hi_hz=hi_hz, rows=rows),
             dict(lin=len(setup['lin']), band=fam.f64(f'{key}_band', closure['band']),
                  fs_eff=closure['fs_eff'], samples=samples, raw_late=setup['raw_late']))
        return setup

    mimo.mode_t60, mimo._late_peaks = mode, peaks
    ce.CorrectionEngine._ba_setup = ba
    try:
        for case in cases():
            current = case.name
            print(f'calibration.compute: {current}', flush=True)
            run = cal.compute(case.project, case.irs, case.settings)
            engine = run.engine
            if current in ('dolby', 'room'):
                for pos, roles in engine.long_spectra(2000).items():
                    for role, h in roles.items():
                        save('late', dict(case=current, position=pos, role=role), cal._late_db(h, engine.f))
            if current in ('dolby', 'room', 'room_filters'):
                # These are precisely the target and H0 passed by compute to score.
                for role, row in run.metrics.items():
                    save('metrics', dict(case=current, role=role, position=engine.main_position,
                                         norm=[100.0, 400.0]), row['before'])
    finally:
        mimo.mode_t60, mimo._late_peaks = original_mode, original_peaks
        ce.CorrectionEngine._ba_setup = original_ba

    current = 'synthetic'
    f = ce.freqs()
    for delay_ms, bands in RECIPES:
        h = np.exp(-2j * np.pi * f * delay_ms / 1000.0) * ce.peq_response(f, bands)
        recipe = dict(delay_ms=delay_ms, bands=bands)
        save('late', dict(recipe=recipe), cal._late_db(h, f))
        for norm in ([100.0, 400.0], [35.0, 250.0]):
            target = np.interp(np.maximum(f, 1.0), [20.0, 100.0, 1000.0, 20000.0], [4.0, 2.0, 0.0, -3.0])
            save('metrics', dict(recipe=recipe, norm=norm,
                                 target=[[20., 4.], [100., 2.], [1000., 0.], [20000., -3.]]),
                 scorecard.metrics(h, f, target, norm))

    # No energy, too little tail to reach thresholds, spike, and ringing modes.
    t = np.arange(720) / 600.0
    edge_irs = [np.zeros(300), np.ones(3), np.array([1.0]),
                np.r_[np.zeros(40), 1.0, np.zeros(259)],
                np.sin(2*np.pi*63*t),
                np.sin(2*np.pi*63*t) * np.exp(-t*7),
                np.sin(2*np.pi*28*t) * np.exp(-t*20)]
    current = 'edge'
    for ir in edge_irs:
        for half_band in (1.5, 3.0):
            mode(ir, 600.0, 63.0, half_band)
        peaks([ir], 8)
    peaks(edge_irs[-3:], 3)
    peaks(edge_irs[-3:], 0)
    # The truncated reverse-filter tail ends at −9.27 dB: −25 is never reached.
    mode(np.ones(1), 10.0, 2.0, 1.0)
    for key, (inputs, outputs) in bundles.items():
        # Each JSON stays under the 200 kB fixture limit.
        for part, start in enumerate(range(0, len(inputs), 60)):
            fam.save(f'{key}_{part}', {'calls': inputs[start:start + 60]},
                     {'calls': outputs[start:start + 60]})
    fam.finish()
    assert sum(fam.written.values()) <= 3_000_000


if __name__ == '__main__':
    main()
