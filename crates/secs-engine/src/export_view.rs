//! Narrow derived state used by response exports.

use crate::{
    EngineConfig, EngineError, MeasurementSet, Spectra, measurement,
    setup::{Setup, SubFilter},
};

/// Long-window spectra and timing on the engine's internal axis.
#[derive(Debug)]
pub struct ExportView {
    pub spectra: Spectra,
    pub base_s: f64,
    pub has_sub_filters: bool,
}

/// Rebuilds the 2 s spectra used by SECS 2's REW exports.
pub fn export_view(
    design: &crate::Design,
    measurements: &MeasurementSet,
    config: &EngineConfig,
) -> Result<ExportView, EngineError> {
    let setup = Setup::new(config);
    let prepared = measurement::prepare(measurements, config, &setup)?;
    let sub_filters: Vec<_> = design
        .chain
        .subs
        .iter()
        .filter(|sub| sub.hp_hz != 0.0 || sub.lp_hz != 0.0 || !sub.peq.is_empty())
        .map(|sub| {
            (
                sub.role.clone(),
                SubFilter {
                    hp_hz: sub.hp_hz,
                    hp_type: sub.hp_type,
                    lp_hz: sub.lp_hz,
                    lp_type: sub.lp_type,
                    peq: sub.peq.clone(),
                },
            )
        })
        .collect();
    let spectra = measurement::long_spectra(measurements, &prepared, &sub_filters, 2000.0)?;
    Ok(ExportView {
        spectra,
        base_s: prepared.base_s,
        has_sub_filters: !sub_filters.is_empty(),
    })
}
