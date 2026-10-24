//! Streaming matrix FIR convolution with shared input transforms.
//!
//! Every FIR is partitioned into `block_size` samples. One forward transform
//! per input supplies all its routes, and one inverse transform per output
//! combines every route before overlap-add. Processing reuses all buffers.

use crate::{Complex64 as C, DspError, invalid};
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub struct FirRoute {
    pub input: usize,
    pub output: usize,
    pub taps: Vec<f64>,
}

struct PartitionedRoute {
    input: usize,
    output: usize,
    partitions: Vec<Vec<C>>,
}

pub struct MatrixConvolver {
    inputs: usize,
    outputs: usize,
    block: usize,
    partitions: usize,
    tail: usize,
    head: usize,
    routes: Vec<PartitionedRoute>,
    history: Vec<Vec<Vec<C>>>,
    overlap: Vec<Vec<f64>>,
    forward: Arc<dyn RealToComplex<f64>>,
    inverse: Arc<dyn ComplexToReal<f64>>,
    time: Vec<f64>,
    spectrum: Vec<C>,
    scratch: Vec<C>,
}

impl std::fmt::Debug for MatrixConvolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MatrixConvolver")
            .field("inputs", &self.inputs)
            .field("outputs", &self.outputs)
            .field("block_size", &self.block)
            .field("partitions", &self.partitions)
            .field("routes", &self.routes.len())
            .field("tail_samples", &self.tail)
            .finish_non_exhaustive()
    }
}

impl MatrixConvolver {
    pub fn new(
        inputs: usize,
        outputs: usize,
        block_size: usize,
        routes: &[FirRoute],
    ) -> Result<Self, DspError> {
        let nfft = block_size
            .checked_mul(2)
            .ok_or_else(|| invalid("convolution block size overflow"))?;
        if inputs == 0 || outputs == 0 || block_size == 0 {
            return Err(invalid(
                "convolution channel and block counts must be positive",
            ));
        }
        if routes.iter().any(|r| {
            r.input >= inputs
                || r.output >= outputs
                || r.taps.is_empty()
                || r.taps.iter().any(|x| !x.is_finite())
        }) {
            return Err(invalid("invalid convolution route or coefficients"));
        }
        let longest = routes.iter().map(|r| r.taps.len()).max().unwrap_or(1);
        let partitions = longest.div_ceil(block_size);
        inputs
            .checked_mul(partitions)
            .and_then(|v| v.checked_mul(block_size + 1))
            .and_then(|v| v.checked_mul(std::mem::size_of::<C>()))
            .ok_or_else(|| invalid("convolution history size overflow"))?;
        outputs
            .checked_mul(block_size)
            .and_then(|v| v.checked_mul(std::mem::size_of::<f64>()))
            .ok_or_else(|| invalid("convolution output size overflow"))?;
        let mut planner = RealFftPlanner::<f64>::new();
        let forward = planner.plan_fft_forward(nfft);
        let inverse = planner.plan_fft_inverse(nfft);
        let mut time = forward.make_input_vec();
        let mut scratch =
            vec![C::default(); forward.get_scratch_len().max(inverse.get_scratch_len())];
        let mut prepared = Vec::with_capacity(routes.len());
        for route in routes {
            let mut spectra = Vec::with_capacity(route.taps.len().div_ceil(block_size));
            for taps in route.taps.chunks(block_size) {
                time.fill(0.0);
                time[..taps.len()].copy_from_slice(taps);
                let mut spectrum = forward.make_output_vec();
                forward
                    .process_with_scratch(&mut time, &mut spectrum, &mut scratch)
                    .map_err(|e| invalid(&format!("convolution forward transform: {e}")))?;
                spectra.push(spectrum);
            }
            prepared.push(PartitionedRoute {
                input: route.input,
                output: route.output,
                partitions: spectra,
            });
        }
        Ok(Self {
            inputs,
            outputs,
            block: block_size,
            partitions,
            tail: longest - 1,
            head: 0,
            routes: prepared,
            history: vec![vec![vec![C::default(); block_size + 1]; partitions]; inputs],
            overlap: vec![vec![0.0; block_size]; outputs],
            spectrum: forward.make_output_vec(),
            forward,
            inverse,
            time,
            scratch,
        })
    }

    /// Process exactly one block. Output channels are overwritten, including
    /// outputs with no routes. Invalid dimensions leave streaming state intact.
    pub fn process(
        &mut self,
        inputs: &[&[f64]],
        outputs: &mut [&mut [f64]],
    ) -> Result<(), DspError> {
        self.process_buffers(inputs, outputs)
    }

    /// Buffer-owning counterpart to [`Self::process`], so an adapter can pass
    /// retained `Vec<f64>` channel buffers without allocating a view vector.
    pub fn process_buffers<I: AsRef<[f64]>, O: AsMut<[f64]>>(
        &mut self,
        inputs: &[I],
        outputs: &mut [O],
    ) -> Result<(), DspError> {
        if inputs.len() != self.inputs
            || outputs.len() != self.outputs
            || inputs.iter().any(|x| {
                x.as_ref().len() != self.block || x.as_ref().iter().any(|v| !v.is_finite())
            })
            || outputs.iter_mut().any(|y| y.as_mut().len() != self.block)
        {
            return Err(invalid("invalid convolution input/output block"));
        }
        for (input, history) in inputs.iter().zip(&mut self.history) {
            self.time[..self.block].copy_from_slice(input.as_ref());
            self.time[self.block..].fill(0.0);
            self.forward
                .process_with_scratch(&mut self.time, &mut history[self.head], &mut self.scratch)
                .map_err(|e| invalid(&format!("convolution forward transform: {e}")))?;
        }
        for (output, destination) in outputs.iter_mut().enumerate() {
            self.spectrum.fill(C::default());
            for route in self.routes.iter().filter(|r| r.output == output) {
                for (partition, filter) in route.partitions.iter().enumerate() {
                    let slot = (self.head + self.partitions - partition) % self.partitions;
                    let input = &self.history[route.input][slot];
                    for ((sum, x), h) in self.spectrum.iter_mut().zip(input).zip(filter) {
                        *sum += *x * *h;
                    }
                }
            }
            self.spectrum[0].im = 0.0;
            self.spectrum[self.block].im = 0.0;
            self.inverse
                .process_with_scratch(&mut self.spectrum, &mut self.time, &mut self.scratch)
                .map_err(|e| invalid(&format!("convolution inverse transform: {e}")))?;
            let normalization = (2 * self.block) as f64;
            for (k, y) in destination.as_mut().iter_mut().enumerate() {
                *y = self.time[k] / normalization + self.overlap[output][k];
                self.overlap[output][k] = self.time[self.block + k] / normalization;
            }
        }
        self.head = (self.head + 1) % self.partitions;
        Ok(())
    }

    pub fn reset(&mut self) {
        self.head = 0;
        for channel in &mut self.history {
            for partition in channel {
                partition.fill(C::default());
            }
        }
        for channel in &mut self.overlap {
            channel.fill(0.0);
        }
        self.time.fill(0.0);
        self.spectrum.fill(C::default());
    }

    pub fn block_size(&self) -> usize {
        self.block
    }
    pub fn input_count(&self) -> usize {
        self.inputs
    }
    pub fn output_count(&self) -> usize {
        self.outputs
    }
    /// Samples to flush after the final input sample.
    pub fn tail_samples(&self) -> usize {
        self.tail
    }
    /// Forward and inverse transform counts, independent of the route count.
    pub fn transforms_per_block(&self) -> (usize, usize) {
        (self.inputs, self.outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_matrix_partitions_match_direct_across_blocks_and_reset() {
        let routes = vec![
            FirRoute {
                input: 0,
                output: 0,
                taps: vec![0.7, -0.2, 0.1],
            },
            FirRoute {
                input: 0,
                output: 1,
                taps: (0..31).map(|k| (k as f64).cos() / 31.0).collect(),
            },
            FirRoute {
                input: 1,
                output: 0,
                taps: (0..19).map(|k| (k as f64).sin() / 19.0).collect(),
            },
            FirRoute {
                input: 1,
                output: 0,
                taps: vec![0.03; 13],
            },
        ];
        for block in [1, 7, 16, 32] {
            let n = block * 8;
            let inputs: Vec<Vec<_>> = (0..2)
                .map(|i| {
                    (0..n)
                        .map(|k| {
                            if k < n / 2 {
                                ((k * 17 + i) as f64).sin()
                            } else {
                                0.0
                            }
                        })
                        .collect()
                })
                .collect();
            let mut expected = vec![vec![0.0; n]; 3];
            for route in &routes {
                for k in 0..n {
                    for (j, h) in route.taps.iter().enumerate().take(k + 1) {
                        expected[route.output][k] += inputs[route.input][k - j] * h;
                    }
                }
            }
            let mut convolver = MatrixConvolver::new(2, 3, block, &routes).unwrap();
            assert_eq!(convolver.transforms_per_block(), (2, 3));
            assert_eq!(convolver.tail_samples(), 30);
            for _ in 0..2 {
                let mut got = vec![vec![0.0; n]; 3];
                for offset in (0..n).step_by(block) {
                    let x: Vec<_> = inputs.iter().map(|x| &x[offset..offset + block]).collect();
                    let mut y: Vec<_> = got
                        .iter_mut()
                        .map(|y| &mut y[offset..offset + block])
                        .collect();
                    convolver.process(&x, &mut y).unwrap();
                }
                for (a, b) in got.iter().flatten().zip(expected.iter().flatten()) {
                    assert!((a - b).abs() < 2e-12, "block={block}: {a} vs {b}");
                }
                convolver.reset();
            }
        }
    }

    #[test]
    fn convolution_rejects_invalid_data_without_consuming_a_block() {
        let route = FirRoute {
            input: 0,
            output: 0,
            taps: vec![1.0, 0.5],
        };
        assert!(MatrixConvolver::new(0, 1, 8, std::slice::from_ref(&route)).is_err());
        assert!(MatrixConvolver::new(1, 1, 0, std::slice::from_ref(&route)).is_err());
        assert!(MatrixConvolver::new(1, 1, usize::MAX, std::slice::from_ref(&route)).is_err());
        assert!(
            MatrixConvolver::new(
                1,
                1,
                8,
                &[FirRoute {
                    taps: vec![f64::NAN],
                    ..route.clone()
                }]
            )
            .is_err()
        );
        let mut convolver = MatrixConvolver::new(1, 1, 8, &[route]).unwrap();
        let mut output = [0.0; 8];
        assert!(convolver.process(&[&[1.0; 7]], &mut [&mut output]).is_err());
        assert!(
            convolver
                .process(&[&[f64::NAN; 8]], &mut [&mut output])
                .is_err()
        );
        convolver
            .process(
                &[&[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]],
                &mut [&mut output],
            )
            .unwrap();
        assert!((output[0] - 1.0).abs() < 1e-12 && (output[1] - 0.5).abs() < 1e-12);
    }
}
