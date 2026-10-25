use crate::{C, Design, ExportView, test_support};
use secs_types::chain::{MimoFilter, MimoPath, MimoPaths};

#[test]
fn design_replay_cached_mimo_matches_fresh_service_bits() {
    for name in ["room_dba", "room_filters"] {
        let case = test_support::replay_case(name);
        let design = &case.design;
        let prepared = crate::measurement::prepare(
            &case.measurements,
            &case.config,
            &crate::setup::Setup::new(&case.config),
        )
        .unwrap();
        let view = crate::export_view::export_view_prepared(
            design,
            &case.measurements,
            &prepared,
            &|| false,
        )
        .unwrap();
        for filter in [
            MimoFilter::Iir {
                gain_db: -20.0,
                delay_ms: 3.0,
                polarity: -1,
                peq: vec![(70.0, 2.0, -3.0)],
                allpass: vec![(90.0, 0.7)],
            },
            MimoFilter::Fir {
                taps_48k: vec![0.1, -0.05, 0.02],
            },
            MimoFilter::FirLowRate {
                taps_low: vec![0.1, -0.05, 0.02],
                fs_low: 600.0,
            },
        ] {
            let paths = MimoPaths {
                fsiso: 150.0,
                paths: case
                    .config
                    .subs
                    .iter()
                    .map(|support| MimoPath {
                        input: case.config.mains[0].clone(),
                        support: support.clone(),
                        filter: filter.clone(),
                        low_hz: 50.0,
                        delay_ms: 7.0 + design.chain.main_extra_ms,
                    })
                    .collect(),
            };
            let expected = service_before(design, &view, &paths);
            let actual = design.replay.mimo_responses(design, &paths).unwrap();
            assert_eq!(actual.len(), expected.len());
            for ((role, actual), (other, expected)) in actual.iter().zip(&expected) {
                assert_eq!(role, other);
                assert_eq!(actual.len(), expected.len());
                for (a, b) in actual.iter().zip(expected) {
                    assert_eq!(
                        (a.re.to_bits(), a.im.to_bits()),
                        (b.re.to_bits(), b.im.to_bits())
                    );
                }
                let (ir, t0) =
                    design
                        .replay
                        .export_ir(design, actual, crate::ExportAxis::Corrected);
                let expected_ir = secs_dsp::fft::irfft(expected, crate::grid::N_FFT);
                assert_eq!(
                    ir.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    expected_ir
                        .iter()
                        .map(|v| f64::from(*v as f32).to_bits())
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    t0.to_bits(),
                    (prepared.base_s - design.chain.latency_ms / 1000.0).to_bits()
                );
            }
        }
    }
}

// The service arithmetic before P93, evaluated on a freshly prepared export view.
fn service_before(design: &Design, view: &ExportView, paths: &MimoPaths) -> Vec<(String, Vec<C>)> {
    let frequencies = crate::grid::design_freqs();
    let mut applied = Vec::new();
    for main in &design.chain.mains {
        let base = crate::stage_responses(
            design,
            &main.role,
            &view.spectra,
            design.spectra.main_position,
        )
        .unwrap()
        .output;
        let mut response = base;
        for path in paths.paths.iter().filter(|path| path.input == main.role) {
            let support = view
                .spectra
                .get(design.spectra.main_position, &path.support)
                .unwrap();
            let delay_ms = path.delay_ms - design.chain.main_extra_ms;
            let delay = delay_response(delay_ms, frequencies);
            let high_pass = secs_dsp::response::lr4(frequencies, path.low_hz, "HP");
            let electrical = cached_mimo_filter_response(&path.filter, frequencies, paths.fsiso);
            for ((((output, acoustic), delay), high_pass), electrical) in response
                .iter_mut()
                .zip(support.iter())
                .zip(delay)
                .zip(high_pass)
                .zip(electrical)
            {
                *output += acoustic * delay * high_pass * electrical;
            }
        }
        applied.push((main.role.clone(), response));
    }
    applied
}

fn delay_response(ms: f64, frequencies: &[f64]) -> Vec<crate::C> {
    frequencies
        .iter()
        .map(|&frequency| {
            crate::C::from_polar(1.0, (-2.0 * std::f64::consts::PI * frequency) * ms * 0.001)
        })
        .collect()
}

fn cached_mimo_filter_response(
    filter: &secs_types::chain::MimoFilter,
    frequencies: &[f64],
    fsiso: f64,
) -> Vec<crate::C> {
    use secs_types::chain::MimoFilter;

    match filter {
        MimoFilter::Iir {
            gain_db,
            delay_ms,
            polarity,
            peq,
            allpass,
        } => {
            let gain = f64::from(*polarity) * 10_f64.powf(gain_db / 20.0);
            let mut response: Vec<_> = delay_response(*delay_ms, frequencies)
                .into_iter()
                .map(|value| value * gain)
                .collect();
            for &(frequency, q, gain_db) in peq {
                for (value, filter) in
                    response
                        .iter_mut()
                        .zip(secs_dsp::response::peaking_response(
                            frequencies,
                            frequency,
                            q,
                            gain_db,
                            crate::grid::SR_F,
                        ))
                {
                    *value *= filter;
                }
            }
            for &(frequency, q) in allpass {
                for (value, filter) in
                    response
                        .iter_mut()
                        .zip(secs_dsp::response::allpass2(frequencies, frequency, q))
                {
                    *value *= filter;
                }
            }
            for (value, filter) in
                response
                    .iter_mut()
                    .zip(secs_dsp::response::lr4(frequencies, fsiso, "LP"))
            {
                *value *= filter;
            }
            response
        }
        MimoFilter::Fir { taps_48k } => {
            let n = 2 * frequencies.len().saturating_sub(1);
            let mut padded = vec![0.0; n];
            let count = n.min(taps_48k.len());
            padded[..count].copy_from_slice(&taps_48k[..count]);
            secs_dsp::fft::rfft(&padded)
        }
        MimoFilter::FirLowRate { taps_low, fs_low } => {
            secs_dsp::fir::lowrate_response(taps_low, *fs_low, frequencies)
        }
    }
}
