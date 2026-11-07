//! Narrow derived state used by response exports.

#[cfg(any(test, feature = "test-support"))]
use crate::EngineConfig;
use crate::{EngineError, MeasurementSet, Spectra, measurement, setup::SubFilter};

/// Long-window spectra and timing on the engine's internal axis.
#[derive(Debug)]
pub struct ExportView {
    pub spectra: Spectra,
    pub base_s: f64,
    pub has_sub_filters: bool,
}

/// Rebuilds the 2 s spectra, reusing the design's preparation for matching inputs.
/// New same-input callers use `design.replay.export_view(design)` directly.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(not(test), allow(dead_code))]
pub fn export_view(
    design: &crate::Design,
    measurements: &MeasurementSet,
    config: &EngineConfig,
) -> Result<ExportView, EngineError> {
    if design.replay.matches(measurements, config) {
        design.replay.export_view(design)
    } else {
        let prepared =
            measurement::prepare(measurements, config, &crate::setup::Setup::new(config))?;
        export_view_prepared(
            design,
            measurements,
            &prepared,
            &secs_compute::Context::serial(),
        )
    }
}

/// [`export_view`] on measurements already prepared with the same config;
/// Returns `Err(Cancelled)` when the context token is cancelled during the build.
pub(crate) fn export_view_prepared(
    design: &crate::Design,
    measurements: &MeasurementSet,
    prepared: &measurement::Prepared,
    ctx: &secs_compute::Context,
) -> Result<ExportView, EngineError> {
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
    let mut spectra =
        measurement::long_spectra_until(measurements, prepared, &sub_filters, 2000.0, ctx)?;
    if let Some(hz) = design.chain.input_hp_hz {
        let highpass =
            secs_dsp::response::band_filter(crate::grid::design_freqs(), hz, "HP", "BW48")?;
        for position in spectra.positions().to_vec() {
            let roles: Vec<_> = spectra.roles_at(position).map(str::to_owned).collect();
            for role in roles {
                ctx.cancel().check()?;
                let response = spectra.get(position, &role).expect("spectrum role");
                let filtered = response
                    .iter()
                    .zip(&highpass)
                    .map(|(h, hp)| h * hp)
                    .collect();
                spectra.set(position, &role, filtered);
            }
        }
    }
    Ok(ExportView {
        spectra,
        base_s: prepared.base_s,
        // The export tail must not reuse unfiltered design spectra (ADR 0031).
        has_sub_filters: !sub_filters.is_empty() || design.chain.input_hp_hz.is_some(),
    })
}
