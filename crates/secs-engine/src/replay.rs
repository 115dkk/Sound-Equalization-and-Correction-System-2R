//! The inputs and preparation retained by an immutable design.

use std::sync::Arc;

use crate::{
    Design, EngineConfig, EngineError, ExportView, MeasurementSet, measurement::Prepared,
    setup::Setup,
};

/// Time origin for a REW export item. Corrected responses retain SECS 2's
/// latency subtraction (D24 is deliberately not changed here).
#[derive(Debug, Clone, Copy)]
pub enum ExportAxis {
    Measurement,
    Corrected,
}

/// What a design was made from. Sample buffers and prepared spectra are shared,
/// never copied. Long-window spectra are computed on request, never cached.
#[derive(Debug)]
pub struct DesignReplay {
    measurements: Arc<MeasurementSet>,
    config: EngineConfig,
    setup: Setup,
    prepared: Prepared,
}

impl DesignReplay {
    pub(crate) fn new(
        measurements: &MeasurementSet,
        config: &EngineConfig,
        setup: Setup,
        prepared: &Prepared,
    ) -> Self {
        Self {
            measurements: Arc::new(measurements.clone()),
            config: config.clone(),
            setup,
            prepared: prepared.clone(),
        }
    }

    pub fn measurements(&self) -> &MeasurementSet {
        &self.measurements
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    pub fn base_s(&self) -> f64 {
        self.prepared.base_s
    }

    /// Rebuilds the 2 s spectra used by SECS 2's REW exports.
    /// `design` must be the design that owns this replay.
    pub fn export_view(&self, design: &Design) -> Result<ExportView, EngineError> {
        self.export_view_until(design, &|| false)
    }

    /// Converts a design-grid export response to REW samples and its time origin.
    /// The spectrum comes from this replay's export view, stage responses, or
    /// applied MIMO response. Keeping that view at the caller avoids rebuilding
    /// the long window for every role and stage. `design` must own this replay.
    pub fn export_ir(
        &self,
        design: &Design,
        spectrum: &[crate::C],
        axis: ExportAxis,
    ) -> (Vec<f64>, f64) {
        let samples = secs_dsp::fft::irfft(spectrum, crate::grid::N_FFT)
            .into_iter()
            .map(|sample| f64::from(sample as f32))
            .collect();
        let t0 = match axis {
            ExportAxis::Measurement => self.base_s(),
            ExportAxis::Corrected => self.base_s() - design.chain.latency_ms / 1000.0,
        };
        (samples, t0)
    }

    /// Applies cached IIR, full-rate FIR or low-rate FIR support paths to the
    /// long-window responses. `design` must be the design that owns this replay.
    pub fn mimo_responses(
        &self,
        design: &Design,
        paths: &secs_types::chain::MimoPaths,
    ) -> Result<Vec<(String, Vec<crate::C>)>, EngineError> {
        crate::replay_mimo::responses(design, &self.export_view(design)?, paths)
    }

    pub(crate) fn export_view_until(
        &self,
        design: &Design,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<ExportView, EngineError> {
        crate::export_view::export_view_prepared(
            design,
            self.measurements(),
            &self.prepared,
            cancelled,
        )
    }

    /// Legacy callers may supply a different input. Comparing shared storage
    /// avoids scanning samples; independently loaded equivalent inputs safely
    /// take the fresh-preparation path.
    pub fn matches(&self, set: &MeasurementSet, config: &EngineConfig) -> bool {
        self.config == *config
            && self.measurements.positions.len() == set.positions.len()
            && self
                .measurements
                .positions
                .iter()
                .zip(&set.positions)
                .all(|((p, a), (q, b))| {
                    p == q
                        && a.len() == b.len()
                        && a.iter().zip(b).all(|((r, a), (s, b))| {
                            r == s
                                && a.sample_rate == b.sample_rate
                                && a.t0.to_bits() == b.t0.to_bits()
                                && match (&a.samples, &b.samples) {
                                    (crate::Samples::F32(a), crate::Samples::F32(b)) => {
                                        Arc::ptr_eq(a, b)
                                    }
                                    (crate::Samples::F64(a), crate::Samples::F64(b)) => {
                                        Arc::ptr_eq(a, b)
                                    }
                                    _ => false,
                                }
                        })
                })
    }

    pub(crate) fn prepared(&self) -> &Prepared {
        &self.prepared
    }

    pub(crate) fn setup(&self) -> &Setup {
        &self.setup
    }

    /// Owned metadata added to the TD-24 reservation. Samples and spectrum
    /// buffers are Arc-shared with the input and the already-counted preparation.
    /// Count cloned containers and strings, with room for their allocation headers.
    pub(crate) fn bytes(set: &MeasurementSet, config: &EngineConfig, setup: &Setup) -> u64 {
        fn strings(values: &[String]) -> usize {
            values.iter().map(|s| 32 + s.len()).sum()
        }
        fn manual(values: &[(String, crate::input::SubManual)]) -> usize {
            values
                .iter()
                .map(|(r, m)| {
                    256 + r.len() + m.bundle.len() + m.allpass.len() * 16 + m.peq.len() * 24
                })
                .sum()
        }
        let measurements: usize = set
            .positions
            .iter()
            .map(|(_, roles)| {
                // Measurement metadata, two cloned Spectra cell tables, and timing
                // entries (only the reference has timing, so all cells overcount it).
                128 + roles.iter().map(|(r, _)| 320 + 5 * r.len()).sum::<usize>()
            })
            .sum();
        let config_bytes = strings(&config.mains)
            + strings(&config.subs)
            + config
                .groups
                .iter()
                .map(|(g, rs)| 64 + g.len() + strings(rs))
                .sum::<usize>()
            + config.target.points.len() * 16
            + config.xo.iter().map(|(g, _)| 48 + g.len()).sum::<usize>()
            + manual(&config.sub_manual)
            + config
                .bundle_layouts
                .iter()
                .map(|(g, _)| 64 + g.len())
                .sum::<usize>();
        let setup_bytes = manual(&setup.sub_manual)
            + setup
                .sub_filters
                .iter()
                .map(|(r, f)| 96 + r.len() + f.peq.len() * 24)
                .sum::<usize>()
            + setup
                .layouts
                .iter()
                .map(|(g, l)| 80 + g.len() + strings(&l.members))
                .sum::<usize>();
        (std::mem::size_of::<Self>() + 128 + measurements + config_bytes + setup_bytes) as u64
    }
}
