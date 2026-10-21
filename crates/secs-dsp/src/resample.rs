//! Rate conversion for discrete impulse responses, preserving transfer gain.
//!
//! Independent windowed-sinc/polyphase implementation. Mathematical references:
//! SciPy's resample_poly/upfirdn documentation and the symmetric four-term
//! Blackman-Harris window (0.35875, 0.48829, 0.14128, 0.01168).
//! [upfirdn](https://docs.scipy.org/doc/scipy/reference/generated/scipy.signal.upfirdn.html)
//! [Blackman-Harris](https://docs.scipy.org/doc/scipy/reference/generated/scipy.signal.windows.blackmanharris.html)
//!
//! Unlike waveform resampling, the FIR gain is `down`, not `up`: an IR's
//! discrete sum/transfer response must stay constant as sample density changes.
//! Both filter tails are retained. The caller subtracts `leading / output_hz`
//! from the returned samples' time origin.

use std::f64::consts::PI;

use crate::{DspError, invalid, pairwise_sum};

pub const MIN_RESAMPLE_RATE: u32 = 8_000;
pub const MAX_RESAMPLE_RATE: u32 = 384_000;
const HALF_WIDTH: usize = 96;
const CUTOFF: f64 = 0.96;

/// Allocation-free shape and timing of a finite IR conversion.
#[derive(Debug, Clone, Copy)]
pub struct IrResamplePlan {
    input_len: usize,
    up: usize,
    down: usize,
    half: usize,
    prepad: usize,
    output_len: usize,
    leading: usize,
}

impl IrResamplePlan {
    pub fn new(input_len: usize, input_hz: u32, output_hz: u32) -> Result<Self, DspError> {
        if input_len == 0 {
            return Err(invalid("IR resampling needs nonempty input"));
        }
        if !(MIN_RESAMPLE_RATE..=MAX_RESAMPLE_RATE).contains(&input_hz)
            || !(MIN_RESAMPLE_RATE..=MAX_RESAMPLE_RATE).contains(&output_hz)
        {
            return Err(invalid(
                "IR resampling supports sample rates from 8000 to 384000 Hz",
            ));
        }
        let mut a = input_hz;
        let mut b = output_hz;
        while b != 0 {
            (a, b) = (b, a % b);
        }
        let up = (output_hz / a) as usize;
        let down = (input_hz / a) as usize;
        if up == down {
            input_len
                .checked_mul(size_of::<f64>())
                .ok_or_else(|| invalid("IR resampling output byte size overflow"))?;
            return Ok(Self {
                input_len,
                up,
                down,
                half: 0,
                prepad: 0,
                output_len: input_len,
                leading: 0,
            });
        }
        let half = HALF_WIDTH
            .checked_mul(up.max(down))
            .ok_or_else(|| invalid("IR resampling kernel length overflow"))?;
        let prepad = (down - half % down) % down;
        let last = (input_len - 1)
            .checked_mul(up)
            .and_then(|n| n.checked_add(2 * half + prepad))
            .ok_or_else(|| invalid("IR resampling output length overflow"))?;
        let output_len = (last / down)
            .checked_add(1)
            .ok_or_else(|| invalid("IR resampling output length overflow"))?;
        output_len
            .checked_mul(size_of::<f64>())
            .ok_or_else(|| invalid("IR resampling output byte size overflow"))?;
        (2 * half + 1)
            .checked_mul(size_of::<f64>())
            .ok_or_else(|| invalid("IR resampling kernel byte size overflow"))?;
        Ok(Self {
            input_len,
            up,
            down,
            half,
            prepad,
            output_len,
            leading: (half + prepad) / down,
        })
    }

    pub fn output_len(&self) -> usize {
        self.output_len
    }

    pub fn leading_output_samples(&self) -> usize {
        self.leading
    }

    pub fn kernel_bytes(&self) -> usize {
        if self.half == 0 {
            0
        } else {
            (2 * self.half + 1) * size_of::<f64>()
        }
    }

    /// Read samples without an intermediate f32-to-f64 allocation. The reader
    /// supplies the same finite value for a given index throughout this call.
    /// `check` allows the engine to propagate its own cancellation error.
    pub fn process<E>(
        &self,
        sample: impl Fn(usize) -> f64,
        mut check: impl FnMut() -> Result<(), E>,
    ) -> Result<Vec<f64>, E>
    where
        E: From<DspError>,
    {
        check()?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(self.output_len)
            .map_err(|_| E::from(invalid("IR resampling output allocation failed")))?;
        if self.half == 0 {
            for n in 0..self.input_len {
                if n % 256 == 0 {
                    check()?;
                }
                output.push(sample(n));
            }
            check()?;
            return Ok(output);
        }
        let cutoff = CUTOFF / self.up.max(self.down) as f64;
        let mut kernel = Vec::new();
        kernel
            .try_reserve_exact(2 * self.half + 1)
            .map_err(|_| E::from(invalid("IR resampling kernel allocation failed")))?;
        for i in 0..=2 * self.half {
            if i % 256 == 0 {
                check()?;
            }
            let x = cutoff * (i as f64 - self.half as f64);
            let sinc = if x == 0.0 {
                1.0
            } else {
                (PI * x).sin() / (PI * x)
            };
            let phase = PI * i as f64 / self.half as f64;
            let window = 0.35875 - 0.48829 * phase.cos() + 0.14128 * (2.0 * phase).cos()
                - 0.01168 * (3.0 * phase).cos();
            kernel.push(cutoff * sinc * window);
        }
        check()?;
        let scale = self.down as f64 / pairwise_sum(&kernel);
        check()?;
        for (i, value) in kernel.iter_mut().enumerate() {
            if i % 256 == 0 {
                check()?;
            }
            *value *= scale;
        }
        for k in 0..self.output_len {
            if k % 256 == 0 {
                check()?;
            }
            let time = k * self.down;
            let mut value = 0.0;
            if time >= self.prepad {
                let time = time - self.prepad;
                let first = time.saturating_sub(2 * self.half).div_ceil(self.up);
                let last = (time / self.up).min(self.input_len - 1);
                for n in first..=last {
                    value += sample(n) * kernel[time - n * self.up];
                }
            }
            if !value.is_finite() {
                return Err(E::from(invalid(
                    "IR resampling produced a nonfinite sample",
                )));
            }
            output.push(value);
        }
        check()?;
        Ok(output)
    }
}
