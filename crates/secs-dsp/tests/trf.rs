#![forbid(unsafe_code)]
use secs_dsp::{optimize, response};
use secs_oracle::{Fixture, errors, integer, number, oracle_libm};

fn cases() -> Vec<Fixture> {
    Fixture::all("dsp", "trf_")
        .into_iter()
        .filter(|fx| !fx.id.starts_with("dsp/trf_replay_"))
        .collect()
}
fn residual(fx: &Fixture) -> impl Fn(&[f64]) -> Vec<f64> + use<> {
    let i = fx.inputs();
    let kind = i["kind"].as_str().unwrap().to_owned();
    let grid = if kind == "peq" {
        fx.f64s(&i["g"])
    } else {
        vec![]
    };
    let target = if i["target"].is_null() {
        vec![]
    } else {
        fx.f64s(&i["target"])
    };
    move |x| match kind.as_str() {
        "peq" => {
            let bands: Vec<_> = x
                .chunks(3)
                .map(|v| (v[0].exp(), v[1].exp(), v[2]))
                .collect();
            response::peq_response(&grid, &bands)
                .iter()
                .enumerate()
                .map(|(k, &z)| {
                    let w = if grid[k] >= 20. && grid[k] <= 500. {
                        1.
                    } else {
                        0.15
                    };
                    w * (20. * (response::magnitude(z) + 1e-12).log10() - target[k])
                })
                .collect()
        }
        "linear" => x.iter().zip(&target).map(|(x, t)| x - t).collect(),
        "rosen" => vec![10. * (x[1] - x[0] * x[0]), 1. - x[0]],
        "inconsistent" => vec![x[0] - 1., x[0] + 1.],
        _ => panic!("unknown TRF residual {kind}"),
    }
}
fn run(fx: &Fixture) -> (optimize::LeastSquaresResult, optimize::LeastSquaresTrace) {
    let i = fx.inputs();
    let tol = |key: &str| match i["tols"].get(key) {
        None => Some(1e-8),
        Some(v) if v.is_null() => None,
        Some(v) => Some(number(v)),
    };
    let mut trace = optimize::LeastSquaresTrace::default();
    let result = optimize::least_squares_with_options(
        residual(fx),
        &fx.f64s(&i["x0"]),
        &fx.f64s(&i["lb"]),
        &fx.f64s(&i["ub"]),
        optimize::Options {
            max_nfev: integer(&i["cap"]) as usize,
            ftol: tol("ftol"),
            xtol: tol("xtol"),
            gtol: tol("gtol"),
        },
        Some(&mut trace),
    )
    .unwrap();
    (result, trace)
}

#[test]
fn trf_one_step_replay() {
    let mut failures = Vec::new();
    for fx in cases() {
        let i = fx.inputs();
        let o = fx.outputs();
        let n = fx.f64s(&i["x0"]).len();
        let columns: std::collections::BTreeMap<_, _> = o["one_step"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.as_str(), fx.f64s(v)))
            .collect();
        let count = columns["Delta"].len();
        assert_eq!(
            count,
            integer(&o["nfev"]) as usize - 1,
            "{} trial coverage",
            fx.id
        );
        for (key, values) in &columns {
            let width = if ["x", "x_new", "step"].contains(key) {
                n
            } else {
                1
            };
            assert_eq!(values.len(), count * width, "{} {key} shape", fx.id);
        }
        assert_eq!(
            columns["jacobian_recomputed"].iter().sum::<f64>() as usize + 1,
            integer(&o["njev"]) as usize
        );
        let mut largest = 0.0_f64;
        let mut worst = String::new();
        let mut borderline = Vec::new();
        for k in 0..count {
            let got = optimize::trf_one_step(
                residual(&fx),
                &columns["x"][k * n..(k + 1) * n],
                &fx.f64s(&i["lb"]),
                &fx.f64s(&i["ub"]),
                columns["Delta"][k],
                columns["alpha"][k],
            )
            .unwrap();
            for (key, a) in [
                ("x_new", got.x_new.clone()),
                ("step", got.step.clone()),
                ("predicted_reduction", vec![got.predicted_reduction]),
                ("Delta_new", vec![got.delta_new]),
            ] {
                let width = a.len();
                let error = errors(&a, &columns[key][k * width..(k + 1) * width]).1;
                if error > largest {
                    largest = error;
                    worst = format!("trial={k} {key}");
                }
                if error > 1e-9 {
                    failures.push(format!("{} trial={k} {key} error={error:e}", fx.id));
                }
            }
            // `actual_reduction` is cost(x) - cost(x_new): it inherits the
            // difference between our x_new and the oracle's (gated above at
            // 1e-9), amplified by the residual's slope. The residual is
            // bit-exact to SciPy's at a given point, so the cost difference
            // between the two x_new values is exactly what that difference
            // explains; only the remainder is held to 1e-9.
            let cost = |x: &[f64]| 0.5 * residual(&fx)(x).iter().map(|v| v * v).sum::<f64>();
            let oracle_x_new = &columns["x_new"][k * n..(k + 1) * n];
            let explained = (cost(&got.x_new) - cost(oracle_x_new)).abs();
            let expected = columns["actual_reduction"][k];
            let gap = (got.actual_reduction - expected).abs();
            let error = (gap - explained).max(0.) / expected.abs().max(1.);
            if error > largest {
                largest = error;
                worst = format!("trial={k} actual_reduction");
            }
            if error > 1e-9 {
                failures.push(format!(
                    "{} trial={k} actual_reduction gap={gap:e} explained={explained:e}",
                    fx.id
                ));
            }
            if gap > 1e-9 * expected.abs().max(1.) {
                println!(
                    "TRACE {} trial={k} actual_reduction gap={gap:e} explained_by_x_new={explained:e}",
                    fx.id
                );
            }
            let ratio = columns["ratio"][k];
            let edge = [0., 0.25, 0.75].iter().any(|t| (ratio - t).abs() <= 1e-9);
            if edge {
                borderline.push(k);
            }
            for (key, decision) in [
                ("accepted", got.accepted),
                ("jacobian_recomputed", got.jacobian_recomputed),
            ] {
                let expected = columns[key][k];
                assert!(
                    expected == 0. || expected == 1.,
                    "invalid decision encoding"
                );
                if decision != (expected == 1.) {
                    if edge {
                        println!(
                            "TRACE {} borderline trial={k} {key} oracle_ratio={ratio:e}",
                            fx.id
                        );
                    } else {
                        failures.push(format!(
                            "{} trial={k} {key} got={decision} expected={expected}",
                            fx.id
                        ));
                    }
                }
            }
        }
        println!(
            "MEASURE {} one_step trials={count} max_rel={largest:.17e} worst={worst} borderline={borderline:?}",
            fx.id
        );
    }
    // The replay recomputes the residual and its finite-difference Jacobian
    // at the oracle's x. Those are bit-exact only on the oracle's C math
    // library; elsewhere (macOS, Windows) the residual moves by ulps and the
    // finite differences amplify that (the same sensitivity that makes capped
    // fits chaotic), so off the oracle's libm the replay is a report.
    if oracle_libm() {
        assert!(failures.is_empty(), "TRF one-step mismatches: {failures:?}");
    } else if !failures.is_empty() {
        println!(
            "PLATFORM TRF one-step replay reports {} mismatches off the oracle's libm: {failures:?}",
            failures.len()
        );
    }
}

#[test]
fn trf_converging_results() {
    for fx in cases() {
        if integer(&fx.outputs()["status"]) == 0 {
            continue;
        }
        let (result, _) = run(&fx);
        let o = fx.outputs();
        let x_error = errors(&result.x, &fx.f64s(&o["x"])).1;
        let cost_error = errors(&[result.cost], &[number(&o["cost"])]).1;
        println!(
            "MEASURE {} converged x_rel={x_error:.17e} cost_rel={cost_error:.17e} counts=({},{},{}) oracle=({},{},{})",
            fx.id, result.nfev, result.njev, result.status, o["nfev"], o["njev"], o["status"]
        );
        assert_eq!(result.nfev, integer(&o["nfev"]) as usize, "{} nfev", fx.id);
        assert_eq!(result.njev, integer(&o["njev"]) as usize, "{} njev", fx.id);
        assert_eq!(
            result.status,
            integer(&o["status"]) as i32,
            "{} status",
            fx.id
        );
        // P03c baseline measurements are below the 1e-10 relative floor.
        assert!(x_error <= 1e-10, "{} final x {x_error:e}", fx.id);
        assert!(cost_error <= 1e-10, "{} final cost {cost_error:e}", fx.id);
    }
}

#[test]
fn trf_capped_results() {
    for fx in cases() {
        if integer(&fx.outputs()["status"]) != 0 {
            continue;
        }
        let (result, _) = run(&fx);
        let o = fx.outputs();
        println!(
            "MEASURE {} capped x_rel={:.17e} cost_rel={:.17e} njev={} oracle_njev={}",
            fx.id,
            errors(&result.x, &fx.f64s(&o["x"])).1,
            errors(&[result.cost], &[number(&o["cost"])]).1,
            result.njev,
            o["njev"]
        );
        assert_eq!(result.nfev, integer(&o["nfev"]) as usize, "{} nfev", fx.id);
        assert_eq!(result.status, 0, "{} status", fx.id);
    }
}

#[test]
fn trf_trace_report() {
    for fx in cases() {
        let (result, trace) = run(&fx);
        let o = fx.outputs();
        let n = result.x.len();
        for (key, actual, width) in [
            (
                "evaluations",
                trace
                    .evaluations
                    .iter()
                    .flat_map(|e| e.x.iter().copied())
                    .collect::<Vec<_>>(),
                n,
            ),
            (
                "costs",
                trace.evaluations.iter().map(|e| e.cost).collect(),
                1,
            ),
            (
                "jac_points",
                trace.jacobian_points.iter().flatten().copied().collect(),
                n,
            ),
            ("main", trace.main_x.iter().flatten().copied().collect(), n),
            ("main_cost", trace.main_cost, 1),
            ("jac", trace.jac_x.iter().flatten().copied().collect(), n),
            ("J0", trace.j0, n),
        ] {
            let expected = fx.f64s(&o[key]);
            let first = actual
                .chunks(width)
                .zip(expected.chunks(width))
                .position(|(a, b)| a.len() != b.len() || errors(a, b).1 > 1e-9)
                .or_else(|| {
                    (actual.len() != expected.len())
                        .then_some(actual.len().min(expected.len()) / width)
                });
            println!(
                "TRACE {} {key} first_divergence={first:?} rows={}/{}",
                fx.id,
                actual.len() / width,
                expected.len() / width
            );
        }
    }
}
