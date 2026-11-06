//! FIR materialization and WAV emission shared by export writers.

use crate::files::wav_bytes;
use crate::{FilterPlan, FirView, GraphError};

/// Materialize every FIR in plan order and return its prefixed WAV filename
/// with complete IEEE-float WAV bytes at `rate`.
pub fn fir_files(
    plan: &FilterPlan,
    prefix: &str,
    rate: u32,
) -> Result<Vec<(String, Vec<u8>)>, GraphError> {
    plan.firs()
        .into_iter()
        .map(|(stem, taps)| {
            taps.validate().map_err(|error| with_stem(stem, error))?;
            let materialized = match taps {
                FirView::Full(taps) => secs_dsp::rate::fir_to_rate(taps, rate),
                FirView::LowRate(taps, fs_low) => {
                    secs_dsp::rate::lowrate_to_rate(taps, fs_low, rate)
                }
            }
            .map_err(|error| GraphError::Invalid(format!("{stem}: {error}")))?;
            let bytes = wav_bytes(&materialized, rate).map_err(|error| with_stem(stem, error))?;
            Ok((format!("{prefix}{stem}.wav"), bytes))
        })
        .collect()
}

fn with_stem(stem: &str, error: GraphError) -> GraphError {
    match error {
        GraphError::NonAscii(detail) => GraphError::NonAscii(format!("{stem}: {detail}")),
        GraphError::NonFinite(detail) => GraphError::NonFinite(format!("{stem}: {detail}")),
        GraphError::Invalid(detail) => GraphError::Invalid(format!("{stem}: {detail}")),
    }
}
