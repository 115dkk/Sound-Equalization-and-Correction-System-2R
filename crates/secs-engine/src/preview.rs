//! BA display curves replayed from an immutable design, without fitting.

use crate::{
    C, Design, LayoutInfo,
    chain::{BaRow, term},
    grid::design_freqs,
};
use secs_dsp::{
    interp, pairwise_sum,
    smooth::{Fraction, PowerSmoother},
};
use secs_types::chain::{Allpass, Peq};

/// Reference-position XO display curves, all in dB except the Hz grid.
#[derive(Debug, Clone, PartialEq)]
pub struct XoCurves {
    /// Logarithmic 5–500 Hz grid, 620 points.
    pub grid: Vec<f64>,
    /// Mean main power, with electrical HP.
    pub main: Vec<f64>,
    /// Mean sub power, with electrical LP and each channel's input correction.
    pub sub: Vec<f64>,
    /// Mean power of each channel's coherent main/sub sum.
    pub sum: Vec<f64>,
    /// Target including the design's reference level.
    pub target: Vec<f64>,
}

#[derive(Debug)]
struct CachedMain {
    role: String,
    raw: Vec<C>,
    corrected: Vec<C>,
    correction: std::sync::Arc<[C]>,
}

/// Immutable per-design responses; building once keeps slider calls below R7's
/// 0.2 s budget. Rebuild this cache whenever the design changes.
#[derive(Debug)]
pub struct XoPreview {
    sub: Vec<C>,
    mains: Vec<CachedMain>,
    grid: Vec<f64>,
    target: Vec<f64>,
    smoother: PowerSmoother,
}

impl XoPreview {
    /// Precomputes BA/bus and corrected main responses at the reference seat.
    pub fn new(design: &Design) -> Self {
        let f = design_freqs();
        let pos = design.spectra.main_position;
        let mut sub = vec![C::new(0.0, 0.0); f.len()];
        if let Some(s) = &design.subs {
            let terms: Vec<_> = s
                .rows
                .iter()
                .map(|(r, row)| {
                    (
                        design
                            .spectra
                            .used
                            .get(pos, r)
                            .expect("reference sub")
                            .as_ref(),
                        row,
                    )
                })
                .collect();
            sub = crate::chain::sub_sum(&terms, f);
            if let Some(bus) = &s.bus_response {
                for (h, b) in sub.iter_mut().zip(bus.iter()) {
                    *h *= b;
                }
            }
        }
        let mains = design
            .mains
            .iter()
            .filter_map(|m| {
                let h = design.spectra.raw.get(pos, &m.role)?;
                let gain = C::new(10f64.powf(m.gain_db / 20.0), 0.0);
                let raw: Vec<_> = h
                    .iter()
                    .zip(crate::measurement::delay_response(m.align_delay_ms))
                    .map(|(h, d)| h * d * gain)
                    .collect();
                let corrected = raw
                    .iter()
                    .enumerate()
                    .map(|(i, h)| {
                        h * m.top_response.as_ref().map_or(C::new(1.0, 0.0), |p| p[i])
                            * m.response[i]
                    })
                    .collect();
                Some(CachedMain {
                    role: m.role.clone(),
                    raw,
                    corrected,
                    correction: m.response.clone(),
                })
            })
            .collect();
        let grid = geomspace(5.0, 500.0, 620);
        let target = design
            .target
            .sample(&grid)
            .iter()
            .map(|t| t + design.ref_level)
            .collect();
        Self {
            sub,
            mains,
            grid,
            target,
            smoother: PowerSmoother::new(f, Fraction::Scalar(12.0)).expect("design grid"),
        }
    }

    /// Applies only a new electrical HP/LP, then power averaging and smoothing.
    /// `design` must be the design passed to `new`. An absent/unknown role uses
    /// the whole group; an unknown group has zero mains and the raw sub curve.
    pub fn curves(
        &self,
        design: &Design,
        group: &str,
        fc: f64,
        corrected: bool,
        role: Option<&str>,
    ) -> XoCurves {
        let group_roles = design
            .groups
            .iter()
            .find(|(g, _)| g == group)
            .map(|(_, roles)| roles);
        let mut mains: Vec<_> = self
            .mains
            .iter()
            .filter(|m| group_roles.is_some_and(|rs| rs.contains(&m.role)))
            .collect();
        // Preserve the group's accumulation order, rather than chain order.
        if let Some(roles) = group_roles {
            mains.sort_by_key(|m| roles.iter().position(|r| r == &m.role).unwrap());
        }
        if let Some(role) = role
            && mains.iter().any(|m| m.role == role)
        {
            mains.retain(|m| m.role == role);
        }
        let f = design_freqs();
        let hp = secs_dsp::response::lr4(f, fc, "HP");
        let lp = secs_dsp::response::lr4(f, fc, "LP");
        let mut main_power = vec![0.0; f.len()];
        let mut sub_power = vec![0.0; f.len()];
        let mut sum_power = vec![0.0; f.len()];
        let power = |h: C| {
            let a = h.re.hypot(h.im);
            a * a
        };
        for i in 0..f.len() {
            let sub = self.sub[i] * lp[i];
            if mains.is_empty() {
                sub_power[i] = power(sub);
                sum_power[i] = power(sub);
            }
            for m in &mains {
                let main = if corrected { m.corrected[i] } else { m.raw[i] } * hp[i];
                let s = if corrected {
                    sub * m.correction[i]
                } else {
                    sub
                };
                main_power[i] += power(main);
                sub_power[i] += power(s);
                sum_power[i] += power(main + s);
            }
            if !mains.is_empty() {
                main_power[i] /= mains.len() as f64;
                sub_power[i] /= mains.len() as f64;
                sum_power[i] /= mains.len() as f64;
            }
        }
        XoCurves {
            grid: self.grid.clone(),
            main: db(&main_power, &self.grid, &self.smoother),
            sub: db(&sub_power, &self.grid, &self.smoother),
            sum: db(&sum_power, &self.grid, &self.smoother),
            target: self.target.clone(),
        }
    }
}

/// The sub's exported alignment, on the output time axis.
#[derive(Debug, Clone, PartialEq)]
pub struct SubValue {
    /// Speaker role.
    pub role: String,
    /// Output gain in dB.
    pub gain_db: f64,
    /// Output delay in milliseconds, including the common shift.
    pub delay_ms: f64,
    /// Output polarity, 1 or -1.
    pub polarity: i8,
    /// Ordered all-pass filters.
    pub allpass: Vec<Allpass>,
    /// BA mode label.
    pub mode: String,
    /// Fixed and generated bundle peaking filters.
    pub peq: Vec<Peq>,
}

/// Sub-combination preview; all curves are reference-position dB values.
#[derive(Debug, Clone)]
pub struct SubCombine {
    /// Sub roles in configuration order.
    pub subs: Vec<String>,
    /// Logarithmic 5–300 Hz display grid, 540 points.
    pub grid: Vec<f64>,
    /// Raw individual sub curves, smoothed to 1/12 octave.
    pub before: Vec<(String, Vec<f64>)>,
    /// Aligned individual sub curves, including bundle and bus PEQ.
    pub after: Vec<(String, Vec<f64>)>,
    /// Coherent sub sum before alignment, in dB.
    pub sum_before: Vec<f64>,
    /// Coherent sub sum after alignment, in dB.
    pub sum_after: Vec<f64>,
    /// Metric band, 20 Hz to 1.5 times the maximum main XO (150 Hz without XO).
    pub band: (f64, f64),
    /// Before/after standard deviations of 1/6-octave sum levels in the band.
    pub flat: (f64, f64),
    /// Before/after mean across-seat deviations; absent for a single seat.
    pub seat: Option<(f64, f64)>,
    /// Number of positions with every sub measured.
    pub positions: usize,
    /// Exported alignment values, in sub order.
    pub values: Vec<SubValue>,
    /// Typed layout data; presentation text belongs to the export layer.
    pub layout_info: Vec<(String, LayoutInfo)>,
}

fn geomspace(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    let start = lo.log10();
    let step = (hi.log10() - start) / (n - 1) as f64;
    let mut out: Vec<_> = (0..n)
        .map(|i| 10f64.powf(start + i as f64 * step))
        .collect();
    out[0] = lo;
    out[n - 1] = hi;
    out
}
fn mean(v: &[f64]) -> f64 {
    pairwise_sum(v) / v.len() as f64
}
fn std(v: &[f64]) -> f64 {
    let m = mean(v);
    mean(&v.iter().map(|x| (x - m) * (x - m)).collect::<Vec<_>>()).sqrt()
}
fn db(power: &[f64], grid: &[f64], smoother: &PowerSmoother) -> Vec<f64> {
    let f = design_freqs();
    let values: Vec<_> = smoother
        .apply(power)
        .expect("design grid")
        .iter()
        .map(|p| 10.0 * (p + 1e-30).log10())
        .collect();
    grid.iter().map(|&g| interp(g, f, &values)).collect()
}
fn curve(h: &[C], grid: &[f64], smoother: &PowerSmoother) -> Vec<f64> {
    let power = &h
        .iter()
        .map(|h| {
            let a = h.re.hypot(h.im);
            a * a
        })
        .collect::<Vec<_>>();
    db(power, grid, smoother)
}

/// Replays before/after sub responses, returning `None` for fewer than two subs.
/// Fixed filters remain in the raw spectra, matching upstream's `H0`; generated
/// bundle filters and bus PEQ appear only after alignment. Does not rerun BA.
pub fn sub_combine(design: &Design) -> Option<SubCombine> {
    let sub = design.subs.as_ref()?;
    if sub.rows.len() < 2 {
        return None;
    }
    let subs: Vec<_> = sub.rows.iter().map(|(r, _)| r.clone()).collect();
    let positions: Vec<_> = design
        .spectra
        .positions
        .iter()
        .copied()
        .filter(|&p| subs.iter().all(|r| design.spectra.raw.get(p, r).is_some()))
        .collect();
    let grid = geomspace(5.0, 300.0, 540);
    let max_xo = design
        .xo
        .iter()
        .filter(|(group, hz)| group != "sub" && *hz != 0.0)
        .map(|(_, hz)| *hz)
        .reduce(f64::max)
        .unwrap_or(100.0);
    let band = (20.0, max_xo * 1.5);
    let metric_grid = geomspace(band.0, band.1, 200);
    let raw = BaRow {
        gain_db: 0.0,
        delay_ms: 0.0,
        polarity: 1,
        allpass: vec![],
    };
    let display_smoother =
        PowerSmoother::new(design_freqs(), Fraction::Scalar(12.0)).expect("design grid");
    let metric_smoother =
        PowerSmoother::new(design_freqs(), Fraction::Scalar(6.0)).expect("design grid");
    let mut each = [Vec::new(), Vec::new()];
    let mut sums = [Vec::new(), Vec::new()];
    let mut flat = [0.0; 2];
    let mut seat = [0.0; 2];
    for state in 0..2 {
        let mut levels = Vec::new();
        for &p in &positions {
            let mut sum = vec![C::new(0.0, 0.0); design_freqs().len()];
            for (role, row) in &sub.rows {
                let spectra = if state == 0 {
                    &design.spectra.raw
                } else {
                    &design.spectra.used
                };
                let h = spectra.get(p, role).expect("complete sub position");
                let mut out = term(h, design_freqs(), if state == 0 { &raw } else { row });
                if state == 1
                    && let Some(bus) = &sub.bus_response
                {
                    for (h, b) in out.iter_mut().zip(bus.iter()) {
                        *h *= b;
                    }
                }
                for (s, h) in sum.iter_mut().zip(&out) {
                    *s += h;
                }
                if p == design.spectra.main_position {
                    each[state].push((role.clone(), curve(&out, &grid, &display_smoother)));
                }
            }
            let level = curve(&sum, &metric_grid, &metric_smoother);
            if p == design.spectra.main_position {
                flat[state] = std(&level);
                sums[state] = curve(&sum, &grid, &display_smoother);
            }
            levels.push(level);
        }
        if positions.len() > 1 {
            // NumPy reduces axis 0 sequentially for this C-contiguous matrix.
            let deviations: Vec<_> = (0..metric_grid.len())
                .map(|i| {
                    let m = levels.iter().fold(0.0, |s, v| s + v[i]) / levels.len() as f64;
                    (levels.iter().fold(0.0, |s, v| s + (v[i] - m).powi(2)) / levels.len() as f64)
                        .sqrt()
                })
                .collect();
            seat[state] = mean(&deviations);
        }
    }
    let values = design
        .chain
        .subs
        .iter()
        .map(|s| SubValue {
            role: s.role.clone(),
            gain_db: s.gain_db,
            delay_ms: s.delay_ms,
            polarity: s.polarity,
            allpass: s.allpass.clone(),
            mode: s.mode.clone(),
            peq: s.peq.clone(),
        })
        .collect();
    let [before, after] = each;
    let [sum_before, sum_after] = sums;
    Some(SubCombine {
        subs,
        grid,
        before,
        after,
        sum_before,
        sum_after,
        band,
        flat: (flat[0], flat[1]),
        seat: (positions.len() > 1).then_some((seat[0], seat[1])),
        positions: positions.len(),
        values,
        layout_info: sub.layout_info.clone(),
    })
}
