//! Applied cached MIMO responses, preserving the service replay arithmetic.

use crate::{C, Design, EngineError, ExportView};
use secs_types::chain::MimoPaths;

pub(crate) fn responses(
    design: &Design,
    view: &ExportView,
    paths: &MimoPaths,
) -> Result<Vec<(String, Vec<C>)>, EngineError> {
    let frequencies = crate::grid::design_freqs();
    let mut applied = Vec::new();
    for main in &design.chain.mains {
        let base = crate::stage_responses(
            design,
            &main.role,
            &view.spectra,
            design.spectra.main_position,
        )
        .ok_or_else(|| EngineError::Input("missing MIMO final response".into()))?
        .output;
        let mut response = base;
        for path in paths.paths.iter().filter(|path| path.input == main.role) {
            let support = view
                .spectra
                .get(design.spectra.main_position, &path.support)
                .ok_or_else(|| {
                    EngineError::Input(format!("missing MIMO support response {}", path.support))
                })?;
            let delay_ms = path.delay_ms - design.chain.main_extra_ms;
            let delay = delay_response(delay_ms, frequencies);
            let high_pass = secs_dsp::response::lr4(frequencies, path.low_hz, "HP");
            let electrical = filter_response(&path.filter, frequencies, paths.fsiso)?;
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
    Ok(applied)
}

fn delay_response(ms: f64, frequencies: &[f64]) -> Vec<crate::C> {
    frequencies
        .iter()
        .map(|&frequency| {
            crate::C::from_polar(1.0, (-2.0 * std::f64::consts::PI * frequency) * ms * 0.001)
        })
        .collect()
}

fn filter_response(
    filter: &secs_types::chain::MimoFilter,
    frequencies: &[f64],
    fsiso: f64,
) -> Result<Vec<crate::C>, EngineError> {
    use secs_types::chain::MimoFilter;

    Ok(match filter {
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
            if !fs_low.is_finite() || *fs_low <= 0.0 || taps_low.iter().any(|tap| !tap.is_finite())
            {
                return Err(EngineError::Input("invalid cached MIMO FIR recipe".into()));
            }
            secs_dsp::fir::lowrate_response(taps_low, *fs_low, frequencies)
        }
    })
}
