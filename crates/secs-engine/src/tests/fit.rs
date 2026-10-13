//! Fitter decision seam and local TRF replay on the actual calibration calls.
use super::cases;
use crate::{fit::*, measurement, setup::Setup};
use secs_dsp::optimize;
use secs_oracle::{Fixture, assert_same, errors, integer, number, oracle_libm};
use serde_json::Value;

/// Shared recorded-fit reader for RC, BA and XO families. Descriptors resolve
/// relative to the owning fixture, not necessarily the engine_fit family.
pub(crate) fn read_recorded_fit(fx: &Fixture, input: &Value, result: &Value) -> RecordedFit {
    let optional = |v: &Value| (!v.is_null()).then(|| number(v));
    RecordedFit {
        g: fx.f64s(&input["g"]),
        corr_db: fx.f64s(&input["corr_db"]),
        n: integer(&input["n"]) as usize,
        lo: number(&input["lo"]),
        hi: number(&input["hi"]),
        boost: number(&input["boost"]),
        ring_ms: optional(&input["ring_ms"]),
        f_max: optional(&input["f_max"]),
        result: cases::peq(result),
    }
}
fn recorded(fx: &Fixture) -> RecordedFit {
    read_recorded_fit(fx, fx.inputs(), &fx.outputs()["result"])
}
fn flat(bands: &[(f64, f64, f64)]) -> Vec<f64> {
    bands.iter().flat_map(|&(f, q, g)| [f, q, g]).collect()
}

#[test]
fn peq_greedy_and_ring_oracle() {
    for fx in Fixture::all("engine_fit", "fit_") {
        let call = recorded(&fx);
        let seed = seed(&call.request());
        let out = fx.outputs();
        for (key, got) in [
            ("seed", flat(&seed.bands)),
            ("x0", seed.x0),
            ("lb", seed.lb),
            ("ub", seed.ub),
        ] {
            let expected = if key == "seed" {
                flat(&cases::peq(&out[key]))
            } else {
                fx.f64s(&out[key])
            };
            assert_same(&format!("{} {key}", fx.id), &got, &expected);
        }
    }
    let fx = Fixture::load("engine_fit", "ring");
    let got: Vec<_> = fx.inputs()["probes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| ring_q_cap(number(&p[0]), number(&p[1]), number(&p[2])))
        .collect();
    assert_same("ring_q_cap", &got, &fx.f64s(&fx.outputs()["caps"]));
}

#[test]
fn peq_frozen_residual_and_jacobian() {
    for fx in Fixture::all("engine_fit", "fit_") {
        let call = recorded(&fx);
        let req = call.request();
        let o = fx.outputs();
        let x = fx.f64s(&o["x0"]);
        let mut trace = optimize::LeastSquaresTrace::default();
        optimize::least_squares_with_trace(
            |x| residual(&req, x),
            &x,
            &fx.f64s(&o["lb"]),
            &fx.f64s(&o["ub"]),
            1,
            Some(&mut trace),
        )
        .unwrap();
        for (key, got) in [("initial_f", residual(&req, &x)), ("initial_j", trace.j0)] {
            if oracle_libm() {
                fx.assert_digest(key, &o["trace"][key], &got);
                println!("MEASURE {} {key} n={} exact digest", fx.id, got.len());
            } else {
                let expected = fx.f64s(&o["trace"][key]["first"]);
                println!(
                    "PLATFORM {} {key} sample_rel={:e}",
                    fx.id,
                    errors(&got[..expected.len()], &expected).1
                );
            }
        }
    }
}

#[test]
fn peq_trf_replay() {
    let mut failures = Vec::new();
    for fx in Fixture::all("engine_fit", "fit_") {
        let call = recorded(&fx);
        let req = call.request();
        let o = fx.outputs();
        let s = &o["one_step"];
        let lb = fx.f64s(&o["lb"]);
        let ub = fx.f64s(&o["ub"]);
        let mut x = fx.f64s(&s["initial_x"]);
        let n = x.len();
        let cols: std::collections::BTreeMap<_, _> = [
            "Delta",
            "alpha",
            "step",
            "actual_reduction",
            "predicted_reduction",
            "ratio",
            "Delta_new",
            "accepted",
            "jacobian_recomputed",
        ]
        .iter()
        .map(|&k| (k, fx.f64s(&s[k])))
        .collect();
        let count = cols["Delta"].len();
        assert_eq!(count + 1, integer(&o["nfev"]) as usize);
        let mut largest = 0.0_f64;
        let mut new_points = Vec::new();
        for k in 0..count {
            let step = &cols["step"][k * n..(k + 1) * n];
            let xn: Vec<_> = (0..n)
                .map(|i| {
                    let v = x[i] + step[i];
                    if v <= lb[i] {
                        lb[i].next_up()
                    } else if v >= ub[i] {
                        ub[i].next_down()
                    } else {
                        v
                    }
                })
                .collect();
            let got = optimize::trf_one_step(
                |x| residual(&req, x),
                &x,
                &lb,
                &ub,
                cols["Delta"][k],
                cols["alpha"][k],
            )
            .unwrap();
            for (key, a, b) in [
                ("step", got.step.as_slice(), step),
                ("x_new", got.x_new.as_slice(), xn.as_slice()),
                (
                    "predicted",
                    &[got.predicted_reduction],
                    &[cols["predicted_reduction"][k]],
                ),
                ("radius", &[got.delta_new], &[cols["Delta_new"][k]]),
            ] {
                let e = errors(a, b).1;
                largest = largest.max(e);
                if e > 1e-9 {
                    failures.push(format!("{} trial {k} {key} {e:e}", fx.id));
                }
            }
            let cost = |x: &[f64]| 0.5 * residual(&req, x).iter().map(|v| v * v).sum::<f64>();
            let expected = cols["actual_reduction"][k];
            let explained = (cost(&got.x_new) - cost(&xn)).abs();
            let e = ((got.actual_reduction - expected).abs() - explained).max(0.0)
                / expected.abs().max(1.0);
            largest = largest.max(e);
            if e > 1e-9 {
                failures.push(format!("{} trial {k} reduction {e:e}", fx.id));
            }
            let edge = [0.0, 0.25, 0.75]
                .iter()
                .any(|v| (cols["ratio"][k] - v).abs() <= 1e-9);
            if !edge
                && (got.accepted != (cols["accepted"][k] == 1.0)
                    || got.jacobian_recomputed != (cols["jacobian_recomputed"][k] == 1.0))
            {
                failures.push(format!("{} trial {k} decisions", fx.id));
            }
            new_points.extend_from_slice(&xn);
            if cols["accepted"][k] == 1.0 {
                x = xn;
            }
        }
        fx.assert_digest("reconstructed x_new", &s["x_new_digest"], &new_points);
        println!(
            "MEASURE {} one_step trials={count} max_rel={largest:.17e} tolerance=1e-9",
            fx.id
        );
    }
    if oracle_libm() {
        assert!(failures.is_empty(), "replay failures: {failures:?}");
    } else {
        println!("PLATFORM replay mismatches={}", failures.len());
    }
}

#[test]
fn peq_converging_and_capped_results() {
    let mut failures = Vec::new();
    for fx in Fixture::all("engine_fit", "fit_") {
        let call = recorded(&fx);
        let req = call.request();
        let o = fx.outputs();
        let s = seed(&req);
        let result = optimize::least_squares(
            |x| residual(&req, x),
            &s.x0,
            &s.lb,
            &s.ub,
            integer(&o["cap"]) as usize,
        )
        .unwrap();
        let bands = result_bands(&req, &result.x);
        let error = errors(&flat(&bands), &flat(&call.result)).1;
        let cost = errors(&[result.cost], &[number(&o["cost"])]).1;
        println!(
            "MEASURE {} result bands_rel={error:.17e} cost_rel={cost:.17e} counts=({},{},{}) oracle=({},{},{}) tolerance=1e-10",
            fx.id, result.nfev, result.njev, result.status, o["nfev"], o["njev"], o["status"]
        );
        if integer(&o["status"]) != 0
            && (error > 1e-10
                || cost > 1e-10
                || result.nfev != integer(&o["nfev"]) as usize
                || result.njev != integer(&o["njev"]) as usize
                || result.status != integer(&o["status"]) as i32)
        {
            failures.push(fx.id.clone());
        }
    }
    assert!(
        failures.is_empty(),
        "converging fits not matched: {failures:?}"
    );
}

#[test]
fn xo_target_rebuilt_inputs() {
    for fx in Fixture::all("engine_fit", "xo_") {
        let i = fx.inputs();
        let case = cases::load(i["case"].as_str().unwrap());
        let setup = Setup::new(&case.config);
        let prep = measurement::prepare(&case.measurements, &case.config, &setup).unwrap();
        let role = i["role"].as_str().unwrap();
        let gain = 10_f64.powf(number(&i["gain_db"]) / 20.0);
        let positions: Vec<_> = i["positions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| integer(p) as u32)
            .collect();
        let h: Vec<Vec<_>> = positions
            .iter()
            .map(|&p| {
                prep.h
                    .get(p, role)
                    .unwrap()
                    .iter()
                    .map(|v| v * gain)
                    .collect()
            })
            .collect();
        let by_pos: Vec<_> = positions
            .iter()
            .zip(&h)
            .map(|(&p, h)| {
                (
                    if p == case.config.main_position {
                        5.0
                    } else {
                        1.0
                    },
                    h.as_slice(),
                )
            })
            .collect();
        let fit_fx = Fixture::load("engine_fit", i["fit"].as_str().unwrap());
        let recorded = recorded(&fit_fx);
        let mut checked = false;
        let mut fitter = |req: &FitRequest<'_>| {
            let e = errors(req.corr_db, &recorded.corr_db).1;
            println!(
                "MEASURE {} xo_request max_rel={e:.17e} tolerance=1e-9",
                fx.id
            );
            checked = true;
            replay_fitter(vec![recorded.clone()], 1e-9)(req)
        };
        let got = xo_target_peq(
            &by_pos,
            &case.config.target,
            number(&i["level_db"]),
            number(&i["lo"]),
            number(&i["hi"]),
            integer(&i["bands"]) as usize,
            number(&i["boost"]),
            (!i["f_max"].is_null()).then(|| number(&i["f_max"])),
            &mut fitter,
        );
        assert!(checked);
        assert_eq!(got, cases::peq(&fx.outputs()["result"]));
    }
}

#[test]
#[should_panic(expected = "fit call 0: worst mismatch corr_db[0]")]
fn replay_fitter_rejects_mismatch() {
    let fx = Fixture::all("engine_fit", "fit_").remove(0);
    let call = recorded(&fx);
    let mut wrong = call.clone();
    wrong.corr_db[0] += 1.0;
    replay_fitter(vec![call], 1e-9)(&wrong.request());
}

#[test]
fn gd_flat_response_has_no_cuts() {
    let f = crate::grid::design_freqs();
    let h = vec![crate::C::new(1.0, 0.0); f.len()];
    assert!(fit_gd_cuts(f, &h, 20.0, 500.0, 6, 3.0).is_empty());
}
