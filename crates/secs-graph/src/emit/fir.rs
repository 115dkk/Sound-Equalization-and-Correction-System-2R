//! FIR materialization and WAV emission shared by export writers.

use crate::files::wav_bytes;
use crate::{FilterPlan, GraphError};

/// Materialize every FIR in plan order and return its prefixed WAV filename
/// with complete IEEE-float WAV bytes at `rate`.
pub fn fir_files(
    plan: &FilterPlan,
    prefix: &str,
    rate: u32,
) -> Result<Vec<(String, Vec<u8>)>, GraphError> {
    fir_files_reported(plan, prefix, rate, &|_, _| {})
}

/// [`fir_files`], calling `made(done, total)` in files: `(0, total)` before
/// the first FIR and after each one is materialized and encoded.
pub fn fir_files_reported(
    plan: &FilterPlan,
    prefix: &str,
    rate: u32,
    made: &dyn Fn(u64, u64),
) -> Result<Vec<(String, Vec<u8>)>, GraphError> {
    let firs = plan.firs();
    let total = u64::try_from(firs.len()).expect("the resident FIR list length fits in u64");
    if total != 0 {
        made(0, total);
    }
    firs.into_iter()
        .enumerate()
        .map(|(index, (stem, taps))| {
            let materialized = taps
                .materialize_at(rate)
                .map_err(|error| with_stem(stem, error))?;
            let bytes = wav_bytes(&materialized, rate).map_err(|error| with_stem(stem, error))?;
            if total != 0 {
                made(
                    u64::try_from(index + 1).expect("the FIR index fits in u64"),
                    total,
                );
            }
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
