#![forbid(unsafe_code)]
use faer::Mat;
use secs_dsp::optimize::{NormalEquations, NormalOptions, NormalTrace, XScale};
use secs_dsp::{Complex64 as C, DspError, complex_div, fft, optimize, response};
use secs_oracle::{Fixture, integer, number};
use std::collections::BTreeMap;
use std::f64::consts::PI;

struct Problem {
    kind: String,
    target: Vec<f64>,
    t: Vec<f64>,
    matrix: Vec<f64>,
    nf: usize,
    n: usize,
    fl: Vec<f64>,
    paths: Vec<C>,
    wl: Vec<f64>,
    we: Vec<f64>,
    effort: f64,
    reg: f64,
}
impl Problem {
    fn new(fx: &Fixture) -> Self {
        let i = fx.inputs();
        let kind = i["kind"].as_str().unwrap().to_owned();
        let values = |key: &str| {
            if i[key].is_null() {
                vec![]
            } else {
                fx.f64s(&i[key])
            }
        };
        let is_mimo = kind == "mimo";
        Self {
            kind,
            target: values("target"),
            t: values("t"),
            matrix: values("matrix"),
            nf: if is_mimo {
                integer(&i["nf"]) as usize
            } else {
                0
            },
            n: if is_mimo {
                integer(&i["n"]) as usize
            } else {
                0
            },
            fl: values("fl"),
            paths: if is_mimo {
                fx.f64s(&i["G"]["re"])
                    .into_iter()
                    .zip(fx.f64s(&i["G"]["im"]))
                    .map(|(re, im)| C::new(re, im))
                    .collect()
            } else {
                vec![]
            },
            wl: values("wl"),
            we: values("we"),
            effort: if is_mimo { number(&i["effort"]) } else { 0. },
            reg: if is_mimo { number(&i["reg"]) } else { 0. },
        }
    }
    fn peaking(&self, f0: f64, q: f64, gain: f64) -> Vec<C> {
        // This oracle calls peaking_response, whose z uses division by SR;
        // the existing peq_response port uses multiplication by reciprocal SR.
        let z: Vec<_> = self
            .fl
            .iter()
            .map(|f| C::from_polar(1., -2. * PI * f / 48000.))
            .collect();
        response::peaking_response_z(&z, f0, q, gain, 48000.)
    }
    fn parts(&self, x: &[f64], derivatives: bool) -> Vec<Vec<Vec<C>>> {
        (0..3)
            .map(|r| {
                let v = &x[r * 11..(r + 1) * 11];
                let peqs: Vec<_> = (0..3)
                    .map(|k| self.peaking(v[2 + 3 * k].exp(), v[3 + 3 * k].exp(), v[4 + 3 * k]))
                    .collect();
                let mut s: Vec<_> = self
                    .fl
                    .iter()
                    .map(|f| {
                        10_f64.powf(v[0] / 20.) * C::from_polar(1., -2. * PI * f * v[1] / 1000.)
                    })
                    .collect();
                for q in &peqs {
                    for (s, q) in s.iter_mut().zip(q) {
                        *s *= q;
                    }
                }
                if !derivatives {
                    return vec![s];
                }
                let mut ds = vec![vec![C::default(); self.fl.len()]; 11];
                for i in 0..self.fl.len() {
                    ds[0][i] = s[i] * 10_f64.ln() / 20.;
                    ds[1][i] = s[i] * C::new(0., -2. * PI * self.fl[i] / 1000.);
                }
                let h = 1e-4;
                for k in 0..3 {
                    for c in 0..3 {
                        let mut up = v[2 + 3 * k..5 + 3 * k].to_vec();
                        let mut dn = up.clone();
                        up[c] += h;
                        dn[c] -= h;
                        let a = self.peaking(up[0].exp(), up[1].exp(), up[2]);
                        let b = self.peaking(dn[0].exp(), dn[1].exp(), dn[2]);
                        for i in 0..self.fl.len() {
                            ds[2 + 3 * k + c][i] =
                                complex_div(s[i] * (a[i] - b[i]) / (2. * h), peqs[k][i]);
                        }
                    }
                }
                ds
            })
            .collect()
    }
    fn fun(&self, x: &[f64]) -> Vec<f64> {
        match self.kind.as_str() {
            "small" => (0..200)
                .map(|i| {
                    let k = i / 50;
                    x[2 * k] * (-x[2 * k + 1] * self.t[i]).exp() - self.target[i]
                })
                .collect(),
            "rank" => self
                .matrix
                .chunks(x.len())
                .zip(&self.target)
                .map(|(a, t)| a.iter().zip(x).map(|(a, x)| a * x).sum::<f64>() - t)
                .collect(),
            "mimo" => {
                let ss = self.parts(x, false);
                let nf = self.fl.len();
                let mut out = Vec::new();
                for p in 0..2 {
                    let spectrum: Vec<_> = (0..nf)
                        .map(|k| {
                            (0..3)
                                .map(|r| self.paths[(p * 3 + r) * nf + k] * ss[r][0][k])
                                .sum()
                        })
                        .collect();
                    let add = fft::irfft(&spectrum, self.nf);
                    out.extend(
                        (0..self.n).map(|k| {
                            self.wl[p * self.n + k] * (add[k] - self.target[p * self.n + k])
                        }),
                    );
                    out.extend(
                        (0..self.n).map(|k| {
                            self.we[p * self.n + k] * (add[k] - self.target[p * self.n + k])
                        }),
                    );
                }
                for s in ss {
                    out.extend(s[0].iter().map(|z| self.effort * z.re));
                    out.extend(s[0].iter().map(|z| self.effort * z.im));
                }
                out.extend(
                    (0..3).flat_map(|r| (0..3).map(move |k| self.reg * x[r * 11 + 4 + 3 * k])),
                );
                out
            }
            _ => panic!("unknown problem"),
        }
    }
    fn jac(&self, x: &[f64]) -> Mat<f64> {
        match self.kind.as_str() {
            "small" => Mat::from_fn(200, 8, |i, j| {
                let k = i / 50;
                let e = (-x[2 * k + 1] * self.t[i]).exp();
                if j == 2 * k {
                    e
                } else if j == 2 * k + 1 {
                    -self.t[i] * x[2 * k] * e
                } else {
                    0.
                }
            }),
            "rank" => Mat::from_fn(200, 4, |i, j| self.matrix[i * 4 + j]),
            "mimo" => {
                let ds = self.parts(x, true);
                let nf = self.fl.len();
                let mut j = Mat::zeros(4 * self.n + 6 * nf + 9, 33);
                for (r, derivatives) in ds.iter().enumerate() {
                    for (c, derivative) in derivatives.iter().enumerate() {
                        let col = 11 * r + c;
                        for p in 0..2 {
                            let spectrum: Vec<_> = (0..nf)
                                .map(|k| self.paths[(p * 3 + r) * nf + k] * derivative[k])
                                .collect();
                            let time = fft::irfft(&spectrum, self.nf);
                            for k in 0..self.n {
                                j[(2 * p * self.n + k, col)] = self.wl[p * self.n + k] * time[k];
                                j[((2 * p + 1) * self.n + k, col)] =
                                    self.we[p * self.n + k] * time[k];
                            }
                        }
                        let row = 4 * self.n + 2 * r * nf;
                        for k in 0..nf {
                            j[(row + k, col)] = self.effort * derivative[k].re;
                            j[(row + nf + k, col)] = self.effort * derivative[k].im;
                        }
                    }
                    for k in 0..3 {
                        j[(4 * self.n + 6 * nf + r * 3 + k, r * 11 + 4 + 3 * k)] = self.reg;
                    }
                }
                j
            }
            _ => panic!("unknown problem"),
        }
    }
}
impl NormalEquations for Problem {
    fn residual(&mut self, x: &[f64]) -> Vec<f64> {
        self.fun(x)
    }
    fn normal(&mut self, x: &[f64], f: &[f64]) -> Result<(Mat<f64>, Vec<f64>), DspError> {
        let j = self.jac(x);
        let n = x.len();
        let h = Mat::from_fn(n, n, |a, b| {
            (0..f.len()).map(|i| j[(i, a)] * j[(i, b)]).sum()
        });
        let g = (0..n)
            .map(|a| (0..f.len()).map(|i| j[(i, a)] * f[i]).sum())
            .collect();
        Ok((h, g))
    }
}
struct Frozen<'a> {
    problem: &'a Problem,
    h: Mat<f64>,
    g: Vec<f64>,
}
impl NormalEquations for Frozen<'_> {
    fn residual(&mut self, x: &[f64]) -> Vec<f64> {
        self.problem.fun(x)
    }
    fn normal(&mut self, _: &[f64], _: &[f64]) -> Result<(Mat<f64>, Vec<f64>), DspError> {
        Ok((self.h.clone(), self.g.clone()))
    }
}

// Strict componentwise relative gate, with the packet's absolute floor near zero.
fn compare(got: &[f64], expected: &[f64], rtol: f64) -> (f64, f64, bool) {
    assert_eq!(got.len(), expected.len());
    let mut relative = 0.0_f64;
    let mut absolute = 0.0_f64;
    let mut pass = true;
    for (a, b) in got.iter().zip(expected) {
        let gap = (a - b).abs();
        absolute = absolute.max(gap);
        relative = relative.max(gap / b.abs().max(1e-12));
        pass &= a.is_finite() && gap <= (rtol * b.abs()).max(1e-12);
    }
    (relative, absolute, pass)
}

#[test]
fn trf_normal_one_step_replay() {
    let mut failures = Vec::new();
    let mut deficient = false;
    let mut zero = false;
    for fx in Fixture::all("trf_normal", "") {
        let problem = Problem::new(&fx);
        let i = fx.inputs();
        let o = fx.outputs();
        let n = fx.f64s(&i["x0"]).len();
        let columns: BTreeMap<_, _> = o["trials"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.as_str(), fx.f64s(v)))
            .collect();
        let count = columns["Delta"].len();
        assert_eq!(count + 1, integer(&o["nfev"]) as usize);
        let mut maxima: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
        for k in 0..count {
            let base = &o["bases"][columns["base"][k] as usize];
            let h = fx.f64s(&base["normal"]);
            let mut frozen = Frozen {
                problem: &problem,
                h: Mat::from_fn(n, n, |i, j| h[i * n + j]),
                g: fx.f64s(&base["gradient"]),
            };
            let trial = optimize::trf_normal_one_step(
                &mut frozen,
                &fx.f64s(&base["x"]),
                &fx.f64s(&i["lb"]),
                &fx.f64s(&i["ub"]),
                columns["Delta"][k],
                columns["alpha"][k],
                &fx.f64s(&base["scale_inv"]),
            )
            .unwrap();
            deficient |= !trial.full_rank;
            zero |= trial.s.contains(&0.);
            for (key, got) in [
                ("step", trial.step.clone()),
                ("predicted_reduction", vec![trial.predicted_reduction]),
                ("Delta_new", vec![trial.delta_new]),
                ("alpha_out", vec![trial.alpha_out]),
            ] {
                let width = got.len();
                let (rel, abs, pass) =
                    compare(&got, &columns[key][k * width..(k + 1) * width], 1e-9);
                let max = maxima.entry(key).or_default();
                max.0 = max.0.max(rel);
                max.1 = max.1.max(abs);
                if !pass {
                    failures.push(format!("{} trial={k} {key} rel={rel:e} abs={abs:e}", fx.id));
                }
            }
            for (key, got) in [("scale_inv", trial.scale_inv), ("v", trial.v)] {
                let (rel, abs, pass) = compare(&got, &fx.f64s(&base[key]), 1e-9);
                let max = maxima.entry(key).or_default();
                max.0 = max.0.max(rel);
                max.1 = max.1.max(abs);
                if !pass {
                    failures.push(format!("{} trial={k} {key} rel={rel:e} abs={abs:e}", fx.id));
                }
            }
            let (rel, abs, _) = compare(&[trial.ratio], &[columns["ratio"][k]], 1e-9);
            let max = maxima.entry("ratio_report").or_default();
            max.0 = max.0.max(rel);
            max.1 = max.1.max(abs);
            if trial.accepted != (columns["accepted"][k] == 1.) {
                failures.push(format!("{} trial={k} acceptance", fx.id));
            }
        }
        for (key, (rel, abs)) in maxima {
            println!(
                "TRF_NORMAL_REPORT {} replay {key} max_rel={rel:.17e} max_abs={abs:.17e} trials={count}",
                fx.id
            );
        }
    }
    assert!(
        deficient && zero,
        "rank-deficient and zero-singular-value coverage"
    );
    assert!(
        failures.is_empty(),
        "normal TRF replay failures: {failures:#?}"
    );
}

#[test]
fn trf_normal_whole_runs() {
    let mut failures = Vec::new();
    for fx in Fixture::all("trf_normal", "") {
        let mut problem = Problem::new(&fx);
        let i = fx.inputs();
        let o = fx.outputs();
        let result = optimize::least_squares_normal(
            &mut problem,
            &fx.f64s(&i["x0"]),
            &fx.f64s(&i["lb"]),
            &fx.f64s(&i["ub"]),
            &NormalOptions {
                max_nfev: integer(&i["cap"]) as usize,
                x_scale: if i["x_scale"] == "jac" {
                    XScale::Jac
                } else {
                    XScale::Unit
                },
                ..NormalOptions::default()
            },
        )
        .unwrap();
        let expected_x = fx.f64s(&o["x"]);
        let status = integer(&o["status"]);
        let (xr, xa, xpass) = compare(&result.x, &expected_x, 1e-7);
        let (cr, ca, _) = compare(&[result.cost], &[number(&o["cost"])], 1e-7);
        // A run that stops on ftol (status 2) only pins x where the cost is not
        // flat: two runs that both stop when the relative decrease falls below
        // ftol agree in cost to about ftol and in residual to about
        // sqrt(ftol) of its norm, whatever x does along flat directions. The
        // residual is what a caller consumes, so that is the gate there.
        let f_rust = problem.fun(&result.x);
        let f_scipy = problem.fun(&expected_x);
        let norm = |v: &[f64]| v.iter().map(|a| a * a).sum::<f64>().sqrt();
        let gap: Vec<f64> = f_rust.iter().zip(&f_scipy).map(|(a, b)| a - b).collect();
        let resid_rel = norm(&gap) / norm(&f_scipy).max(f64::MIN_POSITIVE);
        let ftol = NormalOptions::default().ftol;
        let counts = (result.nfev, result.njev, result.status)
            == (
                integer(&o["nfev"]) as usize,
                integer(&o["njev"]) as usize,
                status as i32,
            );
        println!(
            "TRF_NORMAL_REPORT {} whole x_rel={xr:.17e} x_abs={xa:.17e} cost_rel={cr:.17e} cost_abs={ca:.17e} resid_rel={resid_rel:.17e} counts=({},{},{}) oracle=({},{},{}) capped={}",
            fx.id,
            result.nfev,
            result.njev,
            result.status,
            o["nfev"],
            o["njev"],
            o["status"],
            status == 0
        );
        let pass = match status {
            // Capped runs are reported only (ENGINE.md §3, P07 rule).
            0 => true,
            2 => counts && cr <= ftol && resid_rel <= ftol.sqrt(),
            _ => counts && xpass && cr <= 1e-7,
        };
        // Off the oracle libm the residual's own transcendental calls move the
        // trajectory, so whole runs are reported there (ENGINE.md §3).
        if !pass && secs_oracle::oracle_libm() {
            failures.push(format!(
                "{} status={status} counts_equal={counts} x_rel={xr:e} cost_rel={cr:e} resid_rel={resid_rel:e}",
                fx.id
            ));
        } else if !pass {
            println!("TRF_NORMAL_OFFLIBM {} whole run differs", fx.id);
        }
    }
    assert!(
        failures.is_empty(),
        "normal TRF converged failures: {failures:#?}"
    );
}

#[test]
fn trf_normal_dense_analytic_equivalence() {
    let fx = Fixture::all("trf_normal", "small_unit").pop().unwrap();
    let problem = Problem::new(&fx);
    let i = fx.inputs();
    let mut dense_trace = optimize::LeastSquaresTrace::default();
    optimize::least_squares_analytic(
        |x| problem.fun(x),
        |x| problem.jac(x),
        &fx.f64s(&i["x0"]),
        &fx.f64s(&i["lb"]),
        &fx.f64s(&i["ub"]),
        optimize::Options {
            max_nfev: 150,
            ..optimize::Options::default()
        },
        Some(&mut dense_trace),
    )
    .unwrap();
    let mut normal_trace = NormalTrace::default();
    optimize::least_squares_normal_with_trace(
        &mut Problem::new(&fx),
        &fx.f64s(&i["x0"]),
        &fx.f64s(&i["lb"]),
        &fx.f64s(&i["ub"]),
        &NormalOptions {
            max_nfev: 150,
            ..NormalOptions::default()
        },
        Some(&mut normal_trace),
    )
    .unwrap();
    let dense: Vec<_> = dense_trace.trials.iter().filter(|t| t.accepted).collect();
    let normal: Vec<_> = normal_trace.trials.iter().filter(|t| t.accepted).collect();
    assert_eq!(dense.len(), normal.len());
    for (k, (d, n)) in dense.iter().zip(normal).enumerate() {
        let (rel, abs, pass) = compare(&n.step, &d.step, 1e-9);
        println!("TRF_NORMAL_REPORT dense_equivalence trial={k} rel={rel:e} abs={abs:e}");
        assert!(pass, "accepted step {k}: rel={rel:e} abs={abs:e}");
    }
}

#[test]
fn trf_normal_nonfinite_normal_returns_error() {
    struct Broken;
    impl NormalEquations for Broken {
        fn residual(&mut self, x: &[f64]) -> Vec<f64> {
            vec![x[0] - 1.]
        }
        fn normal(&mut self, _: &[f64], _: &[f64]) -> Result<(Mat<f64>, Vec<f64>), DspError> {
            Ok((Mat::from_fn(1, 1, |_, _| f64::NAN), vec![-1.]))
        }
    }
    for x_scale in [XScale::Jac, XScale::Unit] {
        let result = optimize::least_squares_normal(
            &mut Broken,
            &[0.],
            &[-2.],
            &[2.],
            &NormalOptions {
                x_scale,
                ..NormalOptions::default()
            },
        );
        assert!(result.is_err());
    }
}
