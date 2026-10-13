use super::{cases, design};
use crate::{
    MimoDesign, MimoRequest, Note,
    mimo::design::{convolve, design_input, design_mimo_traced, normal_equations},
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
            "MIMO_OFFLIBM {name} candidates={:?}, reference={}",
            trace.candidates, expected["candidates"]
        );
    }
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
    for (k, step) in trace.trajectory.iter().enumerate() {
        consensus(
            &format!("{name}/lambda{k}/mid"),
            serde_json::json!(step.mid),
            outputs
                .iter()
                .map(|o| o["inputs"][role]["trajectory"][k]["mid"].clone())
                .collect(),
        );
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
        assert!(error <= bound, "{solver} backward error {error:e} > {bound:e}");
    }
    assert!(
        error <= bound * condition * scale,
        "Cholesky/LU difference {:e} > 16·N·κ·ε = {:e}",
        error / scale,
        bound * condition
    );
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
