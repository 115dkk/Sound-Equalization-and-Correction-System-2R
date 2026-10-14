//! The designed DSP as one value: what the engine produces and what the
//! response simulator (`secs-engine`) and the CamillaDSP writer
//! (`secs-camilla`) read (docs/ARCHITECTURE.md §4, docs/ENGINE.md §2).
//!
//! Upstream spread this over `CorrectionResult.channels` / `.subs` and the
//! engine attributes `top_peq`, `sub_bus_peq`, `align_delay_ms` and
//! `main_extra_ms`, which `export_camilladsp.make_config` read directly.
//!
//! Delays here are on the **output axis**: what the DSP applies, the internal
//! (spectral) axis plus [`SignalChain::main_extra_ms`].

use serde::{Deserialize, Serialize};

/// One peaking band `(frequency Hz, Q, gain dB)`, upstream's PEQ tuple.
pub type Peq = (f64, f64, f64);
/// One second-order all-pass `(frequency Hz, Q)`.
pub type Allpass = (f64, f64);

/// Crossover filter family (upstream `BAND_TYPES`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BandType {
    /// Linkwitz–Riley 12 dB/oct.
    #[serde(rename = "LR12")]
    Lr12,
    /// Linkwitz–Riley 24 dB/oct (the default).
    #[serde(rename = "LR24")]
    Lr24,
    /// Linkwitz–Riley 48 dB/oct.
    #[serde(rename = "LR48")]
    Lr48,
    /// Butterworth 12 dB/oct.
    #[serde(rename = "BW12")]
    Bw12,
    /// Butterworth 24 dB/oct.
    #[serde(rename = "BW24")]
    Bw24,
}

impl BandType {
    /// Upstream name (`"LR24"` …).
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

/// A main channel: input correction before bass management, then the main
/// output path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MainChannel {
    /// Speaker role.
    pub role: String,
    /// Input PEQ, applied before the FIR (`ChannelOut.peq`).
    pub input_peq: Vec<Peq>,
    /// Input correction FIR at 48 kHz; `[1.0]` when the mode has no FIR.
    pub fir: Vec<f64>,
    /// Main output gain in dB.
    pub gain_db: f64,
    /// Main output delay in ms, output axis.
    pub delay_ms: f64,
    /// Bass-management crossover in Hz (HP on the main, LP on its sub send).
    pub xo_hz: Option<f64>,
    /// Acoustic-XO PEQ on the main output path (`engine.top_peq`).
    pub top_peq: Vec<Peq>,
    /// Arrival-alignment delay in ms, internal axis (`engine.align_delay_ms`);
    /// MIMO paths delay by it plus `main_extra_ms`.
    pub align_delay_ms: f64,
}

/// A sub output fed by the sub bus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubChannel {
    /// Speaker role.
    pub role: String,
    /// Bus-to-sub gain in dB.
    pub gain_db: f64,
    /// Output delay in ms, output axis.
    pub delay_ms: f64,
    /// Polarity, `1` or `-1`.
    pub polarity: i8,
    /// All-passes, in order.
    pub allpass: Vec<Allpass>,
    /// Fixed high-pass in Hz, 0 for none.
    pub hp_hz: f64,
    /// High-pass family.
    pub hp_type: BandType,
    /// Fixed low-pass in Hz, 0 for none.
    pub lp_hz: f64,
    /// Low-pass family.
    pub lp_type: BandType,
    /// Fixed output PEQ, then generated bundle PEQ bands.
    pub peq: Vec<Peq>,
    /// How BA treated the sub, upstream's label: `auto`, `fixed`,
    /// `bundle:A`, `bundle:A:multi`, `bundle:A:dba-front`, `bundle:A:dba-rear`.
    pub mode: String,
}

/// The whole designed DSP.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalChain {
    /// Sample rate the coefficients are for (48 kHz).
    pub sample_rate: u32,
    /// Main channels in design order (`CorrectionResult.channels`).
    pub mains: Vec<MainChannel>,
    /// Subs in configuration order (`CorrectionResult.subs`).
    pub subs: Vec<SubChannel>,
    /// PEQ on the summed sub bus, copied into every sub output.
    pub sub_bus_peq: Vec<Peq>,
    /// Common shift from the internal to the output axis in ms
    /// (`engine.main_extra_ms`).
    pub main_extra_ms: f64,
    /// Excess-phase lift of the input FIRs in ms.
    pub lift_ms: f64,
    /// Total filter latency in ms: lift plus `main_extra_ms`.
    pub latency_ms: f64,
}

/// What a MIMO support path filters with (upstream `mimo_paths`' filter slot).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MimoFilter {
    /// IIR support (`mimo_iir.IirSupport`): gain, polarity, integer-sample
    /// delay, peaking bands and all-passes, then the path's LR4 high-pass at
    /// [`MimoPath::low_hz`] and LR4 low-pass at [`MimoPaths::fsiso`].
    Iir {
        /// Path gain in dB.
        gain_db: f64,
        /// Path delay in ms, added to [`MimoPath::delay_ms`].
        delay_ms: f64,
        /// Polarity, `1` or `-1`.
        polarity: i8,
        /// Peaking bands, in order.
        peq: Vec<Peq>,
        /// All-passes, in order.
        allpass: Vec<Allpass>,
    },
    /// FIR support: the low-rate ideal FIR raised to 48 kHz
    /// (`export_camilladsp.fir_to_48k`); the Fsiso low-pass is inside it.
    Fir {
        /// Taps at 48 kHz.
        taps_48k: Vec<f64>,
    },
}

/// One MIMO support path: input `input` also drives speaker `support`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MimoPath {
    /// Input role (the channel whose raw signal the path copies).
    pub input: String,
    /// Support speaker role (the output the path adds to).
    pub support: String,
    /// The path filter.
    pub filter: MimoFilter,
    /// Protective LR4 high-pass in Hz: the support's F-support low edge,
    /// 50 Hz when the speaker has no range (upstream fallback).
    pub low_hz: f64,
    /// The support speaker's delay in ms, output axis: its alignment delay
    /// plus [`SignalChain::main_extra_ms`].
    pub delay_ms: f64,
}

/// Every MIMO support path, in upstream order (input, then support).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MimoPaths {
    /// Isolation frequency in Hz (the IIR paths' LR4 low-pass).
    pub fsiso: f64,
    /// The paths.
    pub paths: Vec<MimoPath>,
}
