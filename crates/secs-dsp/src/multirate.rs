//! Experimental low-rate matrix convolution with shared rate converters.
//!
//! This causal anti-alias / interpolation cascade has a different response
//! from the historical edge-tapered, cropped 48 kHz FIR materialization.
//! Applications must opt into it and align other signal paths by the exposed
//! rate-converter delay. It is not a transparent replacement for that FIR.

use crate::{
    DspError,
    convolution::{FirRoute, MatrixConvolver},
    fir::firwin,
    invalid,
};

#[derive(Debug)]
pub struct MultirateMatrixConvolver {
    factor: usize,
    block: usize,
    prototype: Vec<f64>,
    low: MatrixConvolver,
    down_history: Vec<Vec<f64>>,
    down_head: usize,
    up_history: Vec<Vec<f64>>,
    up_head: usize,
    low_inputs: Vec<Vec<f64>>,
    low_outputs: Vec<Vec<f64>>,
    tail: usize,
}

impl MultirateMatrixConvolver {
    /// Hamming-windowed `64 * factor + 1` tap prototype with cutoff at
    /// 0.4 times the low sample rate. Route taps are at that low rate.
    pub fn new(
        inputs: usize,
        outputs: usize,
        low_block_size: usize,
        factor: usize,
        routes: &[FirRoute],
    ) -> Result<Self, DspError> {
        if factor == 0 || low_block_size == 0 || inputs == 0 || outputs == 0 {
            return Err(invalid("multirate sizes must be positive"));
        }
        let length = factor
            .checked_mul(64)
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| invalid("multirate prototype size overflow"))?;
        low_block_size
            .checked_mul(factor)
            .ok_or_else(|| invalid("multirate block size overflow"))?;
        length
            .checked_mul(std::mem::size_of::<f64>())
            .ok_or_else(|| invalid("multirate prototype size overflow"))?;
        let mut prototype = firwin(length, 0.4 * (48000.0 / factor as f64), 48000.0)?;
        // Window evaluation can differ in its last bits at mirrored indices.
        // The new rate-converter contract uses exactly symmetric coefficients.
        for k in 0..length / 2 {
            let value = (prototype[k] + prototype[length - 1 - k]) * 0.5;
            prototype[k] = value;
            prototype[length - 1 - k] = value;
        }
        Self::with_prototype(inputs, outputs, low_block_size, factor, routes, &prototype)
    }

    /// Use an explicit odd, symmetric anti-alias prototype for both rate
    /// converters. Its DC gain is used as supplied; interpolation gains by
    /// `factor`. Symmetry makes the reported integer alignment delay valid.
    pub fn with_prototype(
        inputs: usize,
        outputs: usize,
        low_block_size: usize,
        factor: usize,
        routes: &[FirRoute],
        prototype: &[f64],
    ) -> Result<Self, DspError> {
        if factor == 0 || low_block_size == 0 || inputs == 0 || outputs == 0 {
            return Err(invalid("multirate sizes must be positive"));
        }
        if prototype.is_empty()
            || prototype.len().is_multiple_of(2)
            || prototype.iter().any(|v| !v.is_finite())
            || prototype.iter().zip(prototype.iter().rev()).any(|(a, b)| {
                (a - b).abs() > 8.0 * f64::EPSILON * a.abs().max(b.abs()).max(f64::MIN_POSITIVE)
            })
        {
            return Err(invalid(
                "multirate prototype must be finite, odd and symmetric",
            ));
        }
        let block = low_block_size
            .checked_mul(factor)
            .ok_or_else(|| invalid("multirate block size overflow"))?;
        let up_length = prototype.len().div_ceil(factor);
        inputs
            .checked_mul(prototype.len())
            .and_then(|n| n.checked_mul(8))
            .ok_or_else(|| invalid("multirate decimator size overflow"))?;
        outputs
            .checked_mul(up_length)
            .and_then(|n| n.checked_mul(8))
            .ok_or_else(|| invalid("multirate interpolator size overflow"))?;
        let low = MatrixConvolver::new(inputs, outputs, low_block_size, routes)?;
        let tail = low
            .tail_samples()
            .checked_mul(factor)
            .and_then(|n| {
                (prototype.len() - 1)
                    .checked_mul(2)
                    .and_then(|m| n.checked_add(m))
            })
            .ok_or_else(|| invalid("multirate tail size overflow"))?;
        Ok(Self {
            factor,
            block,
            prototype: prototype.to_vec(),
            low,
            down_history: vec![vec![0.0; prototype.len()]; inputs],
            down_head: 0,
            up_history: vec![vec![0.0; up_length]; outputs],
            up_head: 0,
            low_inputs: vec![vec![0.0; low_block_size]; inputs],
            low_outputs: vec![vec![0.0; low_block_size]; outputs],
            tail,
        })
    }

    /// Process one high-rate block without allocation. Decimation computes
    /// only retained samples; interpolation visits only nonzero polyphases.
    pub fn process(
        &mut self,
        inputs: &[&[f64]],
        outputs: &mut [&mut [f64]],
    ) -> Result<(), DspError> {
        self.process_buffers(inputs, outputs)
    }

    /// Process retained channel buffers without allocating slice-view arrays.
    pub fn process_buffers<I: AsRef<[f64]>, O: AsMut<[f64]>>(
        &mut self,
        inputs: &[I],
        outputs: &mut [O],
    ) -> Result<(), DspError> {
        if inputs.len() != self.input_count()
            || outputs.len() != self.output_count()
            || inputs.iter().any(|x| {
                x.as_ref().len() != self.block || x.as_ref().iter().any(|v| !v.is_finite())
            })
            || outputs.iter_mut().any(|y| y.as_mut().len() != self.block)
        {
            return Err(invalid("invalid multirate input/output block"));
        }
        let length = self.prototype.len();
        for t in 0..self.block {
            for (channel, input) in inputs.iter().enumerate() {
                self.down_history[channel][self.down_head] = input.as_ref()[t];
            }
            if t.is_multiple_of(self.factor) {
                for (channel, history) in self.down_history.iter().enumerate() {
                    let mut value = 0.0;
                    for (k, h) in self.prototype.iter().enumerate() {
                        value += h * history[(self.down_head + length - k) % length];
                    }
                    self.low_inputs[channel][t / self.factor] = value;
                }
            }
            self.down_head = (self.down_head + 1) % length;
        }
        self.low
            .process_buffers(&self.low_inputs, &mut self.low_outputs)?;
        let up_length = self.up_history[0].len();
        for t in 0..self.low.block_size() {
            for (channel, history) in self.up_history.iter_mut().enumerate() {
                history[self.up_head] = self.low_outputs[channel][t];
            }
            for phase in 0..self.factor {
                for (channel, output) in outputs.iter_mut().enumerate() {
                    let mut value = 0.0;
                    for k in (phase..length).step_by(self.factor) {
                        value += self.prototype[k]
                            * self.up_history[channel]
                                [(self.up_head + up_length - k / self.factor) % up_length];
                    }
                    output.as_mut()[t * self.factor + phase] = value * self.factor as f64;
                }
            }
            self.up_head = (self.up_head + 1) % up_length;
        }
        Ok(())
    }

    pub fn reset(&mut self) {
        self.low.reset();
        self.down_head = 0;
        self.up_head = 0;
        for history in &mut self.down_history {
            history.fill(0.0);
        }
        for history in &mut self.up_history {
            history.fill(0.0);
        }
        for block in &mut self.low_inputs {
            block.fill(0.0);
        }
        for block in &mut self.low_outputs {
            block.fill(0.0);
        }
    }

    pub fn block_size(&self) -> usize {
        self.block
    }
    pub fn low_block_size(&self) -> usize {
        self.low.block_size()
    }
    pub fn factor(&self) -> usize {
        self.factor
    }
    pub fn input_count(&self) -> usize {
        self.low.input_count()
    }
    pub fn output_count(&self) -> usize {
        self.low.output_count()
    }
    /// Two linear-phase prototype delays, in high-rate samples.
    pub fn latency_samples(&self) -> usize {
        self.prototype.len() - 1
    }
    /// Conservative high-rate tail length, including both rate converters.
    pub fn tail_samples(&self) -> usize {
        self.tail
    }
    pub fn transforms_per_block(&self) -> (usize, usize) {
        self.low.transforms_per_block()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn direct(x: &[f64], h: &[f64], n: usize) -> Vec<f64> {
        (0..n)
            .map(|t| {
                h.iter()
                    .enumerate()
                    .take(t + 1)
                    .filter(|(k, _)| t - k < x.len())
                    .map(|(k, h)| x[t - k] * h)
                    .sum()
            })
            .collect()
    }

    #[test]
    fn polyphase_stream_matches_explicit_filter_downsample_upsample_filter() {
        let (factor, n) = (3, 504);
        let prototype = [0.05, 0.1, 0.2, 0.3, 0.2, 0.1, 0.05];
        let routes = vec![
            FirRoute {
                input: 0,
                output: 0,
                taps: vec![1.0, -0.3, 0.2],
            },
            FirRoute {
                input: 1,
                output: 0,
                taps: vec![0.4; 19],
            },
            FirRoute {
                input: 0,
                output: 1,
                taps: vec![0.5; 13],
            },
        ];
        let inputs: Vec<Vec<_>> = (0..2)
            .map(|i| {
                (0..n)
                    .map(|k| {
                        if k < n / 2 {
                            ((k * 7 + i) as f64).sin()
                        } else {
                            0.0
                        }
                    })
                    .collect()
            })
            .collect();
        let down: Vec<Vec<_>> = inputs
            .iter()
            .map(|x| {
                direct(x, &prototype, n)
                    .into_iter()
                    .step_by(factor)
                    .collect()
            })
            .collect();
        let mut low = vec![vec![0.0; n / factor]; 2];
        for route in &routes {
            let filtered = direct(&down[route.input], &route.taps, n / factor);
            for (a, b) in low[route.output].iter_mut().zip(filtered) {
                *a += b;
            }
        }
        let expected: Vec<_> = low
            .iter()
            .map(|x| {
                let mut up = vec![0.0; n];
                for (t, x) in x.iter().enumerate() {
                    up[t * factor] = *x;
                }
                direct(&up, &prototype, n)
                    .into_iter()
                    .map(|y| y * factor as f64)
                    .collect::<Vec<_>>()
            })
            .collect();
        for block in [1, 7, 28] {
            let mut filter =
                MultirateMatrixConvolver::with_prototype(2, 2, block, factor, &routes, &prototype)
                    .unwrap();
            assert_eq!(filter.latency_samples(), 6);
            for _ in 0..2 {
                let mut got = vec![vec![0.0; n]; 2];
                for offset in (0..n).step_by(filter.block_size()) {
                    let end = offset + filter.block_size();
                    let x: Vec<_> = inputs.iter().map(|x| &x[offset..end]).collect();
                    let mut y: Vec<_> = got.iter_mut().map(|y| &mut y[offset..end]).collect();
                    filter.process(&x, &mut y).unwrap();
                }
                for (a, b) in got.iter().flatten().zip(expected.iter().flatten()) {
                    assert!((a - b).abs() < 2e-12);
                }
                filter.reset();
            }
        }
    }

    #[test]
    fn default_rate_converter_preserves_dc_rejects_aliases_and_reports_delay() {
        let route = FirRoute {
            input: 0,
            output: 0,
            taps: vec![1.0],
        };
        let (factor, block) = (80, 32);
        let mut filter = MultirateMatrixConvolver::new(1, 1, block, factor, &[route]).unwrap();
        assert_eq!(filter.latency_samples(), 5120);
        let n = filter.block_size();
        let mut output = vec![0.0; n];
        for _ in 0..8 {
            filter
                .process(&[&vec![1.0; n]], &mut [&mut output])
                .unwrap();
        }
        let dc = output.iter().sum::<f64>() / n as f64;
        assert!((dc - 1.0).abs() < 2e-4, "DC gain={dc}");
        assert!(output.iter().all(|y| (y - 1.0).abs() < 1e-3));
        filter.reset();
        for b in 0..8 {
            let input: Vec<_> = (0..n)
                .map(|t| (2.0 * std::f64::consts::PI * 550.0 * (b * n + t) as f64 / 48000.0).sin())
                .collect();
            filter.process(&[&input], &mut [&mut output]).unwrap();
        }
        let rms = (output.iter().map(|y| y * y).sum::<f64>() / n as f64).sqrt();
        assert!(rms < 1e-3, "550 Hz alias RMS={rms}");
        filter.reset();
        let mut impulse_response = Vec::new();
        for b in 0..5 {
            let mut input = vec![0.0; n];
            if b == 0 {
                input[0] = 1.0;
            }
            filter.process(&[&input], &mut [&mut output]).unwrap();
            impulse_response.extend_from_slice(&output);
        }
        let peak = impulse_response
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))
            .unwrap()
            .0;
        assert_eq!(peak, filter.latency_samples());
    }

    #[test]
    fn invalid_multirate_dimensions_and_prototypes_are_rejected() {
        assert!(MultirateMatrixConvolver::new(1, 1, 8, 0, &[]).is_err());
        assert!(MultirateMatrixConvolver::new(1, 1, usize::MAX, 80, &[]).is_err());
        for prototype in [&[][..], &[1.0, 0.0], &[1.0, 2.0, 0.0], &[f64::NAN]] {
            assert!(MultirateMatrixConvolver::with_prototype(1, 1, 8, 3, &[], prototype).is_err());
        }
        let mut filter = MultirateMatrixConvolver::with_prototype(1, 1, 8, 3, &[], &[1.0]).unwrap();
        let mut output = [0.0; 24];
        assert!(filter.process(&[&[0.0; 23]], &mut [&mut output]).is_err());
        assert!(
            filter
                .process(&[&[f64::NAN; 24]], &mut [&mut output])
                .is_err()
        );
    }
}
