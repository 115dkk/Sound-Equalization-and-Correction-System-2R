#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! The export graph: one validated signal chain with its MIMO paths and the
//! topology every export writer shares (docs/ARCHITECTURE.md §4).
//!
//! Building an [`ExportGraph`] is the only validation of a [`SignalChain`]
//! and its [`MimoPaths`]. The CamillaDSP writer (`secs-camilla`), the
//! filter-text and Equalizer APO writers and the FIR support renderer
//! (`secs-filters`) read the graph and serialise it. A check that belongs to
//! one writer only (CamillaDSP options, a filter run's label and stamp, the
//! renderer's block geometry) stays with that writer.
//!
//! Upstream sources: `export_filters.make_plan` for the [`FilterPlan`], and
//! the input rules both `export_camilladsp.py` and `export_filters.py`
//! assumed without checking.

mod plan;
mod validate;

pub use plan::{FilterPlan, FirView, MimoFeed, Op, SubFeed, headroom_db, plan_responses};

use secs_types::chain::{MimoPaths, SignalChain};

/// Sample rate of every exported FIR and delay.
pub const SAMPLE_RATE: u32 = 48_000;
/// Upstream Dolby LFE playback gain.
pub const LFE_GAIN_DB: f64 = 10.0;
/// Upstream Dolby LFE content low-pass frequency.
pub const LFE_LP_HZ: f64 = 120.0;

/// Python/NumPy half-even rounding to an integer.
pub fn round_ties_even_i64(value: f64) -> i64 {
    let floor = value.floor();
    let fraction = value - floor;
    let rounded = if fraction < 0.5 {
        floor
    } else if fraction > 0.5 {
        floor + 1.0
    } else if (floor as i64) & 1 == 0 {
        floor
    } else {
        floor + 1.0
    };
    rounded as i64
}

/// Whether a delay is negative at the writers' resolution: below zero after
/// the half-to-even rounding to whole 48 kHz samples. A smaller negative
/// value is float noise that every writer emits as no delay (#92 TD-94).
pub fn is_negative_delay(milliseconds: f64) -> bool {
    round_ties_even_i64(milliseconds * f64::from(SAMPLE_RATE) / 1000.0) < 0
}

/// The LFE input the export includes, as upstream's `make_plan` takes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LfeChoice {
    /// No LFE input.
    Omit,
    /// An LFE input with this gain, then an LR24 low-pass, summed into every
    /// sub bus. It is an input only when the chain has subs.
    Include {
        /// LFE gain in dB.
        gain_db: f64,
        /// LFE low-pass frequency in hertz.
        lp_hz: f64,
    },
}

impl LfeChoice {
    /// Upstream's Dolby LFE: +10 dB, then a 120 Hz low-pass.
    pub const DOLBY: Self = Self::Include {
        gain_db: LFE_GAIN_DB,
        lp_hz: LFE_LP_HZ,
    };
}

/// Why a chain and its MIMO paths cannot be exported. Each message names
/// the offending field the way the writers' messages did, for example
/// `main.L.delay_ms` or `mimo.paths[2].taps_low`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GraphError {
    /// An identifier is not ASCII.
    #[error("non-ASCII export identifier: {0}")]
    NonAscii(String),
    /// A number is NaN or infinite.
    #[error("non-finite export value: {0}")]
    NonFinite(String),
    /// Any other broken rule, or a plan its transfer functions cannot read.
    #[error("invalid export graph: {0}")]
    Invalid(String),
}

/// MIMO support paths that feed one output through the same protective
/// high-pass and filter family, so a writer may apply that high-pass (and an
/// IIR family's Fsiso low-pass) once, after their sum (F13).
///
/// The key is the support output, the family and the exact low edge (its bit
/// pattern). A writer that writes the high-pass at a coarser precision joins
/// the groups whose written cutoff coincides; how a group's delays split
/// into a shared and a private part is also each writer's own.
#[derive(Debug, Clone, PartialEq)]
pub struct SupportGroup {
    /// Support output, an index into [`FilterPlan::outputs`] (mains, then subs).
    pub output: usize,
    /// Whether the paths are FIR (`Fir` or `FirLowRate`) rather than IIR.
    pub fir: bool,
    /// The members' common [`MimoPath::low_hz`](secs_types::chain::MimoPath::low_hz).
    pub low_hz: f64,
    /// Path indices into [`MimoPaths::paths`] (and [`FilterPlan::mimo`]), ascending.
    pub members: Vec<usize>,
}

/// A validated signal chain with its MIMO paths and the topology every
/// export writer shares: inputs, outputs, per-role operations, sub feeds,
/// MIMO feeds and support groups. Building it is the only validation.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportGraph {
    chain: SignalChain,
    mimo: Option<MimoPaths>,
    plan: FilterPlan,
    support_groups: Vec<SupportGroup>,
}

impl ExportGraph {
    /// Validate `chain`, `mimo` and `lfe`, then build the plan and the
    /// support groups.
    ///
    /// The rules: 48 kHz; every number finite; every role and MIMO
    /// identifier non-empty ASCII letters and digits; no duplicate role and
    /// no role named `LFE`; a sub mode in ASCII; polarities `1` or `-1`;
    /// delays not negative after rounding to whole samples
    /// ([`is_negative_delay`]); a MIMO input is a main and a support an
    /// output; a low-rate FIR at most 48 kHz whose 48 kHz expansion fits.
    pub fn build(
        chain: &SignalChain,
        mimo: Option<&MimoPaths>,
        lfe: LfeChoice,
    ) -> Result<Self, GraphError> {
        validate::graph(chain, mimo, lfe)?;
        let plan = plan::make(chain, mimo, lfe);
        let support_groups = support_groups(&plan.outputs, mimo);
        Ok(Self {
            chain: chain.clone(),
            mimo: mimo.cloned(),
            plan,
            support_groups,
        })
    }

    /// The validated chain.
    pub fn chain(&self) -> &SignalChain {
        &self.chain
    }

    /// The validated MIMO paths, when the export includes MIMO.
    pub fn mimo(&self) -> Option<&MimoPaths> {
        self.mimo.as_ref()
    }

    /// The upstream `make_plan` topology.
    pub fn plan(&self) -> &FilterPlan {
        &self.plan
    }

    /// The MIMO support groups, in order of their first path.
    pub fn support_groups(&self) -> &[SupportGroup] {
        &self.support_groups
    }
}

fn support_groups(outputs: &[String], mimo: Option<&MimoPaths>) -> Vec<SupportGroup> {
    let mut groups: Vec<SupportGroup> = Vec::new();
    for (index, path) in mimo.map_or(&[][..], |mimo| &mimo.paths).iter().enumerate() {
        let output = outputs
            .iter()
            .position(|role| *role == path.support)
            .expect("validated support");
        let fir = path.filter.is_fir();
        if let Some(group) = groups.iter_mut().find(|group| {
            group.output == output
                && group.fir == fir
                && group.low_hz.to_bits() == path.low_hz.to_bits()
        }) {
            group.members.push(index);
        } else {
            groups.push(SupportGroup {
                output,
                fir,
                low_hz: path.low_hz,
                members: vec![index],
            });
        }
    }
    groups
}
