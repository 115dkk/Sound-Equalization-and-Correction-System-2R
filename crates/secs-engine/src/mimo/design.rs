//! Port of SECS 2 `src/secs/mimo.py` (GPL-3.0), support ranges and guards.
//! Normal equations exploit the Toeplitz blocks without allocating A.

use secs_dsp::{
    fft,
    fir::firwin,
    iir::{BType, Sos, butter, sosfilt},
    interp, pairwise_sum,
    response::hp2,
    smooth::{Fraction, smooth_power},
};

use crate::{
    C, Design, EngineError, Note, Spectra, decay, grid::design_freqs, measurement::delay_response,
};

const FS_LOW: f64 = 600.0;
const EARLY: usize = 36;

use super::solve::solve;
use crate::{EngineConfig, MeasurementSet};
#[cfg(test)]
use secs_compute::MapError;
use secs_compute::{Context, MemoryError};

#[derive(Debug, Clone, PartialEq)]
pub struct MimoRequest {
    pub fsiso: f64,
    pub level_db: f64,
    pub f_low: Option<f64>,
    pub max_supports: Option<usize>,
    pub exclude: Vec<String>,
    pub groups: Option<Vec<String>>,
    pub taps_low: usize,
}

impl Default for MimoRequest {
    fn default() -> Self {
        Self {
            fsiso: 150.0,
            level_db: -12.0,
            f_low: None,
            max_supports: None,
            exclude: Vec::new(),
            groups: None,
            taps_low: 180,
        }
    }
}

pub type SupportRanges = Vec<(String, Option<(f64, f64)>)>;
pub type InputSupports = Vec<(String, Vec<(String, Vec<f64>)>)>;

#[derive(Debug, Clone, PartialEq)]
pub struct MimoDesign {
    pub fs_low: f64,
    pub ranges: SupportRanges,
    pub supports: InputSupports,
    pub level_db: Vec<(String, f64)>,
    pub notes: Vec<Note>,
}

impl Default for MimoDesign {
    fn default() -> Self {
        Self {
            fs_low: FS_LOW,
            ranges: Vec::new(),
            supports: Vec::new(),
            level_db: Vec::new(),
            notes: Vec::new(),
        }
    }
}

pub fn lowpass_fir(fc: f64, taps: usize) -> Result<Vec<f64>, EngineError> {
    Ok(firwin(taps, fc, FS_LOW)?)
}

fn geomspace(start: f64, end: f64) -> Vec<f64> {
    let step = (end.log10() - start.log10()) / 199.0;
    let mut values: Vec<_> = (0..200)
        .map(|k| 10f64.powf(start.log10() + k as f64 * step))
        .collect();
    values[0] = start;
    values[199] = end;
    values
}

/// Position-mean one-third-octave support ranges, in main then sub order.
/// Long spectra must already include the sub's fixed and bundle filters.
pub fn support_ranges(
    design: &Design,
    spectra: &Spectra,
    fsiso: f64,
    positions: &[u32],
) -> Result<SupportRanges, EngineError> {
    if positions.is_empty() || !fsiso.is_finite() || fsiso <= 0.0 {
        return Err(EngineError::Input("invalid MIMO range inputs".into()));
    }
    let f = design_freqs();
    let mut out = Vec::new();
    for (role, sub) in design
        .mains
        .iter()
        .map(|m| (m.role.as_str(), None))
        .chain(design.chain.subs.iter().map(|s| (s.role.as_str(), Some(s))))
    {
        let Some(paths): Option<Vec<_>> = positions.iter().map(|&p| spectra.get(p, role)).collect()
        else {
            continue;
        };
        let mut power = vec![0.0; f.len()];
        for h in paths {
            if h.len() != f.len() {
                return Err(EngineError::Input("invalid MIMO spectrum length".into()));
            }
            for (sum, h) in power.iter_mut().zip(h.iter()) {
                let a = h.re.hypot(h.im);
                *sum += a * a;
            }
        }
        for p in &mut power {
            *p /= positions.len() as f64;
        }
        let db: Vec<_> = smooth_power(f, &power, Fraction::Scalar(3.0))?
            .iter()
            .map(|p| 10.0 * (p + 1e-30).log10())
            .collect();
        let (lo_ref, hi_ref) = if sub.is_some() {
            (30.0, 80.0)
        } else {
            (200.0, 1000.0)
        };
        let mut reference = decay::median(
            &f.iter()
                .zip(&db)
                .filter(|(f, _)| (lo_ref..=hi_ref).contains(*f))
                .map(|(_, &v)| v)
                .collect::<Vec<_>>(),
        );
        let mut start = if sub.is_some() {
            50.0
        } else {
            fsiso.min(200.0)
        };
        if let Some(s) = sub.filter(|s| s.hp_hz != 0.0 || s.lp_hz != 0.0) {
            let lo = s.hp_hz.max(15.0);
            let hi = (if s.lp_hz != 0.0 {
                s.lp_hz.min(80.0)
            } else {
                80.0
            })
            .max(lo * 1.5);
            let indices: Vec<_> = f
                .iter()
                .enumerate()
                .filter(|(_, f)| (lo..=hi).contains(f))
                .map(|(k, _)| k)
                .collect();
            let values: Vec<_> = indices.iter().map(|&k| db[k]).collect();
            let k = indices[decay::first_max(&values)];
            reference = db[k];
            start = f[k];
        }
        let low = geomspace(start, 15.0)
            .into_iter()
            .find(|&v| interp(v, f, &db) < reference - 10.0)
            .unwrap_or(15.0)
            .max(if sub.is_some() { 20.0 } else { 50.0 });
        let high = geomspace(start, fsiso)
            .into_iter()
            .find(|&v| interp(v, f, &db) < reference - 10.0)
            .unwrap_or(fsiso);
        out.push((
            role.to_owned(),
            (low < high.min(fsiso) * 0.8).then(|| {
                (
                    (low * 10.0).round_ties_even() / 10.0,
                    (high * 10.0).round_ties_even() / 10.0,
                )
            }),
        ));
    }
    Ok(out)
}

/// Arrival-aligned acoustic path with the fourth-order lower-edge protection.
pub fn support_path(
    mimo: &MimoDesign,
    role: &str,
    spectra: &Spectra,
    position: u32,
    align_delay_ms: &[(String, f64)],
) -> Result<Vec<C>, EngineError> {
    let h = spectra
        .get(position, role)
        .ok_or_else(|| EngineError::Input(format!("missing MIMO path {position}/{role}")))?;
    let delay = align_delay_ms
        .iter()
        .find(|(r, _)| r == role)
        .map_or(0.0, |(_, ms)| *ms);
    let low = mimo
        .ranges
        .iter()
        .find(|(r, _)| r == role)
        .and_then(|(_, r)| *r)
        .map_or(50.0, |r| r.0);
    Ok(h.iter()
        .zip(delay_response(delay))
        .zip(hp2(design_freqs(), low))
        .map(|((&h, d), hp)| h * d * (hp * hp))
        .collect())
}

fn onset(x: &[f64]) -> usize {
    let top = x.iter().map(|v| v.abs()).fold(0.0, f64::max);
    x.iter().position(|v| v.abs() > 0.1 * top).unwrap_or(0)
}

pub(crate) fn convolve(a: &[f64], b: &[f64]) -> Vec<f64> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    // NumPy's convolve swaps the operands when the second is longer, then
    // accumulates the first operand in increasing index order for each output.
    let (a, b) = if a.len() < b.len() { (b, a) } else { (a, b) };
    (0..a.len() + b.len() - 1)
        .map(|t| {
            let mut sum = 0.0;
            for k in t.saturating_sub(b.len() - 1)..=(t.min(a.len() - 1)) {
                sum += a[k] * b[t - k];
            }
            sum
        })
        .collect()
}

/// One block pair `(j, jj)` of the weighted normal equations, summed over
/// positions in order, and the right-hand side of `j` when `j == jj`.
/// The first row/column are direct time sums; the diagonal recurrence handles
/// the truncated last sample and the weight step at `onset + 36`.
fn block_pair(
    xs: &[Vec<f64>],
    paths: &[Vec<Vec<f64>>],
    (j, jj): (usize, usize),
    taps: usize,
    early_weight: f64,
) -> (Vec<f64>, Vec<f64>) {
    let mut total = vec![0.0; taps * taps];
    let mut rhs = vec![0.0; if j == jj { taps } else { 0 }];
    for (x, gs) in xs.iter().zip(paths) {
        let n = x.len();
        let e = (onset(x) + EARLY).min(n);
        let c = |t: usize| if t < e { early_weight } else { 1.0 };
        let (g, h) = (&gs[j], &gs[jj]);
        for (k, b) in rhs.iter_mut().enumerate() {
            for t in e.max(k)..n {
                *b -= x[t] * g[t - k];
            }
        }
        let mut block = vec![0.0; taps * taps];
        for k in 0..taps {
            for t in k..n {
                block[k * taps] += c(t) * g[t - k] * h[t];
            }
        }
        for l in 1..taps {
            for t in l..n {
                block[l] += c(t) * g[t] * h[t - l];
            }
        }
        for k in 0..taps - 1 {
            for l in 0..taps - 1 {
                let mut v = block[k * taps + l] - c(n - 1) * g[n - 1 - k] * h[n - 1 - l];
                if e < n && e > k.max(l) {
                    v += (1.0 - early_weight) * g[e - 1 - k] * h[e - 1 - l];
                }
                block[(k + 1) * taps + l + 1] = v;
            }
        }
        for (sum, v) in total.iter_mut().zip(block) {
            *sum += v;
        }
    }
    (total, rhs)
}

/// A block pair's index and its `(block, rhs)`.
type PairBlock = ((usize, usize), (Vec<f64>, Vec<f64>));

fn block_pairs(candidates: usize) -> Vec<(usize, usize)> {
    (0..candidates)
        .flat_map(|j| (j..candidates).map(move |jj| (j, jj)))
        .collect()
}

/// Place the upper-triangle blocks; AᵀA is symmetric, and the opposite
/// triangle reuses the same value instead of a second accumulation.
fn place(blocks: Vec<PairBlock>, ata: &mut [f64], atb: &mut [f64], taps: usize) {
    let dim = atb.len();
    for ((j, jj), (block, rhs)) in blocks {
        if j == jj {
            atb[j * taps..(j + 1) * taps].copy_from_slice(&rhs);
        }
        for k in 0..taps {
            for l in 0..taps {
                if j != jj || k <= l {
                    let (row, col) = (j * taps + k, jj * taps + l);
                    ata[row * dim + col] = block[k * taps + l];
                    ata[col * dim + row] = block[k * taps + l];
                }
            }
        }
    }
}

/// Weighted normal equations, position-major paths, row-major output.
/// `early_weight=10` is the design system, `0` is candidate ranking.
pub(crate) fn normal_equations(
    xs: &[Vec<f64>],
    paths: &[Vec<Vec<f64>>],
    taps: usize,
    early_weight: f64,
) -> (Vec<f64>, Vec<f64>) {
    let candidates = paths[0].len();
    let dim = candidates * taps;
    let (mut ata, mut atb) = (vec![0.0; dim * dim], vec![0.0; dim]);
    for pair in block_pairs(candidates) {
        let block = block_pair(xs, paths, pair, taps, early_weight);
        place(vec![(pair, block)], &mut ata, &mut atb, taps);
    }
    (ata, atb)
}

/// [`normal_equations`] with the block pairs as a map over `ctx`. Each block
/// keeps the position order of its sums, and placement adds nothing, so the
/// result has the same bits for any thread count.
pub(crate) fn normal_equations_mapped(
    xs: &[Vec<f64>],
    paths: &[Vec<Vec<f64>>],
    taps: usize,
    early_weight: f64,
    ctx: &Context,
) -> Result<(Vec<f64>, Vec<f64>), EngineError> {
    check(ctx)?;
    let candidates = paths[0].len();
    let dim = candidates * taps;
    let (mut ata, mut atb) = (vec![0.0; dim * dim], vec![0.0; dim]);
    // Retain at most eight completed blocks alongside AᵀA. Each block keeps
    // its original position/time sums; placement only copies disjoint entries.
    for pairs in block_pairs(candidates).chunks(8) {
        let blocks = ctx
            .map(pairs.to_vec(), |pair| {
                (pair, block_pair(xs, paths, pair, taps, early_weight))
            })
            .map_err(|_| EngineError::Cancelled)?;
        place(blocks, &mut ata, &mut atb, taps);
    }
    Ok((ata, atb))
}

// Retain the direct form as an independent guard oracle and diagnostic entry.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn worst_band_db(
    xs: &[Vec<f64>],
    paths: &[Vec<Vec<f64>>],
    supports: &[Vec<f64>],
    fsiso: f64,
) -> f64 {
    let n = xs[0].len();
    let nf = 2 * n;
    let bins = secs_dsp::grid::freqs(nf, FS_LOW).expect("MIMO grid");
    let stop = (fsiso / 20.0).log2() + 1e-9;
    let edges: Vec<_> = (0..)
        .map(|k| k as f64 / 3.0)
        .take_while(|&v| v < stop)
        .map(|v| 20.0 * 2f64.powf(v))
        .collect();
    let mut bands: Vec<_> = edges.windows(2).map(|v| (v[0], v[1])).collect();
    let refs: Vec<_> = xs.iter().map(Vec::as_slice).collect();
    bands.extend(
        decay::late_peaks(&refs, 6)
            .iter()
            .map(|&f| (f - 1.5, f + 1.5)),
    );
    let mut before = vec![0.0; bands.len()];
    let mut after = before.clone();
    for (x, gs) in xs.iter().zip(paths) {
        let mut add = vec![0.0; n];
        for (g, s) in gs.iter().zip(supports) {
            for (a, v) in add.iter_mut().zip(convolve(g, s)) {
                *a += v;
            }
        }
        let e = (onset(x) + EARLY).min(n);
        for (acc, addition) in [(&mut before, false), (&mut after, true)] {
            let mut y = vec![0.0; nf];
            for t in e..n {
                y[t] = x[t] + if addition { add[t] } else { 0.0 };
            }
            let spectrum: Vec<_> = fft::rfft(&y)
                .iter()
                .map(|h| {
                    let a = h.re.hypot(h.im);
                    a * a
                })
                .collect();
            for (total, &(lo, hi)) in acc.iter_mut().zip(&bands) {
                let begin = bins.partition_point(|&f| f < lo);
                let end = bins.partition_point(|&f| f < hi);
                *total += pairwise_sum(&spectrum[begin..end]);
            }
        }
    }
    before
        .iter()
        .zip(after)
        .filter(|(a, _)| **a > 0.0)
        .map(|(a, b)| 10.0 * ((b + 1e-30) / a).log10())
        .reduce(f64::max)
        .unwrap_or(-99.0)
}

#[derive(Debug)]
pub(crate) struct BandGuard {
    n: usize,
    starts: Vec<usize>,
    bands: Vec<(usize, usize)>,
    before: Vec<f64>,
}

impl BandGuard {
    pub(crate) fn new(xs: &[Vec<f64>], fsiso: f64) -> Self {
        let n = xs[0].len();
        let bins = secs_dsp::grid::freqs(2 * n, FS_LOW).expect("MIMO grid");
        let stop = (fsiso / 20.0).log2() + 1e-9;
        let edges: Vec<_> = (0..)
            .map(|k| k as f64 / 3.0)
            .take_while(|&v| v < stop)
            .map(|v| 20.0 * 2f64.powf(v))
            .collect();
        let refs: Vec<_> = xs.iter().map(Vec::as_slice).collect();
        let bands = edges
            .windows(2)
            .map(|v| (v[0], v[1]))
            .chain(
                decay::late_peaks(&refs, 6)
                    .into_iter()
                    .map(|f| (f - 1.5, f + 1.5)),
            )
            .map(|(lo, hi)| {
                (
                    bins.partition_point(|&f| f < lo),
                    bins.partition_point(|&f| f < hi),
                )
            })
            .collect();
        let mut guard = Self {
            n,
            starts: xs.iter().map(|x| (onset(x) + EARLY).min(n)).collect(),
            bands,
            before: Vec::new(),
        };
        guard.before = guard.energies(xs, true);
        guard
    }

    fn energies(&self, responses: &[Vec<f64>], baseline: bool) -> Vec<f64> {
        let mut totals = vec![0.0; self.bands.len()];
        for (response, &start) in responses.iter().zip(&self.starts) {
            let mut window = vec![0.0; 2 * self.n];
            for t in start..self.n {
                // Preserve the original baseline's x + 0.0, including -0.0.
                window[t] = if baseline {
                    response[t] + 0.0
                } else {
                    response[t]
                };
            }
            let spectrum: Vec<_> = fft::rfft(&window)
                .iter()
                .map(|h| {
                    let a = h.re.hypot(h.im);
                    a * a
                })
                .collect();
            for (total, &(begin, end)) in totals.iter_mut().zip(&self.bands) {
                *total += pairwise_sum(&spectrum[begin..end]);
            }
        }
        totals
    }

    pub(crate) fn evaluate(&self, ys: &[Vec<f64>]) -> f64 {
        self.before
            .iter()
            .zip(self.energies(ys, false))
            .filter(|(a, _)| **a > 0.0)
            .map(|(a, b)| 10.0 * ((b + 1e-30) / a).log10())
            .reduce(f64::max)
            .unwrap_or(-99.0)
    }
}

/// Absolute modal-tail change, in the upstream 0.15 seconds-per-dB scale.
/// Despite its name this guard does not compare fitted T60 slopes.
pub fn worst_t60_change(
    xs: &[Vec<f64>],
    ys: &[Vec<f64>],
    count: usize,
) -> Result<f64, EngineError> {
    T60Guard::new(xs, count).evaluate(ys)
}

#[derive(Debug)]
struct TailBaseline {
    frequency: f64,
    positions: usize,
    sos: Sos,
    energy: f64,
}

/// Fixed original modal peaks and mode-specific tails, shared across guard
/// trials. Each trial still discovers its own new peaks before taking the
/// union, so reducing the support cannot hide a newly exposed mode.
#[derive(Debug)]
pub(crate) struct T60Guard<'a> {
    xs: &'a [Vec<f64>],
    count: usize,
    original_modes: Vec<f64>,
    starts: Vec<usize>,
    baselines: Vec<TailBaseline>,
}

impl<'a> T60Guard<'a> {
    pub(crate) fn new(xs: &'a [Vec<f64>], count: usize) -> Self {
        Self {
            xs,
            count,
            original_modes: decay::late_peaks(
                &xs.iter().map(Vec::as_slice).collect::<Vec<_>>(),
                count,
            ),
            starts: xs.iter().map(|x| onset(x) + 180).collect(),
            baselines: Vec::new(),
        }
    }

    pub(crate) fn evaluate(&mut self, ys: &[Vec<f64>]) -> Result<f64, EngineError> {
        let mut modes: Vec<_> = self
            .original_modes
            .iter()
            .copied()
            .chain(decay::late_peaks(
                &ys.iter().map(Vec::as_slice).collect::<Vec<_>>(),
                self.count,
            ))
            .map(|f| (f * 10.0).round_ties_even() / 10.0)
            .collect();
        modes.sort_by(f64::total_cmp);
        modes.dedup();
        let positions = self.xs.len().min(ys.len());
        let mut tails = Vec::new();
        for fm in modes {
            let index = self
                .baselines
                .iter()
                .position(|entry| entry.frequency == fm && entry.positions == positions);
            let index = if let Some(index) = index {
                index
            } else {
                let sos = butter(2, &[(fm - 1.5).max(1.0), fm + 1.5], BType::Bandpass, FS_LOW)?;
                let mut energy = 0.0;
                for (x, &t) in self.xs.iter().zip(&self.starts).take(positions) {
                    let xf = sosfilt(&sos, x)?;
                    energy += decay::energy(&xf[t.min(xf.len())..]);
                }
                self.baselines.push(TailBaseline {
                    frequency: fm,
                    positions,
                    sos,
                    energy,
                });
                self.baselines.len() - 1
            };
            let baseline = &self.baselines[index];
            let mut after = 0.0;
            for (y, &t) in ys.iter().zip(&self.starts).take(positions) {
                let yf = sosfilt(&baseline.sos, y)?;
                after += decay::energy(&yf[t.min(yf.len())..]);
            }
            tails.push((baseline.energy, after));
        }
        let top = tails.iter().map(|&(a, _)| a).fold(0.0, f64::max);
        let mut worst: f64 = -99.0;
        for (a, b) in tails {
            if a > 0.0 && a >= top * 10f64.powf(-20.0 / 10.0) {
                worst = worst.max(10.0 * ((b + 1e-30) / a).log10());
            }
        }
        Ok(worst * 0.15)
    }
}

#[cfg(test)]
pub(crate) fn worst_t60_change_before(
    xs: &[Vec<f64>],
    ys: &[Vec<f64>],
    count: usize,
) -> Result<f64, EngineError> {
    let refs =
        |v: &[Vec<f64>]| decay::late_peaks(&v.iter().map(Vec::as_slice).collect::<Vec<_>>(), count);
    let mut modes: Vec<_> = refs(xs)
        .into_iter()
        .chain(refs(ys))
        .map(|f| (f * 10.0).round_ties_even() / 10.0)
        .collect();
    modes.sort_by(f64::total_cmp);
    modes.dedup();
    let mut tails = Vec::new();
    for fm in modes {
        let sos = butter(2, &[(fm - 1.5).max(1.0), fm + 1.5], BType::Bandpass, FS_LOW)?;
        let (mut a, mut b) = (0.0, 0.0);
        for (x, y) in xs.iter().zip(ys) {
            let t = onset(x) + 180;
            let xf = sosfilt(&sos, x)?;
            let yf = sosfilt(&sos, y)?;
            a += decay::energy(&xf[t.min(xf.len())..]);
            b += decay::energy(&yf[t.min(yf.len())..]);
        }
        tails.push((a, b));
    }
    let top = tails.iter().map(|&(a, _)| a).fold(0.0, f64::max);
    let mut worst: f64 = -99.0;
    for (a, b) in tails {
        if a > 0.0 && a >= top * 10f64.powf(-20.0 / 10.0) {
            worst = worst.max(10.0 * ((b + 1e-30) / a).log10());
        }
    }
    Ok(worst * 0.15)
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LambdaStep {
    pub mid: f64,
    pub ratio: f64,
    pub accepted: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GuardStep {
    pub scale: f64,
    pub band_db: f64,
    pub t60_change: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InputTrace {
    pub candidates: Vec<String>,
    pub ranking: Vec<(String, f64)>,
    pub trajectory: Vec<LambdaStep>,
    pub log_lambda: f64,
    pub ratio: f64,
    pub guards: Vec<GuardStep>,
    pub guard_scale: f64,
}
#[derive(Debug)]
pub(crate) struct InputResult {
    pub supports: Vec<(String, Vec<f64>)>,
    pub level_db: f64,
    pub trace: InputTrace,
}

fn check(ctx: &Context) -> Result<(), EngineError> {
    ctx.cancel().check().map_err(|_| EngineError::Cancelled)
}

pub(crate) fn rank_supports(
    candidates: &[String],
    xs: &[Vec<f64>],
    paths: &[Vec<Vec<f64>>],
    taps: usize,
    ctx: &Context,
) -> Result<Vec<(String, f64)>, EngineError> {
    let late_input: Vec<_> = xs
        .iter()
        .flat_map(|x| {
            let e = onset(x) + EARLY;
            x.iter()
                .enumerate()
                .map(move |(t, &x)| if t >= e { x } else { 0.0 })
        })
        .collect();
    let input_energy = decay::energy(&late_input);
    let work = xs.iter().map(Vec::len).sum::<usize>() + 3 * taps;
    let mut scored = Vec::new();
    for (j, role) in candidates.iter().enumerate() {
        check(ctx)?;
        let candidate_paths: Vec<_> = paths.iter().map(|g| vec![g[j].clone()]).collect();
        let (ata, atb) = normal_equations(xs, &candidate_paths, taps, 0.0);
        let diagonal: Vec<_> = (0..taps).map(|k| ata[k * taps + k]).collect();
        let lambda = 1e-2 * pairwise_sum(&diagonal) / taps as f64;
        let u = solve(&ata, &atb, lambda, false)?;
        let path_energy = candidate_paths.iter().map(|gs| decay::energy(&gs[0])).sum();
        let (score, uncertainty) =
            rank_score(input_energy, path_energy, &ata, &atb, &u, lambda, work);
        // Cancellation and nonfinite arithmetic must not make a poor path
        // look perfect. Keep the original residual evaluation in that case.
        let direct = !score.is_finite() || !uncertainty.is_finite() || score <= 8.0 * uncertainty;
        scored.push((
            role.clone(),
            if direct {
                rank_score_direct(xs, paths, j, &u)
            } else {
                score
            },
            if direct { 0.0 } else { uncertainty },
            u,
        ));
    }
    // A tiny scalar roundoff must not change the discrete support ordering.
    // Recompute both members of every overlapping score interval, before
    // the stable sort, in the original time-domain accumulation order.
    let mut direct = vec![false; scored.len()];
    for i in 0..scored.len() {
        for j in i + 1..scored.len() {
            if (scored[i].1 - scored[j].1).abs() <= scored[i].2 + scored[j].2 {
                direct[i] = true;
                direct[j] = true;
            }
        }
    }
    let mut scores = Vec::with_capacity(scored.len());
    for (j, ((role, score, _, u), direct)) in scored.into_iter().zip(direct).enumerate() {
        check(ctx)?;
        scores.push((
            role,
            if direct {
                rank_score_direct(xs, paths, j, &u)
            } else {
                score
            },
        ));
    }
    scores.sort_by(|a, b| a.1.total_cmp(&b.1));
    Ok(scores)
}

fn rank_score(
    input_energy: f64,
    path_energy: f64,
    ata: &[f64],
    atb: &[f64],
    u: &[f64],
    lambda: f64,
    work: usize,
) -> (f64, f64) {
    let products: Vec<_> = u.iter().zip(atb).map(|(u, b)| u * b).collect();
    let ub = pairwise_sum(&products);
    let regularization = lambda * decay::energy(u);
    let u_l1 = u.iter().map(|v| v.abs()).sum::<f64>();
    // Full path energy also bounds cancellation in the Gram's sliding
    // recurrence when the retained late-window block itself is tiny.
    let mut magnitude = input_energy
        + 2.0 * products.iter().map(|v| v.abs()).sum::<f64>()
        + regularization.abs()
        + path_energy * u_l1 * u_l1;
    let mut row_products = vec![0.0; u.len()];
    let mut corrections = Vec::with_capacity(u.len());
    for (i, row) in ata.chunks_exact(u.len()).enumerate() {
        for ((v, a), u) in row_products.iter_mut().zip(row).zip(u) {
            *v = a * u;
        }
        let residual = (pairwise_sum(&row_products) + lambda * u[i]) - atb[i];
        corrections.push(u[i] * residual);
        magnitude += u[i].abs() * row_products.iter().map(|v| v.abs()).sum::<f64>();
    }
    // b = -Aᵀx, M = AᵀA. For a finite-precision solve the exact identity is
    // ||Au+x||² = xᵀx - uᵀb - λuᵀu + uᵀ((M+λI)u-b).
    // The O(taps²) residual correction is cheaper than the old O(P*N*taps)
    // convolutions and does not assume the solver residual is exactly zero.
    let score = ((input_energy - ub) - regularization) + pairwise_sum(&corrections);
    // This deliberately conservative reevaluation trigger scales with Gram
    // recurrences and dot products. It is not a certified error bound or a
    // relaxation of any result-validation tolerance.
    let uncertainty = 64.0 * f64::EPSILON * work.max(1) as f64 * magnitude;
    (score, uncertainty)
}

pub(crate) fn rank_score_direct(
    xs: &[Vec<f64>],
    paths: &[Vec<Vec<f64>>],
    candidate: usize,
    u: &[f64],
) -> f64 {
    let mut residual = Vec::new();
    for (x, gs) in xs.iter().zip(paths) {
        let y = convolve(&gs[candidate], u);
        let e = onset(x) + EARLY;
        residual.extend(
            x.iter()
                .enumerate()
                .map(|(t, &x)| if t >= e { y[t] + x } else { 0.0 }),
        );
    }
    decay::energy(&residual)
}

/// The low-rate stage boundary used by the oracle: paths have lower-edge
/// protection and arrival alignment, but not the common low-pass FIR yet.
pub(crate) fn design_input(
    xs: &[Vec<f64>],
    raw_paths: &[Vec<Vec<f64>>],
    direct: &[f64],
    candidates: &[String],
    req: &MimoRequest,
    ctx: &Context,
) -> Result<InputResult, EngineError> {
    design_input_observed(xs, raw_paths, direct, candidates, req, ctx, &|_, _| {})
}

pub(crate) fn design_input_observed(
    xs: &[Vec<f64>],
    raw_paths: &[Vec<Vec<f64>>],
    direct: &[f64],
    candidates: &[String],
    req: &MimoRequest,
    ctx: &Context,
    observe: &dyn Fn(&str, std::time::Duration),
) -> Result<InputResult, EngineError> {
    design_input_accelerated(xs, raw_paths, direct, candidates, req, ctx, observe, None)
}

#[allow(clippy::too_many_arguments)]
fn design_input_accelerated(
    xs: &[Vec<f64>],
    raw_paths: &[Vec<Vec<f64>>],
    direct: &[f64],
    candidates: &[String],
    req: &MimoRequest,
    ctx: &Context,
    observe: &dyn Fn(&str, std::time::Duration),
    acceleration: Option<&super::acceleration::RunAcceleration<'_>>,
) -> Result<InputResult, EngineError> {
    let started = std::time::Instant::now();
    let taps = req.taps_low;
    let lp = lowpass_fir(req.fsiso, 31)?;
    let n = xs[0].len();
    let all_paths: Vec<Vec<Vec<f64>>> = raw_paths
        .iter()
        .map(|gs| {
            gs.iter()
                .map(|g| {
                    let mut h = convolve(g, &lp);
                    h.truncate(n);
                    h
                })
                .collect()
        })
        .collect();
    observe("path_building", started.elapsed());
    let started = std::time::Instant::now();
    let ranking = if req
        .max_supports
        .is_some_and(|cap| cap > 0 && candidates.len() > cap)
    {
        rank_supports(candidates, xs, &all_paths, taps, ctx)?
    } else {
        Vec::new()
    };
    observe("ranking", started.elapsed());
    let selected: Vec<_> = if ranking.is_empty() {
        candidates.to_vec()
    } else {
        ranking
            .iter()
            .take(req.max_supports.unwrap())
            .map(|(r, _)| r.clone())
            .collect()
    };
    let indices: Vec<_> = selected
        .iter()
        .map(|r| candidates.iter().position(|v| v == r).unwrap())
        .collect();
    let select = |paths: &[Vec<Vec<f64>>]| {
        paths
            .iter()
            .map(|gs| indices.iter().map(|&j| gs[j].clone()).collect())
            .collect::<Vec<_>>()
    };
    design_selected(
        xs,
        &select(raw_paths),
        direct,
        &select(&all_paths),
        &selected,
        &ranking,
        req,
        ctx,
        observe,
        acceleration,
    )
}

#[allow(clippy::too_many_arguments)]
fn design_selected(
    xs: &[Vec<f64>],
    raw_paths: &[Vec<Vec<f64>>],
    direct: &[f64],
    paths: &[Vec<Vec<f64>>],
    candidates: &[String],
    ranking: &[(String, f64)],
    req: &MimoRequest,
    ctx: &Context,
    observe: &dyn Fn(&str, std::time::Duration),
    acceleration: Option<&super::acceleration::RunAcceleration<'_>>,
) -> Result<InputResult, EngineError> {
    let taps = req.taps_low;
    let lp = lowpass_fir(req.fsiso, 31)?;
    let n = xs[0].len();
    let started = std::time::Instant::now();
    let (ata, atb) = normal_equations_mapped(xs, paths, taps, 10.0, ctx)?;
    observe("normal_equations", started.elapsed());
    let started = std::time::Instant::now();
    let dim = atb.len();
    if dim == 0 {
        return Err(EngineError::Input("no MIMO support candidates".into()));
    }
    let diagonal: Vec<_> = (0..dim).map(|k| ata[k * dim + k]).collect();
    let normalization = pairwise_sum(&diagonal) / dim as f64;
    let target = 10f64.powf(req.level_db / 10.0);
    let direct_energy = decay::energy(direct).max(1e-30);
    let result = super::acceleration::run_sweep(
        super::acceleration::RidgeProblem {
            ata: &ata,
            atb: &atb,
            lp: &lp,
            taps,
            normalization,
            target,
            direct_energy,
        },
        ctx.cancel(),
        acceleration,
    )?;
    let (u, ratio, hi, trajectory) = (result.u, result.ratio, result.log_lambda, result.trajectory);
    observe("sweep", started.elapsed());
    let started = std::time::Instant::now();
    let supports: Vec<_> = (0..candidates.len())
        .map(|k| convolve(&lp, &u[k * taps..(k + 1) * taps]))
        .collect();
    let mut scale = 1.0;
    let mut guards = Vec::new();
    let band_guard = BandGuard::new(xs, req.fsiso);
    let mut tail_guard = T60Guard::new(xs, 6);
    for step in 0..5 {
        check(ctx)?;
        let trial: Vec<Vec<_>> = supports
            .iter()
            .map(|s| s.iter().map(|v| v * scale).collect())
            .collect();
        let ys: Vec<_> = xs
            .iter()
            .zip(raw_paths)
            .map(|(x, gs)| {
                let mut add = vec![0.0; n];
                for (g, s) in gs.iter().zip(&trial) {
                    for (v, h) in add.iter_mut().zip(convolve(g, s)) {
                        *v += h;
                    }
                }
                x.iter().zip(add).map(|(x, a)| x + a).collect()
            })
            .collect();
        let band_db = band_guard.evaluate(&ys);
        let t60_change = tail_guard.evaluate(&ys)?;
        guards.push(GuardStep {
            scale,
            band_db,
            t60_change,
        });
        if band_db <= 1.0 && t60_change <= 0.15 {
            break;
        }
        scale = if step == 4 {
            0.0
        } else {
            scale * 10f64.powf(-3.0 / 20.0)
        };
    }
    observe("guard_steps", started.elapsed());
    Ok(InputResult {
        supports: if scale > 0.0 {
            candidates
                .iter()
                .cloned()
                .zip(
                    supports
                        .into_iter()
                        .map(|s| s.into_iter().map(|v| v * scale).collect()),
                )
                .collect()
        } else {
            Vec::new()
        },
        level_db: 10.0 * (ratio * scale * scale).max(1e-30).log10(),
        trace: InputTrace {
            candidates: candidates.to_vec(),
            ranking: ranking.to_vec(),
            trajectory,
            log_lambda: hi,
            ratio,
            guards,
            guard_scale: scale,
        },
    })
}

pub fn design_mimo(
    design: &Design,
    measurements: &MeasurementSet,
    config: &EngineConfig,
    req: &MimoRequest,
    ctx: &Context,
) -> Result<MimoDesign, EngineError> {
    design_mimo_traced(design, measurements, config, req, ctx).map(|(out, _)| out)
}

pub(crate) fn design_mimo_traced(
    design: &Design,
    measurements: &MeasurementSet,
    config: &EngineConfig,
    req: &MimoRequest,
    ctx: &Context,
) -> Result<(MimoDesign, Vec<(String, InputTrace)>), EngineError> {
    design_mimo_inputs(design, measurements, config, req, ctx, None)
}

pub(crate) fn design_mimo_inputs(
    design: &Design,
    measurements: &MeasurementSet,
    config: &EngineConfig,
    req: &MimoRequest,
    ctx: &Context,
    input: Option<&str>,
) -> Result<(MimoDesign, Vec<(String, InputTrace)>), EngineError> {
    design_mimo_inputs_observed(design, measurements, config, req, ctx, input, &|_, _| {})
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn design_mimo_inputs_observed(
    design: &Design,
    measurements: &MeasurementSet,
    config: &EngineConfig,
    req: &MimoRequest,
    ctx: &Context,
    input: Option<&str>,
    observe: &(dyn Fn(&str, std::time::Duration) + Sync),
) -> Result<(MimoDesign, Vec<(String, InputTrace)>), EngineError> {
    check(ctx)?;
    let mut out = MimoDesign::default();
    let positions = &design.spectra.positions;
    if positions.len() < 3 {
        out.notes
            .push(Note::Mimo(crate::notes::MimoNote::InsufficientPositions {
                measured: positions.len(),
            }));
        return Ok((out, Vec::new()));
    }
    validate_request(req)?;
    let started = std::time::Instant::now();
    let prepared = super::prepared::Prepared::new(design, measurements, config, ctx)?;
    observe("preparation", started.elapsed());
    design_mimo_prepared(design, &prepared, req, ctx, input, observe)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn design_mimo_prepared(
    design: &Design,
    prepared: &super::prepared::Prepared<'_>,
    req: &MimoRequest,
    ctx: &Context,
    input: Option<&str>,
    observe: &(dyn Fn(&str, std::time::Duration) + Sync),
) -> Result<(MimoDesign, Vec<(String, InputTrace)>), EngineError> {
    design_mimo_prepared_accelerated(design, prepared, req, ctx, input, observe, None)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn design_mimo_prepared_accelerated(
    design: &Design,
    prepared: &super::prepared::Prepared<'_>,
    req: &MimoRequest,
    ctx: &Context,
    input: Option<&str>,
    observe: &(dyn Fn(&str, std::time::Duration) + Sync),
    acceleration: Option<&super::acceleration::RunAcceleration<'_>>,
) -> Result<(MimoDesign, Vec<(String, InputTrace)>), EngineError> {
    check(ctx)?;
    let mut out = MimoDesign::default();
    let positions = &design.spectra.positions;
    if positions.len() < 3 {
        out.notes
            .push(Note::Mimo(crate::notes::MimoNote::InsufficientPositions {
                measured: positions.len(),
            }));
        return Ok((out, Vec::new()));
    }
    validate_request(req)?;
    let spectra = &prepared.spectra;
    let started = std::time::Instant::now();
    out.ranges = support_ranges(design, spectra, req.fsiso, positions)?;
    if let Some(low) = req.f_low.filter(|v| *v != 0.0) {
        for (_, range) in &mut out.ranges {
            *range =
                range.and_then(|(lo, hi)| (lo.max(low) < hi * 0.8).then_some((lo.max(low), hi)));
        }
    }
    let usable: Vec<_> = out
        .ranges
        .iter()
        .filter(|(r, range)| {
            range.is_some()
                && !req.exclude.contains(r)
                && req.groups.as_ref().is_none_or(|groups| {
                    design
                        .groups
                        .iter()
                        .any(|(g, roles)| groups.contains(g) && roles.contains(r))
                })
        })
        .map(|(r, _)| r.clone())
        .collect();
    if !req.exclude.is_empty() {
        let mut excluded = req.exclude.clone();
        excluded.sort();
        out.notes
            .push(Note::Mimo(crate::notes::MimoNote::ExcludedSupports(
                excluded,
            )));
    }
    out.notes
        .push(Note::Mimo(crate::notes::MimoNote::SupportRanges(
            out.ranges.clone(),
        )));
    observe("ranges", started.elapsed());
    let started = std::time::Instant::now();
    let mains: Vec<_> = design
        .mains
        .iter()
        .filter(|m| input.is_none_or(|r| r == m.role))
        .collect();
    let roles: Vec<_> = usable
        .iter()
        .filter(|r| mains.iter().any(|m| m.role != **r))
        .collect();
    let pairs: Vec<_> = positions
        .iter()
        .flat_map(|&p| roles.iter().map(move |r| (p, *r)))
        .collect();
    let shared_bytes = (pairs.len()
        * (720 * 8 + std::mem::size_of::<Vec<f64>>() + 2 * std::mem::size_of::<(u32, &String)>()))
        as u64;
    let scratch_bytes = (32 * design_freqs().len() * 8) as u64;
    let largest_input = mains
        .iter()
        .map(|m| {
            input_bytes(
                positions.len(),
                usable.iter().filter(|r| **r != m.role).count(),
                req.taps_low,
            ) as u64
        })
        .max()
        .unwrap_or(0);
    // A retained reservation must not leave a worker waiting for bytes that
    // this same invocation cannot release until the worker has finished. The
    // preparation's spectra stay reserved underneath (TD-24).
    let retained = prepared.reserved.saturating_add(shared_bytes);
    let needed = retained.saturating_add(largest_input.max(scratch_bytes));
    if needed > ctx.budget().total() {
        return Err(EngineError::Input(
            MemoryError::TooLarge {
                requested: needed,
                total: ctx.budget().total(),
            }
            .to_string(),
        ));
    }
    let _shared_permit = ctx
        .budget()
        .acquire(shared_bytes, ctx.cancel())
        .map_err(memory_error)?;
    let low_paths = crate::memory::try_map(
        ctx,
        pairs.clone(),
        retained,
        |_| scratch_bytes,
        |(p, r)| {
            let h = support_path(&out, r, spectra, p, &prepared.align_delay_ms)?;
            Ok::<_, EngineError>(secs_dsp::lowrate::to_low(&h, design_freqs(), 720)?)
        },
    )?;
    observe("shared_paths", started.elapsed());
    let started = std::time::Instant::now();
    let results = crate::memory::try_map(
        ctx,
        mains,
        retained,
        |main| {
            input_bytes(
                positions.len(),
                usable.iter().filter(|r| **r != main.role).count(),
                req.taps_low,
            ) as u64
        },
        |main| {
            let candidates: Vec<_> = usable
                .iter()
                .filter(|r| **r != main.role)
                .cloned()
                .collect();
            let started = std::time::Instant::now();
            let mut xs = Vec::new();
            let mut paths = Vec::new();
            for &p in positions {
                check(ctx)?;
                let response =
                    crate::input_response(design, &main.role, spectra, p).ok_or_else(|| {
                        EngineError::Input(format!("missing MIMO input {p}/{}", main.role))
                    })?;
                xs.push(secs_dsp::lowrate::to_low(&response, design_freqs(), 720)?);
                paths.push(
                    candidates
                        .iter()
                        .map(|r| {
                            let index = pairs
                                .iter()
                                .position(|(pos, role)| *pos == p && *role == r)
                                .unwrap();
                            low_paths[index].clone()
                        })
                        .collect(),
                );
            }
            let direct = secs_dsp::lowrate::to_low(&main.response, design_freqs(), 720)?;
            observe("input_preparation", started.elapsed());
            let result = design_input_accelerated(
                &xs,
                &paths,
                &direct,
                &candidates,
                req,
                ctx,
                observe,
                acceleration,
            )?;
            Ok((main.role.clone(), result))
        },
    )?;
    observe("inputs_wall", started.elapsed());
    let mut traces = Vec::new();
    for (role, result) in results {
        let scale = result.trace.guard_scale;
        if scale < 1.0 {
            out.notes.push(Note::Mimo(if scale > 0.0 {
                crate::notes::MimoNote::GuardReduced {
                    role: role.clone(),
                    gain_db: 20.0 * scale.max(1e-9).log10(),
                }
            } else {
                crate::notes::MimoNote::GuardDisabled { role: role.clone() }
            }));
        }
        out.supports.push((role.clone(), result.supports));
        out.level_db.push((role.clone(), result.level_db));
        traces.push((role, result.trace));
    }
    Ok((out, traces))
}

fn validate_request(req: &MimoRequest) -> Result<(), EngineError> {
    if !(0.0..300.0).contains(&req.fsiso)
        || req.fsiso == 0.0
        || !req.level_db.is_finite()
        || req.taps_low == 0
        || req.taps_low > 720
        || req.f_low.is_some_and(|f| !f.is_finite())
    {
        return Err(EngineError::Input("invalid MIMO request".into()));
    }
    Ok(())
}

fn memory_error(error: MemoryError) -> EngineError {
    match error {
        MemoryError::Cancelled => EngineError::Cancelled,
        other => EngineError::Input(other.to_string()),
    }
}

fn input_bytes(positions: usize, candidates: usize, taps: usize) -> usize {
    let dim = candidates * taps;
    8 * (4 * dim * dim
        + taps * taps
        + positions * 720 * (4 * candidates + 8)
        + 32 * design_freqs().len()
        + 16 * dim
        + 16384)
}

// Pre-P18c per-input preparation, retained only as an independent bitwise oracle.
#[cfg(test)]
pub(crate) fn design_mimo_before(
    design: &Design,
    measurements: &MeasurementSet,
    config: &EngineConfig,
    req: &MimoRequest,
    ctx: &Context,
    input: Option<&str>,
) -> Result<(MimoDesign, Vec<(String, InputTrace)>), EngineError> {
    check(ctx)?;
    let mut out = MimoDesign::default();
    let positions = &design.spectra.positions;
    if positions.len() < 3 {
        out.notes
            .push(Note::Mimo(crate::notes::MimoNote::InsufficientPositions {
                measured: positions.len(),
            }));
        return Ok((out, Vec::new()));
    }
    if !(0.0..300.0).contains(&req.fsiso)
        || req.fsiso == 0.0
        || !req.level_db.is_finite()
        || req.taps_low == 0
        || req.taps_low > 720
        || req.f_low.is_some_and(|f| !f.is_finite())
    {
        return Err(EngineError::Input("invalid MIMO request".into()));
    }
    let spectra = crate::export_view(design, measurements, config)?.spectra;
    let prepared =
        crate::measurement::prepare(measurements, config, &crate::setup::Setup::new(config))?;
    out.ranges = support_ranges(design, &spectra, req.fsiso, positions)?;
    if let Some(low) = req.f_low.filter(|v| *v != 0.0) {
        for (_, range) in &mut out.ranges {
            *range =
                range.and_then(|(lo, hi)| (lo.max(low) < hi * 0.8).then_some((lo.max(low), hi)));
        }
    }
    let usable: Vec<_> = out
        .ranges
        .iter()
        .filter(|(r, range)| {
            range.is_some()
                && !req.exclude.contains(r)
                && req.groups.as_ref().is_none_or(|groups| {
                    design
                        .groups
                        .iter()
                        .any(|(g, roles)| groups.contains(g) && roles.contains(r))
                })
        })
        .map(|(r, _)| r.clone())
        .collect();
    if !req.exclude.is_empty() {
        let mut excluded = req.exclude.clone();
        excluded.sort();
        out.notes
            .push(Note::Mimo(crate::notes::MimoNote::ExcludedSupports(
                excluded,
            )));
    }
    out.notes
        .push(Note::Mimo(crate::notes::MimoNote::SupportRanges(
            out.ranges.clone(),
        )));
    let results = ctx
        .try_map(
            design
                .mains
                .iter()
                .filter(|m| input.is_none_or(|r| r == m.role))
                .collect(),
            |main| {
                let candidates: Vec<_> = usable
                    .iter()
                    .filter(|r| **r != main.role)
                    .cloned()
                    .collect();
                let dim = candidates.len() * req.taps_low;
                // Normal equations, Householder reduction, possible fallback factor,
                // and scratch (also covers the mapped blocks during construction),
                // plus path/guard/design-grid buffers. Reserve before allocating.
                let bytes = 8
                    * (4 * dim * dim
                        + req.taps_low * req.taps_low
                        + positions.len() * 720 * (4 * candidates.len() + 8)
                        + 32 * design_freqs().len()
                        + 16 * dim
                        + 16384);
                let _permit =
                    ctx.budget()
                        .acquire(bytes as u64, ctx.cancel())
                        .map_err(|e| match e {
                            MemoryError::Cancelled => EngineError::Cancelled,
                            other => EngineError::Input(other.to_string()),
                        })?;
                let mut xs = Vec::new();
                let mut paths = Vec::new();
                for &p in positions {
                    check(ctx)?;
                    let response = crate::input_response(design, &main.role, &spectra, p)
                        .ok_or_else(|| {
                            EngineError::Input(format!("missing MIMO input {p}/{}", main.role))
                        })?;
                    xs.push(secs_dsp::lowrate::to_low(&response, design_freqs(), 720)?);
                    let gs: Result<Vec<_>, EngineError> = candidates
                        .iter()
                        .map(|r| {
                            let h = support_path(&out, r, &spectra, p, &prepared.align_delay_ms)?;
                            Ok(secs_dsp::lowrate::to_low(&h, design_freqs(), 720)?)
                        })
                        .collect();
                    paths.push(gs?);
                }
                let direct = secs_dsp::lowrate::to_low(&main.response, design_freqs(), 720)?;
                let result = design_input(&xs, &paths, &direct, &candidates, req, ctx)?;
                Ok((main.role.clone(), result))
            },
        )
        .map_err(|e| match e {
            MapError::Cancelled => EngineError::Cancelled,
            MapError::Item { error, .. } => error,
        })?;
    let mut traces = Vec::new();
    for (role, result) in results {
        let scale = result.trace.guard_scale;
        if scale < 1.0 {
            out.notes.push(Note::Mimo(if scale > 0.0 {
                crate::notes::MimoNote::GuardReduced {
                    role: role.clone(),
                    gain_db: 20.0 * scale.max(1e-9).log10(),
                }
            } else {
                crate::notes::MimoNote::GuardDisabled { role: role.clone() }
            }));
        }
        out.supports.push((role.clone(), result.supports));
        out.level_db.push((role.clone(), result.level_db));
        traces.push((role, result.trace));
    }
    Ok((out, traces))
}
