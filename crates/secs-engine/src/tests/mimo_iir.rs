//! P16 stage-boundary and statistical gates; no tolerance depends on a run.
use crate::{C, Tuning, mimo::iir::*};
use secs_compute::{CancelToken, Context, MemoryBudget, NoProgress};
use secs_dsp::optimize::NormalEquations;
use secs_oracle::{Fixture, number, oracle_libm};
use serde_json::Value;
use std::{sync::Arc, time::Instant};

fn context(threads: usize) -> Context {
    Context::new(
        threads,
        MemoryBudget::new(512 * 1024 * 1024),
        Arc::new(NoProgress),
        CancelToken::new(),
    )
}

fn fixtures() -> Vec<Fixture> {
    let fixtures = Fixture::all("engine_mimo_iir", "room");
    assert_eq!(fixtures.len(), 6);
    fixtures
}

fn problem(fx: &Fixture) -> Problem {
    let p = &fx.inputs()["problem"];
    let n = p["n"].as_u64().unwrap() as usize;
    let nf = p["nf"].as_u64().unwrap() as usize;
    let roles: Vec<String> = p["roles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().into())
        .collect();
    let r = roles.len();
    let real = |v: &Value| fx.f64s(v);
    let complex = |v: &Value| {
        fx.c128s(v)
            .into_iter()
            .map(|(re, im)| C::new(re, im))
            .collect::<Vec<_>>()
    };
    Problem {
        f: real(&p["fl"]),
        lp: complex(&p["LP"]),
        n,
        nf,
        xs: real(&p["X"]).chunks(n).map(<[f64]>::to_vec).collect(),
        wl: real(&p["WL"]).chunks(n).map(<[f64]>::to_vec).collect(),
        we: real(&p["WE"]).chunks(n).map(<[f64]>::to_vec).collect(),
        g: complex(&p["G"])
            .chunks(r * (nf / 2 + 1))
            .map(|pos| pos.chunks(nf / 2 + 1).map(<[C]>::to_vec).collect())
            .collect(),
        fir: roles.iter().map(|r| real(&p["fir"][r])).collect(),
        ranges: roles
            .iter()
            .map(|r| (number(&p["ranges"][r][0]), number(&p["ranges"][r][1])))
            .collect(),
        modes: real(&p["modes"]),
        fsiso: number(&p["fsiso"]),
        direct: number(&p["direct"]),
        level_db: number(&p["level_db"]),
        roles,
    }
}

fn pols(fx: &Fixture) -> Vec<i8> {
    fx.outputs()["reference"]["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["polarity"].as_i64().unwrap() as i8)
        .collect()
}

fn relative(got: &[f64], expected: &[f64], norm: bool) -> f64 {
    assert_eq!(got.len(), expected.len());
    if norm {
        got.iter()
            .zip(expected)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            .sqrt()
            / expected
                .iter()
                .map(|v| v * v)
                .sum::<f64>()
                .sqrt()
                .max(1e-30)
    } else {
        got.iter()
            .zip(expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max)
            / expected
                .iter()
                .map(|v| v.abs())
                .fold(0.0, f64::max)
                .max(1e-30)
    }
}

#[test]
fn mimo_iir_functions_match_dense_oracle() {
    let t = Tuning::default();
    let ctx = context(1);
    for fx in fixtures() {
        let pb = problem(&fx);
        let pols = pols(&fx);
        let j = &fx.outputs()["reference"]["joint"];
        let x = fx.f64s(&j["x0"]);
        let mut objective = Objective::new(&pb, &t, &ctx, &pols);
        let f = objective.residual(&x);
        let (h, g) = objective.normal(&x, &f).unwrap();
        let h = (0..x.len())
            .flat_map(|i| (0..x.len()).map(move |j| (i, j)))
            .map(|(i, j)| h[(i, j)])
            .collect::<Vec<_>>();
        for (name, got, expected, norm) in [
            ("residual", f, fx.f64s(&j["residual"]), true),
            ("jtj", h, fx.f64s(&j["jtj"]), false),
            ("jtf", g, fx.f64s(&j["jtf"]), false),
        ] {
            let error = relative(&got, &expected, norm);
            eprintln!("MEASURE IIR {} {name} relative={error:.17e}", fx.id);
            assert!(error <= 1e-10, "{} {name}: {error:e}", fx.id);
        }
        assert_eq!(late_modes(&pb.xs, t.mimo_a_bands, 20.0, pb.fsiso), pb.modes);
    }
}

#[derive(Default)]
struct Costs {
    calls: usize,
    above: usize,
    failures: Vec<String>,
    runs: usize,
}
impl Costs {
    /// `report_capped`: seeds only. A seed is the joint fit's start point,
    /// not an output, and a fit stopped at `max_nfev` ends at a point on a
    /// chaotic trajectory; when either Rust's or the reference's seed hit the
    /// cap, its cost is reported, not asserted. Converged seeds and the rank
    /// guard below stay asserted (packet P16, amendment on capped seeds).
    fn add(
        &mut self,
        name: &str,
        cost: f64,
        reference: &Value,
        others: Vec<&Value>,
        report_capped: bool,
    ) {
        let c0 = number(&reference["cost"]);
        let mut values = vec![c0];
        values.extend(others.iter().map(|v| number(&v["cost"])));
        self.runs = values.len();
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let cap = c0 + 1.25 * (max - min).max(1e-6 * c0);
        self.calls += 1;
        self.above += usize::from(cost > max);
        let gated = (reference["status"].as_i64().unwrap() != 0 || oracle_libm()) && !report_capped;
        eprintln!(
            "IIR_COST {name} cost={cost:.17e} reference={c0:.17e} spread={:.17e} cap={cap:.17e} margin={:.17e} above_all={} gated={gated}",
            max - min,
            cap - cost,
            cost > max
        );
        if !cost.is_finite() || (gated && cost > cap) {
            self.failures.push(name.into());
        }
    }
    fn finish(self) {
        let bound = super::e2e::binomial_quantile(self.calls, 1.0 / (self.runs + 1) as f64);
        eprintln!(
            "IIR_BINOMIAL above_all={}/{} calls={} runs={}",
            self.above, bound, self.calls, self.runs
        );
        assert!(self.above <= bound);
        assert!(
            self.failures.is_empty(),
            "cost gates failed: {:?}",
            self.failures
        );
    }
}

#[test]
fn mimo_iir_seed_costs_within_spread() {
    let t = Tuning::default();
    let ctx = context(1);
    let mut stats = Costs::default();
    for fx in fixtures() {
        let pb = problem(&fx);
        let o = fx.outputs();
        let runs = o["runs"].as_object().unwrap();
        for (r, &(lo, hi)) in pb.ranges.iter().enumerate() {
            let modes: Vec<_> = pb
                .modes
                .iter()
                .copied()
                .filter(|m| (lo..=hi).contains(m))
                .collect();
            let modes = if modes.is_empty() { &pb.modes } else { &modes };
            let seed = fit_one(
                &pb.fir[r],
                lo,
                hi.min(pb.fsiso),
                modes,
                band_of(lo, hi, pb.fsiso, &t),
                &t,
                &ctx,
            )
            .unwrap();
            assert_eq!(
                seed.polarity,
                o["reference"]["seeds"][r]["polarity"].as_i64().unwrap() as i8
            );
            stats.add(
                &format!("{}/{}", fx.id, pb.roles[r]),
                seed.cost,
                &o["reference"]["seeds"][r],
                runs.iter()
                    .filter(|(label, _)| !label.starts_with("ulp_"))
                    .map(|(_, v)| &v["seeds"][r])
                    .chain(o["seed_runs"].as_object().unwrap().values().map(|v| &v[r]))
                    .collect(),
                seed.status == 0 || o["reference"]["seeds"][r]["status"].as_i64().unwrap() == 0,
            );
            eprintln!(
                "IIR_SEED_STATUS {}/{} nfev={} status={} capped={}",
                fx.id,
                pb.roles[r],
                seed.nfev,
                seed.status,
                seed.nfev == 300
            );
        }
    }
    stats.finish();
}

#[test]
fn mimo_iir_joint_costs_within_spread() {
    let t = Tuning::default();
    let ctx = context(1);
    let mut stats = Costs::default();
    for fx in fixtures() {
        let pb = problem(&fx);
        let o = fx.outputs();
        let x0 = fx.f64s(&o["reference"]["joint"]["x0"]);
        let start = Instant::now();
        let result = solve_joint(&pb, &t, &ctx, &x0, &pols(&fx)).unwrap();
        eprintln!(
            "IIR_TIME {} Rust={} Python={} nfev={} status={}",
            fx.id,
            start.elapsed().as_secs_f64(),
            number(&o["reference"]["joint"]["seconds"]),
            result.nfev,
            result.status
        );
        stats.add(
            &fx.id,
            result.cost,
            &o["reference"]["joint"],
            o["runs"]
                .as_object()
                .unwrap()
                .values()
                .map(|v| &v["fixed_joint"])
                .collect(),
            false,
        );
    }
    stats.finish();
}

#[test]
fn mimo_iir_postprocessing_matches_oracle() {
    let t = Tuning::default();
    let ctx = context(1);
    for fx in fixtures() {
        let pb = problem(&fx);
        let o = fx.outputs();
        let x = fx.f64s(&o["reference"]["joint"]["x"]);
        let post = postprocess(&pb, &x, &pols(&fx), &t, &ctx).unwrap();
        let reference = &o["post"];
        let cut_error = (post.level_cut - number(&reference["level_cut"])).abs()
            / post.level_cut.abs().max(1.0);
        let expected_violations = fx.f64s(&reference["violations"]);
        let error = if post.violations.len() == expected_violations.len() {
            relative(&post.violations, &expected_violations, false)
        } else {
            f64::INFINITY
        };
        eprintln!(
            "IIR_POST {} cut={cut_error:e} violations={error:e} removed={:?} steps={}",
            fx.id, post.removed, post.gain_steps
        );
        if oracle_libm() {
            assert!(cut_error <= 1e-9 && error <= 1e-9);
            assert_eq!(serde_json::json!(post.removed), reference["removed"]);
            assert_eq!(
                post.gain_steps,
                reference["gain_steps"].as_u64().unwrap() as usize
            );
            let expected = reference["final"].as_object().unwrap();
            assert_eq!(post.supports.len(), expected.len());
            for (r, s) in &post.supports {
                let e = &expected[&pb.roles[*r]];
                let actual = vec![s.gain_db, s.delay_ms];
                assert!(
                    relative(
                        &actual,
                        &[number(&e["gain_db"]), number(&e["delay_ms"])],
                        false
                    ) <= 1e-9
                );
                assert_eq!(s.polarity, e["polarity"].as_i64().unwrap() as i8);
                let peq: Vec<_> = s.peq.iter().flat_map(|&(f, q, g)| [f, q, g]).collect();
                let ap: Vec<_> = s.allpass.iter().flat_map(|&(f, q)| [f, q]).collect();
                let flatten = |v: &Value| {
                    v.as_array()
                        .unwrap()
                        .iter()
                        .flat_map(|row| row.as_array().unwrap().iter().map(number))
                        .collect::<Vec<_>>()
                };
                assert!(relative(&peq, &flatten(&e["peq"]), false) <= 1e-9);
                assert!(relative(&ap, &flatten(&e["allpass"]), false) <= 1e-9);
            }
        }
    }
}

#[test]
fn mimo_iir_problem_from_replayed_design_matches_oracle() {
    for case_name in ["room", "room_dba"] {
        let case = super::cases::load(case_name);
        let design = super::design::replay(case_name);
        let view = crate::export_view(&design, &case.measurements, &case.config).unwrap();
        let prepared = crate::measurement::prepare(
            &case.measurements,
            &case.config,
            &crate::setup::Setup::new(&case.config),
        )
        .unwrap();
        for input in ["L", "R", "C"] {
            let fx = Fixture::load("engine_mimo_iir", &format!("{case_name}_{input}"));
            let expected = problem(&fx);
            let supports: Vec<_> = expected
                .roles
                .iter()
                .cloned()
                .zip(expected.fir.iter().cloned())
                .collect();
            let ideal = crate::MimoDesign {
                ranges: expected
                    .roles
                    .iter()
                    .cloned()
                    .zip(expected.ranges.iter().copied().map(Some))
                    .collect(),
                ..crate::MimoDesign::default()
            };
            let got = build_problem(
                &design,
                &view.spectra,
                &prepared.align_delay_ms,
                &ideal,
                input,
                &supports,
                &crate::MimoRequest::default(),
                &Tuning::default(),
                &context(1),
            )
            .unwrap();
            let x = relative(&got.xs.concat(), &expected.xs.concat(), true);
            let flatten = |pb: &Problem| {
                pb.g.iter()
                    .flatten()
                    .flatten()
                    .flat_map(|h| [h.re, h.im])
                    .collect::<Vec<_>>()
            };
            let g = relative(&flatten(&got), &flatten(&expected), true);
            let direct = (got.direct - expected.direct).abs() / expected.direct.abs().max(1e-30);
            eprintln!("IIR_PROBLEM {case_name}/{input} X={x:.17e} G={g:.17e} direct={direct:.17e}");
            // Packet amendment: room's known BA divergence precedes this stage.
            if case_name != "room" && oracle_libm() {
                assert!(x <= 1e-9 && g <= 1e-9 && direct <= 1e-9);
            }
        }
    }
}

fn whole(case_name: &str, threads: usize) -> crate::MimoIir {
    let case = super::cases::load(case_name);
    let d = super::design::replay(case_name);
    let ctx = context(threads);
    let req = crate::MimoRequest::default();
    // P14 owns ideal-FIR parity. P16's whole realization starts at its recorded
    // ideal support boundary, just as its oracle spread does.
    let mut ideal = crate::MimoDesign::default();
    for input in ["L", "R", "C"] {
        let fx = Fixture::load("engine_mimo_iir", &format!("{case_name}_{input}"));
        let pb = problem(&fx);
        for (role, range) in pb.roles.iter().zip(&pb.ranges) {
            if !ideal.ranges.iter().any(|(r, _)| r == role) {
                ideal.ranges.push((role.clone(), Some(*range)));
            }
        }
        ideal
            .supports
            .push((input.into(), pb.roles.into_iter().zip(pb.fir).collect()));
    }
    crate::design_mimo_iir(
        &d,
        &case.measurements,
        &case.config,
        &ideal,
        &req,
        &Tuning::default(),
        &ctx,
    )
    .unwrap()
}

#[test]
fn mimo_iir_whole_path_within_spread_and_deterministic() {
    for case_name in ["room", "room_dba"] {
        let one = whole(case_name, 1);
        let four = whole(case_name, 4);
        assert_eq!(one, four);
        for ((_, a), (_, b)) in one.supports.iter().zip(&four.supports) {
            for ((_, a), (_, b)) in a.iter().zip(b) {
                let bits = |s: &crate::IirSupport| {
                    [s.gain_db, s.delay_ms]
                        .into_iter()
                        .chain(s.peq.iter().flat_map(|&(f, q, g)| [f, q, g]))
                        .chain(s.allpass.iter().flat_map(|&(f, q)| [f, q]))
                        .map(f64::to_bits)
                        .collect::<Vec<_>>()
                };
                assert_eq!(bits(a), bits(b));
            }
        }
        let (mut above, mut n, mut median_worse, mut varying) = (0, 0, 0, 0);
        let mut run_count = 0;
        let mut failures = Vec::new();
        for (input, supports) in &one.supports {
            let fx = Fixture::load("engine_mimo_iir", &format!("{case_name}_{input}"));
            let pb = problem(&fx);
            let indexed: Vec<_> = supports
                .iter()
                .map(|(r, s)| {
                    (
                        pb.roles.iter().position(|role| role == r).unwrap(),
                        s.clone(),
                    )
                })
                .collect();
            let (band, tail) = outcome(&pb, &indexed).unwrap();
            let o = fx.outputs();
            let posts: Vec<_> = std::iter::once(&o["post"])
                .chain(o["runs"].as_object().unwrap().values().map(|r| &r["post"]))
                .collect();
            run_count = posts.len();
            for (key, got, q) in [
                (
                    "band_db",
                    band,
                    1e-9 * number(&posts[0]["band_db"]).abs().max(1.0),
                ),
                (
                    "t60_change",
                    tail,
                    1e-9 * number(&posts[0]["t60_change"]).abs().max(1.0),
                ),
                ("kept", -(supports.len() as f64), 1.0),
            ] {
                let mut values: Vec<f64> = posts
                    .iter()
                    .map(|p| {
                        if key == "kept" {
                            -(p["final"].as_object().unwrap().len() as f64)
                        } else {
                            number(&p[key])
                        }
                    })
                    .collect();
                let reference = values[0];
                values.sort_by(f64::total_cmp);
                let min = values[0];
                let max = values[values.len() - 1];
                let cap = 3.0 * (max - min).max(q);
                n += 1;
                above += usize::from(got > max);
                if max > min {
                    varying += 1;
                    median_worse += usize::from(got > crate::decay::median(&values));
                }
                eprintln!(
                    "IIR_WHOLE {case_name}/{input}/{key} Rust={got:.17e} reference={reference:.17e} cap={cap:.17e} margin={:.17e} exceeds_125={}",
                    reference + cap - got,
                    got - reference > 1.25 * (max - min).max(q)
                );
                if case_name != "room"
                    && oracle_libm()
                    && (!got.is_finite() || got - reference > cap)
                {
                    failures.push(format!("{input}/{key}"));
                }
            }
        }
        let bound = super::e2e::binomial_quantile(n, 1.0 / (run_count + 1) as f64);
        let median_bound = super::e2e::binomial_quantile(varying, 0.5);
        eprintln!(
            "IIR_WHOLE_BINOMIAL {case_name} above={above}/{bound} n={n} median={median_worse}/{median_bound} varying={varying}"
        );
        if case_name != "room" && oracle_libm() {
            assert!(above <= bound && median_worse <= median_bound);
        }
        assert!(
            failures.is_empty(),
            "whole-path failures {case_name}: {failures:?}"
        );
    }
}

#[test]
fn mimo_iir_joint_cancels_within_one_second() {
    let fx = fixtures().remove(0);
    let pb = problem(&fx);
    let ctx = context(1);
    let x = fx.f64s(&fx.outputs()["reference"]["joint"]["x0"]);
    let pols = pols(&fx);
    std::thread::scope(|scope| {
        let work = scope.spawn(|| solve_joint(&pb, &Tuning::default(), &ctx, &x, &pols));
        std::thread::sleep(std::time::Duration::from_millis(50));
        let start = Instant::now();
        ctx.cancel().cancel();
        assert!(matches!(
            work.join().unwrap(),
            Err(crate::EngineError::Cancelled)
        ));
        assert!(start.elapsed().as_secs_f64() < 1.0);
    });
}

// Upstream tests/test_mimo_band_limit_1012.py: the v2 knobs, the band edges,
// the parameter bounds, and a seed that stays inside them.
#[test]
fn mimo_iir_band_limits_match_upstream_1012() {
    let t = Tuning::default();
    assert_eq!(
        (
            t.mimo_band_db,
            t.mimo_q_max,
            t.mimo_in_band,
            t.mimo_band_reg
        ),
        (15.0, 10.0, true, 0.05)
    );
    let old = Tuning {
        mimo_band_db: 30.0,
        mimo_q_max: 40.0,
        mimo_in_band: false,
        mimo_band_reg: 0.0,
        ..Tuning::default()
    };
    assert_eq!(band_of(40.0, 120.0, 150.0, &old), (15.0, 300.0));

    let (lo, hi) = band_of(40.0, 200.0, 150.0, &t);
    assert!((lo - 36.0).abs() < 1e-9 && (hi - 165.0).abs() < 1e-9);
    let small = Tuning {
        mimo_k_bands: 3,
        mimo_a_bands: 2,
        ..Tuning::default()
    };
    let (lb, ub) = bounds((lo, hi), &small);
    for k in 0..3 {
        let i = 2 + 3 * k;
        assert!((lb[i].exp() - lo).abs() < 1e-9 && (ub[i].exp() - hi).abs() < 1e-9);
        assert!((ub[i + 1].exp() - 10.0).abs() < 1e-9);
        assert_eq!((lb[i + 2], ub[i + 2]), (-15.0, 15.0));
    }

    // Deep dips and tall peaks: three resonances at the 600 Hz low rate plus a
    // little noise (upstream draws it from NumPy's PCG64; any small noise does).
    let mut x = vec![0.0; 180];
    x[3] = 1.0;
    for (f0, r) in [(28.0, 0.995), (47.0, 0.99), (71.0, 0.985)] {
        let w = 2.0 * std::f64::consts::PI * f0 / 600.0;
        let mut y = vec![0.0; x.len()];
        for n in 0..x.len() {
            let y1 = if n >= 1 { y[n - 1] } else { 0.0 };
            let y2 = if n >= 2 { y[n - 2] } else { 0.0 };
            y[n] = x[n] + 2.0 * r * w.cos() * y1 - r * r * y2;
        }
        x = y;
    }
    let mut state = 1012u64;
    for v in &mut x {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        *v += 0.01 * ((state >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0);
    }
    let band = band_of(30.0, 150.0, 150.0, &t);
    let seed = fit_one(&x, 30.0, 150.0, &[47.0], band, &t, &context(1)).unwrap();
    let support = &unpack(&seed.x, &[seed.polarity], &t)[0];
    assert_eq!(support.peq.len(), t.mimo_k_bands);
    for &(f0, q, g) in &support.peq {
        assert!(g.abs() <= 15.0 + 1e-6, "gain {g}");
        assert!(q <= 10.0 + 1e-6, "q {q}");
        assert!(band.0 - 1e-6 <= f0 && f0 <= band.1 + 1e-6, "f0 {f0}");
    }
}
