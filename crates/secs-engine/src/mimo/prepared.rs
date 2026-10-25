//! Request-local long spectra and alignment, shared by design, fitting and output.

use secs_compute::{Context, MemoryPermit};

use crate::{
    Design, EngineConfig, EngineError, MeasurementSet, Spectra, measurement, setup::Setup,
};

pub(crate) struct Prepared<'ctx> {
    pub spectra: Spectra,
    pub align_delay_ms: Vec<(String, f64)>,
    /// Bytes of the job budget this preparation holds while it lives; later
    /// stages reserve out of what remains.
    pub reserved: u64,
    _permit: Option<MemoryPermit<'ctx>>,
}

impl<'ctx> Prepared<'ctx> {
    /// `(peak, held)` bytes of the request-local spectra (TD-24). While the
    /// preparation runs, `measurement::prepare`'s 500 ms `h` and the 2 s
    /// `long_spectra` are alive together (one spectrum per IR cell each, plus
    /// the build scratch with one response per sub filter); afterwards only
    /// the 2 s set is held, until the preparation is dropped.
    pub(crate) fn bytes(measurements: &MeasurementSet, config: &EngineConfig) -> (u64, u64) {
        let set = measurement::spectra_bytes(measurements);
        let filters = Setup::new(config).sub_filters.len().max(config.subs.len());
        (
            set.saturating_mul(2)
                .saturating_add(measurement::build_scratch_bytes(filters)),
            set,
        )
    }

    /// Reserves the peak on the calling thread before any spectrum is built,
    /// then keeps only the 2 s set's share until this preparation is dropped.
    pub(crate) fn new(
        design: &Design,
        measurements: &MeasurementSet,
        config: &EngineConfig,
        ctx: &'ctx Context,
    ) -> Result<Self, EngineError> {
        let (peak, held) = Self::bytes(measurements, config);
        let mut permit = crate::memory::reserve(ctx, peak, 0)?;
        let cancelled = || ctx.cancel().is_cancelled();
        let prepared =
            measurement::prepare_until(measurements, config, &Setup::new(config), &cancelled)?;
        let spectra =
            crate::export_view::export_view_prepared(design, measurements, &prepared, &cancelled)?
                .spectra;
        let align_delay_ms = prepared.align_delay_ms;
        drop(prepared.h);
        drop(prepared.h0);
        permit.shrink_to(held);
        Ok(Self {
            spectra,
            align_delay_ms,
            reserved: held,
            _permit: Some(permit),
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
            reserved: 0,
            _permit: None,
        })
    }
}
