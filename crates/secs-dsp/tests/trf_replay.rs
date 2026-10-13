//! SciPy TRF call-boundary replay. V and uf must share each sign adjustment.
#![forbid(unsafe_code)]
use secs_dsp::{optimize, response};
use secs_oracle::{Fixture, errors, number};

#[test]
fn trf_intermediate_report() {
    for fx in Fixture::all("dsp", "trf_replay_") {
        let i = fx.inputs();
        let grid = fx.f64s(&i["g"]);
        let target = fx.f64s(&i["target"]);
        let fun = |x: &[f64]| -> Vec<f64> {
            let bands: Vec<_> = x
                .chunks(3)
                .map(|v| (v[0].exp(), v[1].exp(), v[2]))
                .collect();
            response::peq_response(&grid, &bands)
                .iter()
                .enumerate()
                .map(|(k, &z)| {
                    let weight = if grid[k] >= 20. && grid[k] <= 500. {
                        1.
                    } else {
                        0.15
                    };
                    weight * (20. * (response::magnitude(z) + 1e-12).log10() - target[k])
                })
                .collect()
        };
        for (index, expected) in fx.outputs()["trials"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let x = fx.f64s(&expected["x"]);
            let mut frozen = optimize::LeastSquaresTrace::default();
            optimize::least_squares_with_trace(
                &fun,
                &x,
                &fx.f64s(&i["lb"]),
                &fx.f64s(&i["ub"]),
                1,
                Some(&mut frozen),
            )
            .unwrap();
            for (key, actual) in [("f", fun(&x)), ("J", frozen.j0)] {
                let (abs, rel) = errors(&actual, &fx.f64s(&expected[key]));
                println!(
                    "TRACE {} frozen={index} {key} max_abs={abs:e} max_rel={rel:e}",
                    fx.id
                );
            }
        }
        let mut trace = optimize::LeastSquaresTrace::default();
        optimize::least_squares_with_trace(
            fun,
            &fx.f64s(&i["x0"]),
            &fx.f64s(&i["lb"]),
            &fx.f64s(&i["ub"]),
            4,
            Some(&mut trace),
        )
        .unwrap();
        assert_eq!(trace.trials.len(), 3);
        let mut first = None;
        for (index, trial) in trace.trials.iter().take(3).enumerate() {
            let actual = serde_json::to_value(trial).unwrap();
            let expected = &fx.outputs()["trials"][index];
            let n = trial.s.len();
            let v = fx.f64s(&expected["V"]);
            let signs: Vec<_> = (0..n)
                .map(|k| {
                    let alignment: f64 = (0..n)
                        .map(|j| trial.right_vectors[j * n + k] * v[j * n + k])
                        .sum();
                    if alignment < 0. { -1. } else { 1. }
                })
                .collect();
            for key in [
                "x",
                "f",
                "J",
                "g",
                "g_norm",
                "v",
                "dv",
                "Delta",
                "d",
                "diag_h",
                "g_h",
                "J_h",
                "theta",
                "s",
                "V",
                "uf",
                "alpha_in",
                "alpha_out",
                "n_iter",
                "p_h",
                "step",
                "step_h",
                "x_new",
                "actual_reduction",
                "predicted_reduction",
                "ratio",
                "Delta_new",
            ] {
                let mut a = if actual[key].is_array() {
                    fx.f64s(&actual[key])
                } else {
                    vec![number(&actual[key])]
                };
                let b = if expected[key].is_number() {
                    vec![number(&expected[key])]
                } else {
                    fx.f64s(&expected[key])
                };
                if key == "V" || key == "uf" {
                    for (k, a) in a.iter_mut().enumerate() {
                        *a *= signs[k % n];
                    }
                }
                let (abs, rel) = errors(&a, &b);
                println!(
                    "TRACE {} trial={index} {key} max_abs={abs:.17e} max_rel={rel:.17e}",
                    fx.id
                );
                let outside = if key == "n_iter" { a != b } else { rel > 1e-9 };
                if outside && first.is_none() {
                    let k = a
                        .iter()
                        .zip(&b)
                        .position(|(a, b)| (a - b).abs() > 1e-9 * b.abs().max(1.))
                        .unwrap_or(0);
                    first = Some(format!(
                        "trial={index} {key}[{k}] got={:.17e} expected={:.17e}",
                        a[k], b[k]
                    ));
                }
            }
        }
        println!("TRACE {} FIRST={first:?}", fx.id);
    }
}
