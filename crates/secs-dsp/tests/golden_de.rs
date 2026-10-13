#![forbid(unsafe_code)]

use secs_dsp::optimize::{DifferentialEvolutionOptions, differential_evolution};
use secs_dsp::random::RandomState;
use secs_oracle::{Fixture, assert_close, assert_same, integer, number, oracle_libm, rtol};

#[test]
fn golden_random_state() {
    for fx in Fixture::all("dsp_de", "rs_") {
        let seed = integer(&fx.inputs()["seed"]) as u32;
        let out = fx.outputs();
        let mut rng = RandomState::new(seed);
        let raw = fx.blob(&out["raw"]);
        let (words, remainder) = raw.as_chunks::<4>();
        assert!(remainder.is_empty());
        assert_eq!(words.len(), 5000);
        for (index, bytes) in words.iter().enumerate() {
            assert_eq!(
                rng.next_u32(),
                u32::from_le_bytes(*bytes),
                "{} raw {index}",
                fx.id
            );
        }
        let expected = fx.f64s(&out["samples"]);
        let mut rng = RandomState::new(seed);
        let got: Vec<_> = (0..expected.len()).map(|_| rng.random_sample()).collect();
        assert_same(&format!("{} samples", fx.id), &got, &expected);
        let mut rng = RandomState::new(seed);
        let mut shuffled = Vec::new();
        let mut previous = "";
        for (i, op) in out["operations"].as_array().unwrap().iter().enumerate() {
            let kind = op["kind"].as_str().unwrap();
            let args = &op["args"];
            let expected = fx.f64s(&op["values"]);
            let got: Vec<f64> = match kind {
                "randint" => (0..expected.len())
                    .map(|_| rng.randint(integer(&args[0]) as usize).unwrap() as f64)
                    .collect(),
                "uniform" => (0..expected.len())
                    .map(|_| rng.uniform(number(&args[0]), number(&args[1])).unwrap())
                    .collect(),
                "random_sample" => (0..expected.len()).map(|_| rng.random_sample()).collect(),
                "permutation" => rng
                    .permutation(integer(&args[0]) as usize)
                    .into_iter()
                    .map(|v| v as f64)
                    .collect(),
                "shuffle" => {
                    let n = integer(&args[0]) as usize;
                    if previous != "shuffle" {
                        shuffled = (0..n).collect();
                    }
                    rng.shuffle(&mut shuffled);
                    shuffled.iter().map(|&v| v as f64).collect()
                }
                _ => panic!("unknown operation {kind}"),
            };
            assert_same(&format!("{} op{i} {kind}", fx.id), &got, &expected);
            previous = kind;
        }
    }
}

fn objective(kind: &str, x: &[f64]) -> f64 {
    let mut total = 0.0;
    match kind {
        "ba" => {
            for (j, value) in x.iter().enumerate() {
                let target = 0.125 * (j + 1) as f64 * if j % 2 == 0 { 1.0 } else { -1.0 };
                let error = value - target;
                total += (1.0 + 0.25 * j as f64) * error * error;
                total += 0.03125 * error.abs();
            }
            for pair in x.windows(2) {
                total += 0.0625 * (pair[1] - pair[0]).abs();
            }
            total
        }
        "rosen" => {
            for pair in x.windows(2) {
                let a = pair[1] - pair[0] * pair[0];
                let b = 1.0 - pair[0];
                total += 100.0 * a * a + b * b;
            }
            total
        }
        "plateau" => x.iter().fold(0.0, |s, v| s + (v.abs() * 2.0).floor()),
        "constant" => 7.0,
        "offset" => 10.0 + objective("ba", x),
        "nan" => f64::NAN,
        "inf" => f64::INFINITY,
        _ => panic!("unknown cost {kind}"),
    }
}

fn compare(name: &str, actual: &[f64], expected: &[f64]) {
    if oracle_libm() {
        assert_same(name, actual, expected);
    } else {
        assert_close(name, actual, expected, 0.0, rtol(0.0));
    }
}

/// A trace is a sidecar while short and a digest when long (oracle/export_dsp_de.py).
fn compare_trace(fx: &Fixture, key: &str, actual: &[f64], expected: &serde_json::Value) {
    let name = format!("{} {key}", fx.id);
    if !Fixture::is_digest(expected) {
        return compare(&name, actual, &fx.f64s(expected));
    }
    if oracle_libm() {
        fx.assert_digest(&name, expected, actual);
        println!("MEASURE {name} n={} exact (digest)", actual.len());
    } else {
        assert_eq!(
            actual.len() as u64,
            integer(&expected["length"]),
            "{name} length"
        );
        let first = fx.f64s(&expected["first"]);
        let last = fx.f64s(&expected["last"]);
        compare(&format!("{name} first"), &actual[..first.len()], &first);
        compare(
            &format!("{name} last"),
            &actual[actual.len() - last.len()..],
            &last,
        );
    }
}

#[test]
fn golden_differential_evolution() {
    let mut count = 0;
    let mut converged = 0;
    let mut capped = 0;
    for fx in Fixture::all("dsp_de", "de_") {
        let input = fx.inputs();
        let out = fx.outputs();
        let kind = input["kind"].as_str().unwrap();
        let bounds: Vec<_> = input["bounds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| (number(&pair[0]), number(&pair[1])))
            .collect();
        let x0 = (!input["x0"].is_null()).then(|| fx.f64s(&input["x0"]));
        let options = DifferentialEvolutionOptions {
            maxiter: integer(&input["maxiter"]) as usize,
            popsize: integer(&input["popsize"]) as usize,
            tol: number(&input["tol"]),
            seed: integer(&input["seed"]) as u32,
        };
        let mut evaluations = Vec::new();
        let mut costs = Vec::new();
        let result = differential_evolution(
            |x| {
                evaluations.extend_from_slice(x);
                let cost = objective(kind, x);
                costs.push(cost);
                cost
            },
            &bounds,
            x0.as_deref(),
            options,
        )
        .unwrap();
        assert_eq!(result.nit as u64, integer(&out["nit"]), "{} nit", fx.id);
        assert_eq!(result.nfev as u64, integer(&out["nfev"]), "{} nfev", fx.id);
        assert_eq!(
            result.success,
            out["success"].as_bool().unwrap(),
            "{} success",
            fx.id
        );
        assert_eq!(
            result.message,
            out["message"].as_str().unwrap(),
            "{} message",
            fx.id
        );
        assert_eq!(evaluations.len(), result.nfev * bounds.len());
        compare_trace(&fx, "evaluations", &evaluations, &out["evaluations"]);
        compare_trace(&fx, "costs", &costs, &out["costs"]);
        compare(&format!("{} x", fx.id), &result.x, &fx.f64s(&out["x"]));
        compare(
            &format!("{} fun", fx.id),
            &[result.fun],
            &[number(&out["fun"])],
        );
        count += 1;
        if result.success {
            converged += 1;
        } else {
            capped += 1;
        }
    }
    assert!(count >= 60 && converged > 0 && capped > 0);
}

#[test]
fn de_invalid_arguments_do_not_evaluate_cost() {
    let options = DifferentialEvolutionOptions::default();
    let cost = |_: &[f64]| -> f64 { panic!("invalid arguments must not evaluate cost") };
    assert!(differential_evolution(cost, &[], None, options).is_err());
    assert!(differential_evolution(cost, &[(0.0, f64::INFINITY)], None, options).is_err());
    assert!(differential_evolution(cost, &[(f64::NAN, 1.0)], None, options).is_err());
    assert!(differential_evolution(cost, &[(0.0, 1.0)], Some(&[]), options).is_err());
    assert!(differential_evolution(cost, &[(0.0, 1.0)], Some(&[2.0]), options).is_err());
    let mut rng = RandomState::new(1);
    assert!(rng.randint(0).is_err());
    assert!(rng.uniform(1.0, 0.0).is_err());
    assert!(rng.uniform(0.0, f64::INFINITY).is_err());
    assert!(rng.uniform(f64::NAN, 1.0).is_err());
    assert_eq!(rng.next_u32(), RandomState::new(1).next_u32());
}
