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
    /// `(peak, held)` bytes of the request-local spectra (TD-24). The design
    /// owns its 500 ms preparation; this request allocates only the 2 s set
    /// and build scratch, then holds the 2 s set until it is dropped. Keep
    /// the existing conservative peak (both windows) and admission threshold.
    pub(crate) fn bytes(measurements: &MeasurementSet, config: &EngineConfig) -> (u64, u64) {
        Self::bytes_with_filters(
            measurements,
            Setup::new(config).sub_filters.len().max(config.subs.len()),
        )
    }

    fn bytes_with_filters(measurements: &MeasurementSet, filters: usize) -> (u64, u64) {
        let set = measurement::spectra_bytes(measurements);
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
        let replay = &design.replay;
        let same = replay.matches(measurements, config);
        let (peak, held) = if same {
            Self::bytes_with_filters(
                replay.measurements(),
                replay
                    .setup()
                    .sub_filters
                    .len()
                    .max(replay.config().subs.len()),
            )
        } else {
            Self::bytes(measurements, config)
        };
        let mut permit = crate::memory::reserve(ctx, peak, 0)?;
        let cancelled = || ctx.cancel().is_cancelled();
        let (spectra, align_delay_ms) = if same {
            (
                replay.export_view_until(design, &cancelled)?.spectra,
                replay.prepared().align_delay_ms.clone(),
            )
        } else {
            let prepared =
                measurement::prepare_until(measurements, config, &Setup::new(config), &cancelled)?;
            let spectra = crate::export_view::export_view_prepared(
                design,
                measurements,
                &prepared,
                &cancelled,
            )?
            .spectra;
            (spectra, prepared.align_delay_ms)
        };
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
        let prepared = measurement::prepare(measurements, config, &Setup::new(config))?;
        let spectra =
            crate::export_view::export_view_prepared(design, measurements, &prepared, &|| false)?
                .spectra;
        Ok(Self {
            spectra,
            align_delay_ms: prepared.align_delay_ms,
            reserved: 0,
            _permit: None,
        })
    }
}
