//! What the engine is given: measurements and the resolved design settings.
//!
//! Upstream: the arguments of `CorrectionEngine.__init__`
//! (`correction_engine.py:411–417`) as `calibration.make_engine` passes them
//! (`calibration.py:564–581`), plus the attributes it writes after
//! construction (`infra_boost_max_db`, `xo_peq_*`, the lowered
//! `max_boost_db`). The `.secs`-level settings (`CalibrationSettings`) are
//! resolved into these values by the orchestrator, not here.

use secs_types::target::TargetCurve;
use std::sync::Arc;

/// Impulse-response samples in the precision they were stored with.
///
/// Upstream widens every IR to float64 (`np.asarray(ir, float)`) before any
/// arithmetic, which [`Samples::at`] reproduces exactly; keeping float32
/// sources as float32 halves their memory.
#[derive(Debug, Clone)]
pub enum Samples {
    /// WAV, REW and mdat measurements.
    F32(Arc<[f32]>),
    /// Synthetic or computed responses.
    F64(Arc<[f64]>),
}

impl Samples {
    /// Number of samples.
    pub fn len(&self) -> usize {
        match self {
            Self::F32(s) => s.len(),
            Self::F64(s) => s.len(),
        }
    }

    /// Whether there are no samples.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Sample `i` widened to f64 (exact for float32).
    pub fn at(&self, i: usize) -> f64 {
        match self {
            Self::F32(s) => f64::from(s[i]),
            Self::F64(s) => s[i],
        }
    }
}

/// One measured impulse response: upstream's `(ir, sr, t0)` tuple.
#[derive(Debug, Clone)]
pub struct Measurement {
    /// The impulse response.
    pub samples: Samples,
    /// Its sample rate in Hz.
    pub sample_rate: u32,
    /// Time in seconds of sample 0 relative to the measurement's own clock
    /// (REW's `timingOffset`), 0 when unknown.
    pub t0: f64,
}

/// Measurements by position, then role.
///
/// Positions are ascending (upstream iterates `sorted(irs)`); roles keep the
/// order they were given in, which is the order upstream's dictionaries
/// iterate.
#[derive(Debug, Clone, Default)]
pub struct MeasurementSet {
    /// `(position, [(role, measurement)])`, positions strictly ascending.
    pub positions: Vec<(u32, Vec<(String, Measurement)>)>,
}

impl MeasurementSet {
    /// The measurement of `role` at `position`, if any.
    pub fn get(&self, position: u32, role: &str) -> Option<&Measurement> {
        self.positions
            .iter()
            .find(|(p, _)| *p == position)
            .and_then(|(_, roles)| roles.iter().find(|(r, _)| r == role).map(|(_, m)| m))
    }
}

/// One peaking band `(frequency Hz, Q, gain dB)`, upstream's PEQ tuple.
pub type Peq = (f64, f64, f64);
/// One second-order all-pass `(frequency Hz, Q)`.
pub type Allpass = (f64, f64);

/// Crossover filter family (`BAND_TYPES`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandType {
    /// Linkwitz–Riley 12 dB/oct.
    Lr12,
    /// Linkwitz–Riley 24 dB/oct (the default).
    Lr24,
    /// Linkwitz–Riley 48 dB/oct.
    Lr48,
    /// Butterworth 12 dB/oct.
    Bw12,
    /// Butterworth 24 dB/oct.
    Bw24,
}

impl BandType {
    /// Upstream name (`'LR24'` …), as `secs_dsp::response::band_filter` takes it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Lr12 => "LR12",
            Self::Lr24 => "LR24",
            Self::Lr48 => "LR48",
            Self::Bw12 => "BW12",
            Self::Bw24 => "BW24",
        }
    }

    /// The type named `name`, or `LR24` for anything else (upstream's fallback).
    pub fn from_name(name: &str) -> Self {
        match name {
            "LR12" => Self::Lr12,
            "LR48" => Self::Lr48,
            "BW12" => Self::Bw12,
            "BW24" => Self::Bw24,
            _ => Self::Lr24,
        }
    }
}

/// How BA treats a sub (R27).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubMode {
    /// The engine finds gain, delay, polarity and all-passes.
    Auto,
    /// The values are used as given (they are DSP output values).
    Fixed,
    /// Relative values inside the bundle are kept; the engine finds the
    /// bundle's common gain, delay and polarity.
    Bundle,
}

/// DBA row of a sub in a DBA layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbaRow {
    /// Front array.
    Front,
    /// Rear array (inverted and delayed by the propagation time).
    Rear,
}

/// A sub's manual settings: upstream `calibration.sub_entry()`, normalised.
#[derive(Debug, Clone, PartialEq)]
pub struct SubManual {
    /// BA mode.
    pub mode: SubMode,
    /// Bundle name (`"A"`, `"B"`) when `mode` is [`SubMode::Bundle`].
    pub bundle: String,
    /// Output gain in dB.
    pub gain_db: f64,
    /// Output delay in ms.
    pub delay_ms: f64,
    /// Output polarity, `1` or `-1`.
    pub polarity: i8,
    /// Up to two all-passes.
    pub allpass: Vec<Allpass>,
    /// Whether the sub may serve as a MIMO support (M3).
    pub mimo_support: bool,
    /// Fixed high-pass in Hz, 0 for none.
    pub hp_hz: f64,
    /// High-pass family.
    pub hp_type: BandType,
    /// Fixed low-pass in Hz, 0 for none.
    pub lp_hz: f64,
    /// Low-pass family.
    pub lp_type: BandType,
    /// Fixed PEQ bands, at most three.
    pub peq: Vec<Peq>,
    /// DBA row, used only by a DBA layout.
    pub dba_row: DbaRow,
}

/// Bundle layout kind (R30).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutKind {
    /// Double bass array: front row, inverted and delayed rear row.
    Dba,
    /// Position-based multi-sub: one common filter, per-sub level.
    Multi,
}

/// A bundle layout (`bundle_layouts` entry other than `'values'`).
#[derive(Debug, Clone, PartialEq)]
pub struct BundleLayout {
    /// Layout kind.
    pub kind: LayoutKind,
    /// User trim of the rear-row delay in ms (DBA).
    pub rear_trim_ms: f64,
}

/// The resolved design settings: the engine constructor's surface.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Main roles, in configuration group order.
    pub mains: Vec<String>,
    /// Sub roles, in configuration order.
    pub subs: Vec<String>,
    /// Configuration groups and their roles, in order (`SpeakerConfig.groups()`).
    pub groups: Vec<(String, Vec<String>)>,
    /// Target curve (a copy taken when the computation starts).
    pub target: TargetCurve,
    /// Crossover in Hz per main group, in upstream dictionary order.
    pub xo: Vec<(String, f64)>,
    /// Reference (listening) position.
    pub main_position: u32,
    /// Channel FIR taps; 0 means PEQ only.
    pub taps: usize,
    /// Excess-phase lift in ms, before the FIR-length cap.
    pub lift_ms: f64,
    /// Boost ceiling in dB for RC, and the cap of the XO and bus PEQ boosts.
    pub max_boost_db: f64,
    /// Input PEQ bands; `None` means 16.
    pub peq_bands: Option<usize>,
    /// Total filter latency budget in ms; `None` means lift + 15.
    pub max_latency_ms: Option<f64>,
    /// RC range `(low Hz, high Hz)`; a missing high bound means automatic
    /// (Schroeder frequency, at most 500 Hz).
    pub rc_range: (f64, Option<f64>),
    /// Room volume in m³, for the Schroeder frequency.
    pub room_volume_m3: Option<f64>,
    /// Octave fraction of the excess-phase smoothing (6, or 24 in linear mode).
    pub phase_frac: f64,
    /// Manual sub settings by role, in the order the settings hold them.
    pub sub_manual: Vec<(String, SubManual)>,
    /// Main output delay added when fixed subs are present, in ms.
    pub sub_main_delay_ms: f64,
    /// Bundle layouts by bundle name, in the order the settings hold them.
    pub bundle_layouts: Vec<(String, BundleLayout)>,
    /// Ceiling in dB of the two-way infra sub level match.
    pub infra_boost_max_db: f64,
    /// Whether the main acoustic-XO PEQ runs.
    pub xo_peq_on: bool,
    /// Lower edge of the XO PEQ fitting range in Hz, 0 for automatic.
    pub xo_peq_lo_hz: f64,
    /// Upper edge of the XO PEQ fitting range in Hz, 0 for automatic.
    pub xo_peq_hi_hz: f64,
}

/// Which positions the common excess phase is measured over (`SECS_PHASE_POS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhasePositions {
    /// All positions where the channel was measured (default).
    Common,
    /// The reference position only.
    Main,
}

/// BA all-pass policy (constructor `sub_allpass`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubAllpass {
    /// No all-passes (production; their DE variables still exist, D11).
    None,
    /// Q up to 3.
    Full,
    /// Q up to 1.
    LowQ,
}

/// Bundle gain policy (constructor `bundle_gain`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleGain {
    /// The engine finds the bundle's common gain (production).
    Auto,
    /// The bundle keeps the gains as entered.
    Fixed,
}

/// Engine knobs SECS 2 kept in environment variables or constructor
/// defaults that production never changes. Defaults are upstream's.
#[derive(Debug, Clone, PartialEq)]
pub struct Tuning {
    /// `SECS_BOOST_RING_MS`: ringing cap of RC PEQ bands in ms; `None` disables.
    pub boost_ring_ms: Option<f64>,
    /// `SECS_BA_DELAY_MS`: BA fine-delay search half-width in ms.
    pub ba_delay_ms: f64,
    /// `SECS_BA_EFF`: weight of the summing-efficiency reward in the BA cost.
    pub ba_eff_weight: f64,
    /// `SECS_SUB_BUS_FIX`: bus PEQ capped at 1.25 × XO and the second BA pass.
    pub sub_bus_fix: bool,
    /// `SECS_MAIN_W_LO`: reference-position weight below 200 Hz in RC.
    pub main_weight_lo: f64,
    /// `SECS_PHASE_POS`: positions of the common excess phase.
    pub phase_positions: PhasePositions,
    /// `SECS_LONG_BOOST_DB`: boost ceiling at long room modes in dB.
    pub long_boost_db: f64,
    /// `SECS_NOBOOST_MS`: GD-spike threshold of the no-boost mask in ms
    /// (default 1e9, effectively off).
    pub noboost_ms: f64,
    /// Constructor `xo_stage`: acoustic-XO target PEQ and bus PEQ.
    pub xo_stage: bool,
    /// Constructor `sub_allpass`.
    pub sub_allpass: SubAllpass,
    /// Constructor `gd_cuts`: high-Q cuts at excess-GD peaks.
    pub gd_cuts: bool,
    /// Constructor `bundle_gain`.
    pub bundle_gain: BundleGain,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            boost_ring_ms: Some(30.0),
            ba_delay_ms: 12.0,
            ba_eff_weight: 1.0,
            sub_bus_fix: false,
            main_weight_lo: 5.0,
            phase_positions: PhasePositions::Common,
            long_boost_db: 3.0,
            noboost_ms: 1e9,
            xo_stage: true,
            sub_allpass: SubAllpass::None,
            gd_cuts: false,
            bundle_gain: BundleGain::Auto,
        }
    }
}
