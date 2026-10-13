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
    for fx in Fixture::all_calls("engine_fit", "fit_") {
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
    for fx in Fixture::all_calls("engine_fit", "fit_") {
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

// ADR 0004: frozen oracle-libm measurement, not recalibrated by the test.
// P07c traced the seven trials above 1e-9 (table in oracle/README.md): all
// return the Gauss–Newton step with alpha=0 and zero Newton iterations.
// J_h is exact and p_h agrees within 1.57e-14, but (bound-x)/p for a component
// 1–43 ulps from its bound amplifies the difference to a 1.09e-7 stride error
// and a 6.95e-8 selected-step error. Feeding Rust's p_h to unmodified SciPy
// select_step reproduces Rust's selected step bit for bit in all seven cases.
// This is shared TRF bound sensitivity, not an alpha stopping-test flip.
const REPLAY_MEASURED: f64 = 6.952_128_117_632_128e-8;
fn replay_budget() -> f64 {
    (1.5 * REPLAY_MEASURED).max(1e-9)
}

fn replay(fx: &Fixture) -> (bool, Vec<String>) {
    let mut failures = Vec::new();
    let mut exact = true;
    {
        let call = recorded(fx);
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
        let mut worst_trial = 0;
        let mut condition = 0.0;
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
            if k == 0 {
                condition = got.s[0] / got.s[got.s.len() - 1];
            }
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
                exact &= a.iter().zip(b).all(|(a, b)| a.to_bits() == b.to_bits());
                if e > largest {
                    largest = e;
                    worst_trial = k;
                    condition = got.s[0] / got.s[got.s.len() - 1];
                }
                if e > replay_budget() {
                    failures.push(format!("{} trial {k} {key} {e:e}", fx.id));
                }
            }
            let cost = |x: &[f64]| 0.5 * residual(&req, x).iter().map(|v| v * v).sum::<f64>();
            let expected = cols["actual_reduction"][k];
            let explained = (cost(&got.x_new) - cost(&xn)).abs();
            let e = ((got.actual_reduction - expected).abs() - explained).max(0.0)
                / expected.abs().max(1.0);
            exact &= got.actual_reduction.to_bits() == expected.to_bits();
            if e > largest {
                largest = e;
                worst_trial = k;
                condition = got.s[0] / got.s[got.s.len() - 1];
            }
            if e > replay_budget() {
                failures.push(format!("{} trial {k} reduction {e:e}", fx.id));
            }
            let edge = [0.0, 0.25, 0.75]
                .iter()
                .any(|v| (cols["ratio"][k] - v).abs() <= 1e-9);
            exact &= got.accepted == (cols["accepted"][k] == 1.0)
                && got.jacobian_recomputed == (cols["jacobian_recomputed"][k] == 1.0);
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
            "MEASURE {} one_step trials={count} max_rel={largest:.17e} worst_trial={worst_trial} augmented_j_condition={condition:.17e} tolerance={:.17e} exact={exact}",
            fx.inputs()["id"].as_str().unwrap(),
            replay_budget()
        );
    }
    (exact, failures)
}

#[test]
fn peq_trf_replay() {
    let failures: Vec<_> = Fixture::all_calls("engine_fit", "fit_")
        .iter()
        .flat_map(|fx| replay(fx).1)
        .collect();
    println!(
        "MEASURE peq_replay frozen_max_rel={REPLAY_MEASURED:.17e} budget={:.17e}",
        replay_budget()
    );
    if oracle_libm() {
        assert!(failures.is_empty(), "replay failures: {failures:?}");
    } else {
        println!("PLATFORM replay mismatches={}", failures.len());
    }
}

#[test]
fn peq_results_within_secs2_spread() {
    let mut failures = Vec::new();
    let (mut above_all, mut calls) = (0_usize, 0_usize);
    for fx in Fixture::all_calls("engine_fit", "fit_") {
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
        // Evaluate the returned bands, including the production ring-Q clamp.
        let response = secs_dsp::response::peq_response(req.g, &bands);
        let cost = 0.5
            * response
                .iter()
                .zip(req.g)
                .zip(req.corr_db)
                .map(|((&h, &g), &target)| {
                    let weight = if g >= req.lo && g <= req.hi {
                        1.0
                    } else {
                        0.15
                    };
                    let r = weight
                        * (20.0 * (secs_dsp::response::magnitude(h) + 1e-12).log10() - target);
                    r * r
                })
                .sum::<f64>();
        let c0 = number(&o["final_cost"]);
        let environments = o["environments"].as_array().unwrap();
        assert_eq!(environments.len(), 3);
        let perturbations = o["perturbations"].as_array().unwrap();
        assert_eq!(perturbations.len(), 8);
        let (mut cmin, mut cmax) = (c0, c0);
        let mut cmax_run = "reference";
        let mut costs = format!("reference:{c0:.17e}");
        let reference = flat(&call.result);
        let actual = flat(&bands);
        let mut nearest = ("reference", errors(&actual, &reference));
        let mut identical = true;
        let mut counts = format!("reference:({},{},{})", o["nfev"], o["njev"], o["status"]);
        for env in environments.iter().chain(perturbations) {
            let c = number(&env["cost"]);
            assert!(c.is_finite());
            let label = env["environment"].as_str().unwrap();
            costs += &format!(" {label}:{c:.17e}");
            cmin = cmin.min(c);
            if c > cmax {
                cmax = c;
                cmax_run = label;
            }
            let other = flat(&cases::peq(&env["result"]));
            identical &= c.to_bits() == c0.to_bits()
                && env["nfev"] == o["nfev"]
                && env["njev"] == o["njev"]
                && env["status"] == o["status"]
                && other.len() == reference.len()
                && other
                    .iter()
                    .zip(&reference)
                    .all(|(a, b)| a.to_bits() == b.to_bits());
            if actual.len() == other.len() {
                let e = errors(&actual, &other);
                if e.1 < nearest.1.1 {
                    nearest = (env["environment"].as_str().unwrap(), e);
                }
            }
            counts += &format!(
                " {}:({},{},{})",
                env["environment"].as_str().unwrap(),
                env["nfev"],
                env["njev"],
                env["status"]
            );
        }
        // ADR 0004: Rust may differ from the reference by up to 1.25 times
        // SECS 2's own spread, the range of the twelve runs. Anchoring the
        // margin at `cmax - c0` instead fails whenever the reference happens
        // to be the worst run: the twelve runs are draws from the same
        // last-bit distribution, and Rust is a thirteenth draw (it exceeds
        // all twelve on about 1/13 of the calls, which `SPREAD_RANK` shows).
        // Twelve runs cannot resolve a spread far below the typical one
        // (median 1.7e-5 of the cost over these calls, as small as 2e-8 on
        // some): floor it at 1e-6 of the cost. Without the floor macOS's
        // libm failed `fit_dolby_rc_015` at 1.30 times a 1.1e-7 range.
        let limit = c0 + 1.25 * (cmax - cmin).max(1e-6 * c0);
        // A fit that stops at its evaluation cap ends wherever its trajectory
        // is at the cap. The twelve runs share the oracle's libm, so on another
        // libm (Windows, macOS) a capped fit is one more trajectory they did not
        // sample; it is reported there and the design's end result is gated by
        // `engine_e2e` (docs/ENGINE.md §3). Converging fits are gated everywhere.
        let capped = integer(&o["status"]) == 0;
        let gated = !capped || oracle_libm();
        let pass = cost.is_finite() && (cost <= limit || !gated);
        above_all += usize::from(cost > cmax);
        calls += 1;
        println!(
            "SPREAD {} stage={} case={} rust_c0={:.17e} spread={:.17e} cmax_run={cmax_run} rust_cost={cost:.17e} nearest={} bands_abs={:.17e} bands_rel={:.17e} counts=({},{},{}) {} within={} pass={pass}",
            fx.inputs()["id"].as_str().unwrap(),
            fx.meta()["stage"],
            fx.meta()["case"],
            cost / c0,
            (cmax - cmin) / c0,
            nearest.0,
            nearest.1.0,
            nearest.1.1,
            result.nfev,
            result.njev,
            result.status,
            counts,
            cost <= limit
        );
        if !pass {
            println!(
                "SPREAD_FAIL {} rust_cost={cost:.17e} limit={limit:.17e} costs=[{costs}]",
                fx.inputs()["id"].as_str().unwrap()
            );
            failures.push(fx.inputs()["id"].as_str().unwrap().to_owned());
        }
        if identical && oracle_libm() && replay(&fx).0 {
            assert_eq!(
                actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                reference.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                "{} zero-spread exact replay",
                fx.id
            );
        }
    }
    println!(
        "SPREAD_RANK rust above all twelve runs on {above_all} of {calls} calls (exchangeable: about {:.1})",
        calls as f64 / 13.0
    );
    // As a thirteenth draw Rust exceeds all twelve runs with probability 1/13
    // per call. More than the binomial 99.9 % quantile means Rust is
    // systematically worse, on any libm.
    let p = 1.0 / 13.0;
    let mean = calls as f64 * p;
    let bound = (mean + 3.1 * (mean * (1.0 - p)).sqrt()).ceil() as usize;
    assert!(
        above_all <= bound,
        "rust above all twelve runs on {above_all} of {calls} calls (bound {bound})"
    );
    assert!(
        failures.is_empty(),
        "fits outside SECS 2 spread: {failures:?}"
    );
}

#[test]
fn xo_target_rebuilt_inputs() {
    for fx in Fixture::all_calls("engine_fit", "xo_") {
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
        let fit_fx = Fixture::all_calls("engine_fit", "fit_")
            .into_iter()
            .find(|f| f.inputs()["id"] == i["fit"])
            .unwrap();
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
    let fx = Fixture::all_calls("engine_fit", "fit_").remove(0);
    let call = recorded(&fx);
    let mut wrong = call.clone();
    wrong.corr_db[0] += 1.0;
    replay_fitter(vec![call], 1e-9)(&wrong.request());
}

#[test]
fn gd_flat_response_has_no_cuts() {
    // P09 owns the end-to-end GD gate once RC/BA full spectra can be rebuilt;
    // engine_fit keeps samples/digests and the twelve-run envelope of each inner fit.
    let f = crate::grid::design_freqs();
    let h = vec![crate::C::new(1.0, 0.0); f.len()];
    assert!(fit_gd_cuts(f, &h, 20.0, 500.0, 6, 3.0).is_empty());
}
