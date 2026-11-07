//! Release-only audit harness; fixture loading and warm-up are outside timings.
#![forbid(unsafe_code)]

use secs_dsp::Complex64 as C;
use secs_engine::{
    DesignRequest, ProjectView, design,
    mimo::{self, MimoDesign, MimoRequest, design_mimo, design_mimo_iir},
    preview,
    testkit::perf,
    xo,
};
use secs_engine_testkit as cases;

use std::{hint::black_box, time::Instant};

fn measure<T>(op: &str, case: &str, threads: usize, mut call: impl FnMut() -> T) {
    if std::env::var_os("SECS_PERF_MEMORY_ONCE").is_some() {
        black_box(call());
        return;
    }
    let warm_started = Instant::now();
    let warm = black_box(call());
    let warm_seconds = warm_started.elapsed().as_secs_f64();
    drop(warm);
    let mut samples = Vec::new();
    let runs = std::env::var("SECS_PERF_RUNS")
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or_else(|_| {
            if (op.starts_with("mimo_") && warm_seconds > 60.0)
                || (matches!(op, "design" | "recommend") && warm_seconds > 10.0)
            {
                3
            } else {
                5
            }
        });
    for _ in 0..runs {
        let start = Instant::now();
        let result = black_box(call());
        samples.push(start.elapsed().as_secs_f64());
        drop(result);
    }
    let mut sorted = samples.clone();
    sorted.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({"implementation":"rust", "op":op, "case":case, "threads":threads, "samples_s":samples, "median_s":sorted[sorted.len() / 2]})
    );
}

fn big_case() -> cases::Case {
    let case = cases::load("big");
    if let Ok(folder) = std::env::var("SECS_PERF_IR_DUMP") {
        std::fs::create_dir_all(&folder).unwrap();
        for (position, roles) in &case.measurements.positions {
            for (role, measurement) in roles {
                let data: Vec<_> = (0..measurement.samples.len())
                    .flat_map(|i| (measurement.samples.at(i) as f32).to_le_bytes())
                    .collect();
                std::fs::write(
                    std::path::Path::new(&folder).join(format!("{position}_{role}.f32")),
                    data,
                )
                .unwrap();
            }
        }
    }
    case
}

// Candidate-only implementations: production keeps just the selected solver.
struct SpectralSweep {
    q: faer::Mat<f64>,
    eigenvalues: faer::diag::Diag<f64>,
    c: Vec<f64>,
}
impl SpectralSweep {
    fn new(ata: &[f64], atb: &[f64]) -> Self {
        use faer::{
            Par,
            dyn_stack::{MemBuffer, MemStack},
            linalg::evd,
        };
        let n = atb.len();
        let a = faer::Mat::from_fn(n, n, |i, j| ata[i * n + j]);
        let mut q = faer::Mat::zeros(n, n);
        let mut eigenvalues = faer::diag::Diag::zeros(n);
        let mut memory = MemBuffer::new(evd::self_adjoint_evd_scratch::<f64>(
            n,
            evd::ComputeEigenvectors::Yes,
            Par::Seq,
            Default::default(),
        ));
        evd::self_adjoint_evd(
            a.as_ref(),
            eigenvalues.as_mut(),
            Some(q.as_mut()),
            Par::Seq,
            MemStack::new(&mut memory),
            Default::default(),
        )
        .unwrap();
        let c = (0..n)
            .map(|j| (0..n).map(|i| q[(i, j)] * atb[i]).sum())
            .collect();
        Self { q, eigenvalues, c }
    }
    fn solve(&self, lambda: f64) -> Vec<f64> {
        let n = self.c.len();
        let mut out = vec![0.0; n];
        for j in 0..n {
            assert!(self.eigenvalues[j] + lambda > 0.0);
            let y = self.c[j] / (self.eigenvalues[j] + lambda);
            for (i, v) in out.iter_mut().enumerate() {
                *v += self.q[(i, j)] * y;
            }
        }
        out
    }
}

struct ReusedCholesky<'a> {
    ata: &'a [f64],
    atb: &'a [f64],
    factor: Option<faer::Mat<f64>>,
    lambda: f64,
    factorizations: usize,
    iterations: usize,
}
impl<'a> ReusedCholesky<'a> {
    fn new(ata: &'a [f64], atb: &'a [f64]) -> Self {
        Self {
            ata,
            atb,
            factor: None,
            lambda: 0.0,
            factorizations: 0,
            iterations: 0,
        }
    }
    fn precondition(&self, rhs: &[f64]) -> Vec<f64> {
        use faer::{
            Par,
            dyn_stack::{MemBuffer, MemStack},
            linalg::cholesky::llt,
        };
        let n = rhs.len();
        let mut rhs = faer::Mat::from_fn(n, 1, |i, _| rhs[i]);
        let mut memory = MemBuffer::new(llt::solve::solve_in_place_scratch::<f64>(n, 1, Par::Seq));
        llt::solve::solve_in_place(
            self.factor.as_ref().unwrap().as_ref(),
            rhs.as_mut(),
            Par::Seq,
            MemStack::new(&mut memory),
        );
        (0..n).map(|i| rhs[(i, 0)]).collect()
    }
    fn multiply(&self, x: &[f64], lambda: f64) -> Vec<f64> {
        self.ata
            .chunks_exact(x.len())
            .enumerate()
            .map(|(i, row)| row.iter().zip(x).map(|(a, x)| a * x).sum::<f64>() + lambda * x[i])
            .collect()
    }
    fn solve(&mut self, lambda: f64) -> Vec<f64> {
        use faer::{
            Par,
            dyn_stack::{MemBuffer, MemStack},
            linalg::cholesky::llt,
        };
        let dot = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(a, b)| a * b).sum::<f64>();
        let n = self.atb.len();
        if self.factor.is_some() && (lambda.log10() - self.lambda.log10()).abs() <= 1.5f64.log10() {
            let mut x = self.precondition(self.atb);
            let residual = |x: &[f64]| {
                self.atb
                    .iter()
                    .zip(self.multiply(x, lambda))
                    .map(|(b, a)| b - a)
                    .collect::<Vec<_>>()
            };
            let mut r = residual(&x);
            let mut z = self.precondition(&r);
            let mut p = z.clone();
            let mut rz = dot(&r, &z);
            let threshold = 1e-28 * dot(self.atb, self.atb);
            for iteration in 0..=30 {
                if dot(&r, &r) <= threshold {
                    let true_residual = residual(&x);
                    if dot(&true_residual, &true_residual) <= threshold {
                        self.iterations += iteration;
                        return x;
                    }
                }
                if iteration == 30 {
                    break;
                }
                let ap = self.multiply(&p, lambda);
                let alpha = rz / dot(&p, &ap);
                for i in 0..n {
                    x[i] += alpha * p[i];
                    r[i] -= alpha * ap[i];
                }
                z = self.precondition(&r);
                let next = dot(&r, &z);
                let beta = next / rz;
                for i in 0..n {
                    p[i] = z[i] + beta * p[i];
                }
                rz = next;
            }
            self.iterations += 30;
        }
        let mut a = faer::Mat::from_fn(n, n, |i, j| {
            self.ata[i * n + j] + if i == j { lambda } else { 0.0 }
        });
        let mut memory = MemBuffer::new(llt::factor::cholesky_in_place_scratch::<f64>(
            n,
            Par::Seq,
            Default::default(),
        ));
        llt::factor::cholesky_in_place(
            a.as_mut(),
            Default::default(),
            Par::Seq,
            MemStack::new(&mut memory),
            Default::default(),
        )
        .unwrap();
        self.factor = Some(a);
        self.lambda = lambda;
        self.factorizations += 1;
        self.precondition(self.atb)
    }
}

fn main() {
    if cfg!(debug_assertions) {
        panic!("run with --release");
    }
    let args: Vec<_> = std::env::args().collect();
    let op = args.get(1).map(String::as_str).unwrap_or("design");
    let name = args.get(2).map(String::as_str).unwrap_or("dolby");
    let threads = std::env::var("SECS_PERF_THREADS")
        .ok()
        .map(|v| v.parse().unwrap())
        .unwrap_or_else(|| std::thread::available_parallelism().unwrap().get());
    let ctx = perf::unbounded_context(threads);
    let case = if name == "big" {
        big_case()
    } else {
        cases::load(name)
    };
    if op == "mimo_case" {
        println!(
            "{}",
            serde_json::json!({"positions":case.measurements.positions.len(),"mains":case.config.mains,"subs":case.config.subs})
        );
        return;
    }
    let request = DesignRequest {
        measurements: &case.measurements,
        config: &case.config,
        tuning: &case.tuning,
        subs_only: false,
        adapter_notes: &[],
    };
    let run = || design(&request, &ctx, &*perf::production_fitters(&ctx)).unwrap();
    match op {
        "mimo_profile_design" | "mimo_sweep_candidates" => {
            let d = run();
            let req = MimoRequest::default();
            let spectra = d.export_view().unwrap().spectra;
            let prepared = perf::prepare(
                &case.measurements,
                &case.config,
                &perf::Setup::new(&case.config),
            )
            .unwrap();
            let positions: Vec<_> = d.positions().collect();
            let input_role = d.main_roles().next().unwrap().to_owned();
            let ideal = MimoDesign {
                ranges: mimo::support_ranges(&d, &spectra, req.fsiso, &positions).unwrap(),
                ..MimoDesign::default()
            };
            let roles: Vec<_> = ideal
                .ranges
                .iter()
                .filter(|(role, range)| range.is_some() && *role != input_role)
                .map(|(role, _)| role.clone())
                .collect();
            let mut xs = Vec::new();
            let mut paths = Vec::new();
            for &position in &positions {
                xs.push(
                    secs_dsp::lowrate::to_low(
                        &perf::input_response(&d, &input_role, &spectra, position).unwrap(),
                        perf::design_freqs(),
                        720,
                    )
                    .unwrap(),
                );
                paths.push(
                    roles
                        .iter()
                        .map(|r| {
                            secs_dsp::lowrate::to_low(
                                &mimo::support_path(
                                    &ideal,
                                    r,
                                    &spectra,
                                    position,
                                    &prepared.align_delay_ms,
                                )
                                .unwrap(),
                                perf::design_freqs(),
                                720,
                            )
                            .unwrap()
                        })
                        .collect::<Vec<_>>(),
                );
            }
            let lp = mimo::lowpass_fir(req.fsiso, 31).unwrap();
            let filtered: Vec<Vec<Vec<f64>>> = paths
                .iter()
                .map(|gs| {
                    gs.iter()
                        .map(|g| {
                            let mut h = perf::mimo_convolve(g, &lp);
                            h.truncate(720);
                            h
                        })
                        .collect()
                })
                .collect();
            let direct = secs_dsp::lowrate::to_low(
                perf::main_response(&d, &input_role).unwrap(),
                perf::design_freqs(),
                720,
            )
            .unwrap();
            measure("mimo_profile_normal", name, threads, || {
                perf::mimo_normal_equations(
                    &xs,
                    &filtered,
                    req.taps_low,
                    10.0,
                    perf::mimo_early_samples(req.mimo_early_ms),
                )
            });
            let (ata, atb) = perf::mimo_normal_equations(
                &xs,
                &filtered,
                req.taps_low,
                10.0,
                perf::mimo_early_samples(req.mimo_early_ms),
            );
            let n = atb.len();
            let norm = secs_dsp::pairwise_sum(&(0..n).map(|k| ata[k * n + k]).collect::<Vec<_>>())
                / n as f64;
            if op == "mimo_sweep_candidates" {
                let result =
                    perf::mimo_design_input(&xs, &paths, &direct, &roles, &req, &ctx, &|_, _| {})
                        .unwrap();
                let lambdas: Vec<_> = result
                    .trajectory_midpoints()
                    .iter()
                    .map(|mid| norm * 10f64.powf(*mid))
                    .collect();
                println!(
                    "{}",
                    serde_json::json!({"dimension": n, "lambdas": lambdas})
                );
                measure("sweep_cholesky", name, threads, || {
                    lambdas
                        .iter()
                        .map(|&l| perf::mimo_solve(&ata, &atb, l, false).unwrap())
                        .collect::<Vec<_>>()
                });
                measure("sweep_spectral", name, threads, || {
                    let sweep = SpectralSweep::new(&ata, &atb);
                    lambdas.iter().map(|&l| sweep.solve(l)).collect::<Vec<_>>()
                });
                measure("sweep_tridiagonal", name, threads, || {
                    let sweep = perf::MimoRidgeSweep::new(&ata, &atb);
                    lambdas
                        .iter()
                        .map(|&l| sweep.solve(l).unwrap())
                        .collect::<Vec<_>>()
                });
                measure("sweep_reuse", name, threads, || {
                    let mut sweep = ReusedCholesky::new(&ata, &atb);
                    let answers: Vec<_> = lambdas.iter().map(|&l| sweep.solve(l)).collect();
                    eprintln!(
                        "reuse: factors={} iterations={}",
                        sweep.factorizations, sweep.iterations
                    );
                    answers
                });
                return;
            }
            let samples = std::cell::RefCell::new(Vec::new());
            measure("mimo_profile_input", name, threads, || {
                let stages = std::cell::RefCell::new(std::collections::BTreeMap::new());
                let start = Instant::now();
                let result = perf::mimo_design_input(
                    &xs,
                    &paths,
                    &direct,
                    &roles,
                    &req,
                    &ctx,
                    &|stage, elapsed| {
                        stages
                            .borrow_mut()
                            .insert(stage.to_owned(), elapsed.as_secs_f64());
                    },
                )
                .unwrap();
                let total = start.elapsed().as_secs_f64();
                let mut stages = stages.into_inner();
                stages.insert("other".into(), total - stages.values().sum::<f64>());
                samples.borrow_mut().push(stages);
                result
            });
            println!(
                "{}",
                serde_json::json!({"profile_samples": &samples.borrow()[1..]})
            );
            println!("{}", serde_json::json!({"roles":roles,"dimension":n}));
        }
        "mimo_design_one" | "mimo_design_all" | "mimo_iir_one" | "mimo_iir_all" | "mimo_iir"
        | "mimo_fir" => {
            let design = run();
            let req = MimoRequest::default();
            let profiles = std::sync::Mutex::new(Vec::new());
            let first_role = design.main_roles().next().unwrap().to_owned();
            if op.starts_with("mimo_design_") {
                let input = (op == "mimo_design_one").then_some(first_role.as_str());
                measure(op, name, threads, || {
                    let stages =
                        std::sync::Mutex::new(std::collections::BTreeMap::<String, f64>::new());
                    let result = perf::mimo_design_inputs_observed(
                        &design,
                        &case.measurements,
                        &case.config,
                        &req,
                        &ctx,
                        input,
                        &|stage, elapsed| {
                            *stages.lock().unwrap().entry(stage.to_owned()).or_default() +=
                                elapsed.as_secs_f64();
                        },
                    )
                    .unwrap();
                    profiles.lock().unwrap().push(stages.into_inner().unwrap());
                    result
                });
            } else if matches!(op, "mimo_iir_one" | "mimo_iir_all") {
                let mut ideal =
                    design_mimo(&design, &case.measurements, &case.config, &req, &ctx).unwrap();
                if op == "mimo_iir_one" {
                    ideal.supports.retain(|(role, _)| *role == first_role);
                    assert!(
                        !ideal.supports[0].1.is_empty(),
                        "first input has no ideal supports"
                    );
                }
                measure(op, name, threads, || {
                    design_mimo_iir(
                        &design,
                        &case.measurements,
                        &case.config,
                        &ideal,
                        &req,
                        &case.tuning,
                        &ctx,
                    )
                    .unwrap()
                });
            } else {
                let mut settings: secs_types::calibration::CalibrationSettings =
                    serde_json::from_value(case.settings.clone()).unwrap();
                settings.mimo_on = true;
                settings.mimo_fir = op == "mimo_fir";
                measure(op, name, threads, || {
                    let stages =
                        std::sync::Mutex::new(std::collections::BTreeMap::<String, f64>::new());
                    let result = perf::mimo_compute_observed(
                        &design,
                        &case.measurements,
                        &case.config,
                        &settings,
                        &case.tuning,
                        &ctx,
                        &|stage, elapsed| {
                            *stages.lock().unwrap().entry(stage.to_owned()).or_default() +=
                                elapsed.as_secs_f64();
                        },
                    )
                    .unwrap();
                    profiles.lock().unwrap().push(stages.into_inner().unwrap());
                    result
                });
            }
            let profiles = profiles.into_inner().unwrap();
            if profiles.len() > 1 {
                println!("{}", serde_json::json!({"profile_samples": &profiles[1..]}));
            }
        }
        "design" => measure(op, name, threads, run),
        "stages" => {
            use std::{collections::BTreeMap, sync::Mutex};
            let collected = Mutex::new(BTreeMap::<String, Vec<f64>>::new());
            for iteration in 0..6 {
                let started = Instant::now();
                let result = perf::design_observed(
                    &request,
                    &ctx,
                    &*perf::production_fitters(&ctx),
                    &|stage, elapsed| {
                        if iteration > 0 {
                            collected
                                .lock()
                                .unwrap()
                                .entry(stage.into())
                                .or_default()
                                .push(elapsed.as_secs_f64());
                        }
                    },
                )
                .unwrap();
                if iteration > 0 {
                    collected
                        .lock()
                        .unwrap()
                        .entry("design".into())
                        .or_default()
                        .push(started.elapsed().as_secs_f64());
                }
                drop(result);
            }
            for (stage, samples) in collected.into_inner().unwrap() {
                let mut sorted = samples.clone();
                sorted.sort_by(f64::total_cmp);
                println!(
                    "{}",
                    serde_json::json!({"implementation":"rust", "op":stage, "case":name, "threads":threads, "samples_s":samples, "median_s":sorted[sorted.len() / 2]})
                );
            }
        }
        "fit_rc1" | "fit_rc2" | "fit_bus" | "fit_xo" | "profile_fit" | "trace_fit" => {
            let kind = match op {
                "fit_bus" => "bus",
                "fit_xo" => "xo",
                "profile_fit" | "trace_fit" => args.get(3).map(String::as_str).unwrap_or("bus"),
                _ => "rc",
            };
            let calls = if name == "dolby_ultra" {
                Vec::new()
            } else {
                cases::Fixture::all_calls("engine_fit", &format!("fit_{name}_{kind}_"))
            };
            let index = usize::from(op == "fit_rc2");
            let (fx, i) = if let Some(fx) = calls.get(index) {
                (fx.clone(), fx.inputs().clone())
            } else {
                let stage = match kind {
                    "bus" => "Ba",
                    "xo" => "Top:L",
                    _ => "Rc:L",
                };
                let selected = cases::Fixture::all_calls("engine_design", &format!("{name}_fits_"))
                    .into_iter()
                    .filter(|c| c.inputs()["stage"] == stage)
                    .nth(index)
                    .expect("recorded design fit");
                if let Some(r) = selected.inputs().get("reference") {
                    let source = cases::Fixture::load(
                        r["family"].as_str().unwrap(),
                        r["fixture"].as_str().unwrap(),
                    );
                    let input = if let Some(index) = r["call"].as_u64() {
                        source.calls()[index as usize].inputs().clone()
                    } else {
                        let candidate = r["candidate"].as_u64().unwrap() as usize;
                        let fit = r["fit"].as_u64().unwrap() as usize;
                        source.outputs()["candidates"][candidate]["fits"][fit]["inputs"].clone()
                    };
                    (source, input)
                } else {
                    let input = selected.inputs().clone();
                    (selected, input)
                }
            };
            let g = fx.f64s(&i["g"]);
            let corr = fx.f64s(&i["corr_db"]);
            let optional = |v: &serde_json::Value| (!v.is_null()).then(|| cases::number(v));
            let request = perf::FitRequest {
                g: &g,
                corr_db: &corr,
                n: i["n"].as_u64().unwrap() as usize,
                lo: cases::number(&i["lo"]),
                hi: cases::number(&i["hi"]),
                boost: cases::number(&i["boost"]),
                ring_ms: optional(&i["ring_ms"]),
                f_max: optional(&i["f_max"]),
                bands: None,
            };
            if op == "profile_fit" {
                let seed = perf::fit_seed(&request);
                for _ in 0..6 {
                    let mut cache = perf::FitResidualCache::new(&g, seed.bands.len());
                    let residual_time = std::cell::Cell::new(std::time::Duration::ZERO);
                    let calls = std::cell::Cell::new(0_u64);
                    let started = Instant::now();
                    let solution = secs_dsp::optimize::least_squares(
                        |x| {
                            let t = Instant::now();
                            let out = cache.evaluate(&request, x);
                            residual_time.set(residual_time.get() + t.elapsed());
                            calls.set(calls.get() + 1);
                            out
                        },
                        &seed.x0,
                        &seed.lb,
                        &seed.ub,
                        if request.ring_ms.is_some() { 150 } else { 400 },
                    )
                    .unwrap();
                    println!(
                        "{}",
                        serde_json::json!({"op":"profile_fit", "case":name, "total_s":started.elapsed().as_secs_f64(), "residual_s":residual_time.get().as_secs_f64(), "calls":calls.get(), "nfev":solution.nfev, "njev":solution.njev})
                    );
                }
            } else if op == "trace_fit" {
                let seed = perf::fit_seed(&request);
                let mut cache = perf::FitResidualCache::new(&g, seed.bands.len());
                let mut trace = secs_dsp::optimize::LeastSquaresTrace::default();
                let sol = secs_dsp::optimize::least_squares_with_trace(
                    |x| cache.evaluate(&request, x),
                    &seed.x0,
                    &seed.lb,
                    &seed.ub,
                    if request.ring_ms.is_some() { 150 } else { 400 },
                    Some(&mut trace),
                )
                .unwrap();
                println!(
                    "{}",
                    serde_json::json!({"nfev":sol.nfev, "njev":sol.njev,
                    "x0":seed.x0, "initial_cost":trace.initial_cost,
                    "main_x":trace.main_x,"main_cost":trace.main_cost,
                    "trials":trace.trials})
                );
            } else {
                let bytes = std::fs::read(
                    std::env::var("SECS_PERF_FIT_INPUTS").expect("nine shared f64 input arrays"),
                )
                .unwrap();
                assert_eq!(bytes.len(), 9 * corr.len() * 8);
                let inputs: Vec<_> = bytes
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .map(|b| f64::from_le_bytes(*b))
                    .collect();
                assert_eq!(
                    &bytes[..corr.len() * 8],
                    corr.iter()
                        .flat_map(|x| x.to_le_bytes())
                        .collect::<Vec<_>>()
                );
                let mut rows = Vec::new();
                let mut medians = Vec::new();
                for (index, corr) in inputs.chunks_exact(corr.len()).enumerate() {
                    let request = perf::FitRequest {
                        corr_db: corr,
                        ..request
                    };
                    // The production fit body, including seed and result conversion.
                    // No timers or counters inside the residual hot loop.
                    let call = || {
                        let seed = perf::fit_seed(&request);
                        if seed.bands.is_empty() {
                            return (Vec::new(), 0, 0);
                        }
                        let mut cache = perf::FitResidualCache::new(&g, seed.bands.len());
                        let sol = secs_dsp::optimize::least_squares(
                            |x| cache.evaluate(&request, x),
                            &seed.x0,
                            &seed.lb,
                            &seed.ub,
                            if request.ring_ms.is_some() { 150 } else { 400 },
                        )
                        .unwrap();
                        (perf::fit_result_bands(&request, &sol.x), sol.nfev, sol.njev)
                    };
                    black_box(call());
                    let mut samples = Vec::new();
                    let mut counts = Vec::new();
                    for _ in 0..5 {
                        let started = Instant::now();
                        let result = black_box(call());
                        samples.push(started.elapsed().as_secs_f64());
                        counts.push((result.1, result.2));
                    }
                    assert!(counts.iter().all(|c| *c == counts[0]));
                    let mut sorted = samples.clone();
                    sorted.sort_by(f64::total_cmp);
                    medians.push(sorted[2]);
                    rows.push(serde_json::json!({"input":index,"samples_s":samples,
                        "median_s":sorted[2],"nfev":counts[0].0,"njev":counts[0].1}));
                }
                medians.sort_by(f64::total_cmp);
                println!(
                    "{}",
                    serde_json::json!({"implementation":"rust","op":op,
                    "case":name,"threads":threads,"inputs":rows,"median_s":medians[4]})
                );
            }
        }
        "gd" => {
            let bytes = std::fs::read(
                std::env::var("SECS_PERF_GD_INPUT").expect("recorded complex128 spectrum"),
            )
            .unwrap();
            let spectrum: Vec<_> = bytes
                .as_chunks::<16>()
                .0
                .iter()
                .map(|b| {
                    C::new(
                        f64::from_le_bytes(b[..8].try_into().unwrap()),
                        f64::from_le_bytes(b[8..].try_into().unwrap()),
                    )
                })
                .collect();
            assert_eq!(bytes.len() % 16, 0);
            assert_eq!(spectrum.len(), perf::design_freqs().len());
            measure(op, name, threads, || {
                perf::fit_gd_cuts(perf::design_freqs(), &spectrum, 20.0, 500.0, 6, 3.0)
            });
        }
        "prepare" => measure(op, name, threads, || {
            perf::prepare(
                &case.measurements,
                &case.config,
                &perf::Setup::new(&case.config),
            )
            .unwrap()
        }),
        "recommend" => {
            let settings = serde_json::from_value(case.settings.clone()).unwrap();
            let view = ProjectView {
                groups: &case.config.groups,
                project_xo: &case.config.xo,
                target: &case.config.target,
                inverted: &[],
            };
            measure(op, name, threads, || {
                xo::recommend_xo(
                    &view,
                    &case.measurements,
                    &settings,
                    &case.tuning,
                    &xo::XO_CANDIDATES,
                    &ctx,
                )
                .unwrap()
            });
        }
        "profile_scores" => {
            let result = run();
            let f = perf::design_freqs();
            let role = result.main_roles().next().unwrap();
            let h = perf::main_after(&result, role).unwrap();
            let target = case
                .config
                .target
                .sample(&f.iter().map(|f| f.max(1.0)).collect::<Vec<_>>());
            measure("score_metrics", name, threads, || {
                perf::score_metrics(h, f, &target, (100.0, 400.0))
            });
            measure("score_ifft", name, threads, || {
                secs_dsp::fft::irfft(h, perf::N_FFT)
            });
            measure("score_gd", name, threads, || {
                secs_dsp::response::gd_curve(
                    h,
                    f,
                    &[20.0, 40.0, 80.0, 160.0, 320.0, 640.0],
                    secs_dsp::smooth::Fraction::Scalar(24.0),
                )
                .unwrap()
            });
        }
        "profile_sub" => {
            let result = run();
            let f = perf::design_freqs();
            let (h, row) = perf::first_sub_profile(&result).unwrap();
            measure("sub_term", name, threads, || perf::term(h, f, row));
            let power: Vec<_> = h
                .iter()
                .map(|h| {
                    let a = h.re.hypot(h.im);
                    a * a
                })
                .collect();
            measure("sub_smooth", name, threads, || {
                secs_dsp::smooth::smooth_power(f, &power, secs_dsp::smooth::Fraction::Scalar(12.0))
                    .unwrap()
            });
            let smoothed =
                secs_dsp::smooth::smooth_power(f, &power, secs_dsp::smooth::Fraction::Scalar(12.0))
                    .unwrap();
            measure("sub_log", name, threads, || {
                smoothed
                    .iter()
                    .map(|p| 10.0 * (p + 1e-30).log10())
                    .collect::<Vec<_>>()
            });
        }
        "preview" | "cache" | "sub_combine" => {
            let result = run();
            let cache = preview::XoPreview::new(&result);
            match op {
                "cache" => measure(op, name, threads, || preview::XoPreview::new(&result)),
                "preview" => measure(op, name, threads, || {
                    cache.curves(&result, "front", 80.0, true, None)
                }),
                _ => measure(op, name, threads, || preview::sub_combine(&result)),
            }
        }
        _ => panic!("unknown operation: {op}"),
    }
}
