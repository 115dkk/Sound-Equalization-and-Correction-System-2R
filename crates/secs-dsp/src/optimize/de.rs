//! Port of SciPy 1.18.1 `scipy/optimize/_differentialevolution.py`.
//! Copyright SciPy Developers, BSD-3-Clause (see the SciPy notice in
//! crates/secs-dsp/THIRD_PARTY_LICENSES.txt).
//! Only SECS 2's unconstrained, serial, immediate best1bin path is supported.

use crate::{DspError, invalid, numeric::sum, random::RandomState};

/// Options used by SECS 2 bass alignment. Remaining SciPy options are fixed:
/// best1bin, mutation=(0.5, 1), recombination=0.7, Latin hypercube, atol=0,
/// immediate updating, one worker, no polishing/constraints/integrality.
#[derive(Debug, Clone, Copy)]
pub struct DifferentialEvolutionOptions {
    /// Number of generations after the initial population evaluation.
    pub maxiter: usize,
    /// Population multiplier per non-fixed dimension (minimum population five).
    pub popsize: usize,
    /// Relative population-energy standard-deviation tolerance.
    pub tol: f64,
    /// Legacy RandomState integer seed, not a Generator seed.
    pub seed: u32,
}

impl Default for DifferentialEvolutionOptions {
    fn default() -> Self {
        Self {
            maxiter: 60,
            popsize: 12,
            tol: 1e-6,
            seed: 1,
        }
    }
}

/// SciPy's unpolished differential-evolution result.
#[derive(Debug, Clone)]
pub struct DifferentialEvolutionResult {
    pub x: Vec<f64>,
    pub fun: f64,
    pub nit: usize,
    pub nfev: usize,
    pub success: bool,
    pub message: String,
}

/// Minimize a scalar cost with the exact random consumption of SciPy 1.18.1
/// `differential_evolution(..., seed=integer, polish=False)` in the BA configuration.
///
/// Bounds must contain finite pairs and at least one dimension; equal and reversed
/// pairs retain SciPy's scaling semantics. `x0`, when supplied, replaces the first
/// LHS member *after* all initialization draws. Its length must match the bounds.
/// Nonfinite objective values are not repaired: comparisons, NaN promotion, and
/// all-infinite population reevaluation follow SciPy. Negative/NaN tolerances
/// likewise simply prevent convergence. Allocation/count overflow is a programmer
/// error, as with the other DSP array APIs.
///
/// Returns an error for malformed bounds or an out-of-bounds initial point.
/// The closure is called serially in population order, including every trial.
pub fn differential_evolution<F>(
    mut cost: F,
    bounds: &[(f64, f64)],
    x0: Option<&[f64]>,
    options: DifferentialEvolutionOptions,
) -> Result<DifferentialEvolutionResult, DspError>
where
    F: FnMut(&[f64]) -> f64,
{
    let n = bounds.len();
    if n == 0 || bounds.iter().any(|(a, b)| !a.is_finite() || !b.is_finite()) {
        return Err(invalid("bounds must contain finite real valued pairs"));
    }
    if x0.is_some_and(|x| x.len() != n) {
        return Err(invalid("x0 length must match bounds"));
    }
    let center: Vec<_> = bounds.iter().map(|(a, b)| 0.5 * (a + b)).collect();
    let width: Vec<_> = bounds.iter().map(|(a, b)| (a - b).abs()).collect();
    let varying = bounds.iter().filter(|(a, b)| a != b).count().max(1);
    let members = options
        .popsize
        .checked_mul(varying)
        .expect("population size overflow")
        .max(5);
    members.checked_mul(n).expect("population shape overflow");
    let mut rng = RandomState::new(options.seed);
    let segment = 1.0 / members as f64;
    // np.linspace(endpoint=False) multiplies arange by the step (not i / members).
    let samples: Vec<Vec<_>> = (0..members)
        .map(|i| {
            (0..n)
                .map(|_| segment * rng.random_sample() + i as f64 * segment)
                .collect()
        })
        .collect();
    let mut population = vec![vec![0.0; n]; members];
    for j in 0..n {
        for (i, source) in rng.permutation(members).into_iter().enumerate() {
            population[i][j] = samples[source][j];
        }
    }
    if let Some(x0) = x0 {
        for j in 0..n {
            let reciprocal = 1.0 / width[j];
            let reciprocal = if reciprocal.is_finite() {
                reciprocal
            } else {
                0.0
            };
            let value = (x0[j] - center[j]) * reciprocal + 0.5;
            if value > 1.0 || value < 0.0 {
                return Err(invalid(
                    "Some entries in x0 lay outside the specified bounds",
                ));
            }
            population[0][j] = value;
        }
    }
    let scale = |unit: &[f64]| -> Vec<f64> {
        unit.iter()
            .enumerate()
            .map(|(j, x)| center[j] + (x - 0.5) * width[j])
            .collect()
    };
    let mut energies: Vec<_> = population.iter().map(|x| cost(&scale(x))).collect();
    let mut nfev = members;
    promote(&mut population, &mut energies);
    let mut indices: Vec<_> = (0..members).collect();
    let mut nit = 0;
    let mut success = false;
    for generation in 1..=options.maxiter {
        nit = generation;
        // __next__ repeats initial evaluation whenever every energy is infinite.
        if energies.iter().all(|x| x.is_infinite()) {
            for (energy, x) in energies.iter_mut().zip(&population) {
                *energy = cost(&scale(x));
                nfev += 1;
            }
            promote(&mut population, &mut energies);
        }
        let mutation = rng.uniform(0.5, 1.0)?;
        for candidate in 0..members {
            let fill = rng.randint(n)?;
            // _select_samples shuffles the SAME index array each time, and only
            // then excludes candidate. best1 uses the first two of its five.
            rng.shuffle(&mut indices);
            let mut selected = indices.iter().copied().filter(|&i| i != candidate);
            let r0 = selected.next().unwrap();
            let r1 = selected.next().unwrap();
            let mut trial = population[candidate].clone();
            for j in 0..n {
                let crossover = rng.random_sample() < 0.7;
                if crossover || j == fill {
                    trial[j] =
                        population[0][j] + mutation * (population[r0][j] - population[r1][j]);
                }
            }
            // Draw only after every crossover draw, and in ascending coordinate order.
            for value in &mut trial {
                if *value > 1.0 || *value < 0.0 {
                    *value = rng.random_sample();
                }
            }
            let energy = cost(&scale(&trial));
            nfev += 1;
            if energy <= energies[candidate] {
                population[candidate] = trial;
                energies[candidate] = energy;
                if energy <= energies[0] {
                    // argmin's first-tie rule matters for plateaus; do not just
                    // swap the current candidate into the best slot.
                    promote(&mut population, &mut energies);
                }
            }
        }
        if converged(&energies, options.tol) {
            success = true;
            break;
        }
    }
    Ok(DifferentialEvolutionResult {
        x: scale(&population[0]),
        fun: energies[0],
        nit,
        nfev,
        success,
        message: if success {
            "Optimization terminated successfully."
        } else {
            "Maximum number of iterations has been exceeded."
        }
        .to_owned(),
    })
}

fn promote(population: &mut [Vec<f64>], energies: &mut [f64]) {
    let mut best = 0;
    for i in 1..energies.len() {
        // np.argmin chooses the first NaN and otherwise the first minimum.
        if energies[best].is_nan() {
            break;
        }
        if energies[i].is_nan() || energies[i] < energies[best] {
            best = i;
        }
    }
    energies.swap(0, best);
    population.swap(0, best);
}

fn converged(energies: &[f64], tol: f64) -> bool {
    if energies.iter().any(|x| x.is_infinite()) {
        return false;
    }
    let mean = sum(energies) / energies.len() as f64;
    let squares: Vec<_> = energies
        .iter()
        .map(|e| {
            let d = e - mean;
            d * d
        })
        .collect();
    (sum(&squares) / energies.len() as f64).sqrt() <= tol * mean.abs()
}
