//! The shared greedy/TRF PEQ fitter, ringing limits, GD cuts and acoustic XO target.
//! Upstream: `correction_engine.py` `fit_peq`, `_ring_q_cap`, `_ring_boxes`,
//! `fit_gd_cuts`, `CorrectionEngine._xo_target_peq`.

use crate::grid::{SR_F, design_freqs};
use faer::Mat;
use secs_dsp::{
    Complex64 as C, optimize, response,
    smooth::{self, Fraction},
};
use secs_types::{chain::Peq, target::TargetCurve};
use std::{cell::RefCell, f64::consts::PI};

/// One `fit_peq` call's arguments. The grid and correction must have equal,
/// nonzero length; frequencies, bounds and residuals must be finite and valid.
#[derive(Debug, Clone)]
pub struct FitRequest<'a> {
    pub g: &'a [f64],
    pub corr_db: &'a [f64],
    pub n: usize,
    pub lo: f64,
    pub hi: f64,
    pub boost: f64,
    pub ring_ms: Option<f64>,
    pub f_max: Option<f64>,
    /// Optional correction-band union, intersected with `lo..hi` for weights.
    /// None retains the legacy contiguous fit and unconstrained centres.
    pub bands: Option<&'a [(f64, f64)]>,
}

impl FitRequest<'_> {
    fn in_range(&self, f: f64) -> bool {
        f >= self.lo && f <= self.hi && self.bands.is_none_or(|bands| crate::rc::in_bands(f, bands))
    }
}

/// Production uses `fit_peq`; stage goldens inject recorded decisions here.
/// Callers may reuse the result of an identical request across candidates.
pub type Fitter<'f> = dyn FnMut(&FitRequest<'_>) -> Vec<Peq> + 'f;

/// How PEQ fits differentiate their bands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PeqDerivatives {
    /// SciPy's two-point differences, as upstream.
    #[default]
    TwoPoint,
    /// Closed-form cascade Jacobian (experimental).
    Analytic,
}

pub fn ring_q_cap(f0: f64, gain_db: f64, ring_ms: f64) -> f64 {
    PI * f0 * ring_ms / 1000.0 / 10_f64.powf(gain_db / 40.0)
}

pub(crate) fn ring_boxes(bands: &[Peq], lb: &mut [f64], ub: &mut [f64], ring_ms: f64) {
    for (i, &(f0, _, gain)) in bands.iter().enumerate() {
        let k = 3 * i;
        lb[k] = lb[k].max((f0 / 1.2).ln());
        ub[k] = ub[k].min((f0 * 1.2).ln()).max(lb[k] + 1e-3);
        if gain > 0.0 {
            lb[k + 2] = 0.0;
            ub[k + 2] = (gain + 3.0).min(ub[k + 2]);
        } else {
            ub[k + 2] = (gain / 2.0).min(-0.25);
        }
        ub[k + 1] = ub[k + 1]
            .exp()
            .min(ring_q_cap(f0 / 1.2, ub[k + 2], ring_ms))
            .max(0.31)
            .ln();
    }
}

pub(crate) struct Seed {
    pub bands: Vec<Peq>,
    pub x0: Vec<f64>,
    pub lb: Vec<f64>,
    pub ub: Vec<f64>,
}

pub(crate) fn seed(req: &FitRequest<'_>) -> Seed {
    assert_eq!(req.g.len(), req.corr_db.len());
    let mut bands = Vec::new();
    for _ in 0..req.n {
        let current = response::peq_response(req.g, &bands);
        let r: Vec<_> = req
            .g
            .iter()
            .zip(req.corr_db)
            .zip(current)
            .map(|((&g, &c), h)| {
                if req.in_range(g) {
                    c - 20.0 * (response::magnitude(h) + 1e-12).log10()
                } else {
                    0.0
                }
            })
            .collect();
        let k = argmax(&r.iter().map(|v| v.abs()).collect::<Vec<_>>());
        if r[k].abs() < 0.5 {
            break;
        }
        let gain = r[k].clamp(-25.0, req.boost);
        let half = r[k].abs() / 2.0;
        let same = |i: usize| r[i].abs() >= half && r[i].signum() == r[k].signum();
        let mut i0 = k;
        let mut i1 = k;
        while i0 > 0 && same(i0 - 1) {
            i0 -= 1;
        }
        while i1 + 1 < r.len() && same(i1 + 1) {
            i1 += 1;
        }
        let f0 = req.g[k];
        let mut q = (f0 / (req.g[i1] - req.g[i0]).max(f0 * 0.02)).clamp(0.5, 15.0);
        if let Some(ms) = req.ring_ms {
            q = q.min(ring_q_cap(f0, gain, ms)).max(0.31);
        }
        bands.push((f0, q, gain));
    }
    let fmax = req.f_max.unwrap_or(2000.0_f64.max(req.hi * 1.2));
    let mut lb = [15_f64.ln(), 0.3_f64.ln(), -25.0].repeat(bands.len());
    let mut ub = [fmax.ln(), 20_f64.ln(), req.boost].repeat(bands.len());
    if let Some(ms) = req.ring_ms {
        ring_boxes(&bands, &mut lb, &mut ub, ms);
    }
    // A multi-band seed must not drift into an uncorrected gap during TRF.
    // Legacy fits keep their original bounds, including the split-fit overlap.
    if let Some(regions) = req.bands {
        for (i, &(f, _, _)) in bands.iter().enumerate() {
            let &(lo, hi) = regions
                .iter()
                .find(|&&(lo, hi)| f >= lo && f <= hi)
                .expect("seed belongs to a correction band");
            lb[3 * i] = lb[3 * i].max(lo.ln());
            ub[3 * i] = ub[3 * i].min(hi.ln());
        }
    }
    let x0 = bands
        .iter()
        .flat_map(|&(f, q, g)| [f.ln(), q.ln(), g])
        .enumerate()
        .map(|(i, x)| {
            let margin = if req.bands.is_some() {
                1e-6_f64.min((ub[i] - lb[i]) / 4.0)
            } else {
                1e-6
            };
            x.max(lb[i] + margin).min(ub[i] - margin)
        })
        .collect();
    Seed { bands, x0, lb, ub }
}

pub(crate) fn residual(req: &FitRequest<'_>, x: &[f64]) -> Vec<f64> {
    let bands: Vec<_> = x
        .as_chunks::<3>()
        .0
        .iter()
        .map(|b| (b[0].exp(), b[1].exp(), b[2]))
        .collect();
    weighted_residual(req, response::peq_response(req.g, &bands))
}

fn weighted_residual(req: &FitRequest<'_>, h: Vec<C>) -> Vec<f64> {
    h.into_iter()
        .enumerate()
        .map(|(i, h)| {
            let w = if req.in_range(req.g[i]) { 1.0 } else { 0.15 };
            w * (20.0 * (response::magnitude(h) + 1e-12).log10() - req.corr_db[i])
        })
        .collect()
}

/// Two-point Jacobian probes change only one band. Reuse the other bands'
/// responses, but always multiply the complete cascade in its original order.
pub(crate) struct ResidualCache {
    z: Vec<C>,
    bands: Vec<Option<([u64; 3], Vec<C>)>>,
}

impl ResidualCache {
    pub(crate) fn new(g: &[f64], bands: usize) -> Self {
        Self {
            z: g.iter()
                .map(|&f| C::from_polar(1.0, (-2.0 * PI * f) * (1.0 / 48000.0)))
                .collect(),
            bands: vec![None; bands],
        }
    }

    pub(crate) fn evaluate(&mut self, req: &FitRequest<'_>, x: &[f64]) -> Vec<f64> {
        let mut h = vec![C::new(1.0, 0.0); self.z.len()];
        for (cached, b) in self.bands.iter_mut().zip(x.as_chunks::<3>().0) {
            let key = b.map(f64::to_bits);
            if cached.as_ref().is_none_or(|(old, _)| *old != key) {
                *cached = Some((
                    key,
                    response::peaking_response_z(&self.z, b[0].exp(), b[1].exp(), b[2], 48000.0),
                ));
            }
            for (h, p) in h.iter_mut().zip(&cached.as_ref().unwrap().1) {
                *h *= p;
            }
        }
        weighted_residual(req, h)
    }
}

struct AnalyticBand {
    key: [u64; 3],
    response: Vec<C>,
    logarithmic_derivatives: Option<[Vec<C>; 3]>,
}

/// Analytic response and derivative cache. The two-point path keeps using
/// `ResidualCache`, so its arithmetic and cache behavior remain unchanged.
struct AnalyticResidualCache {
    z: Vec<C>,
    bands: Vec<Option<AnalyticBand>>,
    key: Vec<u64>,
    cascade: Vec<C>,
}

impl AnalyticResidualCache {
    fn new(g: &[f64], bands: usize) -> Self {
        Self {
            z: g.iter()
                .map(|&f| C::from_polar(1.0, (-2.0 * PI * f) * (1.0 / 48000.0)))
                .collect(),
            bands: (0..bands).map(|_| None).collect(),
            key: Vec::new(),
            cascade: Vec::new(),
        }
    }

    fn response(&mut self, x: &[f64]) {
        if self.key.len() == x.len()
            && self.key.iter().zip(x).all(|(&a, &b)| a == b.to_bits())
            && !self.cascade.is_empty()
        {
            return;
        }
        self.cascade.clear();
        self.cascade.resize(self.z.len(), C::new(1.0, 0.0));
        for (cached, b) in self.bands.iter_mut().zip(x.as_chunks::<3>().0) {
            let key = b.map(f64::to_bits);
            if cached.as_ref().is_none_or(|old| old.key != key) {
                *cached = Some(AnalyticBand {
                    key,
                    response: response::peaking_response_z(
                        &self.z,
                        b[0].exp(),
                        b[1].exp(),
                        b[2],
                        48000.0,
                    ),
                    logarithmic_derivatives: None,
                });
            }
            for (h, p) in self
                .cascade
                .iter_mut()
                .zip(&cached.as_ref().unwrap().response)
            {
                *h *= p;
            }
        }
        self.key.clear();
        self.key.extend(x.iter().map(|v| v.to_bits()));
    }

    fn evaluate(&mut self, req: &FitRequest<'_>, x: &[f64]) -> Vec<f64> {
        self.response(x);
        weighted_residual(req, self.cascade.clone())
    }

    fn jacobian(&mut self, req: &FitRequest<'_>, x: &[f64]) -> Mat<f64> {
        self.response(x);
        let factors: Vec<_> = self
            .cascade
            .iter()
            .enumerate()
            .map(|(i, &h)| {
                let weight = if req.in_range(req.g[i]) { 1.0 } else { 0.15 };
                let magnitude = response::magnitude(h);
                weight * (20.0 / 10_f64.ln()) * magnitude / (magnitude + 1e-12)
            })
            .collect();
        let mut jac = Mat::zeros(req.g.len(), x.len());
        for (k, (cached, b)) in self.bands.iter_mut().zip(x.as_chunks::<3>().0).enumerate() {
            let cached = cached.as_mut().unwrap();
            let derivatives = cached.logarithmic_derivatives.get_or_insert_with(|| {
                response::peaking_log_derivatives_z(&self.z, b[0].exp(), b[1].exp(), b[2], 48000.0)
            });
            for c in 0..3 {
                for i in 0..req.g.len() {
                    jac[(i, 3 * k + c)] = factors[i] * derivatives[c][i].re;
                }
            }
        }
        jac
    }
}

pub(crate) fn result_bands(req: &FitRequest<'_>, x: &[f64]) -> Vec<Peq> {
    x.as_chunks::<3>()
        .0
        .iter()
        .map(|b| {
            let (f, mut q, gain) = (b[0].exp(), b[1].exp(), b[2]);
            if let Some(ms) = req.ring_ms {
                q = q.min(ring_q_cap(f, gain, ms));
            }
            (f, q, gain)
        })
        .collect()
}

/// Fits the requested region, retaining upstream's out-of-region weight 0.15,
/// default TRF tolerances, and 400/150 residual-evaluation caps.
pub fn fit_peq(req: &FitRequest<'_>) -> Vec<Peq> {
    fit_peq_until(req, &|| false)
}

/// `fit_peq` that gives up once `cancelled()` turns true. From then on the
/// residual reads zero, so the trust-region solver stops at its next gradient
/// test instead of running to its evaluation cap; the bands it returns are
/// meaningless and the caller discards them. Until then the arithmetic is
/// exactly `fit_peq`'s.
pub fn fit_peq_until(req: &FitRequest<'_>, cancelled: &dyn Fn() -> bool) -> Vec<Peq> {
    fit_peq_with(req, cancelled, PeqDerivatives::TwoPoint)
}

/// `fit_peq_until` with a chosen derivative; `TwoPoint` is the same arithmetic.
pub fn fit_peq_with(
    req: &FitRequest<'_>,
    cancelled: &dyn Fn() -> bool,
    derivatives: PeqDerivatives,
) -> Vec<Peq> {
    let seed = seed(req);
    if seed.bands.is_empty() {
        return Vec::new();
    }
    let max_nfev = if req.ring_ms.is_some() { 150 } else { 400 };
    let sol = match derivatives {
        PeqDerivatives::TwoPoint => {
            let mut cache = ResidualCache::new(req.g, seed.bands.len());
            optimize::least_squares(
                |x| {
                    if cancelled() {
                        vec![0.0; req.g.len()]
                    } else {
                        cache.evaluate(req, x)
                    }
                },
                &seed.x0,
                &seed.lb,
                &seed.ub,
                max_nfev,
            )
        }
        PeqDerivatives::Analytic => {
            let cache = RefCell::new(AnalyticResidualCache::new(req.g, seed.bands.len()));
            optimize::least_squares_analytic(
                |x| {
                    if cancelled() {
                        vec![0.0; req.g.len()]
                    } else {
                        cache.borrow_mut().evaluate(req, x)
                    }
                },
                |x| {
                    if cancelled() {
                        Mat::zeros(req.g.len(), x.len())
                    } else {
                        cache.borrow_mut().jacobian(req, x)
                    }
                },
                &seed.x0,
                &seed.lb,
                &seed.ub,
                optimize::Options {
                    max_nfev,
                    ..optimize::Options::default()
                },
                None,
            )
        }
    }
    .expect("valid PEQ fit");
    result_bands(req, &sol.x)
}

fn argmax(v: &[f64]) -> usize {
    assert!(!v.is_empty());
    (1..v.len()).fold(0, |k, i| if v[i] > v[k] { i } else { k })
}
fn geomspace(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    let start = lo.log10();
    let step = (hi.log10() - start) / (n - 1) as f64;
    let mut g: Vec<_> = (0..n)
        .map(|i| 10_f64.powf(i as f64 * step + start))
        .collect();
    g[0] = lo;
    g[n - 1] = hi;
    g
}
fn interp(x: f64, f: &[f64], y: &[f64]) -> f64 {
    let i = f.partition_point(|v| *v <= x).saturating_sub(1);
    if x <= f[0] {
        y[0]
    } else if i + 1 == f.len() || x == f[i] {
        y[i]
    } else {
        (y[i + 1] - y[i]) / (f[i + 1] - f[i]) * (x - f[i]) + y[i]
    }
}
fn median(v: &[f64]) -> f64 {
    let mut v = v.to_vec();
    v.sort_by(f64::total_cmp);
    let k = v.len() / 2;
    if v.len().is_multiple_of(2) {
        (v[k - 1] + v[k]) / 2.0
    } else {
        v[k]
    }
}
// NumPy's pairwise contiguous sum, used by the GD improvement decision.
fn sum(v: &[f64]) -> f64 {
    if v.len() < 8 {
        return v.iter().fold(-0.0, |s, x| s + x);
    }
    if v.len() > 128 {
        let m = (v.len() / 2) / 8 * 8;
        return sum(&v[..m]) + sum(&v[m..]);
    }
    let mut r: [f64; 8] = v[..8].try_into().unwrap();
    let end = v.len() / 8 * 8;
    for c in v[8..end].as_chunks::<8>().0 {
        for (s, x) in r.iter_mut().zip(c) {
            *s += x;
        }
    }
    let s = ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
    v[end..].iter().fold(s, |s, x| s + x)
}

pub fn fit_gd_cuts(f: &[f64], h: &[C], lo: f64, hi: f64, n_max: usize, depth_db: f64) -> Vec<Peq> {
    fit_gd_cuts_until(f, h, lo, hi, n_max, depth_db, &|| false)
}

/// `fit_gd_cuts` that gives up once `cancelled()` turns true (TD-49), as
/// `fit_peq_until` does: no further cut starts, and the running fit's residual
/// reads zero so the trust-region solver stops at its next gradient test. The
/// cuts it returns are then meaningless and the caller discards them. Until
/// then the arithmetic is exactly `fit_gd_cuts`'s.
pub fn fit_gd_cuts_until(
    f: &[f64],
    h: &[C],
    lo: f64,
    hi: f64,
    n_max: usize,
    depth_db: f64,
    cancelled: &dyn Fn() -> bool,
) -> Vec<Peq> {
    let g = geomspace(lo, hi, 1500);
    gd_cuts_on_grid(f, h, &g, None, n_max, depth_db, cancelled)
}

pub(crate) fn fit_gd_cuts_bands_until(
    f: &[f64],
    h: &[C],
    bands: &[(f64, f64)],
    n_max: usize,
    depth_db: f64,
    cancelled: &dyn Fn() -> bool,
) -> Vec<Peq> {
    let g = crate::rc::band_grid(bands, 20.0, 1500);
    gd_cuts_on_grid(f, h, &g, Some(bands), n_max, depth_db, cancelled)
}

#[allow(clippy::too_many_arguments)]
fn gd_cuts_on_grid(
    f: &[f64],
    h: &[C],
    g: &[f64],
    bands: Option<&[(f64, f64)]>,
    n_max: usize,
    depth_db: f64,
    cancelled: &dyn Fn() -> bool,
) -> Vec<Peq> {
    let mut cur = h.to_vec();
    let mut cuts = Vec::new();
    for _ in 0..n_max {
        if cancelled() {
            break;
        }
        let gd = response::gd_curve(&cur, f, g, Fraction::Scalar(24.0)).expect("valid GD grid");
        let base: Vec<_> = (0..g.len())
            .map(|i| median(&gd[i.saturating_sub(40)..(i + 40).min(g.len())]))
            .collect();
        let exc: Vec<_> = gd.iter().zip(&base).map(|(g, b)| g - b).collect();
        let k = argmax(&exc);
        if exc[k] < 10.0 {
            break;
        }
        let f0 = g[k];
        let width = 2_f64.powf(1.0 / 3.0);
        let gw: Vec<_> = g
            .iter()
            .copied()
            .filter(|&g| g >= f0 / width && g <= f0 * width)
            .collect();
        let magnitude: Vec<_> = cur.iter().map(|&h| response::magnitude(h)).collect();
        let mag0: Vec<_> = gw
            .iter()
            .map(|&g| 20.0 * (interp(g, f, &magnitude).abs() + 1e-12).log10())
            .collect();
        let res = |x: &[f64]| {
            let hp: Vec<_> = cur
                .iter()
                .zip(response::peaking_response(
                    f,
                    x[0].exp(),
                    x[1].exp(),
                    x[2],
                    SR_F,
                ))
                .map(|(h, p)| h * p)
                .collect();
            let gd2 =
                response::gd_curve(&hp, f, &gw, Fraction::Scalar(24.0)).expect("valid GD grid");
            let mag: Vec<_> = hp.iter().map(|&h| response::magnitude(h)).collect();
            gd2.iter()
                .zip(&gw)
                .map(|(gd, &w)| (gd - interp(w, g, &base)) / 5.0)
                .chain(
                    gw.iter()
                        .zip(&mag0)
                        .map(|(&w, m)| 20.0 * (interp(w, f, &mag).abs() + 1e-12).log10() - m),
                )
                .collect::<Vec<_>>()
        };
        let x0 = [f0.ln(), 8_f64.ln(), -1.0];
        let mut lb = [(f0 / 1.1).ln(), 2_f64.ln(), -depth_db];
        let mut ub = [(f0 * 1.1).ln(), 40_f64.ln(), 0.0];
        if let Some(bands) = bands {
            let &(lo, hi) = bands
                .iter()
                .find(|&&(lo, hi)| f0 >= lo && f0 <= hi)
                .expect("GD peak belongs to a correction band");
            lb[0] = lb[0].max(lo.ln());
            ub[0] = ub[0].min(hi.ln());
        }
        let sol = optimize::least_squares(
            |x| {
                if cancelled() {
                    vec![0.0; 2 * gw.len()]
                } else {
                    res(x)
                }
            },
            &x0,
            &lb,
            &ub,
            60,
        )
        .expect("valid GD fit");
        if cancelled() {
            break;
        }
        let band = (sol.x[0].exp(), sol.x[1].exp(), sol.x[2]);
        let norm = |x: &[f64]| sum(&res(x).iter().map(|v| v * v).collect::<Vec<_>>());
        if norm(&sol.x) >= norm(&[f0.ln(), 8_f64.ln(), 0.0]) * 0.95 || band.2 > -0.1 {
            break;
        }
        cuts.push(band);
        for (h, p) in cur
            .iter_mut()
            .zip(response::peaking_response(f, band.0, band.1, band.2, SR_F))
        {
            *h *= p;
        }
    }
    cuts
}

/// Acoustic XO target before the electrical crossover; position order is
/// preserved during power accumulation. No ringing cap is applied here.
#[allow(clippy::too_many_arguments)]
pub fn xo_target_peq(
    by_pos: &[(f64, &[C])],
    target: &TargetCurve,
    level_db: f64,
    lo: f64,
    hi: f64,
    bands: usize,
    boost: f64,
    f_max: Option<f64>,
    fitter: &mut Fitter<'_>,
) -> Vec<Peq> {
    let f = design_freqs();
    let mut power = vec![0.0; f.len()];
    let mut weight = 0.0;
    for &(w, h) in by_pos {
        assert_eq!(h.len(), f.len());
        weight += w;
        for (p, &h) in power.iter_mut().zip(h) {
            let m = response::magnitude(h);
            *p += w * (m * m);
        }
    }
    for p in &mut power {
        *p /= weight;
    }
    let frac = secs_dsp::grid::frac_for(f);
    let sm = smooth::smooth_power(f, &power, Fraction::PerBin(&frac)).expect("design grid");
    let sm3 = smooth::smooth_power(f, &power, Fraction::Scalar(3.0)).expect("design grid");
    let tgt = target.sample(&f.iter().map(|v| v.max(1.0)).collect::<Vec<_>>());
    let corr: Vec<_> = sm
        .iter()
        .zip(sm3)
        .zip(tgt)
        .map(|((&s, s3), t)| {
            (t + level_db - (10.0 * (s + 1e-30).log10()).max(10.0 * (s3 + 1e-30).log10() - 3.0))
                .clamp(-25.0, boost)
        })
        .collect();
    let g = geomspace(15.0, 2000.0, 500);
    let c: Vec<_> = g
        .iter()
        .map(|&g| {
            let soft = ((g / (lo / 1.4)).log2() / 0.5).clamp(0.0, 1.0)
                * (((hi * 1.4) / g).log2() / 0.5).clamp(0.0, 1.0);
            interp(g, f, &corr) * soft
        })
        .collect();
    fitter(&FitRequest {
        g: &g,
        corr_db: &c,
        n: bands,
        lo,
        hi,
        boost,
        ring_ms: None,
        f_max,
        bands: None,
    })
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct RecordedFit {
    pub g: Vec<f64>,
    pub corr_db: Vec<f64>,
    pub n: usize,
    pub lo: f64,
    pub hi: f64,
    pub boost: f64,
    pub ring_ms: Option<f64>,
    pub f_max: Option<f64>,
    pub result: Vec<Peq>,
}
#[cfg(any(test, feature = "test-support"))]
impl RecordedFit {
    pub fn request(&self) -> FitRequest<'_> {
        FitRequest {
            g: &self.g,
            corr_db: &self.corr_db,
            n: self.n,
            lo: self.lo,
            hi: self.hi,
            boost: self.boost,
            ring_ms: self.ring_ms,
            f_max: self.f_max,
            bands: None,
        }
    }
}
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn replay_fitter(
    calls: Vec<RecordedFit>,
    rtol: f64,
) -> impl FnMut(&FitRequest<'_>) -> Vec<Peq> {
    let mut next = 0;
    move |req| {
        let index = next;
        let call = calls
            .get(index)
            .unwrap_or_else(|| panic!("fit call {index}: unexpected extra call"));
        assert_eq!(req.n, call.n, "fit call {index}: n");
        let mut worst = (0.0_f64, String::new());
        for (name, a, b) in [
            ("g", req.g, call.g.as_slice()),
            ("corr_db", req.corr_db, call.corr_db.as_slice()),
            (
                "scalars",
                &[req.lo, req.hi, req.boost],
                &[call.lo, call.hi, call.boost],
            ),
        ] {
            assert_eq!(a.len(), b.len(), "fit call {index}: {name} length");
            for (i, (&a, &b)) in a.iter().zip(b).enumerate() {
                let err = (a - b).abs() / b.abs().max(1.0);
                if !err.is_finite() || err > worst.0 {
                    worst = (err, format!("{name}[{i}] got={a:e} expected={b:e}"));
                }
            }
        }
        for (name, a, b) in [
            ("ring_ms", req.ring_ms, call.ring_ms),
            ("f_max", req.f_max, call.f_max),
        ] {
            assert_eq!(
                a.is_some(),
                b.is_some(),
                "fit call {index}: {name} presence"
            );
            if let (Some(a), Some(b)) = (a, b) {
                let err = (a - b).abs() / b.abs().max(1.0);
                if !err.is_finite() || err > worst.0 {
                    worst = (err, format!("{name} got={a:e} expected={b:e}"));
                }
            }
        }
        assert!(
            worst.0.is_finite() && worst.0 <= rtol,
            "fit call {index}: worst mismatch {} relative={} tolerance={rtol}",
            worst.1,
            worst.0
        );
        next += 1;
        call.result.clone()
    }
}

#[cfg(test)]
mod analytic_tests {
    use super::*;

    #[test]
    fn analytic_peq_jacobian_differentiates_the_weighted_residual() {
        let g = geomspace(15.0, 3000.0, 100);
        let corr = vec![0.0; g.len()];
        let req = FitRequest {
            g: &g,
            corr_db: &corr,
            n: 3,
            lo: 20.0,
            hi: 250.0,
            boost: 6.0,
            ring_ms: None,
            f_max: None,
            bands: None,
        };
        let x = [
            30.0_f64.ln(),
            1.0_f64.ln(),
            -10.0,
            80.0_f64.ln(),
            4.0_f64.ln(),
            3.0,
            200.0_f64.ln(),
            2.0_f64.ln(),
            -5.0,
        ];
        let mut cache = AnalyticResidualCache::new(&g, 3);
        let jac = cache.jacobian(&req, &x);
        for j in 0..x.len() {
            let samples: Vec<_> = [-2.0, -1.0, 1.0, 2.0]
                .iter()
                .map(|multiple| {
                    let mut probe = x;
                    probe[j] += multiple * 1e-3;
                    residual(&req, &probe)
                })
                .collect();
            let error = (0..g.len())
                .map(|i| {
                    let central = (samples[0][i] - 8.0 * samples[1][i] + 8.0 * samples[2][i]
                        - samples[3][i])
                        / 0.012;
                    (jac[(i, j)] - central).abs()
                })
                .fold(0.0_f64, f64::max);
            let scale = (0..g.len())
                .map(|i| jac[(i, j)].abs())
                .fold(1.0_f64, f64::max);
            assert!(error < 3e-6 * scale, "column {j}: {error}/{scale}");
        }
        let cached = cache.evaluate(&req, &x);
        assert!(
            cached
                .iter()
                .zip(residual(&req, &x))
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
    }

    #[test]
    fn analytic_peq_jacobian_retains_the_additive_magnitude_floor() {
        let g = [100.0];
        let corr = [0.0];
        let req = FitRequest {
            g: &g,
            corr_db: &corr,
            n: 16,
            lo: 20.0,
            hi: 250.0,
            boost: 6.0,
            ring_ms: None,
            f_max: None,
            bands: None,
        };
        let x = [100.0_f64.ln(), 2.0_f64.ln(), -25.0].repeat(16);
        let mut cache = AnalyticResidualCache::new(&g, 16);
        let jac = cache.jacobian(&req, &x);
        let magnitude = response::magnitude(cache.cascade[0]);
        let expected = magnitude / (magnitude + 1e-12);
        assert!(expected > 0.0 && expected < 1e-6);
        assert!((jac[(0, 2)] / expected - 1.0).abs() < 1e-8);
    }
}
