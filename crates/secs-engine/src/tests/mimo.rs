use super::{cases, design};
use crate::{
    MimoDesign, MimoRequest, Note,
    mimo::design::{InputResult, convolve, design_input, design_mimo_traced, normal_equations},
};
use secs_compute::{CancelToken, Context, MemoryBudget, NoProgress};
use secs_oracle::{Fixture, number, oracle_libm};
use serde_json::Value;
use std::sync::Arc;

const CASES: [&str; 9] = [
    "room",
    "room_dba",
    "room_filters",
    "room_multi",
    "room_cap2",
    "room_fir",
    "room_low40",
    "room_exclude",
    "room_two_positions",
];
const LABELS: [&str; 8] = [
    "reference",
    "haswell",
    "skylakex",
    "simd",
    "ulp_0",
    "ulp_1",
    "ulp_2",
    "ulp_3",
];

fn request(fx: &Fixture) -> MimoRequest {
    let s = &fx.inputs()["settings"];
    MimoRequest {
        taps_low: s["taps_low"].as_u64().unwrap_or(180) as usize,
        max_supports: s["max_supports"].as_u64().map(|v| v as usize),
        f_low: s["f_low"].as_f64(),
        exclude: s["exclude"]
            .as_array()
            .map(|a| a.iter().map(|v| v.as_str().unwrap().into()).collect())
            .unwrap_or_default(),
        ..MimoRequest::default()
    }
}

#[test]
fn mimo_toeplitz_normal_equations_match_dense() {
    let mut seed = 771_u64;
    let mut random = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 11) as f64 / (1_u64 << 53) as f64 * 2.0 - 1.0
    };
    for onsets in [[0, 2], [3, 4], [7, 20]] {
        let (n, taps, candidates) = (40, 12, 2);
        let mut xs: Vec<Vec<_>> = (0..2).map(|_| (0..n).map(|_| random()).collect()).collect();
        for (x, onset) in xs.iter_mut().zip(onsets) {
            x[..onset].fill(0.0);
            x[onset] = 2.0;
        }
        let paths: Vec<Vec<Vec<_>>> = (0..2)
            .map(|_| {
                (0..candidates)
                    .map(|_| (0..n).map(|_| random()).collect())
                    .collect()
            })
            .collect();
        for early in [0.0, 10.0] {
            let (ata, atb) = normal_equations(&xs, &paths, taps, early);
            let dim = taps * candidates;
            let mut dense_a = Vec::new();
            let mut dense_b = Vec::new();
            for p in 0..2 {
                for t in 0..n {
                    let late = t >= onsets[p] + 36;
                    let weight = if late { 1.0 } else { early.sqrt() };
                    dense_a.push(
                        (0..dim)
                            .map(|k| {
                                if t >= k % taps {
                                    weight * paths[p][k / taps][t - k % taps]
                                } else {
                                    0.0
                                }
                            })
                            .collect::<Vec<_>>(),
                    );
                    dense_b.push(if late { -xs[p][t] } else { 0.0 });
                }
            }
            let expected_a: Vec<_> = (0..dim * dim)
                .map(|ij| {
                    dense_a
                        .iter()
                        .map(|row| row[ij / dim] * row[ij % dim])
                        .sum::<f64>()
                })
                .collect();
            let expected_b: Vec<_> = (0..dim)
                .map(|k| {
                    dense_a
                        .iter()
                        .zip(&dense_b)
                        .map(|(row, b)| row[k] * b)
                        .sum::<f64>()
                })
                .collect();
            for (kind, got, expected) in [("AtA", ata, expected_a), ("Atb", atb, expected_b)] {
                let scale = expected.iter().map(|v| v.abs()).fold(0.0, f64::max);
                let error = got
                    .iter()
                    .zip(&expected)
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0, f64::max);
                eprintln!(
                    "MEASURE MIMO dense {onsets:?} early={early} {kind} relative={:.12e}",
                    error / scale.max(1e-30)
                );
                assert!(error <= 1e-12 * scale.max(1e-30));
            }
        }
    }
}

#[derive(Default)]
struct Statistics {
    total: usize,
    n: usize,
    worse_all: usize,
    varying: usize,
    worse_median: usize,
    largest_absolute: f64,
    largest_cap_fraction: f64,
    failures: Vec<String>,
}
impl Statistics {
    // Taps, lambda and support level are neutral descriptors. Only the
    // guard measures have a worse direction (larger is worse).
    fn value(&mut self, name: &str, got: f64, all: Vec<f64>, directional: bool) {
        assert_eq!(all.len(), 8);
        let reference = all[0];
        let min = all.iter().copied().fold(f64::INFINITY, f64::min);
        let max = all.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let spread = max - min;
        let q = 1e-9 * reference.abs().max(1.0);
        let worse = if directional {
            got - reference
        } else {
            (got - reference).abs()
        };
        let cap = 3.0 * spread.max(q);
        self.total += 1;
        self.largest_absolute = self.largest_absolute.max((got - reference).abs());
        self.largest_cap_fraction = self.largest_cap_fraction.max(worse / cap);
        if worse > (1.25 * spread).max(q) {
            eprintln!(
                "MIMO_125 {name} Rust={got:.12e} reference={reference:.12e} spread={spread:.12e}"
            );
        }
        if !got.is_finite() || worse > cap {
            if oracle_libm() {
                self.failures.push(format!(
                    "{name}: Rust={got:.12e}, eight={all:?}, cap={cap:.12e}"
                ));
            } else {
                eprintln!("MIMO_OFFLIBM {name}: Rust={got:.12e}, eight={all:?}, cap={cap:.12e}");
            }
        }
        if directional {
            self.n += 1;
            self.worse_all += usize::from(got > max);
            if spread > 0.0 {
                self.varying += 1;
                let mut sorted = all;
                sorted.sort_by(f64::total_cmp);
                self.worse_median += usize::from(got > (sorted[3] + sorted[4]) / 2.0);
            }
        }
    }
    fn finish(self, case: &str) {
        let all_bound = super::e2e::binomial_quantile(self.n, 1.0 / 9.0);
        let median_bound = super::e2e::binomial_quantile(self.varying, 0.5);
        eprintln!(
            "MIMO_SUMMARY {case} values={} max_abs={:.12e} max_cap_fraction={:.12e} worse_all={}/{} n={} worse_median={}/{} varying={}",
            self.total,
            self.largest_absolute,
            self.largest_cap_fraction,
            self.worse_all,
            all_bound,
            self.n,
            self.worse_median,
            median_bound,
            self.varying
        );
        for failure in &self.failures {
            eprintln!("MIMO_FAIL {case} {failure}");
        }
        assert!(
            self.failures.is_empty(),
            "{} MIMO value failures in {case}",
            self.failures.len()
        );
        assert!(
            self.worse_all <= all_bound && self.worse_median <= median_bound,
            "MIMO binomial {case}"
        );
    }
}

fn consensus(name: &str, got: Value, all: Vec<Value>) {
    if all.iter().all(|v| v == &all[0]) {
        assert_eq!(got, all[0], "{name}");
    } else {
        eprintln!("MIMO_DISCRETE_SPREAD {name}: Rust={got} eight={all:?}");
    }
}

fn compare_input(
    case: &str,
    role: &str,
    result: &crate::mimo::design::InputResult,
    runs: &[Fixture],
    stats: &mut Statistics,
) {
    let trace = &result.trace;
    let outputs: Vec<_> = runs.iter().map(|r| r.outputs()).collect();
    let expected = &outputs[0]["inputs"][role];
    let name = format!("{case}/{role}");
    if oracle_libm() {
        assert_eq!(
            serde_json::json!(trace.candidates),
            expected["candidates"],
            "{name}"
        );
        if !trace.ranking.is_empty() {
            assert_eq!(
                serde_json::json!(trace.ranking.iter().map(|(r, _)| r).collect::<Vec<_>>()),
                expected["ranking"]["order"]
            );
        }
    } else {
        eprintln!(
            "MIMO_OFFLIBM {name} candidates={:?}, reference={}; ranking={:?}, reference={}",
            trace.candidates,
            expected["candidates"],
            trace.ranking.iter().map(|(r, _)| r).collect::<Vec<_>>(),
            expected["ranking"]["order"]
        );
    }
    consensus(
        &format!("{name}/support_roles"),
        serde_json::json!(result.supports.iter().map(|(r, _)| r).collect::<Vec<_>>()),
        outputs
            .iter()
            .map(|o| {
                serde_json::json!(
                    o["supports"][role]
                        .as_object()
                        .unwrap()
                        .keys()
                        .collect::<Vec<_>>()
                )
            })
            .collect(),
    );
    assert_eq!(trace.trajectory.len(), 19, "{name}/lambda steps");
    for (j, taps) in &result.supports {
        let all: Option<Vec<_>> = runs
            .iter()
            .map(|r| {
                let value = &r.outputs()["supports"][role][j];
                (!value.is_null()).then(|| r.f64s(value))
            })
            .collect();
        if let Some(all) = all {
            assert!(all.iter().all(|a| a.len() == taps.len()));
            for (k, &tap) in taps.iter().enumerate() {
                stats.value(
                    &format!("{name}/{j}/tap{k}"),
                    tap,
                    all.iter().map(|a| a[k]).collect(),
                    false,
                );
            }
        } else {
            eprintln!("MIMO_DISCRETE_SPREAD {name}/{j} support absent in some runs");
        }
    }
    for (key, value) in [("log_lambda", trace.log_lambda), ("ratio", trace.ratio)] {
        stats.value(
            &format!("{name}/{key}"),
            value,
            outputs
                .iter()
                .map(|o| number(&o["inputs"][role][key]))
                .collect(),
            false,
        );
    }
    stats.value(
        &format!("{name}/level_db"),
        result.level_db,
        outputs
            .iter()
            .map(|o| number(&o["level_db"][role]))
            .collect(),
        false,
    );
    for (j, value) in &trace.ranking {
        stats.value(
            &format!("{name}/ranking/{j}"),
            *value,
            outputs
                .iter()
                .map(|o| number(&o["inputs"][role]["ranking"]["scores"][j]))
                .collect(),
            false,
        );
    }
    // Bisection steps are comparable only while every run is at the same
    // midpoint: once a ratio lands on the other side of the target in some
    // run, later midpoints differ and their acceptance and ratio describe
    // different λ. From the first differing midpoint on, the steps are
    // reported, and the final λ and ratio (above) carry the judgement.
    let mut same_path = true;
    for (k, step) in trace.trajectory.iter().enumerate() {
        let mids: Vec<Value> = outputs
            .iter()
            .map(|o| o["inputs"][role]["trajectory"][k]["mid"].clone())
            .collect();
        same_path &= mids.iter().all(|m| m == &serde_json::json!(step.mid));
        if !same_path {
            eprintln!(
                "MIMO_LAMBDA_DIVERGED {name}/lambda{k}: Rust mid={} accepted={} ratio={:e}; runs mid={mids:?}",
                step.mid, step.accepted, step.ratio
            );
            continue;
        }
        consensus(
            &format!("{name}/lambda{k}/accepted"),
            serde_json::json!(step.accepted),
            outputs
                .iter()
                .map(|o| o["inputs"][role]["trajectory"][k]["accepted"].clone())
                .collect(),
        );
        stats.value(
            &format!("{name}/lambda{k}/ratio"),
            step.ratio,
            outputs
                .iter()
                .map(|o| number(&o["inputs"][role]["trajectory"][k]["ratio"]))
                .collect(),
            false,
        );
    }
    consensus(
        &format!("{name}/guard_steps"),
        serde_json::json!(trace.guards.len()),
        outputs
            .iter()
            .map(|o| serde_json::json!(o["inputs"][role]["guards"].as_array().unwrap().len()))
            .collect(),
    );
    consensus(
        &format!("{name}/guard_scale"),
        serde_json::json!(trace.guard_scale),
        outputs
            .iter()
            .map(|o| o["inputs"][role]["guard_scale"].clone())
            .collect(),
    );
    for (k, guard) in trace.guards.iter().enumerate() {
        if outputs
            .iter()
            .any(|o| o["inputs"][role]["guards"][k].is_null())
        {
            continue;
        }
        for (key, value) in [("band_db", guard.band_db), ("t60_change", guard.t60_change)] {
            stats.value(
                &format!("{name}/guard{k}/{key}"),
                value,
                outputs
                    .iter()
                    .map(|o| number(&o["inputs"][role]["guards"][k][key]))
                    .collect(),
                true,
            );
        }
    }
}

#[test]
fn mimo_stage_boundary_goldens() {
    for case in CASES.into_iter().filter(|c| *c != "room_two_positions") {
        let fx = Fixture::load("engine_mimo", case);
        let req = request(&fx);
        let runs: Vec<_> = LABELS
            .iter()
            .map(|l| Fixture::load("engine_mimo", &format!("{case}_{l}")))
            .collect();
        let stage = &fx.inputs()["stage"];
        let mut stats = Statistics::default();
        for (role, data) in stage["inputs"].as_object().unwrap() {
            let xs: Vec<_> = data["xs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| fx.f64s(v))
                .collect();
            let direct = fx.f64s(&data["direct"]);
            let candidates: Vec<_> = runs[0].outputs()["ranges"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|v| {
                    !v[1].is_null()
                        && v[0].as_str().unwrap() != role
                        && !req.exclude.iter().any(|r| r == v[0].as_str().unwrap())
                })
                .map(|v| v[0].as_str().unwrap().to_owned())
                .collect();
            let paths: Vec<Vec<Vec<_>>> = (0..xs.len())
                .map(|p| {
                    candidates
                        .iter()
                        .map(|r| fx.f64s(&stage["paths"][r][p]))
                        .collect()
                })
                .collect();
            let result =
                design_input(&xs, &paths, &direct, &candidates, &req, &Context::serial()).unwrap();
            compare_input(case, role, &result, &runs, &mut stats);
        }
        stats.finish(case);
    }
}

/// BA's differential evolution diverges on `room` even on the oracle libm,
/// and on any case off it (docs/ENGINE.md §3; the M2 tests use the same
/// rule). Everything downstream of BA, including MIMO's inputs and results on
/// the Rust path, is then a different valid design: reported, not asserted.
/// The stage-boundary tests, which feed recorded inputs, stay asserted.
fn ba_diverges(base: &str) -> bool {
    base == "room" || !oracle_libm()
}

#[test]
fn mimo_whole_path_goldens() {
    // Replay only the preceding engine's optimizer decisions. MIMO receives
    // Rust spectra, chain responses and alignment, never recorded low-rate data.
    for base in ["room", "room_dba", "room_filters", "room_multi"] {
        let case = cases::load(base);
        let mut d = design::replay(base);
        for name in CASES.into_iter().filter(|name| {
            *name == base
                || (base == "room" && !["room_dba", "room_filters", "room_multi"].contains(name))
        }) {
            let fx = Fixture::load("engine_mimo", name);
            let req = request(&fx);
            if let Some(positions) = fx.inputs()["settings"]["positions"].as_array() {
                // Upstream's positions override is represented by the design's
                // position selection; this note-only case is last for room.
                d.spectra.positions = positions
                    .iter()
                    .map(|p| p.as_u64().unwrap() as u32)
                    .collect();
            }
            let ctx = Context::serial();
            let public =
                crate::design_mimo(&d, &case.measurements, &case.config, &req, &ctx).unwrap();
            let (traced, traces) =
                design_mimo_traced(&d, &case.measurements, &case.config, &req, &ctx).unwrap();
            assert_eq!(public, traced, "{name}/public and traced entry points");
            let runs: Vec<_> = LABELS
                .iter()
                .map(|label| Fixture::load("engine_mimo", &format!("{name}_{label}")))
                .collect();
            let reference = runs[0].outputs();
            assert_eq!(public.fs_low, 600.0, "{name}/sample rate");
            let ranges: Vec<_> = public
                .ranges
                .iter()
                .map(|(r, band)| {
                    (
                        r,
                        band.map(|(lo, hi)| {
                            (
                                (lo * 10.0).round_ties_even() / 10.0,
                                (hi * 10.0).round_ties_even() / 10.0,
                            )
                        }),
                    )
                })
                .collect();
            assert_eq!(
                serde_json::json!(ranges),
                reference["ranges"],
                "{name}/ranges"
            );
            if ba_diverges(base) {
                for (role, level_db) in &public.level_db {
                    eprintln!(
                        "MIMO_BA_DIVERGED whole/{name}/{role}: level_db Rust={level_db:.6} reference={:.6}",
                        number(&reference["level_db"][role])
                    );
                }
                continue;
            }
            let notes: Vec<_> = public
                .notes
                .iter()
                .map(|n| match n {
                    Note::Mimo(text) => text,
                    other => panic!("unexpected MIMO note: {other:?}"),
                })
                .collect();
            consensus(
                &format!("{name}/notes"),
                serde_json::json!(notes),
                runs.iter().map(|r| r.outputs()["notes"].clone()).collect(),
            );
            let expected_roles: Vec<_> = reference["inputs"].as_object().unwrap().keys().collect();
            assert_eq!(
                public.supports.iter().map(|(r, _)| r).collect::<Vec<_>>(),
                expected_roles
            );
            assert_eq!(
                public.level_db.iter().map(|(r, _)| r).collect::<Vec<_>>(),
                expected_roles
            );
            assert_eq!(
                traces.iter().map(|(r, _)| r).collect::<Vec<_>>(),
                expected_roles
            );
            let mut stats = Statistics::default();
            for (((role, supports), (_, level_db)), (_, trace)) in
                public.supports.into_iter().zip(public.level_db).zip(traces)
            {
                compare_input(
                    &format!("whole/{name}"),
                    &role,
                    &InputResult {
                        supports,
                        level_db,
                        trace,
                    },
                    &runs,
                    &mut stats,
                );
            }
            stats.finish(&format!("whole/{name}"));
        }
    }
}

#[test]
fn mimo_cap_four_is_subset_of_six_candidates() {
    let case = cases::load("room_dba");
    let d = design::replay("room_dba");
    let ctx = Context::serial();
    let uncapped = crate::design_mimo(
        &d,
        &case.measurements,
        &case.config,
        &MimoRequest::default(),
        &ctx,
    )
    .unwrap();
    let capped = crate::design_mimo(
        &d,
        &case.measurements,
        &case.config,
        &MimoRequest {
            max_supports: Some(4),
            ..MimoRequest::default()
        },
        &ctx,
    )
    .unwrap();
    assert_eq!(uncapped.supports.len(), d.mains.len());
    assert_eq!(capped.supports.len(), uncapped.supports.len());
    for ((role, all), (selected_role, selected)) in uncapped.supports.iter().zip(&capped.supports) {
        assert_eq!(role, selected_role);
        assert_eq!(all.len(), 6, "{role}/uncapped supports");
        assert_eq!(selected.len(), 4, "{role}/capped supports");
        assert!(
            selected
                .iter()
                .all(|(r, _)| all.iter().any(|(a, _)| a == r)),
            "{role}/subset"
        );
    }
}

#[test]
fn mimo_cholesky_matches_lu_on_golden_system() {
    let fx = Fixture::load("engine_mimo", "room");
    let reference = Fixture::load("engine_mimo", "room_reference");
    let stage = &fx.inputs()["stage"];
    let xs: Vec<_> = stage["inputs"]["L"]["xs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| fx.f64s(v))
        .collect();
    let lp = crate::mimo::lowpass_fir(150.0, 31).unwrap();
    let candidates = reference.outputs()["inputs"]["L"]["candidates"]
        .as_array()
        .unwrap();
    let paths: Vec<Vec<Vec<_>>> = (0..xs.len())
        .map(|p| {
            candidates
                .iter()
                .map(|r| {
                    let mut g = convolve(&fx.f64s(&stage["paths"][r.as_str().unwrap()][p]), &lp);
                    g.truncate(720);
                    g
                })
                .collect()
        })
        .collect();
    let (ata, atb) = normal_equations(&xs, &paths, 180, 10.0);
    let dim = atb.len();
    let mean = secs_dsp::pairwise_sum(&(0..dim).map(|k| ata[k * dim + k]).collect::<Vec<_>>())
        / dim as f64;
    let lambda = mean * 10f64.powf(number(&reference.outputs()["inputs"]["L"]["log_lambda"]));
    let a = crate::mimo::solve::solve(&ata, &atb, lambda, false).unwrap();
    let b = crate::mimo::solve::solve(&ata, &atb, lambda, true).unwrap();
    let scale = b.iter().map(|v| v.abs()).fold(0.0, f64::max);
    let error = a
        .iter()
        .zip(&b)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f64::max);
    eprintln!("MEASURE MIMO Cholesky/LU relative={:.12e}", error / scale);
    // Two backward-stable solvers of one system agree to about κ·ε, so "the
    // same mathematics" means: each solution's normwise backward error is at
    // the rounding level, and they differ by at most the condition-scaled
    // bound (packet P14, Amendment 2). A fixed tolerance would fail on the
    // ill-conditioned final systems at small λ.
    let matrix = faer::Mat::from_fn(dim, dim, |i, j| {
        ata[i * dim + j] + if i == j { lambda } else { 0.0 }
    });
    let matrix_norm = (0..dim)
        .map(|i| (0..dim).map(|j| matrix[(i, j)].abs()).sum::<f64>())
        .fold(0.0, f64::max);
    let rhs_norm = atb.iter().map(|v| v.abs()).fold(0.0, f64::max);
    let bound = 16.0 * dim as f64 * f64::EPSILON;
    let mut backward = Vec::new();
    for (solver, x) in [("Cholesky", &a), ("LU", &b)] {
        let xnorm = x.iter().map(|v| v.abs()).fold(0.0, f64::max);
        let residual = (0..dim)
            .map(|i| {
                let mut dot = 0.0;
                for j in 0..dim {
                    dot += matrix[(i, j)] * x[j];
                }
                (dot - atb[i]).abs()
            })
            .fold(0.0, f64::max);
        let error = residual / (matrix_norm * xnorm + rhs_norm);
        eprintln!("MEASURE MIMO {solver} backward_error={error:.12e} bound={bound:.12e}");
        backward.push((solver, error));
    }
    use faer::{
        Par,
        dyn_stack::{MemBuffer, MemStack},
        linalg::evd,
    };
    let mut eigenvalues = faer::diag::Diag::<f64>::zeros(dim);
    let mut memory = MemBuffer::new(evd::self_adjoint_evd_scratch::<f64>(
        dim,
        evd::ComputeEigenvectors::No,
        Par::Seq,
        Default::default(),
    ));
    evd::self_adjoint_evd(
        matrix.as_ref(),
        eigenvalues.as_mut(),
        None,
        Par::Seq,
        MemStack::new(&mut memory),
        Default::default(),
    )
    .unwrap();
    let condition = eigenvalues[dim - 1] / eigenvalues[0];
    eprintln!(
        "MEASURE MIMO condition={condition:.12e} N={dim} conditioned_bound={:.12e}",
        bound * condition
    );
    for (solver, error) in backward {
        assert!(
            error <= bound,
            "{solver} backward error {error:e} > {bound:e}"
        );
    }
    assert!(
        error <= bound * condition * scale,
        "Cholesky/LU difference {:e} > 16·N·κ·ε = {:e}",
        error / scale,
        bound * condition
    );
}

#[test]
fn mimo_ridge_sweep_falls_back() {
    use crate::mimo::solve::{RidgeSweep, solve};
    // A PSD system with a zero shifted pivot; LU is required for the second,
    // deliberately indefinite system. Neither pivot is clamped or repaired.
    for (a, b, lambda) in [
        (vec![0.0, 0.0, 0.0, 1.0], vec![1.0, 2.0], 0.0),
        (vec![0.0, 1.0, 1.0, 0.0], vec![1.0, 2.0], 0.0),
        (vec![1.0], vec![f64::INFINITY], 1.0),
        (vec![1.0], vec![1.0], f64::NAN),
    ] {
        let sweep = RidgeSweep::new(&a, &b);
        assert!(sweep.shifted(lambda).is_none());
        match (sweep.solve(lambda), solve(&a, &b, lambda, false)) {
            (Ok(got), Ok(expected)) => assert_eq!(got, expected),
            (Err(_), Err(_)) => {}
            _ => panic!("fallback did not preserve the original solve"),
        }
    }
}

#[test]
fn mimo_ridge_sweep_recorded_systems_and_trajectories() {
    use crate::mimo::solve::{RidgeSweep, solve};
    use faer::{
        Par,
        dyn_stack::{MemBuffer, MemStack},
        linalg::evd,
    };
    for case in CASES.into_iter().filter(|c| *c != "room_two_positions") {
        let fx = Fixture::load("engine_mimo", case);
        let reference = Fixture::load("engine_mimo", &format!("{case}_reference"));
        let req = request(&fx);
        let stage = &fx.inputs()["stage"];
        let lp = crate::mimo::lowpass_fir(req.fsiso, 31).unwrap();
        for (role, data) in stage["inputs"].as_object().unwrap() {
            let recorded = &reference.outputs()["inputs"][role];
            let candidates: Vec<_> = recorded["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r.as_str().unwrap().to_owned())
                .collect();
            let xs: Vec<_> = data["xs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| fx.f64s(v))
                .collect();
            let direct = fx.f64s(&data["direct"]);
            let raw: Vec<Vec<Vec<_>>> = (0..xs.len())
                .map(|p| {
                    candidates
                        .iter()
                        .map(|r| fx.f64s(&stage["paths"][r][p]))
                        .collect()
                })
                .collect();
            // The capped case records the selected system; no ranking is needed
            // to rebuild that exact system a second time.
            let mut selected_req = req.clone();
            selected_req.max_supports = None;
            let result = design_input(
                &xs,
                &raw,
                &direct,
                &candidates,
                &selected_req,
                &Context::serial(),
            )
            .unwrap();
            let paths: Vec<Vec<Vec<_>>> = raw
                .iter()
                .map(|gs| {
                    gs.iter()
                        .map(|g| {
                            let mut h = convolve(g, &lp);
                            h.truncate(xs[0].len());
                            h
                        })
                        .collect()
                })
                .collect();
            let (ata, atb) = normal_equations(&xs, &paths, req.taps_low, 10.0);
            for threads in [1, 4] {
                let ctx = Context::new(
                    threads,
                    MemoryBudget::new(u64::MAX),
                    Arc::new(NoProgress),
                    CancelToken::new(),
                );
                let (mapped_a, mapped_b) = crate::mimo::design::normal_equations_mapped(
                    &xs,
                    &paths,
                    req.taps_low,
                    10.0,
                    &ctx,
                )
                .unwrap();
                assert!(
                    ata.iter()
                        .zip(mapped_a)
                        .all(|(a, b)| a.to_bits() == b.to_bits()),
                    "{case}/{role} mapped matrix"
                );
                assert!(
                    atb.iter()
                        .zip(mapped_b)
                        .all(|(a, b)| a.to_bits() == b.to_bits()),
                    "{case}/{role} mapped RHS"
                );
            }
            let n = atb.len();
            let norm = secs_dsp::pairwise_sum(&(0..n).map(|i| ata[i * n + i]).collect::<Vec<_>>())
                / n as f64;
            let sweep = RidgeSweep::new(&ata, &atb);
            let a = faer::Mat::from_fn(n, n, |i, j| ata[i * n + j]);
            let mut eigenvalues = faer::diag::Diag::zeros(n);
            let mut memory = MemBuffer::new(evd::self_adjoint_evd_scratch::<f64>(
                n,
                evd::ComputeEigenvectors::No,
                Par::Seq,
                Default::default(),
            ));
            evd::self_adjoint_evd(
                a.as_ref(),
                eigenvalues.as_mut(),
                None,
                Par::Seq,
                MemStack::new(&mut memory),
                Default::default(),
            )
            .unwrap();
            let maxnorm = |x: &[f64]| x.iter().map(|v| v.abs()).fold(0.0, f64::max);
            let bound = 16.0 * n as f64 * f64::EPSILON;
            let prove = |log: f64| {
                let lambda = norm * 10f64.powf(log);
                let got = sweep.solve(lambda).unwrap();
                let expected = solve(&ata, &atb, lambda, false).unwrap();
                let matrix_norm = ata
                    .chunks_exact(n)
                    .enumerate()
                    .map(|(i, row)| {
                        row.iter()
                            .enumerate()
                            .map(|(j, a)| (a + if i == j { lambda } else { 0.0 }).abs())
                            .sum::<f64>()
                    })
                    .fold(0.0, f64::max);
                for x in [&got, &expected] {
                    let residual = ata
                        .chunks_exact(n)
                        .enumerate()
                        .map(|(i, row)| {
                            let mut dot = 0.0;
                            for j in 0..n {
                                dot += (row[j] + if i == j { lambda } else { 0.0 }) * x[j];
                            }
                            (dot - atb[i]).abs()
                        })
                        .fold(0.0, f64::max);
                    let error = residual / (matrix_norm * maxnorm(x) + maxnorm(&atb));
                    assert!(
                        error <= bound,
                        "{case}/{role} log={log} backward={error:e} bound={bound:e}"
                    );
                }
                let condition = (eigenvalues[n - 1] + lambda) / (eigenvalues[0] + lambda);
                assert!(condition.is_finite() && condition > 0.0);
                let difference = got
                    .iter()
                    .zip(&expected)
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0, f64::max)
                    / maxnorm(&expected);
                assert!(
                    difference <= bound * condition,
                    "{case}/{role} log={log} relative={difference:e} bound={:e}",
                    bound * condition
                );
            };
            for step in recorded["trajectory"].as_array().unwrap() {
                prove(number(&step["mid"]));
            }
            let trajectory = |use_sweep: bool| {
                let (mut lo, mut hi) = (-8.0, 4.0);
                let mut out = Vec::new();
                for k in 0..19 {
                    let mid = if k == 0 { hi } else { (lo + hi) / 2.0 };
                    let lambda = norm * 10f64.powf(mid);
                    let u = if use_sweep {
                        sweep.solve(lambda)
                    } else {
                        solve(&ata, &atb, lambda, false)
                    }
                    .unwrap();
                    let mut energy = 0.0;
                    for s in u.chunks_exact(req.taps_low) {
                        energy += crate::decay::energy(&convolve(&lp, s));
                    }
                    let accepted = energy / crate::decay::energy(&direct).max(1e-30)
                        <= 10f64.powf(req.level_db / 10.0);
                    out.push((mid, accepted));
                    if k > 0 {
                        if accepted {
                            hi = mid;
                        } else {
                            lo = mid;
                        }
                    }
                }
                (out, hi)
            };
            let expected = trajectory(false);
            assert_eq!(trajectory(true), expected, "{case}/{role}");
            assert_eq!(
                result
                    .trace
                    .trajectory
                    .iter()
                    .map(|s| (s.mid, s.accepted))
                    .collect::<Vec<_>>(),
                expected.0
            );
            assert_eq!(result.trace.log_lambda, expected.1);
            for (mid, _) in expected.0 {
                prove(mid);
            }
            eprintln!("RIDGE_PROOF {case}/{role} N={n} recorded and computed trajectories pass");
        }
    }
}

#[test]
fn mimo_ranges_and_upstream_behaviors() {
    let case = cases::load("room");
    let mut d = design::replay("room");
    let ctx = Context::serial();
    let req = MimoRequest::default();
    let (uncapped, _) =
        design_mimo_traced(&d, &case.measurements, &case.config, &req, &ctx).unwrap();
    let fx = Fixture::load("engine_mimo", "room_reference");
    assert_eq!(serde_json::json!(uncapped.ranges), fx.outputs()["ranges"]);
    let (capped, _) = design_mimo_traced(
        &d,
        &case.measurements,
        &case.config,
        &MimoRequest {
            max_supports: Some(2),
            ..req.clone()
        },
        &ctx,
    )
    .unwrap();
    for ((_, all), (_, selected)) in uncapped.supports.iter().zip(&capped.supports) {
        assert_eq!(selected.len(), 2);
        assert!(
            selected
                .iter()
                .all(|(r, _)| all.iter().any(|(a, _)| a == r))
        );
    }
    let (excluded, traces) = design_mimo_traced(
        &d,
        &case.measurements,
        &case.config,
        &MimoRequest {
            exclude: vec!["SW2".into()],
            ..req.clone()
        },
        &ctx,
    )
    .unwrap();
    assert!(
        traces
            .iter()
            .all(|(_, t)| !t.candidates.iter().any(|r| r == "SW2"))
    );
    assert!(
        excluded
            .notes
            .contains(&Note::Mimo("서포트 제외(사용자): SW2".into()))
    );
    let view = crate::export_view(&d, &case.measurements, &case.config).unwrap();
    let baseline =
        crate::mimo::support_ranges(&d, &view.spectra, 150.0, &d.spectra.positions).unwrap();
    d.chain.subs[0].lp_hz = 25.0;
    let filtered = crate::export_view(&d, &case.measurements, &case.config).unwrap();
    let ranges =
        crate::mimo::support_ranges(&d, &filtered.spectra, 150.0, &d.spectra.positions).unwrap();
    let high = |ranges: &[(String, Option<(f64, f64)>)], role: &str| {
        ranges.iter().find(|(r, _)| r == role).unwrap().1.unwrap().1
    };
    assert!(high(&ranges, "SW1") < high(&baseline, "SW1"));
    d.spectra.positions.truncate(2);
    let (short, traces) =
        design_mimo_traced(&d, &case.measurements, &case.config, &req, &ctx).unwrap();
    assert_eq!(
        short,
        MimoDesign {
            notes: vec![Note::Mimo("측정 위치 2곳 — MIMO 는 3곳 이상일 때만".into())],
            ..MimoDesign::default()
        }
    );
    assert!(traces.is_empty());
}

#[test]
fn mimo_input_selection_preserves_all_input_result() {
    let case = cases::load("room");
    let d = design::replay("room");
    let ctx = Context::new(
        1,
        MemoryBudget::new(512 * 1024 * 1024),
        Arc::new(NoProgress),
        CancelToken::new(),
    );
    let req = MimoRequest::default();
    let (all, traces) =
        design_mimo_traced(&d, &case.measurements, &case.config, &req, &ctx).unwrap();
    for (index, main) in d.mains.iter().enumerate() {
        let (one, trace) = crate::mimo::design::design_mimo_inputs(
            &d,
            &case.measurements,
            &case.config,
            &req,
            &ctx,
            Some(&main.role),
        )
        .unwrap();
        assert_eq!(one.ranges, all.ranges);
        assert_eq!(one.supports, vec![all.supports[index].clone()]);
        assert_eq!(one.level_db, vec![all.level_db[index].clone()]);
        assert_eq!(trace, vec![traces[index].clone()]);
        for ((_, a), (_, b)) in one.supports[0].1.iter().zip(&all.supports[index].1) {
            assert!(a.iter().zip(b).all(|(a, b)| a.to_bits() == b.to_bits()));
        }
    }
}

#[test]
fn mimo_deterministic_threads() {
    let case = cases::load("room");
    let d = design::replay("room");
    let make = |n| {
        Context::new(
            n,
            MemoryBudget::new(512 * 1024 * 1024),
            Arc::new(NoProgress),
            CancelToken::new(),
        )
    };
    let request = MimoRequest::default();
    let one = design_mimo_traced(&d, &case.measurements, &case.config, &request, &make(1)).unwrap();
    let four =
        design_mimo_traced(&d, &case.measurements, &case.config, &request, &make(4)).unwrap();
    assert_eq!(one, four);
    for ((_, a), (_, b)) in one.0.supports.iter().zip(&four.0.supports) {
        for ((_, a), (_, b)) in a.iter().zip(b) {
            assert!(a.iter().zip(b).all(|(a, b)| a.to_bits() == b.to_bits()));
        }
    }
}

#[test]
fn mimo_whole_path_inputs_match_recorded() {
    let mut failures = Vec::new();
    for base in ["room", "room_dba", "room_filters", "room_multi"] {
        let case = cases::load(base);
        let mut d = design::replay(base);
        let spectra = crate::export_view(&d, &case.measurements, &case.config)
            .unwrap()
            .spectra;
        let prepared = crate::measurement::prepare(
            &case.measurements,
            &case.config,
            &crate::setup::Setup::new(&case.config),
        )
        .unwrap();
        for name in CASES.into_iter().filter(|name| {
            *name == base
                || (base == "room" && !["room_dba", "room_filters", "room_multi"].contains(name))
        }) {
            let fx = Fixture::load("engine_mimo", name);
            let stage = &fx.inputs()["stage"];
            if let Some(positions) = fx.inputs()["settings"]["positions"].as_array() {
                d.spectra.positions = positions
                    .iter()
                    .map(|p| p.as_u64().unwrap() as u32)
                    .collect();
            }
            let m = crate::design_mimo(
                &d,
                &case.measurements,
                &case.config,
                &request(&fx),
                &Context::serial(),
            )
            .unwrap();
            if d.spectra.positions.len() < 3 {
                assert!(stage.is_null(), "{name}/note-only stage");
                assert!(m.ranges.is_empty() && m.supports.is_empty());
                continue;
            }
            assert_eq!(serde_json::json!(d.spectra.positions), stage["positions"]);
            let mut compare = |label: &str, got: &[f64], recorded: &Value| {
                let expected = fx.f64s(recorded);
                assert_eq!(got.len(), expected.len(), "{name}/{label}");
                let norm = expected.iter().map(|v| v * v).sum::<f64>().sqrt();
                let error = got
                    .iter()
                    .zip(&expected)
                    .map(|(a, b)| (a - b) * (a - b))
                    .sum::<f64>()
                    .sqrt();
                let relative = error / norm.max(f64::MIN_POSITIVE);
                eprintln!("MEASURE MIMO_INPUT {name}/{label} relative={relative:.12e}");
                if !relative.is_finite() || relative > 1e-9 {
                    if !ba_diverges(base) {
                        failures.push(format!("{name}/{label}: relative={relative:.12e} > 1e-9"));
                    } else {
                        eprintln!("MIMO_BA_DIVERGED {name}/{label} relative={relative:.12e}");
                    }
                }
            };
            assert_eq!(stage["inputs"].as_object().unwrap().len(), d.mains.len());
            for main in &d.mains {
                let direct =
                    secs_dsp::lowrate::to_low(&main.response, crate::grid::design_freqs(), 720)
                        .unwrap();
                compare(
                    &format!("{}/direct", main.role),
                    &direct,
                    &stage["inputs"][&main.role]["direct"],
                );
                assert_eq!(
                    stage["inputs"][&main.role]["xs"].as_array().unwrap().len(),
                    d.spectra.positions.len()
                );
            }
            for (i, &p) in d.spectra.positions.iter().enumerate() {
                for main in &d.mains {
                    let response = crate::input_response(&d, &main.role, &spectra, p).unwrap();
                    let x = secs_dsp::lowrate::to_low(&response, crate::grid::design_freqs(), 720)
                        .unwrap();
                    compare(
                        &format!("{}/xs/{p}", main.role),
                        &x,
                        &stage["inputs"][&main.role]["xs"][i],
                    );
                }
                for (role, recorded) in stage["paths"].as_object().unwrap() {
                    assert_eq!(
                        recorded.as_array().unwrap().len(),
                        d.spectra.positions.len()
                    );
                    let response = crate::mimo::design::support_path(
                        &m,
                        role,
                        &spectra,
                        p,
                        &prepared.align_delay_ms,
                    )
                    .unwrap();
                    let path =
                        secs_dsp::lowrate::to_low(&response, crate::grid::design_freqs(), 720)
                            .unwrap();
                    compare(&format!("{role}/path/{p}"), &path, &recorded[i]);
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "MIMO reference inputs differ: {failures:#?}"
    );
}

#[test]
fn mimo_shared_preparation_bit_identical() {
    for name in CASES {
        let base = match name {
            "room_dba" | "room_filters" | "room_multi" => name,
            _ => "room",
        };
        let case = cases::load(base);
        let mut d = design::replay(base);
        let fx = Fixture::load("engine_mimo", name);
        if let Some(positions) = fx.inputs()["settings"]["positions"].as_array() {
            d.spectra.positions = positions
                .iter()
                .map(|p| p.as_u64().unwrap() as u32)
                .collect();
        }
        let req = request(&fx);
        let ctx = Context::serial();
        let before = crate::mimo::design::design_mimo_before(
            &d,
            &case.measurements,
            &case.config,
            &req,
            &ctx,
            None,
        )
        .unwrap();
        for threads in [1, 4] {
            let ctx = Context::new(
                threads,
                MemoryBudget::new(512 * 1024 * 1024),
                Arc::new(NoProgress),
                CancelToken::new(),
            );
            let after =
                design_mimo_traced(&d, &case.measurements, &case.config, &req, &ctx).unwrap();
            assert_eq!(before, after, "{name}/{threads}");
            assert_eq!(
                design_bits(&before.0),
                design_bits(&after.0),
                "{name}/{threads} design bits"
            );
            let trace_bits = |traces: &[(String, crate::mimo::design::InputTrace)]| {
                let mut values = Vec::new();
                for (_, t) in traces {
                    values.extend([t.log_lambda, t.ratio, t.guard_scale]);
                    values.extend(t.ranking.iter().map(|(_, v)| *v));
                    for step in &t.trajectory {
                        values.extend([step.mid, step.ratio]);
                    }
                    for step in &t.guards {
                        values.extend([step.scale, step.band_db, step.t60_change]);
                    }
                }
                values.into_iter().map(f64::to_bits).collect::<Vec<_>>()
            };
            assert_eq!(
                trace_bits(&before.1),
                trace_bits(&after.1),
                "{name}/{threads} trace bits"
            );
        }
    }
}

#[test]
fn mimo_shared_paths_reject_combined_budget() {
    let case = cases::load("room");
    let d = design::replay("room");
    let req = MimoRequest::default();
    let prepared =
        crate::mimo::prepared::Prepared::new(&d, &case.measurements, &case.config).unwrap();
    let ranges =
        crate::mimo::support_ranges(&d, &prepared.spectra, req.fsiso, &d.spectra.positions)
            .unwrap();
    let candidates = d
        .mains
        .iter()
        .map(|main| {
            ranges
                .iter()
                .filter(|(r, range)| range.is_some() && *r != main.role)
                .count()
        })
        .max()
        .unwrap();
    let dim = candidates * req.taps_low;
    // Every old per-input permit fits, but the retained shared table plus
    // that permit does not. Reject rather than waiting on our own reservation.
    let input_bytes = 8
        * (4 * dim * dim
            + req.taps_low * req.taps_low
            + d.spectra.positions.len() * 720 * (4 * candidates + 8)
            + 32 * crate::grid::design_freqs().len()
            + 16 * dim
            + 16384);
    let ctx = Context::new(
        1,
        MemoryBudget::new(input_bytes as u64),
        Arc::new(NoProgress),
        CancelToken::new(),
    );
    assert!(matches!(
        design_mimo_traced(
            &d,
            &case.measurements,
            &case.config,
            &MimoRequest::default(),
            &ctx
        ),
        Err(crate::EngineError::Input(_))
    ));
}

pub(super) fn design_bits(d: &MimoDesign) -> Vec<u64> {
    let mut values = vec![d.fs_low];
    for (_, range) in &d.ranges {
        if let Some((lo, hi)) = range {
            values.extend([*lo, *hi]);
        }
    }
    values.extend(d.level_db.iter().map(|(_, v)| *v));
    for (_, supports) in &d.supports {
        for (_, taps) in supports {
            values.extend(taps);
        }
    }
    values.into_iter().map(f64::to_bits).collect()
}
