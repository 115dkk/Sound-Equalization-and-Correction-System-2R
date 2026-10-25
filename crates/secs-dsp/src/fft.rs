//! NumPy-compatible FFTs over cached `realfft`/`rustfft` plans, at most 16
//! of each kind per thread.
//!
//! From Impulcifer-py313 `crates/impulcifer-dsp/src/fft.rs` (MIT, Copyright (c)
//! 2024- 115dkk); see docs/THIRD_PARTY.md. Infallible transforms panic on zero
//! lengths; nonfinite samples propagate through arithmetic, as in NumPy.
//!
//! The signatures here are shared by `secs-mdat` and the engine; change them
//! only together with their callers.

use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
pub use rustfft::num_complex::Complex64;
use rustfft::{Fft, FftPlanner};
use std::{cell::RefCell, sync::Arc};

/// Plans each thread keeps per kind (real forward, real inverse, complex
/// forward, complex inverse): the most recently used lengths (TD-97).
/// Resampling plans FFTs at source lengths, so an unbounded cache grows with
/// every length a long session meets.
pub(crate) const PLAN_CACHE: usize = 16;

type RealPlans =
    Plans<RealFftPlanner<f64>, Arc<dyn RealToComplex<f64>>, Arc<dyn ComplexToReal<f64>>>;
type ComplexPlans = Plans<FftPlanner<f64>, Arc<dyn Fft<f64>>, Arc<dyn Fft<f64>>>;

thread_local! {
    // Work buffers retain only their largest capacity per thread, rather than
    // one allocation per cached transform length.
    static REAL_PLANS: RefCell<RealPlans> = RefCell::new(Plans::new(RealFftPlanner::new));
    static REAL_WORK: RefCell<RealWork> = RefCell::new(RealWork::default());
    static COMPLEX_PLANS: RefCell<ComplexPlans> = RefCell::new(Plans::new(FftPlanner::new));
}

/// The most recently used plans of each direction in front of one planner.
///
/// `realfft`'s and `rustfft`'s planners keep every length they ever planned,
/// so the planner is replaced whenever a cache evicts a plan: afterwards it
/// holds only plans made since, all of them still cached here, and dropping
/// the evicted plan frees it. A planner picks its algorithm from the length
/// and the CPU alone, so a replaced planner gives the same plans and results.
struct Plans<Planner, F, I> {
    planner: Planner,
    renew: fn() -> Planner,
    /// Lengths the current planner holds.
    held: usize,
    forward: Vec<(usize, F)>,
    inverse: Vec<(usize, I)>,
}

impl<Planner, F: Clone, I: Clone> Plans<Planner, F, I> {
    fn new(renew: fn() -> Planner) -> Self {
        Self {
            planner: renew(),
            renew,
            held: 0,
            forward: Vec::new(),
            inverse: Vec::new(),
        }
    }

    fn forward(&mut self, len: usize, make: impl FnOnce(&mut Planner, usize) -> F) -> F {
        let Self {
            planner,
            renew,
            held,
            forward,
            ..
        } = self;
        cached(forward, len, planner, *renew, held, make)
    }

    fn inverse(&mut self, len: usize, make: impl FnOnce(&mut Planner, usize) -> I) -> I {
        let Self {
            planner,
            renew,
            held,
            inverse,
            ..
        } = self;
        cached(inverse, len, planner, *renew, held, make)
    }
}

/// The plan for `len` from `recent` (least recent first), which it moves to
/// the end, or a new one from `planner`. A full cache drops its least recent
/// plan and replaces the planner before planning.
fn cached<Planner, P: Clone>(
    recent: &mut Vec<(usize, P)>,
    len: usize,
    planner: &mut Planner,
    renew: fn() -> Planner,
    held: &mut usize,
    make: impl FnOnce(&mut Planner, usize) -> P,
) -> P {
    if let Some(i) = recent.iter().position(|(n, _)| *n == len) {
        let entry = recent.remove(i);
        let plan = entry.1.clone();
        recent.push(entry);
        return plan;
    }
    if recent.len() >= PLAN_CACHE {
        recent.remove(0);
        *planner = renew();
        *held = 0;
    }
    let plan = make(planner, len);
    *held += 1;
    recent.push((len, plan.clone()));
    plan
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
    let plan = real_forward(x.len());
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
    let plan = real_forward(x.len());
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
    let plan = REAL_PLANS.with(|p| {
        p.borrow_mut()
            .inverse(n, |planner, n| planner.plan_fft_inverse(n))
    });
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
    COMPLEX_PLANS
        .with(|p| {
            p.borrow_mut()
                .forward(x.len(), |planner, n| planner.plan_fft_forward(n))
        })
        .process(&mut result);
    result
}

/// Complex inverse normalized by 1/N. Panics on empty input.
pub fn ifft(x: &[Complex64]) -> Vec<Complex64> {
    assert!(!x.is_empty(), "FFT length must be positive");
    let mut result = x.to_vec();
    COMPLEX_PLANS
        .with(|p| {
            p.borrow_mut()
                .inverse(x.len(), |planner, n| planner.plan_fft_inverse(n))
        })
        .process(&mut result);
    result.iter_mut().for_each(|v| *v /= x.len() as f64);
    result
}

fn real_forward(len: usize) -> Arc<dyn RealToComplex<f64>> {
    REAL_PLANS.with(|p| {
        p.borrow_mut()
            .forward(len, |planner, n| planner.plan_fft_forward(n))
    })
}

/// What this thread's caches hold: real forward and inverse, complex forward
/// and inverse plans, then the lengths the real and complex planners keep.
#[cfg(test)]
fn plan_counts() -> [usize; 6] {
    let real = REAL_PLANS.with(|p| {
        let p = p.borrow();
        [p.forward.len(), p.inverse.len(), p.held]
    });
    let complex = COMPLEX_PLANS.with(|p| {
        let p = p.borrow();
        [p.forward.len(), p.inverse.len(), p.held]
    });
    [
        real[0], real[1], complex[0], complex[1], real[2], complex[2],
    ]
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

    fn bits(x: &[Complex64]) -> Vec<(u64, u64)> {
        x.iter().map(|z| (z.re.to_bits(), z.im.to_bits())).collect()
    }

    /// The transforms a fresh planner gives for `len`: no cache in front.
    fn uncached(
        x: &[f64],
        z: &[Complex64],
    ) -> (Vec<Complex64>, Vec<f64>, Vec<Complex64>, Vec<Complex64>) {
        let n = x.len();
        let mut real = RealFftPlanner::<f64>::new();
        let forward = real.plan_fft_forward(n);
        let mut spectrum = forward.make_output_vec();
        forward.process(&mut x.to_vec(), &mut spectrum).unwrap();
        let inverse = real.plan_fft_inverse(n);
        let mut hermitian = spectrum.clone();
        hermitian[0].im = 0.0;
        if n.is_multiple_of(2) {
            hermitian[n / 2].im = 0.0;
        }
        let mut back = inverse.make_output_vec();
        inverse.process(&mut hermitian, &mut back).unwrap();
        back.iter_mut().for_each(|v| *v /= n as f64);
        let mut complex = FftPlanner::<f64>::new();
        let mut forward = z.to_vec();
        complex.plan_fft_forward(n).process(&mut forward);
        let mut inverse = z.to_vec();
        complex.plan_fft_inverse(n).process(&mut inverse);
        inverse.iter_mut().for_each(|v| *v /= n as f64);
        (spectrum, back, forward, inverse)
    }

    #[test]
    fn plan_cache_is_bounded_and_bit_identical() {
        // TD-97: a fresh thread starts with empty caches.
        std::thread::spawn(|| {
            let signal = |n: usize| -> (Vec<f64>, Vec<Complex64>) {
                let x: Vec<f64> = (0..n).map(|i| ((i * 37 % 11) as f64 - 5.0) / 3.0).collect();
                let z = x.iter().map(|&v| Complex64::new(v, 0.5 - v)).collect();
                (x, z)
            };
            // 100 distinct lengths, then the first ones again after eviction.
            for n in (2..102).chain(2..6) {
                let (x, z) = signal(n);
                let spectrum = rfft(&x);
                let (expected, back, forward, inverse) = uncached(&x, &z);
                assert_eq!(bits(&spectrum), bits(&expected), "rfft {n}");
                let got: Vec<_> = irfft(&spectrum, n).iter().map(|v| v.to_bits()).collect();
                assert_eq!(
                    got,
                    back.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    "irfft {n}"
                );
                assert_eq!(bits(&fft(&z)), bits(&forward), "fft {n}");
                assert_eq!(bits(&ifft(&z)), bits(&inverse), "ifft {n}");
                let counts = plan_counts();
                // A planner holds only plans made since it was last replaced,
                // and each of those is still cached.
                assert!(
                    counts[..4].iter().all(|&c| c <= PLAN_CACHE)
                        && counts[4] <= counts[0] + counts[1]
                        && counts[5] <= counts[2] + counts[3],
                    "after length {n}: plans {counts:?}, bound {PLAN_CACHE} per kind"
                );
            }
        })
        .join()
        .unwrap();
    }
}
