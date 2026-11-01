//! Streaming consumer of the FIR support paths in a designed signal chain.
//!
//! Inputs are the **raw main inputs**, in [`SignalChain::mains`] order. Outputs
//! are the MIMO contributions, in main-then-sub order. Input correction, bass
//! management and the ordinary main/sub paths belong to the base renderer.
//! [`MimoFirPlayback::process_with_base`] combines its already-rendered output
//! with these contributions and aligns the base by the additional rate-converter
//! delay. No audio-device backend or external-player extension is implied.
//!
//! Full-rate mode uses the compatible 48 kHz materialization, in f64 before WAV
//! quantization. Multirate mode is explicitly experimental: its anti-alias and
//! interpolation filters change the response from the historically cropped FIR.
//! Both modes reuse transforms between routes and allocate only at construction.

use secs_camilla::pyfmt::round_ndigits;
use secs_dsp::{
    DspError,
    convolution::{FirRoute, MatrixConvolver},
    fir::lowrate_to_48k,
    iir::{BType, Sos, butter},
    multirate::MultirateMatrixConvolver,
};
use secs_graph::{ExportGraph, LfeChoice, round_ties_even_i64};
use secs_types::chain::{BandType, MimoFilter, MimoPaths, SignalChain};
use std::f64::consts::PI;

const SAMPLE_RATE: f64 = 48_000.0;
/// The reference renderer accepts output delays up to one minute.
pub const MAX_OUTPUT_DELAY_SAMPLES: usize = 60 * 48_000;

/// The rate and block geometry of the support renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackMode {
    /// Compatible expanded FIR, with a block size in 48 kHz samples.
    FullRate { block_size: usize },
    /// Low-rate FIR with shared input decimation and output interpolation.
    /// Every path must be `FirLowRate` at exactly `48_000 / factor` hertz.
    Multirate {
        low_block_size: usize,
        factor: usize,
    },
}

fn invalid(message: &str) -> DspError {
    DspError::InvalidArgument(message.into())
}

#[derive(Debug)]
struct Delay {
    samples: Vec<f64>,
    next: usize,
}
impl Delay {
    fn new(samples: usize) -> Self {
        Self {
            samples: vec![0.0; samples],
            next: 0,
        }
    }
    fn sample(&mut self, input: f64) -> f64 {
        if self.samples.is_empty() {
            return input;
        }
        let output = self.samples[self.next];
        self.samples[self.next] = input;
        self.next = (self.next + 1) % self.samples.len();
        output
    }
    fn reset(&mut self) {
        self.samples.fill(0.0);
        self.next = 0;
    }
}

#[derive(Debug, Default)]
struct Biquads {
    rows: Vec<[f64; 6]>,
    state: Vec<[f64; 2]>,
}
impl Biquads {
    fn append(&mut self, sos: &Sos) {
        self.rows.extend_from_slice(&sos.0);
        self.state.resize(self.rows.len(), [0.0; 2]);
    }
    fn band(&mut self, frequency: f64, kind: BandType, high: bool) -> Result<(), DspError> {
        if frequency == 0.0 {
            return Ok(());
        }
        let (order, copies) = match kind {
            BandType::Lr12 => (1, 2),
            BandType::Lr24 => (2, 2),
            BandType::Lr48 => (4, 2),
            BandType::Bw12 => (2, 1),
            BandType::Bw24 => (4, 1),
            BandType::Bw48 => (8, 1),
        };
        let sos = butter(
            order,
            &[frequency],
            if high {
                BType::Highpass
            } else {
                BType::Lowpass
            },
            SAMPLE_RATE,
        )?;
        for _ in 0..copies {
            self.append(&sos);
        }
        Ok(())
    }
    fn peq(&mut self, frequency: f64, q: f64, gain_db: f64) -> Result<(), DspError> {
        // Match the parameters actually emitted by the Camilla writer.
        let frequency = round_ndigits(frequency, 4);
        let q = round_ndigits(q, 5);
        let gain_db = round_ndigits(gain_db, 4);
        if !frequency.is_finite()
            || frequency <= 0.0
            || frequency >= SAMPLE_RATE / 2.0
            || !q.is_finite()
            || q <= 0.0
            || !gain_db.is_finite()
        {
            return Err(invalid("invalid playback peaking EQ"));
        }
        let a = 10_f64.powf(gain_db / 40.0);
        let w = 2.0 * PI * frequency / SAMPLE_RATE;
        let alpha = w.sin() / (2.0 * q);
        let a0 = 1.0 + alpha / a;
        let row = [
            (1.0 + alpha * a) / a0,
            -2.0 * w.cos() / a0,
            (1.0 - alpha * a) / a0,
            1.0,
            -2.0 * w.cos() / a0,
            (1.0 - alpha / a) / a0,
        ];
        if row.iter().any(|x| !x.is_finite()) {
            return Err(invalid("invalid playback peaking coefficients"));
        }
        self.append(&Sos(vec![row]));
        Ok(())
    }
    fn process(&mut self, values: &mut [f64]) {
        for (row, state) in self.rows.iter().zip(&mut self.state) {
            for value in values.iter_mut() {
                let input = *value;
                *value = row[0] * input + state[0];
                state[0] = row[1] * input - row[4] * *value + state[1];
                state[1] = row[2] * input - row[5] * *value;
            }
        }
    }
    fn reset(&mut self) {
        self.state.fill([0.0; 2]);
    }
}

#[derive(Debug)]
struct Group {
    output: usize,
    low_hz: f64,
    delay_samples: usize,
    highpass: Biquads,
    delay: Delay,
}

#[derive(Debug)]
enum Kernel {
    Empty { block_size: usize },
    Full(MatrixConvolver),
    Low(MultirateMatrixConvolver),
}
impl Kernel {
    fn process<I: AsRef<[f64]>>(
        &mut self,
        input: &[I],
        output: &mut [Vec<f64>],
    ) -> Result<(), DspError> {
        match self {
            Self::Empty { .. } => Ok(()),
            Self::Full(kernel) => kernel.process_buffers(input, output),
            Self::Low(kernel) => kernel.process_buffers(input, output),
        }
    }
    fn reset(&mut self) {
        match self {
            Self::Empty { .. } => {}
            Self::Full(kernel) => kernel.reset(),
            Self::Low(kernel) => kernel.reset(),
        }
    }
    fn block_size(&self) -> usize {
        match self {
            Self::Empty { block_size } => *block_size,
            Self::Full(kernel) => kernel.block_size(),
            Self::Low(kernel) => kernel.block_size(),
        }
    }
    fn latency(&self) -> usize {
        match self {
            Self::Empty { .. } | Self::Full(_) => 0,
            Self::Low(kernel) => kernel.latency_samples(),
        }
    }
    fn tail(&self) -> usize {
        match self {
            Self::Empty { .. } => 0,
            Self::Full(kernel) => kernel.tail_samples(),
            Self::Low(kernel) => kernel.tail_samples(),
        }
    }
    fn transforms(&self) -> (usize, usize) {
        match self {
            Self::Empty { .. } => (0, 0),
            Self::Full(kernel) => kernel.transforms_per_block(),
            Self::Low(kernel) => kernel.transforms_per_block(),
        }
    }
}

/// FIR support renderer with common output filtering and optional base alignment.
///
/// Different high-pass cutoffs or delays on one output form separate transform
/// groups. Ordinary designs share them, requiring one inverse FFT per output.
/// Fixed sub HP/LP/PEQ is applied after the support groups have been summed.
#[derive(Debug)]
pub struct MimoFirPlayback {
    inputs: Vec<String>,
    outputs: Vec<String>,
    kernel: Kernel,
    pre: Vec<Biquads>,
    input_buffers: Vec<Vec<f64>>,
    groups: Vec<Group>,
    group_buffers: Vec<Vec<f64>>,
    fixed: Vec<Biquads>,
    base_delays: Vec<Delay>,
}

impl MimoFirPlayback {
    /// Build a renderer. IIR support routes are rejected rather than omitted.
    /// The chain and paths are validated by building their [`ExportGraph`];
    /// the renderer adds only its own limits (main inputs, block geometry,
    /// high-pass range, delay buffer, FIR routes, multirate rate).
    pub fn new(
        chain: &SignalChain,
        mimo: &MimoPaths,
        mode: PlaybackMode,
    ) -> Result<Self, DspError> {
        let graph = ExportGraph::build(chain, Some(mimo), LfeChoice::Omit)
            .map_err(|error| invalid(&error.to_string()))?;
        if chain.mains.is_empty() {
            return Err(invalid("playback requires main inputs"));
        }
        if matches!(mode, PlaybackMode::Multirate { factor: 0, .. }) {
            return Err(invalid("multirate factor must be positive"));
        }
        let block_size = match mode {
            PlaybackMode::FullRate { block_size } => block_size,
            PlaybackMode::Multirate {
                low_block_size,
                factor,
            } => low_block_size
                .checked_mul(factor)
                .ok_or_else(|| invalid("playback block size overflow"))?,
        };
        if block_size == 0 {
            return Err(invalid("playback block size must be positive"));
        }
        let inputs = graph.plan().inputs.clone();
        let outputs = graph.plan().outputs.clone();
        // The graph groups support paths by output, family and exact low edge.
        // The renderer applies the high-pass rounded to three places, as the
        // CamillaDSP writer emits it, and one whole-sample delay per group: it
        // joins graph groups whose rounded cutoff coincides and splits them by
        // each path's rounded delay, in path order.
        let mut support_group = vec![0; mimo.paths.len()];
        for (index, group) in graph.support_groups().iter().enumerate() {
            for &member in &group.members {
                support_group[member] = index;
            }
        }
        let mut groups: Vec<Group> = Vec::new();
        let mut routes = Vec::with_capacity(mimo.paths.len());
        for (path_index, path) in mimo.paths.iter().enumerate() {
            let shared = &graph.support_groups()[support_group[path_index]];
            let input = inputs
                .iter()
                .position(|r| r == &path.input)
                .expect("validated MIMO input");
            if shared.low_hz < 0.0
                || shared.low_hz >= SAMPLE_RATE / 2.0
                || path.delay_ms * 48.0 > MAX_OUTPUT_DELAY_SAMPLES as f64
            {
                return Err(invalid("invalid playback high-pass or output delay"));
            }
            let output = shared.output;
            let low_hz = round_ndigits(shared.low_hz, 3);
            // Preserve the writer's floating-point evaluation order at half samples.
            let delay_samples = round_ties_even_i64(path.delay_ms * SAMPLE_RATE / 1000.0) as usize;
            let group = if let Some(index) = groups.iter().position(|g| {
                g.output == output && g.low_hz == low_hz && g.delay_samples == delay_samples
            }) {
                index
            } else {
                let mut highpass = Biquads::default();
                highpass.band(low_hz, BandType::Lr24, true)?;
                groups.push(Group {
                    output,
                    low_hz,
                    delay_samples,
                    highpass,
                    delay: Delay::new(delay_samples),
                });
                groups.len() - 1
            };
            let taps = match (&path.filter, mode) {
                (MimoFilter::Fir { taps_48k }, PlaybackMode::FullRate { .. }) => taps_48k.clone(),
                (MimoFilter::FirLowRate { taps_low, fs_low }, PlaybackMode::FullRate { .. }) => {
                    lowrate_to_48k(taps_low, *fs_low)
                }
                (
                    MimoFilter::FirLowRate { taps_low, fs_low },
                    PlaybackMode::Multirate { factor, .. },
                ) => {
                    if *fs_low != SAMPLE_RATE / factor as f64 {
                        return Err(invalid(
                            "every low-rate FIR must match the selected integer rate factor",
                        ));
                    }
                    taps_low.clone()
                }
                (MimoFilter::Fir { .. }, PlaybackMode::Multirate { .. }) => {
                    return Err(invalid(
                        "multirate playback requires retained low-rate coefficients",
                    ));
                }
                (MimoFilter::Iir { .. }, _) => {
                    return Err(invalid("MimoFirPlayback accepts FIR routes only"));
                }
            };
            routes.push(FirRoute {
                input,
                output: group,
                taps,
            });
        }
        let group_count = groups.len();
        let kernel = if routes.is_empty() {
            Kernel::Empty { block_size }
        } else {
            match mode {
                PlaybackMode::FullRate { block_size } => Kernel::Full(MatrixConvolver::new(
                    inputs.len(),
                    group_count,
                    block_size,
                    &routes,
                )?),
                PlaybackMode::Multirate {
                    low_block_size,
                    factor,
                } => Kernel::Low(MultirateMatrixConvolver::new(
                    inputs.len(),
                    group_count,
                    low_block_size,
                    factor,
                    &routes,
                )?),
            }
        };
        let group_buffers = vec![vec![0.0; kernel.block_size()]; group_count];
        let base_delays = (0..outputs.len())
            .map(|_| Delay::new(kernel.latency()))
            .collect();
        let mut fixed: Vec<_> = (0..outputs.len()).map(|_| Biquads::default()).collect();
        for (index, sub) in chain.subs.iter().enumerate() {
            let filter = &mut fixed[inputs.len() + index];
            filter.band(sub.hp_hz, sub.hp_type, true)?;
            filter.band(sub.lp_hz, sub.lp_type, false)?;
            for &(frequency, q, gain) in &sub.peq {
                filter.peq(frequency, q, gain)?;
            }
        }
        let mut pre = Vec::new();
        if let Some(hz) = chain.input_hp_hz {
            for _ in &inputs {
                let mut filter = Biquads::default();
                filter.band(hz, BandType::Bw48, true)?;
                pre.push(filter);
            }
        }
        let input_buffers = vec![vec![0.0; block_size]; pre.len()];
        Ok(Self {
            pre,
            input_buffers,
            inputs,
            outputs,
            kernel,
            groups,
            group_buffers,
            fixed,
            base_delays,
        })
    }

    pub fn input_roles(&self) -> &[String] {
        &self.inputs
    }
    pub fn output_roles(&self) -> &[String] {
        &self.outputs
    }
    /// Required input and output block length, at 48 kHz.
    pub fn block_size(&self) -> usize {
        self.kernel.block_size()
    }
    /// Alignment delay from the rate converters, excluding host block buffering
    /// and delays already represented by the designed filters and output paths.
    pub fn additional_latency_samples(&self) -> usize {
        self.kernel.latency()
    }
    /// FIR/rate-converter tail plus the longest explicit output delay. The IIR
    /// output filters have infinite tails; callers choose extra settling time.
    pub fn fir_tail_samples(&self) -> usize {
        self.kernel.tail()
            + self
                .groups
                .iter()
                .map(|g| g.delay_samples)
                .max()
                .unwrap_or(0)
    }
    /// Forward transforms per input block and inverse transforms per group.
    pub fn transforms_per_block(&self) -> (usize, usize) {
        self.kernel.transforms()
    }

    fn validate(&self, inputs: &[&[f64]], outputs: &[&mut [f64]]) -> Result<(), DspError> {
        if inputs.len() != self.inputs.len()
            || outputs.len() != self.outputs.len()
            || inputs
                .iter()
                .any(|x| x.len() != self.block_size() || x.iter().any(|v| !v.is_finite()))
            || outputs.iter().any(|y| y.len() != self.block_size())
        {
            return Err(invalid("invalid playback input/output block"));
        }
        Ok(())
    }

    fn render_support(
        &mut self,
        inputs: &[&[f64]],
        outputs: &mut [&mut [f64]],
    ) -> Result<(), DspError> {
        if self.pre.is_empty() {
            self.kernel.process(inputs, &mut self.group_buffers)?;
        } else {
            for ((filter, buffer), input) in
                self.pre.iter_mut().zip(&mut self.input_buffers).zip(inputs)
            {
                buffer.copy_from_slice(input);
                filter.process(buffer);
            }
            self.kernel
                .process(&self.input_buffers, &mut self.group_buffers)?;
        }
        for output in outputs.iter_mut() {
            output.fill(0.0);
        }
        if self.groups.is_empty() {
            return Ok(());
        }
        for (group, values) in self.groups.iter_mut().zip(&mut self.group_buffers) {
            group.highpass.process(values);
            for (out, value) in outputs[group.output].iter_mut().zip(values) {
                *out += group.delay.sample(*value);
            }
        }
        for (filter, output) in self.fixed.iter_mut().zip(outputs) {
            filter.process(output);
        }
        Ok(())
    }

    /// Overwrite one block with MIMO contributions only. Do not alternate this
    /// method and `process_with_base` in one stream: their base-delay timelines
    /// differ. Call `reset` when switching modes or starting a new stream.
    pub fn process(
        &mut self,
        inputs: &[&[f64]],
        outputs: &mut [&mut [f64]],
    ) -> Result<(), DspError> {
        self.validate(inputs, outputs)?;
        self.render_support(inputs, outputs)
    }

    /// Add to base outputs already rendered at 48 kHz, in `output_roles` order,
    /// including the common input high-pass and ordinary fixed sub filters.
    /// The base renderer must apply the cut itself, before input correction.
    /// Only the extra multirate
    /// delay is added to the base; its existing design latency is retained.
    pub fn process_with_base(
        &mut self,
        inputs: &[&[f64]],
        base: &[&[f64]],
        outputs: &mut [&mut [f64]],
    ) -> Result<(), DspError> {
        self.validate(inputs, outputs)?;
        if base.len() != self.outputs.len()
            || base
                .iter()
                .any(|x| x.len() != self.block_size() || x.iter().any(|v| !v.is_finite()))
        {
            return Err(invalid("invalid playback base block"));
        }
        self.render_support(inputs, outputs)?;
        for ((delay, input), output) in self.base_delays.iter_mut().zip(base).zip(outputs) {
            for (value, sample) in output.iter_mut().zip(*input) {
                *value += delay.sample(*sample);
            }
        }
        Ok(())
    }

    /// Reset convolution, rate-converter, biquad and delay state.
    pub fn reset(&mut self) {
        self.kernel.reset();
        for filter in &mut self.pre {
            filter.reset();
        }
        for buffer in &mut self.input_buffers {
            buffer.fill(0.0);
        }
        for group in &mut self.groups {
            group.highpass.reset();
            group.delay.reset();
        }
        for buffer in &mut self.group_buffers {
            buffer.fill(0.0);
        }
        for filter in &mut self.fixed {
            filter.reset();
        }
        for delay in &mut self.base_delays {
            delay.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secs_dsp::iir::sosfilt;
    use secs_types::chain::{MainChannel, MimoPath, SubChannel};

    fn chain() -> SignalChain {
        SignalChain {
            input_hp_hz: None,
            sample_rate: 48_000,
            mains: ["L", "R"]
                .iter()
                .map(|role| MainChannel {
                    role: (*role).into(),
                    input_peq: vec![],
                    fir: vec![1.0],
                    gain_db: 0.0,
                    delay_ms: 0.0,
                    xo_hz: None,
                    top_peq: vec![],
                    align_delay_ms: 0.0,
                })
                .collect(),
            subs: vec![SubChannel {
                role: "SW1".into(),
                gain_db: 0.0,
                delay_ms: 0.0,
                polarity: 1,
                allpass: vec![],
                hp_hz: 23.0,
                hp_type: BandType::Bw12,
                lp_hz: 0.0,
                lp_type: BandType::Lr24,
                peq: vec![],
                mode: "auto".into(),
            }],
            sub_bus_peq: vec![],
            main_extra_ms: 0.0,
            lift_ms: 0.0,
            latency_ms: 0.0,
        }
    }
    fn direct(input: &[f64], taps: &[f64]) -> Vec<f64> {
        (0..input.len())
            .map(|n| {
                taps.iter()
                    .take(n + 1)
                    .enumerate()
                    .map(|(k, h)| h * input[n - k])
                    .sum()
            })
            .collect()
    }

    #[test]
    fn input_hp_playback_filters_raw_inputs_before_full_and_lowrate_support() {
        let off = chain();
        let mut on = off.clone();
        on.input_hp_hz = Some(27.0);
        let paths = MimoPaths {
            fsiso: 150.0,
            paths: ["L", "R"]
                .iter()
                .map(|input| MimoPath {
                    input: (*input).into(),
                    support: "SW1".into(),
                    low_hz: 30.0,
                    delay_ms: 1.0,
                    filter: MimoFilter::FirLowRate {
                        taps_low: vec![0.3, -0.1, 0.02],
                        fs_low: 600.0,
                    },
                })
                .collect(),
        };
        let inputs: Vec<Vec<_>> = (0..2)
            .map(|c| {
                (0..10240)
                    .map(|i| ((i * (c + 1)) as f64 / 130.0).sin())
                    .collect()
            })
            .collect();
        let hp = butter(8, &[27.0], BType::Highpass, SAMPLE_RATE).unwrap();
        let filtered: Vec<_> = inputs
            .iter()
            .map(|input| sosfilt(&hp, input).unwrap())
            .collect();
        for mode in [
            PlaybackMode::FullRate { block_size: 80 },
            PlaybackMode::Multirate {
                low_block_size: 1,
                factor: 80,
            },
        ] {
            let mut actual = MimoFirPlayback::new(&on, &paths, mode).unwrap();
            let mut reference = MimoFirPlayback::new(&off, &paths, mode).unwrap();
            let mut max_abs = 0.0_f64;
            let mut previous = Vec::new();
            for pass in 0..2 {
                let mut rendered = Vec::new();
                for i in (0..10240).step_by(80) {
                    let input: Vec<_> = inputs.iter().map(|x| &x[i..i + 80]).collect();
                    let expected_input: Vec<_> = filtered.iter().map(|x| &x[i..i + 80]).collect();
                    let mut a = vec![vec![0.0; 80]; 3];
                    let mut b = a.clone();
                    actual
                        .process(
                            &input,
                            &mut a.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>(),
                        )
                        .unwrap();
                    reference
                        .process(
                            &expected_input,
                            &mut b.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>(),
                        )
                        .unwrap();
                    for (a, b) in a.iter().flatten().zip(b.iter().flatten()) {
                        max_abs = max_abs.max((a - b).abs());
                    }
                    rendered.extend_from_slice(&a[2]);
                }
                if pass == 0 {
                    previous = rendered;
                } else {
                    assert_eq!(previous, rendered);
                }
                actual.reset();
                reference.reset();
            }
            eprintln!("input_hp playback {mode:?} max_abs={max_abs:.12e}");
            assert!(max_abs < 2e-12);
        }
    }

    #[test]
    fn playback_groups_match_per_path_reference_and_reset() {
        let chain = chain();
        let paths = MimoPaths {
            fsiso: 150.0,
            paths: [("L", 40.12345, 2.5), ("R", 40.12345, 2.5), ("L", 55.0, 5.5)]
                .iter()
                .map(|&(input, low_hz, samples)| MimoPath {
                    input: input.into(),
                    support: "SW1".into(),
                    low_hz,
                    delay_ms: samples / 48.0,
                    filter: MimoFilter::Fir {
                        taps_48k: vec![0.17, -0.04, 0.01, 0.003],
                    },
                })
                .collect(),
        };
        let mut renderer =
            MimoFirPlayback::new(&chain, &paths, PlaybackMode::FullRate { block_size: 32 })
                .unwrap();
        assert_eq!(renderer.transforms_per_block(), (2, 2));
        let input: Vec<Vec<f64>> = (0..2)
            .map(|c| {
                (0..1024)
                    .map(|n| ((n * (c + 1)) as f64 / 13.0).sin() * 0.5)
                    .collect()
            })
            .collect();
        let mut expected = vec![0.0; 1024];
        for path in &paths.paths {
            let MimoFilter::Fir { taps_48k } = &path.filter else {
                unreachable!()
            };
            let index = usize::from(path.input == "R");
            let mut response = direct(&input[index], taps_48k);
            let hp = butter(
                2,
                &[round_ndigits(path.low_hz, 3)],
                BType::Highpass,
                SAMPLE_RATE,
            )
            .unwrap();
            response = sosfilt(&hp, &sosfilt(&hp, &response).unwrap()).unwrap();
            let delay = round_ties_even_i64(path.delay_ms * SAMPLE_RATE / 1000.0) as usize;
            for n in delay..expected.len() {
                expected[n] += response[n - delay];
            }
        }
        expected = sosfilt(
            &butter(2, &[23.0], BType::Highpass, SAMPLE_RATE).unwrap(),
            &expected,
        )
        .unwrap();
        let mut first = Vec::new();
        for pass in 0..2 {
            let mut rendered = Vec::new();
            for n in (0..1024).step_by(32) {
                let mut outputs = [vec![0.0; 32], vec![0.0; 32], vec![0.0; 32]];
                let mut views: Vec<_> = outputs.iter_mut().map(Vec::as_mut_slice).collect();
                renderer
                    .process(&[&input[0][n..n + 32], &input[1][n..n + 32]], &mut views)
                    .unwrap();
                assert!(outputs[..2].iter().flatten().all(|x| *x == 0.0));
                rendered.extend_from_slice(&outputs[2]);
            }
            for (a, b) in rendered.iter().zip(&expected) {
                assert!((a - b).abs() < 2e-12, "{a} != {b}");
            }
            if pass == 0 {
                first = rendered;
            } else {
                assert_eq!(first, rendered);
            }
            renderer.reset();
        }
    }

    #[test]
    fn playback_keeps_half_sample_writer_rounding_and_rejects_blocks_before_advancing() {
        let chain = chain();
        let paths = MimoPaths {
            fsiso: 150.0,
            paths: vec![MimoPath {
                input: "L".into(),
                support: "R".into(),
                low_hz: 0.0,
                delay_ms: 5.427083333333334,
                filter: MimoFilter::Fir {
                    taps_48k: vec![1.0],
                },
            }],
        };
        // Multiplying milliseconds by 48 rounds this boundary to 260 instead.
        let mut renderer =
            MimoFirPlayback::new(&chain, &paths, PlaybackMode::FullRate { block_size: 32 })
                .unwrap();
        let mut input = vec![0.0; 32];
        input[0] = 1.0;
        let zero = vec![0.0; 32];
        let bad_base = vec![f64::NAN; 32];
        let mut output = vec![vec![0.0; 32]; 3];
        assert!(
            renderer
                .process_with_base(
                    &[&input, &zero],
                    &[&bad_base, &zero, &zero],
                    &mut output.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>()
                )
                .is_err()
        );
        for block in 0..10 {
            renderer
                .process_with_base(
                    &[&input, &zero],
                    &[&zero, &zero, &zero],
                    &mut output.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>(),
                )
                .unwrap();
            for (i, value) in output[1].iter().enumerate() {
                let expected = if block * 32 + i == 261 { 1.0 } else { 0.0 };
                assert!((value - expected).abs() < 1e-15);
            }
            input.fill(0.0);
        }
    }

    #[test]
    fn playback_lowrate_materialization_and_base_alignment_are_explicit() {
        let chain = chain();
        let paths = MimoPaths {
            fsiso: 150.0,
            paths: vec![MimoPath {
                input: "L".into(),
                support: "R".into(),
                low_hz: 0.0,
                delay_ms: 0.0,
                filter: MimoFilter::FirLowRate {
                    taps_low: vec![0.3, -0.1],
                    fs_low: 600.0,
                },
            }],
        };
        let mut full =
            MimoFirPlayback::new(&chain, &paths, PlaybackMode::FullRate { block_size: 80 })
                .unwrap();
        let mut expanded = paths.clone();
        expanded.paths[0].filter = MimoFilter::Fir {
            taps_48k: lowrate_to_48k(&[0.3, -0.1], 600.0),
        };
        let mut legacy =
            MimoFirPlayback::new(&chain, &expanded, PlaybackMode::FullRate { block_size: 80 })
                .unwrap();
        let mut impulse = vec![0.0; 80];
        impulse[0] = 1.0;
        let zero = vec![0.0; 80];
        let mut a = vec![vec![0.0; 80]; 3];
        let mut b = a.clone();
        full.process(
            &[&impulse, &zero],
            &mut a.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>(),
        )
        .unwrap();
        legacy
            .process(
                &[&impulse, &zero],
                &mut b.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>(),
            )
            .unwrap();
        assert_eq!(a, b);
        assert_eq!(full.additional_latency_samples(), 0);
        assert!(
            MimoFirPlayback::new(
                &chain,
                &expanded,
                PlaybackMode::Multirate {
                    low_block_size: 1,
                    factor: 80
                }
            )
            .is_err()
        );
        let mut low = MimoFirPlayback::new(
            &chain,
            &paths,
            PlaybackMode::Multirate {
                low_block_size: 1,
                factor: 80,
            },
        )
        .unwrap();
        assert_eq!(low.additional_latency_samples(), 5120);
        for n in 0..66 {
            let base = if n == 0 { &impulse } else { &zero };
            let mut out = vec![vec![0.0; 80]; 3];
            low.process_with_base(
                &[&zero, &zero],
                &[base, &zero, &zero],
                &mut out.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>(),
            )
            .unwrap();
            for (i, value) in out[0].iter().enumerate() {
                assert_eq!(*value, if n * 80 + i == 5120 { 1.0 } else { 0.0 });
            }
        }
        let mut bad = paths.clone();
        bad.paths[0].input = "unknown".into();
        assert!(
            MimoFirPlayback::new(&chain, &bad, PlaybackMode::FullRate { block_size: 80 }).is_err()
        );
        let empty = MimoPaths {
            fsiso: 150.0,
            paths: vec![],
        };
        let mut bypass = MimoFirPlayback::new(
            &chain,
            &empty,
            PlaybackMode::Multirate {
                low_block_size: 1,
                factor: 80,
            },
        )
        .unwrap();
        assert_eq!(bypass.transforms_per_block(), (0, 0));
        assert_eq!(bypass.additional_latency_samples(), 0);
        let mut out = vec![vec![0.0; 80]; 3];
        bypass
            .process_with_base(
                &[&zero, &zero],
                &[&impulse, &zero, &zero],
                &mut out.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>(),
            )
            .unwrap();
        assert_eq!(out[0], impulse);
    }
}
