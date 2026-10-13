//! NumPy-compatible FFTs over cached `realfft`/`rustfft` plans.
//!
//! From Impulcifer-py313 `crates/impulcifer-dsp/src/fft.rs` (MIT, Copyright (c)
//! 2024- 115dkk); see docs/THIRD_PARTY.md. Infallible transforms panic on zero
//! lengths; nonfinite samples propagate through arithmetic, as in NumPy.
//!
//! The signatures here are shared by `secs-mdat` and the engine; change them
//! only together with their callers.

use realfft::RealFftPlanner;
use rustfft::FftPlanner;
pub use rustfft::num_complex::Complex64;
use std::cell::RefCell;

thread_local! {
    // Plans are cached by length; work buffers retain only their largest capacity
    // per thread, rather than one allocation per cached transform length.
    static REAL_PLANNER: RefCell<RealFftPlanner<f64>> = RefCell::new(RealFftPlanner::new());
    static REAL_WORK: RefCell<RealWork> = RefCell::new(RealWork::default());
    static COMPLEX_PLANNER: RefCell<FftPlanner<f64>> = RefCell::new(FftPlanner::new());
}

#[derive(Default)]
struct RealWork {
    input: Vec<f64>,
    spectrum: Vec<Complex64>,
    scratch: Vec<Complex64>,
}

/// Unnormalized real transform, including the last bin for odd lengths.
pub fn rfft(x: &[f64]) -> Vec<Complex64> {
    assert!(!x.is_empty(), "FFT length must be positive");
    let plan = REAL_PLANNER.with(|p| p.borrow_mut().plan_fft_forward(x.len()));
    REAL_WORK.with(|work| {
        let mut work = work.borrow_mut();
        let RealWork { input, scratch, .. } = &mut *work;
        input.resize(x.len(), 0.0);
        input.copy_from_slice(x);
        scratch.resize(plan.get_scratch_len(), Complex64::default());
        let mut output = plan.make_output_vec();
        plan.process_with_scratch(input, &mut output, scratch)
            .expect("valid FFT buffers");
        output
    })
}

/// Internal consuming transform avoids copying freshly padded/centered data.
#[allow(dead_code)]
pub(crate) fn rfft_owned(mut x: Vec<f64>) -> Vec<Complex64> {
    assert!(!x.is_empty(), "FFT length must be positive");
    let plan = REAL_PLANNER.with(|p| p.borrow_mut().plan_fft_forward(x.len()));
    let mut output = plan.make_output_vec();
    REAL_WORK.with(|work| {
        let mut work = work.borrow_mut();
        work.scratch
            .resize(plan.get_scratch_len(), Complex64::default());
        plan.process_with_scratch(&mut x, &mut output, &mut work.scratch)
            .expect("valid FFT buffers");
    });
    output
}

/// Normalized real inverse with explicit original length. Like NumPy, crop or
/// zero-pad the spectrum, and ignore imaginary DC (and even-length Nyquist).
/// Panics if `n == 0` or the spectrum is empty. NumPy 2.4.6 empty-spectrum
/// output was allocator-dependent on the oracle machine, so reject it rather
/// than treating uninitialized values as a numerical contract.
pub fn irfft(spec: &[Complex64], n: usize) -> Vec<f64> {
    assert!(n > 0, "FFT length must be positive");
    assert!(!spec.is_empty(), "inverse FFT spectrum must be nonempty");
    let plan = REAL_PLANNER.with(|p| p.borrow_mut().plan_fft_inverse(n));
    let mut output = plan.make_output_vec();
    REAL_WORK.with(|work| {
        let mut work = work.borrow_mut();
        let RealWork {
            spectrum, scratch, ..
        } = &mut *work;
        spectrum.resize(n / 2 + 1, Complex64::default());
        let len = spectrum.len().min(spec.len());
        spectrum[..len].copy_from_slice(&spec[..len]);
        spectrum[len..].fill(Complex64::default());
        spectrum[0].im = 0.0;
        if n.is_multiple_of(2) {
            spectrum[n / 2].im = 0.0;
        }
        scratch.resize(plan.get_scratch_len(), Complex64::default());
        plan.process_with_scratch(spectrum, &mut output, scratch)
            .expect("valid Hermitian spectrum");
    });
    output.iter_mut().for_each(|v| *v /= n as f64);
    output
}

/// Unnormalized complex forward transform. Panics on empty input.
pub fn fft(x: &[Complex64]) -> Vec<Complex64> {
    assert!(!x.is_empty(), "FFT length must be positive");
    let mut result = x.to_vec();
    COMPLEX_PLANNER
        .with(|p| p.borrow_mut().plan_fft_forward(x.len()))
        .process(&mut result);
    result
}

/// Complex inverse normalized by 1/N. Panics on empty input.
pub fn ifft(x: &[Complex64]) -> Vec<Complex64> {
    assert!(!x.is_empty(), "FFT length must be positive");
    let mut result = x.to_vec();
    COMPLEX_PLANNER
        .with(|p| p.borrow_mut().plan_fft_inverse(x.len()))
        .process(&mut result);
    result.iter_mut().for_each(|v| *v /= x.len() as f64);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfft_irfft_round_trip_even_and_odd() {
        for n in [1_usize, 2, 7, 8, 1000, 1001] {
            let x: Vec<f64> = (0..n).map(|i| ((i * 37 % 11) as f64 - 5.0) / 3.0).collect();
            let spectrum = rfft(&x);
            assert_eq!(spectrum.len(), n / 2 + 1);
            let back = irfft(&spectrum, n);
            for (a, b) in x.iter().zip(&back) {
                assert!((a - b).abs() < 1e-12, "n={n}: {a} vs {b}");
            }
        }
    }
}
