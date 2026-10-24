//! Bass alignment, ported from `correction_engine.py:624–1095,1371–1428`.
//! D1 is deliberately not applied: bus delay still penalizes coherent addition.

use crate::chain::{BaRow, PreparedBaTerm, gather, sub_sum, sub_sum_prepared};
use crate::decay::{BaLateGrid, ba_late_db};
use crate::fit::{Fitter, xo_target_peq};
use crate::grid::{BINS, SR_F, design_freqs};
use crate::input::{BandType, BundleGain, DbaRow, LayoutKind, Peq, SubAllpass, SubMode};
use crate::measurement::{Prepared, Spectra, onset_index, ramp_phase};
use crate::notes::Note;
use crate::setup::{Setup, SubFilter};
use crate::{C, EngineConfig, EngineError, MeasurementSet, Tuning};
use secs_compute::CancelToken;
use secs_dsp::optimize::{DifferentialEvolutionOptions, differential_evolution};
use secs_dsp::{
    fft, pairwise_sum, response,
    smooth::{self, Fraction},
};
use std::{f64::consts::PI, sync::Arc};

pub struct BaInput<'a> {
    pub measurements: &'a MeasurementSet,
    pub prepared: &'a Prepared,
    pub setup: &'a Setup,
    pub config: &'a EngineConfig,
    pub tuning: &'a Tuning,
    pub xo_of: &'a [(String, f64)],
    pub ref_level: f64,
    pub cancel: &'a CancelToken,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManualPlan {
    pub fixed: Vec<String>,
    pub bundles: Vec<(String, Vec<String>)>,
    pub auto: Vec<String>,
    pub gain_anchored: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutInfo {
    pub kind: LayoutKind,
    pub members: Vec<String>,
    pub gains: Vec<(String, f64)>,
    pub front: Vec<String>,
    pub rear: Vec<String>,
    pub rear_ms: f64,
    pub auto_ms: Option<f64>,
    pub trim_ms: f64,
    pub min_ms: Option<f64>,
    pub peq: Vec<Peq>,
    #[cfg(test)]
    candidates: Vec<(f64, f64)>,
}

pub struct SubAlignment {
    pub rows: Vec<BaRow>,
    pub sub_filters: Vec<(String, SubFilter)>,
    pub h_subs: Vec<(u32, String, Arc<[C]>)>,
    pub layout_rel: Vec<(String, Vec<(String, BaRow)>)>,
    pub layout_info: Vec<(String, LayoutInfo)>,
    pub sub_level_db: Option<f64>,
    pub bus_gain: f64,
    pub bus_peq: Vec<Peq>,
    pub sub_sum_by_pos: Vec<(u32, Arc<[C]>)>,
    pub plan: Option<ManualPlan>,
    pub notes: Vec<Note>,
}

fn check(input: &BaInput<'_>) -> Result<(), EngineError> {
    if input.cancel.is_cancelled() {
        Err(EngineError::Cancelled)
    } else {
        Ok(())
    }
}
fn row(gain_db: f64, delay_ms: f64, polarity: i8, allpass: Vec<(f64, f64)>) -> BaRow {
    BaRow {
        gain_db,
        delay_ms,
        polarity,
        allpass,
    }
}
fn mean(x: &[f64]) -> f64 {
    pairwise_sum(x) / x.len() as f64
}
fn median(mut x: Vec<f64>) -> f64 {
    x.sort_by(f64::total_cmp);
    let m = x.len() / 2;
    if x.len().is_multiple_of(2) {
        (x[m - 1] + x[m]) / 2.0
    } else {
        x[m]
    }
}
fn mag(x: C) -> f64 {
    x.re.hypot(x.im)
}
fn power(x: C) -> f64 {
    let a = mag(x);
    a * a
}
fn weight(pos: u32, main: u32) -> f64 {
    if pos == main { 5.0 } else { 1.0 }
}
fn base(input: &BaInput<'_>, role: &str) -> f64 {
    input
        .prepared
        .align_delay_ms
        .iter()
        .find(|(r, _)| r == role)
        .unwrap()
        .1
}
fn complete(h: &Spectra, roles: &[String]) -> Vec<u32> {
    h.positions()
        .iter()
        .copied()
        .filter(|&p| roles.iter().all(|r| h.get(p, r).is_some()))
        .collect()
}
fn sums(
    h: &Spectra,
    roles: &[String],
    rows: &[BaRow],
    idx: &[usize],
    positions: &[u32],
) -> Vec<(u32, Arc<[C]>)> {
    let f = gather(design_freqs(), idx);
    positions
        .iter()
        .map(|&p| {
            let cells: Vec<_> = roles
                .iter()
                .map(|r| gather(h.get(p, r).unwrap(), idx))
                .collect();
            let terms: Vec<_> = cells
                .iter()
                .zip(rows)
                .map(|(h, r)| (h.as_slice(), r))
                .collect();
            (p, sub_sum(&terms, &f).into())
        })
        .collect()
}
fn grid(hi: f64) -> Vec<usize> {
    let f = design_freqs();
    let lo = f.partition_point(|f| *f < 20.0);
    let hi = f.partition_point(|f| *f <= hi) - 1;
    let start = (lo as f64).log10();
    let stop = (hi as f64).log10();
    let mut out: Vec<_> = (0..160)
        .map(|i| {
            let v = if i == 159 {
                stop
            } else {
                start + i as f64 * ((stop - start) / 159.0)
            };
            10f64.powf(v).round_ties_even() as usize
        })
        .collect();
    out.dedup();
    out
}

/// NumPy polyfit: descending Vandermonde, strided column norm, scaled SVD,
/// rcond = n * eps. A three-column one-sided Jacobi SVD avoids normal equations.
/// LAPACK's divide-and-conquer SVD has different final bits; the oracle measures
/// their effect on the objective, rather than claiming bit-identical coefficients.
struct Quadratic {
    x: Vec<f64>,
    columns: [Vec<f64>; 3],
    right: [[f64; 3]; 3],
    scale: [f64; 3],
    norm: [f64; 3],
}
impl Quadratic {
    fn new(x: Vec<f64>) -> Self {
        let mut columns = [
            x.iter().map(|v| v * v).collect::<Vec<_>>(),
            x.clone(),
            vec![1.0; x.len()],
        ];
        // NumPy axis=0 on its C-order Vandermonde is a sequential reduction.
        let scale = std::array::from_fn(|j| columns[j].iter().fold(0.0, |s, v| s + v * v).sqrt());
        for j in 0..3 {
            for v in &mut columns[j] {
                *v /= scale[j];
            }
        }
        let mut right = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        for _ in 0..30 {
            let mut changed = false;
            for (p, q) in [(0, 1), (0, 2), (1, 2)] {
                let a = columns[p].iter().map(|v| v * v).sum::<f64>();
                let b = columns[q].iter().map(|v| v * v).sum::<f64>();
                let c = columns[p]
                    .iter()
                    .zip(&columns[q])
                    .map(|(v, w)| v * w)
                    .sum::<f64>();
                if c.abs() <= f64::EPSILON * (a * b).sqrt() {
                    continue;
                }
                changed = true;
                let z = (b - a) / (2.0 * c);
                let t = z.signum() / (z.abs() + (1.0 + z * z).sqrt());
                let co = 1.0 / (1.0 + t * t).sqrt();
                let si = co * t;
                let (left, right_column) = columns.split_at_mut(q);
                for (a, b) in left[p].iter_mut().zip(&mut right_column[0]) {
                    let old_a = *a;
                    let old_b = *b;
                    *a = co * old_a - si * old_b;
                    *b = si * old_a + co * old_b;
                }
                for r in &mut right {
                    let a = r[p];
                    let b = r[q];
                    r[p] = co * a - si * b;
                    r[q] = si * a + co * b;
                }
            }
            if !changed {
                break;
            }
        }
        let norm = std::array::from_fn(|j| {
            pairwise_sum(&columns[j].iter().map(|v| v * v).collect::<Vec<_>>())
        });
        Self {
            x,
            columns,
            right,
            scale,
            norm,
        }
    }
    fn rough(&self, y: &[f64]) -> f64 {
        let max = self.norm.iter().copied().fold(0.0, f64::max).sqrt();
        let mut coef = [0.0; 3];
        for j in 0..3 {
            if self.norm[j].sqrt() <= self.x.len() as f64 * f64::EPSILON * max {
                continue;
            }
            let dot = pairwise_sum(
                &self.columns[j]
                    .iter()
                    .zip(y)
                    .map(|(a, b)| a * b)
                    .collect::<Vec<_>>(),
            ) / self.norm[j];
            for (i, c) in coef.iter_mut().enumerate() {
                *c += self.right[i][j] * dot;
            }
        }
        for (c, s) in coef.iter_mut().zip(self.scale) {
            *c /= s;
        }
        mean(
            &self
                .x
                .iter()
                .zip(y)
                .map(|(x, y)| {
                    let d = y - ((coef[0] * x + coef[1]) * x + coef[2]);
                    d * d
                })
                .collect::<Vec<_>>(),
        )
    }
}
fn variance(levels: &[Vec<f64>]) -> f64 {
    if levels.len() <= 1 {
        return 0.0;
    }
    let mut out = vec![0.0; levels[0].len()];
    for i in 0..out.len() {
        let m = levels.iter().fold(0.0, |s, l| s + l[i]) / levels.len() as f64;
        out[i] = levels.iter().fold(0.0, |s, l| s + (l[i] - m).powi(2)) / levels.len() as f64;
    }
    mean(&out)
}
fn gd_variance(h: &[C], f: &[f64]) -> f64 {
    let mut phase: Vec<_> = h.iter().map(|h| h.im.atan2(h.re)).collect();
    let original = phase.clone();
    let mut correction = 0.0;
    for i in 1..phase.len() {
        let d = original[i] - original[i - 1];
        let mut dm = (d + PI).rem_euclid(2.0 * PI) - PI;
        if dm == -PI && d > 0.0 {
            dm = PI;
        }
        if d.abs() >= PI {
            correction += dm - d;
        }
        phase[i] += correction;
    }
    let x: Vec<_> = f.iter().map(|f| 2.0 * PI * f).collect();
    let n = x.len();
    let mut gd = vec![0.0; n];
    gd[0] = (phase[1] - phase[0]) / (x[1] - x[0]);
    gd[n - 1] = (phase[n - 1] - phase[n - 2]) / (x[n - 1] - x[n - 2]);
    for i in 1..n - 1 {
        let a = x[i] - x[i - 1];
        let b = x[i + 1] - x[i];
        gd[i] = (-b / (a * (a + b))) * phase[i - 1]
            + ((b - a) / (a * b)) * phase[i]
            + (a / (b * (a + b))) * phase[i + 1];
    }
    for v in &mut gd {
        *v = -*v * 1000.0;
    }
    let m = median(gd.clone());
    mean(&gd.iter().map(|v| (v - m).powi(2)).collect::<Vec<_>>())
}

fn individual_db(power: &[Vec<f64>]) -> Vec<Vec<f64>> {
    power
        .iter()
        .map(|row| row.iter().map(|p| 10.0 * (p + 1e-30).log10()).collect())
        .collect()
}

struct BaSetup {
    grid: Vec<usize>,
    positions: Vec<u32>,
    late: BaLateGrid,
    raw_late: Vec<f64>,
    h_grid: Vec<Vec<Vec<C>>>,
    h_lin: Vec<Vec<Vec<C>>>,
    f: Vec<f64>,
    quadratic: Quadratic,
}
impl BaSetup {
    fn new(input: &BaInput<'_>, h: &Spectra, hi: f64) -> Self {
        let grid = grid(hi);
        let positions = complete(h, &input.config.subs);
        let f = gather(design_freqs(), &grid);
        let late = BaLateGrid::new(hi);
        let h_grid = positions
            .iter()
            .map(|&p| {
                input
                    .config
                    .subs
                    .iter()
                    .map(|r| gather(h.get(p, r).unwrap(), &grid))
                    .collect()
            })
            .collect();
        let h_lin: Vec<Vec<Vec<C>>> = positions
            .iter()
            .map(|&p| {
                input
                    .config
                    .subs
                    .iter()
                    .map(|r| h.get(p, r).unwrap()[..late.lin].to_vec())
                    .collect()
            })
            .collect();
        let rows: Vec<_> = input
            .config
            .subs
            .iter()
            .map(|r| row(0.0, base(input, r), 1, vec![]))
            .collect();
        let raw_late = h_lin
            .iter()
            .map(|hs| {
                let terms: Vec<_> = hs
                    .iter()
                    .zip(&rows)
                    .map(|(h, r)| (h.as_slice(), r))
                    .collect();
                ba_late_db(&late, &sub_sum(&terms, &design_freqs()[..late.lin]))
            })
            .collect();
        let quadratic = Quadratic::new(f.iter().map(|f| f.ln()).collect());
        Self {
            grid,
            positions,
            late,
            raw_late,
            h_grid,
            h_lin,
            f,
            quadratic,
        }
    }
    fn indiv(&self, rows: Option<&[BaRow]>) -> Vec<Vec<f64>> {
        self.h_grid
            .iter()
            .map(|hs| {
                let mut total = vec![0.0; self.grid.len()];
                for (j, h) in hs.iter().enumerate() {
                    let g = rows.map_or(1.0, |r| 10f64.powf(r[j].gain_db / 10.0));
                    for (s, &v) in total.iter_mut().zip(h) {
                        *s += power(v) * g;
                    }
                }
                total
            })
            .collect()
    }
    // Existing cost-oracle callers supply powers. Production precomputes the
    // fixed reference in dB once before entering differential evolution.
    #[cfg(test)]
    fn cost(
        &self,
        input: &BaInput<'_>,
        rows: &[BaRow],
        fixed_indiv: Option<&[Vec<f64>]>,
        bus: Option<&[C]>,
    ) -> f64 {
        let fixed_db = fixed_indiv.map(individual_db);
        self.cost_with_db(input, rows, fixed_db.as_deref(), bus)
    }

    fn cost_with_db(
        &self,
        input: &BaInput<'_>,
        rows: &[BaRow],
        fixed_indiv: Option<&[Vec<f64>]>,
        bus: Option<&[C]>,
    ) -> f64 {
        if input.cancel.is_cancelled() {
            return 1e30;
        }
        let owned;
        let indiv = if let Some(i) = fixed_indiv {
            i
        } else {
            owned = individual_db(&self.indiv(Some(rows)));
            &owned
        };
        let grid_terms: Vec<_> = rows
            .iter()
            .map(|r| PreparedBaTerm::new(&self.f, r))
            .collect();
        let lin_terms: Vec<_> = rows
            .iter()
            .map(|r| PreparedBaTerm::new(&design_freqs()[..self.late.lin], r))
            .collect();
        let mut levels = Vec::new();
        let mut total = 0.0;
        let mut weights = 0.0;
        for (k, &pos) in self.positions.iter().enumerate() {
            let sum = sub_sum_prepared(&self.h_grid[k], &grid_terms, self.f.len());
            let db: Vec<_> = sum
                .iter()
                .map(|&h| 20.0 * (mag(h) + 1e-20).log10())
                .collect();
            let ind = &indiv[k];
            let rough = self.quadratic.rough(&db);
            let loss = mean(
                &ind.iter()
                    .zip(&db)
                    .map(|(i, d)| (i - d - 3.0).max(0.0).powi(2))
                    .collect::<Vec<_>>(),
            );
            let eff = mean(&db.iter().zip(ind).map(|(d, i)| d - i).collect::<Vec<_>>());
            let with_bus: Vec<_> = sum
                .iter()
                .zip(&self.grid)
                .map(|(&s, &i)| bus.map_or(s, |b| s * b[i]))
                .collect();
            let gd = gd_variance(&with_bus, &self.f);
            let ls = sub_sum_prepared(&self.h_lin[k], &lin_terms, self.late.lin);
            let worse = (ba_late_db(&self.late, &ls) - self.raw_late[k]).max(0.0);
            let w = weight(pos, input.config.main_position);
            weights += w;
            total += w
                * (rough + 0.5 * loss + 0.02 * gd + 0.5 * worse * worse
                    - input.tuning.ba_eff_weight * eff);
            levels.push(db);
        }
        total / weights + 0.5 * variance(&levels)
    }
}

fn manual_value(setup: &Setup, role: &str) -> BaRow {
    setup
        .sub_manual
        .iter()
        .find(|(r, _)| r == role)
        .map_or_else(
            || row(0.0, 0.0, 1, vec![]),
            |(_, m)| {
                row(
                    m.gain_db,
                    m.delay_ms,
                    if m.polarity < 0 { -1 } else { 1 },
                    m.allpass.iter().take(2).copied().collect(),
                )
            },
        )
}
fn manual_plan(input: &BaInput<'_>) -> Option<ManualPlan> {
    let mut p = ManualPlan {
        fixed: vec![],
        bundles: vec![],
        auto: vec![],
        gain_anchored: false,
    };
    for r in &input.config.subs {
        match input
            .setup
            .sub_manual
            .iter()
            .find(|(s, _)| s == r)
            .map(|(_, m)| m)
        {
            Some(m) if m.mode == SubMode::Fixed => p.fixed.push(r.clone()),
            Some(m) if m.mode == SubMode::Bundle => {
                let name = if m.bundle.is_empty() { "A" } else { &m.bundle };
                if let Some((_, rs)) = p.bundles.iter_mut().find(|(b, _)| b == name) {
                    rs.push(r.clone());
                } else {
                    p.bundles.push((name.to_owned(), vec![r.clone()]));
                }
            }
            _ => p.auto.push(r.clone()),
        }
    }
    if p.fixed.is_empty() && p.bundles.is_empty() {
        return None;
    }
    p.gain_anchored = !p.fixed.is_empty()
        || (!p.bundles.is_empty() && input.tuning.bundle_gain == BundleGain::Fixed);
    Some(p)
}

struct Unit {
    roles: Vec<(String, BaRow)>,
    center: f64,
    gain_var: bool,
    ap_var: bool,
    auto: bool,
}
struct Search {
    bounds: Vec<(f64, f64)>,
    units: Vec<Unit>,
    constants: Vec<(String, BaRow)>,
    fixed_indiv: bool,
    skip: bool,
}
impl Search {
    fn new(input: &BaInput<'_>, alignment: &SubAlignment) -> Self {
        let qmax = if input.tuning.sub_allpass == SubAllpass::LowQ {
            1.0
        } else {
            3.0
        };
        let aps = [(20.0, 150.0), (0.3, qmax), (20.0, 150.0), (0.3, qmax)];
        let mut out = Self {
            bounds: vec![],
            units: vec![],
            constants: vec![],
            fixed_indiv: true,
            skip: false,
        };
        let Some(plan) = &alignment.plan else {
            for r in &input.config.subs {
                out.bounds.extend([
                    (-8.0, 0.0),
                    (-input.tuning.ba_delay_ms, input.tuning.ba_delay_ms),
                    (0.0, 1.0),
                ]);
                out.bounds.extend(aps);
                out.units.push(Unit {
                    roles: vec![(r.clone(), row(0.0, 0.0, 1, vec![]))],
                    center: base(input, r),
                    gain_var: true,
                    ap_var: true,
                    auto: true,
                });
            }
            out.skip = input.config.subs.len() == 1;
            return out;
        };
        let extra = if plan.fixed.is_empty() {
            0.0
        } else {
            input.config.sub_main_delay_ms
        };
        out.constants = plan
            .fixed
            .iter()
            .map(|r| {
                let mut v = manual_value(input.setup, r);
                v.delay_ms -= extra;
                (r.clone(), v)
            })
            .collect();
        let off = if plan.fixed.is_empty() {
            0.0
        } else {
            median(
                out.constants
                    .iter()
                    .map(|(r, v)| v.delay_ms - base(input, r))
                    .collect(),
            )
        };
        let mut anchored = plan.fixed.clone();
        if input.tuning.bundle_gain == BundleGain::Fixed {
            anchored.extend(plan.bundles.iter().flat_map(|(_, rs)| rs).cloned());
        }
        let reference = (!anchored.is_empty()).then(|| {
            mean(
                &anchored
                    .iter()
                    .map(|r| manual_value(input.setup, r).gain_db)
                    .collect::<Vec<_>>(),
            )
        });
        out.fixed_indiv = reference.is_none();
        let gain_bounds =
            |rel: f64| reference.map_or((-rel - 8.0, -rel), |r| (r - rel - 8.0, r - rel + 8.0));
        let delay_bounds = |center: f64| {
            let lo = if plan.fixed.is_empty() {
                -input.tuning.ba_delay_ms
            } else {
                (-input.tuning.ba_delay_ms).max(-extra - center)
            };
            (lo, input.tuning.ba_delay_ms.max(lo + 1.0))
        };
        for r in &plan.auto {
            let center = base(input, r) + off;
            out.bounds
                .extend([gain_bounds(0.0), delay_bounds(center), (0.0, 1.0)]);
            out.bounds.extend(aps);
            out.units.push(Unit {
                roles: vec![(r.clone(), row(0.0, 0.0, 1, vec![]))],
                center,
                gain_var: true,
                ap_var: true,
                auto: true,
            });
        }
        for (name, rs) in &plan.bundles {
            let layout = alignment.layout_rel.iter().find(|(b, _)| b == name);
            let mut vals = layout.map_or_else(
                || {
                    rs.iter()
                        .map(|r| (r.clone(), manual_value(input.setup, r)))
                        .collect()
                },
                |(_, v)| v.clone(),
            );
            let dmin = vals
                .iter()
                .map(|(_, v)| v.delay_ms)
                .fold(f64::INFINITY, f64::min);
            let center = vals
                .iter()
                .map(|(r, v)| base(input, r) - (v.delay_ms - dmin))
                .fold(f64::NEG_INFINITY, f64::max)
                + off;
            let gain_var = input.tuning.bundle_gain == BundleGain::Auto || layout.is_some();
            let ap_var = layout.is_some() && input.tuning.sub_allpass != SubAllpass::None;
            if gain_var {
                out.bounds.push(gain_bounds(mean(
                    &vals.iter().map(|(_, v)| v.gain_db).collect::<Vec<_>>(),
                )));
            }
            out.bounds.extend([delay_bounds(center), (0.0, 1.0)]);
            if ap_var {
                out.bounds.extend(aps);
            }
            for (_, v) in &mut vals {
                v.delay_ms -= dmin;
            }
            out.units.push(Unit {
                roles: vals,
                center,
                gain_var,
                ap_var,
                auto: false,
            });
        }
        out.skip = out.units.is_empty() || (out.units.len() == 1 && plan.fixed.is_empty());
        out
    }
    fn unpack(&self, input: &BaInput<'_>, x: &[f64]) -> Vec<BaRow> {
        let mut out = self.constants.clone();
        let mut k = 0;
        for u in &self.units {
            let gain = if u.gain_var {
                let v = x[k];
                k += 1;
                v
            } else {
                0.0
            };
            let d = x[k];
            let pol = if x[k + 1] < 0.5 { 1 } else { -1 };
            k += 2;
            let mut aps = vec![];
            if u.ap_var {
                if input.tuning.sub_allpass != SubAllpass::None
                    && (u.auto || (x[k] > 0.0 && x[k + 2] > 0.0))
                {
                    aps.extend([(x[k], x[k + 1]), (x[k + 2], x[k + 3])]);
                }
                k += 4;
            }
            for (r, v) in &u.roles {
                let mut a = v.allpass.clone();
                a.extend_from_slice(&aps);
                out.push((
                    r.clone(),
                    row(
                        v.gain_db + gain,
                        u.center + v.delay_ms + d,
                        v.polarity * pol,
                        a,
                    ),
                ));
            }
        }
        input
            .config
            .subs
            .iter()
            .map(|r| out.iter().find(|(s, _)| s == r).unwrap().1.clone())
            .collect()
    }
}

fn level(input: &BaInput<'_>, by_pos: &[(u32, Arc<[C]>)]) -> f64 {
    let h = &by_pos
        .iter()
        .find(|(p, _)| *p == input.config.main_position)
        .unwrap_or(&by_pos[0])
        .1;
    let indices: Vec<_> = design_freqs()
        .iter()
        .enumerate()
        .filter(|(_, f)| **f >= 30.0 && **f <= 80.0)
        .map(|(i, _)| i)
        .collect();
    let target = input
        .config
        .target
        .sample(&gather(design_freqs(), &indices));
    median(
        indices
            .iter()
            .zip(target)
            .map(|(&i, t)| 20.0 * (mag(h[i]) + 1e-20).log10() - t)
            .collect(),
    )
}
#[allow(clippy::too_many_arguments)]
fn target_peq(
    input: &BaInput<'_>,
    by_pos: &[(u32, Arc<[C]>)],
    lvl: f64,
    hi: f64,
    bands: usize,
    boost: f64,
    fmax: Option<f64>,
    fitter: &mut Fitter<'_>,
) -> Vec<Peq> {
    let by_pos: Vec<_> = by_pos
        .iter()
        .map(|(p, h)| (weight(*p, input.config.main_position), h.as_ref()))
        .collect();
    xo_target_peq(
        &by_pos,
        &input.config.target,
        lvl,
        20.0,
        hi,
        bands,
        boost,
        fmax,
        fitter,
    )
}
fn bus_fit(input: &BaInput<'_>, by_pos: &[(u32, Arc<[C]>)], fitter: &mut Fitter<'_>) -> Vec<Peq> {
    if !input.tuning.xo_stage || input.xo_of.is_empty() {
        return vec![];
    }
    let xo: Vec<_> = input.xo_of.iter().map(|(_, v)| *v).collect();
    let hi = if input.tuning.sub_bus_fix {
        1.25 * xo.iter().copied().fold(f64::NEG_INFINITY, f64::max)
    } else {
        2.0 * mean(&xo)
    };
    target_peq(
        input,
        by_pos,
        input.ref_level,
        hi,
        10,
        6.0_f64.min(input.config.max_boost_db),
        input.tuning.sub_bus_fix.then_some(hi),
        fitter,
    )
}
fn bus_response(
    input: &BaInput<'_>,
    h: &Spectra,
    alignment: &SubAlignment,
    rows: &[BaRow],
    fitter: &mut Fitter<'_>,
) -> Result<Option<Vec<C>>, EngineError> {
    check(input)?;
    if !input.tuning.xo_stage || input.xo_of.is_empty() {
        return Ok(None);
    }
    let mut by_pos = sums(
        h,
        &input.config.subs,
        rows,
        &(0..BINS).collect::<Vec<_>>(),
        &complete(h, &input.config.subs),
    );
    if !by_pos.iter().any(|(p, _)| *p == input.config.main_position) {
        return Ok(None);
    }
    if !alignment.plan.as_ref().is_some_and(|p| p.gain_anchored) {
        let gain = 10f64.powf((input.ref_level - level(input, &by_pos)) / 20.0);
        for (_, h) in &mut by_pos {
            *h = h.iter().map(|v| *v * C::new(gain, 0.0)).collect();
        }
    }
    let peq = bus_fit(input, &by_pos, fitter);
    check(input)?;
    Ok((!peq.is_empty()).then(|| response::peq_response(design_freqs(), &peq)))
}

// The layout closure is kept separate from BaLateGrid: it is its own upstream
// definition, even though its current window endpoints happen to be identical.
fn layout_late(spec: &[C], band: &[f64], fs: f64) -> f64 {
    let y = fft::irfft(
        &spec
            .iter()
            .zip(band)
            .map(|(s, b)| s * b)
            .collect::<Vec<_>>(),
        2 * (spec.len() - 1),
    );
    let top = y.iter().map(|v| v.abs()).fold(0.0, f64::max);
    let k = y.iter().position(|v| v.abs() > 0.1 * top).unwrap_or(0);
    let energy = |a: f64, b: f64| {
        pairwise_sum(
            &y[(k + (a * fs) as usize).min(y.len())..(k + (b * fs) as usize).min(y.len())]
                .iter()
                .map(|v| v * v)
                .collect::<Vec<_>>(),
        )
    };
    10.0 * (energy(0.1, 0.38) / (energy(0.0, 0.05) + 1e-30) + 1e-30).log10()
}
fn layout_stage(
    input: &BaInput<'_>,
    h: &mut Spectra,
    out: &mut SubAlignment,
    hi: f64,
    fitter: &mut Fitter<'_>,
) -> Result<(), EngineError> {
    let f = design_freqs();
    let grid = grid(hi);
    let fg = gather(f, &grid);
    let quadratic = Quadratic::new(fg.iter().map(|f| f.ln()).collect());
    let lin = f.partition_point(|f| *f < 200.0);
    let li: Vec<_> = (0..lin).collect();
    let band: Vec<_> = f[..lin]
        .iter()
        .map(|f| ((f - 15.0) / 5.0).clamp(0.0, 1.0) * ((hi * 1.3 - f) / (hi * 0.3)).clamp(0.0, 1.0))
        .collect();
    let mask: Vec<_> = f
        .iter()
        .enumerate()
        .filter(|(_, f)| **f >= 20.0 && **f <= hi)
        .map(|(i, _)| i)
        .collect();
    for (name, lay) in &input.setup.layouts {
        check(input)?;
        let positions = complete(h, &lay.members);
        if positions.is_empty() {
            return Err(EngineError::Input(format!(
                "bundle {name}: no complete position"
            )));
        }
        let levels: Vec<_> = lay
            .members
            .iter()
            .map(|r| {
                let mut pw = vec![0.0; mask.len()];
                let mut total = 0.0;
                for &p in h.positions() {
                    if let Some(h) = h.get(p, r) {
                        let w = weight(p, input.config.main_position);
                        total += w;
                        for (v, &i) in pw.iter_mut().zip(&mask) {
                            *v += w * power(h[i]);
                        }
                    }
                }
                median(
                    pw.iter()
                        .map(|v| 10.0 * (v / total + 1e-30).log10())
                        .collect(),
                )
            })
            .collect();
        let low = levels.iter().copied().fold(f64::INFINITY, f64::min);
        let gains: Vec<_> = lay
            .members
            .iter()
            .cloned()
            .zip(levels.iter().map(|v| low - v))
            .collect();
        let rear: Vec<_> = lay
            .members
            .iter()
            .filter(|r| {
                input
                    .setup
                    .sub_manual
                    .iter()
                    .any(|(s, m)| s == *r && m.dba_row == DbaRow::Rear)
            })
            .cloned()
            .collect();
        let front: Vec<_> = lay
            .members
            .iter()
            .filter(|r| !rear.contains(r))
            .cloned()
            .collect();
        let mut info = LayoutInfo {
            kind: lay.kind,
            members: lay.members.clone(),
            gains: gains.clone(),
            front: front.clone(),
            rear: vec![],
            rear_ms: 0.0,
            auto_ms: None,
            trim_ms: 0.0,
            min_ms: None,
            peq: vec![],
            #[cfg(test)]
            candidates: vec![],
        };
        let mut rows: Vec<_> = gains.iter().map(|(_, g)| row(*g, 0.0, 1, vec![])).collect();
        if lay.kind == LayoutKind::Dba {
            if front.is_empty() || rear.is_empty() {
                info.kind = LayoutKind::Multi;
                out.notes.push(Note::LayoutFallback(name.clone()));
            } else {
                let arrival = |p, r: &str| -> Result<f64, EngineError> {
                    let m = input
                        .measurements
                        .get(p, r)
                        .ok_or_else(|| EngineError::Input(format!("missing DBA IR {p}/{r}")))?;
                    Ok(1000.0 * (m.t0 + onset_index(&m.samples) as f64 / SR_F))
                };
                let mut gap = 0.0_f64;
                for &p in &positions {
                    let r = rear
                        .iter()
                        .map(|r| arrival(p, r))
                        .collect::<Result<Vec<_>, _>>()?;
                    let fr = front
                        .iter()
                        .map(|r| arrival(p, r))
                        .collect::<Result<Vec<_>, _>>()?;
                    gap = gap.max((mean(&r) - mean(&fr)).abs());
                }
                let lo = (gap.min(59.0) * 4.0).ceil() / 4.0;
                let mut best = (f64::INFINITY, 0.0);
                for k in 0..=((60.0 - lo) * 4.0) as usize {
                    check(input)?;
                    let delay = lo + k as f64 * 0.25;
                    let trial: Vec<_> = gains
                        .iter()
                        .map(|(r, g)| {
                            row(
                                *g,
                                if rear.contains(r) { delay } else { 0.0 },
                                if rear.contains(r) { -1 } else { 1 },
                                vec![],
                            )
                        })
                        .collect();
                    let gs = sums(h, &lay.members, &trial, &grid, &positions);
                    let ls = sums(h, &lay.members, &trial, &li, &positions);
                    let mut cost = 0.0;
                    let mut wsum = 0.0;
                    let mut dbs = vec![];
                    for ((p, g), (_, l)) in gs.iter().zip(&ls) {
                        let w = weight(*p, input.config.main_position);
                        wsum += w;
                        let db: Vec<_> =
                            g.iter().map(|v| 20.0 * (mag(*v) + 1e-20).log10()).collect();
                        cost += w * quadratic.rough(&db);
                        cost += w * 0.2 * layout_late(l, &band, 2.0 * f[lin - 1]);
                        dbs.push(db);
                    }
                    cost = cost / wsum + 0.5 * variance(&dbs);
                    #[cfg(test)]
                    info.candidates.push((delay, cost));
                    if cost < best.0 {
                        best = (cost, delay);
                    }
                }
                let delay = (best.1 + lay.rear_trim_ms).max(0.0);
                for ((r, _), v) in gains.iter().zip(&mut rows) {
                    if rear.contains(r) {
                        v.delay_ms = delay;
                        v.polarity = -1;
                    }
                }
                info.rear = rear;
                info.rear_ms = delay;
                info.auto_ms = Some(best.1);
                info.trim_ms = lay.rear_trim_ms;
                info.min_ms = Some(gap);
            }
        }
        let by_pos = sums(
            h,
            &lay.members,
            &rows,
            &(0..BINS).collect::<Vec<_>>(),
            &positions,
        );
        let peq = target_peq(
            input,
            &by_pos,
            level(input, &by_pos),
            hi,
            6,
            6.0,
            None,
            fitter,
        );
        check(input)?;
        if !peq.is_empty() {
            let response = response::peq_response(f, &peq);
            for r in &lay.members {
                if !out.sub_filters.iter().any(|(s, _)| s == r) {
                    out.sub_filters.push((
                        r.clone(),
                        SubFilter {
                            hp_hz: 0.0,
                            hp_type: BandType::Lr24,
                            lp_hz: 0.0,
                            lp_type: BandType::Lr24,
                            peq: vec![],
                        },
                    ));
                }
                out.sub_filters
                    .iter_mut()
                    .find(|(s, _)| s == r)
                    .unwrap()
                    .1
                    .peq
                    .extend_from_slice(&peq);
                for p in h.positions().to_vec() {
                    if let Some(old) = h.get(p, r) {
                        let new: Arc<[C]> = old.iter().zip(&response).map(|(a, b)| a * b).collect();
                        h.set(p, r, new.clone());
                        out.h_subs.push((p, r.clone(), new));
                    }
                }
            }
        }
        info.peq = peq;
        out.layout_rel.push((
            name.clone(),
            lay.members.iter().cloned().zip(rows).collect(),
        ));
        out.layout_info.push((name.clone(), info));
        out.notes.push(Note::Layout(name.clone()));
    }
    Ok(())
}

fn two_way(input: &BaInput<'_>, h: &Spectra, out: &mut SubAlignment) -> Result<usize, EngineError> {
    let filters = &out.sub_filters;
    let low: Vec<_> = input
        .config
        .subs
        .iter()
        .filter(|r| {
            filters
                .iter()
                .any(|(s, f)| s == *r && f.lp_hz > 0.0 && f.hp_hz == 0.0)
        })
        .cloned()
        .collect();
    let high: Vec<_> = input
        .config
        .subs
        .iter()
        .filter(|r| !low.contains(r))
        .collect();
    if low.is_empty() || high.is_empty() {
        return Ok(0);
    }
    let xo = filters.iter().find(|(r, _)| r == &low[0]).unwrap().1.lp_hz;
    if high.iter().any(|r| {
        (filters
            .iter()
            .find(|(s, _)| s == *r)
            .map_or(0.0, |(_, f)| f.hp_hz)
            - xo)
            .abs()
            > 0.5
    }) {
        return Ok(0);
    }
    let movable: Vec<_> = low
        .into_iter()
        .filter(|r| !out.plan.as_ref().is_some_and(|p| p.fixed.contains(r)))
        .collect();
    if movable.is_empty() {
        return Ok(0);
    }
    let f = design_freqs();
    let lo: Vec<_> = f
        .iter()
        .enumerate()
        .filter(|(_, f)| **f >= 12.0_f64.max(xo * 0.55) && **f <= xo * 0.85)
        .map(|(i, _)| i)
        .collect();
    let hi: Vec<_> = f
        .iter()
        .enumerate()
        .filter(|(_, f)| **f >= xo * 1.3 && **f <= 80.0_f64.min(xo * 3.2))
        .map(|(i, _)| i)
        .collect();
    if lo.is_empty() || hi.is_empty() {
        return Ok(0);
    }
    let target = input
        .config
        .target
        .sample(&f.iter().map(|f| f.max(1.0)).collect::<Vec<_>>());
    let mut total = 0.0;
    let mut iterations = 0;
    for _ in 0..4 {
        iterations += 1;
        check(input)?;
        let sum = sums(
            h,
            &input.config.subs,
            &out.rows,
            &(0..BINS).collect::<Vec<_>>(),
            &[input.config.main_position],
        );
        let p: Vec<_> = sum[0].1.iter().map(|h| power(*h)).collect();
        let sm = smooth::smooth_power(f, &p, Fraction::Scalar(3.0))?;
        let db: Vec<_> = sm
            .iter()
            .zip(&target)
            .map(|(p, t)| 10.0 * (p + 1e-30).log10() - t)
            .collect();
        let diff = median(gather(&db, &hi)) - median(gather(&db, &lo));
        let step = diff.clamp(-24.0 - total, input.config.infra_boost_max_db - total);
        if step.abs() < 0.1 {
            break;
        }
        for (r, v) in input.config.subs.iter().zip(&mut out.rows) {
            if movable.contains(r) {
                v.gain_db += step;
            }
        }
        total += step;
    }
    if total.abs() >= 0.1 {
        out.notes.push(Note::TwoWay {
            roles: movable,
            gain_db: total,
            xo_hz: xo,
        });
    }
    Ok(iterations)
}

type DeSolver<'a> = dyn FnMut(
        &mut dyn FnMut(&[f64]) -> f64,
        &[(f64, f64)],
        Option<&[f64]>,
        DifferentialEvolutionOptions,
    ) -> Result<secs_dsp::optimize::DifferentialEvolutionResult, secs_dsp::DspError>
    + 'a;

pub fn align(input: &BaInput<'_>, fitter: &mut Fitter<'_>) -> Result<SubAlignment, EngineError> {
    align_with(input, fitter, &mut |cost, bounds, x0, options| {
        differential_evolution(cost, bounds, x0, options)
    })
}

fn align_with(
    input: &BaInput<'_>,
    fitter: &mut Fitter<'_>,
    solver: &mut DeSolver<'_>,
) -> Result<SubAlignment, EngineError> {
    check(input)?;
    let mut out = SubAlignment {
        rows: vec![],
        sub_filters: input.setup.sub_filters.clone(),
        h_subs: vec![],
        layout_rel: vec![],
        layout_info: vec![],
        sub_level_db: None,
        bus_gain: 1.0,
        bus_peq: vec![],
        sub_sum_by_pos: vec![],
        plan: manual_plan(input),
        notes: vec![],
    };
    if input.config.subs.is_empty() {
        return Ok(out);
    }
    let hi = input
        .xo_of
        .iter()
        .map(|(_, x)| *x)
        .reduce(f64::max)
        .unwrap_or(100.0)
        * 1.5;
    let mut h = input.prepared.h.clone();
    layout_stage(input, &mut h, &mut out, hi, fitter)?;
    let search = Search::new(input, &out);
    if out.plan.is_some() {
        out.notes.push(Note::BaManual);
    }
    if out.plan.is_none() && input.config.subs.len() == 1 {
        out.rows = vec![row(0.0, base(input, &input.config.subs[0]), 1, vec![])];
    } else if search.skip {
        out.rows = search.unpack(input, &vec![0.0; search.bounds.len()]);
    } else {
        let s = BaSetup::new(input, &h, hi);
        if s.positions.is_empty() {
            return Err(EngineError::Input("BA has no complete position".into()));
        }
        let indiv = search.fixed_indiv.then(|| individual_db(&s.indiv(None)));
        let res = solver(
            &mut |x| s.cost_with_db(input, &search.unpack(input, x), indiv.as_deref(), None),
            &search.bounds,
            None,
            DifferentialEvolutionOptions::default(),
        )?;
        check(input)?;
        out.notes.push(Note::BaCost(res.fun));
        let mut x = res.x;
        if input.tuning.sub_bus_fix
            && let Some(p1) = bus_response(input, &h, &out, &search.unpack(input, &x), fitter)?
        {
            let x0: Vec<_> = x
                .iter()
                .zip(&search.bounds)
                .map(|(x, (a, b))| x.clamp(*a, *b))
                .collect();
            let res = solver(
                &mut |x| {
                    s.cost_with_db(input, &search.unpack(input, x), indiv.as_deref(), Some(&p1))
                },
                &search.bounds,
                Some(&x0),
                DifferentialEvolutionOptions {
                    maxiter: 40,
                    seed: 2,
                    ..Default::default()
                },
            )?;
            check(input)?;
            let b1 = bus_response(input, &h, &out, &search.unpack(input, &x), fitter)?;
            let c1 = s.cost_with_db(
                input,
                &search.unpack(input, &x),
                indiv.as_deref(),
                b1.as_deref(),
            );
            let b2 = bus_response(input, &h, &out, &search.unpack(input, &res.x), fitter)?;
            let c2 = s.cost_with_db(
                input,
                &search.unpack(input, &res.x),
                indiv.as_deref(),
                b2.as_deref(),
            );
            out.notes.push(Note::BaBusPass {
                first: c1,
                second: c2,
                kept_first: c2 >= c1,
            });
            if c2 < c1 {
                x = res.x;
            }
        }
        out.rows = search.unpack(input, &x);
    }
    two_way(input, &h, &mut out)?;
    check(input)?;
    out.sub_sum_by_pos = sums(
        &h,
        &input.config.subs,
        &out.rows,
        &(0..BINS).collect::<Vec<_>>(),
        &complete(&h, &input.config.subs),
    );
    if !out
        .sub_sum_by_pos
        .iter()
        .any(|(p, _)| *p == input.config.main_position)
    {
        return Err(EngineError::Input("missing BA reference position".into()));
    }
    let lvl = level(input, &out.sub_sum_by_pos);
    out.sub_level_db = Some(lvl - input.ref_level);
    if out.plan.as_ref().is_some_and(|p| p.gain_anchored) {
        out.notes.push(Note::BusGainAnchored(lvl - input.ref_level));
    } else {
        out.bus_gain = 10f64.powf((input.ref_level - lvl) / 20.0);
    }
    for (_, h) in &mut out.sub_sum_by_pos {
        *h = h.iter().map(|v| *v * C::new(out.bus_gain, 0.0)).collect();
    }
    out.bus_peq = bus_fit(input, &out.sub_sum_by_pos, fitter);
    check(input)?;
    if input.tuning.xo_stage && !input.xo_of.is_empty() {
        let p = response::peq_response(design_freqs(), &out.bus_peq);
        for (_, h) in &mut out.sub_sum_by_pos {
            *h = h.iter().zip(&p).map(|(v, p)| v * p).collect();
        }
    }
    Ok(out)
}

/// Main spectra include acoustic-XO PEQ, but not their electrical HP or delay.
#[allow(clippy::type_complexity)]
pub fn bus_delay(
    alignment: &SubAlignment,
    mains: &[(String, Vec<(u32, Arc<[C]>)>)],
    align_delay_ms: &[(String, f64)],
    xo_of: &[(String, f64)],
    lo_ms: f64,
) -> (f64, i8) {
    bus_delay_observe(
        alignment,
        mains,
        align_delay_ms,
        xo_of,
        lo_ms,
        &mut |_, _, _| {},
    )
}

#[allow(clippy::type_complexity)]
fn bus_delay_observe(
    alignment: &SubAlignment,
    mains: &[(String, Vec<(u32, Arc<[C]>)>)],
    align_delay_ms: &[(String, f64)],
    xo_of: &[(String, f64)],
    lo_ms: f64,
    observe: &mut dyn FnMut(i8, f64, f64),
) -> (f64, i8) {
    if alignment.plan.as_ref().is_some_and(|p| !p.fixed.is_empty()) {
        return (0.0, 1);
    }
    let mut prepared = vec![];
    for (r, positions) in mains {
        let Some(&(_, xo)) = xo_of.iter().find(|(s, x)| s == r && *x != 0.0) else {
            continue;
        };
        let idx: Vec<_> = design_freqs()
            .iter()
            .enumerate()
            .filter(|(_, f)| **f >= xo / 2.0 && **f <= xo * 2.0)
            .map(|(i, _)| i)
            .collect();
        let f = gather(design_freqs(), &idx);
        let d = align_delay_ms.iter().find(|(s, _)| s == r).unwrap().1;
        let hp = response::lr4(&f, xo, "HP");
        let lp = response::lr4(&f, xo, "LP");
        for (pos, h) in positions {
            let sub = &alignment
                .sub_sum_by_pos
                .iter()
                .find(|(p, _)| p == pos)
                .expect("complete sub position")
                .1;
            let main: Vec<_> = idx
                .iter()
                .enumerate()
                .map(|(k, &i)| h[i] * C::from_polar(1.0, ramp_phase(f[k], d) * 0.001) * hp[k])
                .collect();
            let sub: Vec<_> = idx
                .iter()
                .enumerate()
                .map(|(k, &i)| sub[i] * lp[k])
                .collect();
            prepared.push((f.clone(), main, sub));
        }
    }
    if prepared.is_empty() {
        return (0.0, 1);
    }
    let mut best = (1e18, 0.0, 1);
    for pol in [1, -1] {
        let mut k = 0;
        while lo_ms + k as f64 * 0.25 < 15.01 {
            let d = lo_ms + k as f64 * 0.25;
            k += 1;
            let mut cost = 0.0;
            for (f, main, sub) in &prepared {
                let errors: Vec<_> = f
                    .iter()
                    .zip(main)
                    .zip(sub)
                    .map(|((&f, &m), &s)| {
                        let s = s
                            * C::new(f64::from(pol), 0.0)
                            * C::from_polar(1.0, ramp_phase(f, d) * 0.001);
                        let total = power(m + s);
                        let reference = power(m) + power(s);
                        (10.0 * (reference + 1e-30).log10() - 10.0 * (total + 1e-30).log10())
                            .powi(2)
                    })
                    .collect();
                cost += mean(&errors);
            }
            observe(pol, d, cost);
            if cost < best.0 {
                best = (cost, d, pol);
            }
        }
    }
    (best.1, best.2)
}
pub fn fold(rows: &[BaRow], bus_gain: f64, bus_delay_ms: f64, bus_polarity: i8) -> Vec<BaRow> {
    rows.iter()
        .map(|r| {
            row(
                r.gain_db + 20.0 * bus_gain.log10(),
                r.delay_ms + bus_delay_ms,
                r.polarity * bus_polarity,
                r.allpass.clone(),
            )
        })
        .collect()
}
pub fn sub_mode_label(role: &str, setup: &Setup, layout_info: &[(String, LayoutInfo)]) -> String {
    let Some((_, m)) = setup.sub_manual.iter().find(|(r, _)| r == role) else {
        return "auto".into();
    };
    match m.mode {
        SubMode::Auto => "auto".into(),
        SubMode::Fixed => "fixed".into(),
        SubMode::Bundle => {
            let name = if m.bundle.is_empty() { "A" } else { &m.bundle };
            if let Some((_, info)) = layout_info.iter().find(|(n, _)| n == name) {
                let kind = match info.kind {
                    LayoutKind::Multi => "multi",
                    LayoutKind::Dba => {
                        if info.rear.iter().any(|r| r == role) {
                            "dba-rear"
                        } else {
                            "dba-front"
                        }
                    }
                };
                format!("bundle:{name}:{kind}")
            } else {
                format!("bundle:{name}")
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/ba.rs"]
mod tests;
