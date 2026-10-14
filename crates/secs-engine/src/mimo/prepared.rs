//! Request-local long spectra and alignment, shared by design, fitting and output.

use crate::{
    Design, EngineConfig, EngineError, MeasurementSet, Spectra, measurement, setup::Setup,
};

pub(crate) struct Prepared {
    pub spectra: Spectra,
    pub align_delay_ms: Vec<(String, f64)>,
}

impl Prepared {
    pub(crate) fn new(
        design: &Design,
        measurements: &MeasurementSet,
        config: &EngineConfig,
    ) -> Result<Self, EngineError> {
        let prepared = measurement::prepare(measurements, config, &Setup::new(config))?;
        let spectra =
            crate::export_view::export_view_prepared(design, measurements, &prepared)?.spectra;
        Ok(Self {
            spectra,
            align_delay_ms: prepared.align_delay_ms,
        })
    }

    #[cfg(test)]
    pub(crate) fn before(
        design: &Design,
        measurements: &MeasurementSet,
        config: &EngineConfig,
    ) -> Result<Self, EngineError> {
        let spectra = crate::export_view(design, measurements, config)?.spectra;
        let prepared = measurement::prepare(measurements, config, &Setup::new(config))?;
        Ok(Self {
            spectra,
            align_delay_ms: prepared.align_delay_ms,
        })
    }
}
