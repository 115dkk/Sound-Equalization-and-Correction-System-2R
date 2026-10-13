"""Observe the real engine's initialisation, preparation and level stages."""
import hashlib

import common
import numpy as np
from engine_cases import cases
from secs import calibration as cal
from secs import correction_engine as ce


def main():
    fam = common.Family('engine_prep')
    f = ce.freqs()
    bins = np.union1d(np.flatnonzero((f >= 15) & (f <= 300)), np.arange(0, len(f), 257))
    bank, by_digest = [], {}
    for case in cases():
        e = case.engine()
        setup = {k: getattr(e, k) for k in ('sub_manual', 'sub_filters', 'layouts', 'lift_ms',
                 'max_latency_ms', 'rc_lo', 'rc_hi_manual', 'rc_hi', 'peq_bands')}
        for k in ('sub_manual', 'sub_filters', 'layouts'):
            setup[k] = list(setup[k].items())
        setup['lift_note'] = bool(e.lift_note)
        starts = [[p, [[r, ce.onset_index(np.asarray(ir, float)),
                        ce.windowed(ir, sr, ce.onset_index(np.asarray(ir, float)))[1]]
                       for r, (ir, sr, _) in rs.items()]] for p, rs in sorted(e.irs.items())]
        e._prepare()

        def spectra(label, table):
            cells = []
            for p, rs in table.items():
                for r, h in rs.items():
                    values = np.asarray(h[bins], dtype='<c16')
                    digest = hashlib.sha256(values.tobytes()).hexdigest()
                    if digest not in by_digest:
                        by_digest[digest] = len(bank)
                        bank.append(values)
                    cells.append([p, r, by_digest[digest]])
            return cells

        h0_diff = {p: {r: h for r, h in rs.items() if not np.array_equal(h, e.H[p][r])}
                   for p, rs in e.H0.items()}
        levels = [[r, e._band_level(r, 300.0, 3000.0)] for r in e.mains]
        outputs = {'setup': setup, 'starts': starts, 'arrival_s': list(e.arrival.items()),
                   'align_delay_ms': list(e.align_delay_ms.items()), 'base_s': min(e.arrival.values()),
                   'h': spectra('h', e.H), 'h0_diff': spectra('h0', h0_diff),
                   'long_2000': spectra('long2000', e.long_spectra(2000)),
                   'long_500': spectra('long500', e.long_spectra(500)),
                   'weights': list(e._weights().items()),
                   'freq_weights': [[p, fam.f64(f'{case.name}_fw_{p}', w[bins])] for p, w in e._freq_weights().items()],
                   'levels': levels, 'reference_db': min(v for _, v in levels),
                   'schroeder_hz': e.schroeder_hz(),
                   'target_design': fam.f64(f'{case.name}_target', e.target_db_fn(f)[bins]),
                   'target_log': fam.f64(f'{case.name}_target_log', e.target_db_fn(cal.SPL_GRID))}
        fam.save(case.name, inputs={'case': case.name, 'bins': fam.f64('bins', bins),
                 'log_grid': fam.f64('log_grid', cal.SPL_GRID)}, outputs=outputs)
    # `_delay` is complex scalar arithmetic NumPy evaluates in a fixed order
    # (the division by 1000 is a multiplication by 0.001): pin it bit for bit.
    delays = []
    for ms in (-12.0, -3.25, -0.0104, 0.0, 0.25, 1.5, 7.123456, 12.0, 199.99):
        d = e._delay(ms)
        delays.append({'ms': ms, 'digest': fam.f64_digest(d.view(np.float64)),
                       'sampled': fam.c128(f'delay_{len(delays)}', d[bins])})
    fam.save('delay', inputs={'bins': fam.f64('bins', bins)}, outputs={'delays': delays})
    fam.save('spectra', inputs={'bins_per_spectrum': len(bins), 'spectra': len(bank)},
             outputs={'values': fam.c128('spectra', np.concatenate(bank))})
    fam.finish()
    assert sum(fam.written.values()) <= 3_000_000


if __name__ == '__main__':
    main()
