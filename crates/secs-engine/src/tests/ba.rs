//! P10: oracle-population replay separates objective roundoff from DE decisions.
use super::*;
use crate::fit::replay_fitter;
use crate::tests::{cases, fit::read_recorded_fit};
use secs_oracle::{Fixture, assert_close, integer, number, oracle_libm};
use serde_json::Value;

fn rows(v: &Value) -> Vec<BaRow> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|r| {
            row(
                number(&r[0]),
                number(&r[1]),
                r[2].as_i64().unwrap() as i8,
                r[3].as_array()
                    .unwrap()
                    .iter()
                    .map(|a| (number(&a[0]), number(&a[1])))
                    .collect(),
            )
        })
        .collect()
}
fn flat(rows: &[BaRow]) -> Vec<f64> {
    rows.iter()
        .flat_map(|r| {
            [r.gain_db, r.delay_ms, f64::from(r.polarity)]
                .into_iter()
                .chain(r.allpass.iter().flat_map(|&(f, q)| [f, q]))
        })
        .collect()
}
fn empty(input: &BaInput<'_>) -> SubAlignment {
    SubAlignment {
        rows: vec![],
        sub_filters: input.setup.sub_filters.clone(),
        h_subs: vec![],
        layout_rel: vec![],
        layout_info: vec![],
        sub_level_db: None,
        bus_gain: 1.0,
        bus_peq: vec![],
        bus_rolloff_hz: None,
        sub_sum_by_pos: vec![],
        plan: manual_plan(input),
        notes: vec![],
    }
}
fn compare(name: &str, a: &[f64], b: &[f64], tol: f64) {
    let tol = if oracle_libm() { tol } else { tol.max(1e-9) };
    assert_close(name, a, b, tol, tol);
}

#[test]
fn ba_oracle_auto_manual_bus() {
    for fx in Fixture::all("engine_ba", "") {
        let o = fx.outputs();
        let mut case = cases::load(fx.inputs()["case"].as_str().unwrap());
        let knobs = &fx.inputs()["knobs"];
        if knobs["sub_allpass"] == "full" {
            case.tuning.sub_allpass = SubAllpass::Full;
        }
        if knobs["bundle_gain"] == "fixed" {
            case.tuning.bundle_gain = BundleGain::Fixed;
        }
        case.tuning.sub_bus_fix = knobs["sub_bus_fix"].as_bool().unwrap_or(false);
        let setup = Setup::new(&case.config);
        let prep = crate::measurement::prepare(&case.measurements, &case.config, &setup).unwrap();
        let xo: Vec<_> = cases::pairs(&o["xo_of"])
            .map(|(r, x)| (r.to_owned(), number(x)))
            .collect();
        let cancel = CancelToken::new();
        let input = BaInput {
            measurements: &case.measurements,
            prepared: &prep,
            setup: &setup,
            config: &case.config,
            tuning: &case.tuning,
            xo_of: &xo,
            ref_level: number(&o["ref_level"]),
            cancel: &cancel,
        };
        let hi = xo.iter().map(|(_, x)| *x).reduce(f64::max).unwrap_or(100.0) * 1.5;
        let calls = o["fits"].as_array().unwrap();
        let bundle: Vec<_> = calls
            .iter()
            .filter(|c| c["stage"] == "bundle")
            .map(|c| read_recorded_fit(&fx, &c["inputs"], &c["result"]))
            .collect();
        let mut fitter = replay_fitter(bundle, 1e-9);
        let mut h = prep.h.clone();
        let mut out = empty(&input);
        layout_stage(&input, &mut h, &mut out, hi, &mut fitter).unwrap();
        for ((name, info), expected) in out.layout_info.iter().zip(o["layouts"].as_array().unwrap())
        {
            assert_eq!(name, expected["name"].as_str().unwrap());
            let e = &expected["info"];
            assert_eq!(
                info.members,
                e["members"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| s.as_str().unwrap().to_owned())
                    .collect::<Vec<_>>()
            );
            assert_eq!(info.auto_ms, e["auto_ms"].as_f64());
            if let Some(gap) = info.min_ms {
                compare(
                    &format!("{} DBA gap", fx.id),
                    &[gap],
                    &[number(&e["min_ms"])],
                    1e-12,
                );
                if fx.inputs()["case"] == "room_dba" {
                    assert_eq!((gap * 4.0).ceil() / 4.0, 2.75);
                    let arrival = |rs: &[String]| {
                        mean(
                            &rs.iter()
                                .map(|r| {
                                    let m = case
                                        .measurements
                                        .get(case.config.main_position, r)
                                        .unwrap();
                                    1000.0 * (m.t0 + onset_index(&m.samples) as f64 / SR_F)
                                })
                                .collect::<Vec<_>>(),
                        )
                    };
                    let reference_gap = (arrival(&info.rear) - arrival(&info.front)).abs();
                    assert_eq!((reference_gap * 4.0).ceil() / 4.0, 1.50);
                }
                let candidates = fx.f64s(&expected["candidate_values"]);
                compare(
                    &format!("{} DBA candidates", fx.id),
                    &info
                        .candidates
                        .iter()
                        .flat_map(|&(d, c)| [d, c])
                        .collect::<Vec<_>>(),
                    &candidates,
                    1e-12,
                );
                let index = integer(&expected["winner_index"]) as usize;
                assert_eq!(info.auto_ms.unwrap(), candidates[2 * index]);
            }
            for (r, g) in &info.gains {
                compare(
                    &format!("{} gain {r}", fx.id),
                    &[*g],
                    &[number(&e["gains"][r])],
                    1e-10,
                );
            }
        }
        for ((name, rel), (expected_name, expected)) in
            out.layout_rel.iter().zip(cases::pairs(&o["layout_rel"]))
        {
            assert_eq!(name, expected_name);
            let expected_rows: Vec<_> = rel.iter().map(|(r, _)| expected[r].clone()).collect();
            compare(
                "layout relative rows",
                &flat(&rel.iter().map(|(_, r)| r.clone()).collect::<Vec<_>>()),
                &flat(&rows(&serde_json::json!(expected_rows))),
                1e-12,
            );
        }
        for (r, f) in &out.sub_filters {
            let e = cases::pairs(&o["sub_filters"])
                .find(|(s, _)| s == r)
                .unwrap()
                .1;
            assert_eq!(f.peq, cases::peq(&e["peq"]));
            assert_eq!((f.hp_hz, f.lp_hz), (number(&e["hp"]), number(&e["lp"])));
        }
        let plan = &o["plan"];
        if let Some(p) = &out.plan {
            assert_eq!(p.gain_anchored, plan["gain_anchored"].as_bool().unwrap());
            assert_eq!(serde_json::json!(p.fixed), plan["fixed"]);
            assert_eq!(serde_json::json!(p.auto), plan["auto"]);
            for (b, rs) in &p.bundles {
                assert_eq!(serde_json::json!(rs), plan["bundles"][b]);
            }
        } else {
            assert!(plan.is_null());
        }
        let labels: Vec<_> = case
            .config
            .subs
            .iter()
            .map(|r| sub_mode_label(r, &setup, &out.layout_info))
            .collect();
        assert_eq!(serde_json::json!(labels), o["labels"]);
        let search = Search::new(&input, &out);
        let s = BaSetup::new(&input, &h, hi);
        for expected in o["setups"].as_array().unwrap() {
            assert_eq!(
                s.grid,
                fx.f64s(&expected["grid"])
                    .iter()
                    .map(|v| *v as usize)
                    .collect::<Vec<_>>()
            );
            assert_eq!(serde_json::json!(s.positions), expected["positions"]);
            assert_eq!(s.late.lin, fx.f64s(&expected["lin"]).len());
            let late: Vec<_> = expected["raw_late"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| number(&r[1]))
                .collect();
            compare(&format!("{} raw_late", fx.id), &s.raw_late, &late, 1e-12);
        }
        let indiv = search.fixed_indiv.then(|| s.indiv(None));
        for (pass, e) in o["searches"].as_array().unwrap().iter().enumerate() {
            let bounds: Vec<_> = e["bounds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| (number(&b[0]), number(&b[1])))
                .collect();
            assert_eq!(search.bounds, bounds);
            let bus = (!e["bus_peq"].is_null())
                .then(|| response::peq_response(design_freqs(), &cases::peq(&e["bus_peq"])));
            let options = DifferentialEvolutionOptions {
                maxiter: integer(&e["options"]["maxiter"]) as usize,
                popsize: integer(&e["options"]["popsize"]) as usize,
                tol: number(&e["options"]["tol"]),
                seed: integer(&e["options"]["seed"]) as u32,
            };
            let x0 = (!e["x0"].is_null()).then(|| fx.f64s(&e["x0"]));
            let oracle_cost = fx.f64s(&e["costs"]);
            let keep: Vec<_> = fx.f64s(&e["keep"]).iter().map(|v| *v as usize).collect();
            let vectors = fx.f64s(&e["vectors"]);
            let mut actual = vec![];
            let mut all_x = vec![];
            let mut n = 0;
            let mut max = 0.0_f64;
            let replay = differential_evolution(
                |x| {
                    all_x.extend_from_slice(x);
                    let c = s.cost(
                        &input,
                        &search.unpack(&input, x),
                        indiv.as_deref(),
                        bus.as_deref(),
                    );
                    max = max.max((c - oracle_cost[n]).abs() / oracle_cost[n].abs().max(1.0));
                    actual.push(c);
                    let v = oracle_cost[n];
                    n += 1;
                    v
                },
                &bounds,
                x0.as_deref(),
                options,
            )
            .unwrap();
            assert_eq!(n, oracle_cost.len());
            if oracle_libm() {
                fx.assert_digest("BA oracle-path evaluations", &e["evaluations"], &all_x);
            }
            for (k, &index) in keep.iter().enumerate() {
                assert_close(
                    "kept trial",
                    &all_x[index * bounds.len()..(index + 1) * bounds.len()],
                    &vectors[k * bounds.len()..(k + 1) * bounds.len()],
                    1e-12,
                    1e-12,
                );
            }
            // Measured maximum 5.063437532380502e-14; 1.5x, floor 1e-13.
            compare(
                &format!("{} pass{pass} costs", fx.id),
                &actual,
                &oracle_cost,
                1e-13,
            );
            assert_eq!(replay.x, fx.f64s(&e["x"]));
            // Replay immediate selection, including argmin's first-tie promotion.
            let pop = (bounds.len() * options.popsize).max(5);
            let mut a = actual[..pop].to_vec();
            let mut b = oracle_cost[..pop].to_vec();
            let promote = |a: &mut Vec<f64>, b: &mut Vec<f64>| {
                let j = (0..b.len()).min_by(|&i, &j| b[i].total_cmp(&b[j])).unwrap();
                a.swap(0, j);
                b.swap(0, j);
            };
            promote(&mut a, &mut b);
            let mut near = 0;
            for k in pop..n {
                let j = (k - pop) % pop;
                let budget = 2e-13 * b[j].abs().max(oracle_cost[k].abs()).max(1.0);
                if (oracle_cost[k] - b[j]).abs() <= budget {
                    near += 1;
                } else {
                    assert_eq!(
                        actual[k] <= a[j],
                        oracle_cost[k] <= b[j],
                        "{} pass{pass} selection {k}",
                        fx.id
                    );
                }
                if oracle_cost[k] <= b[j] {
                    if j != 0
                        && (oracle_cost[k] - b[0]).abs()
                            > 2e-13 * b[0].abs().max(oracle_cost[k].abs()).max(1.0)
                    {
                        assert_eq!(
                            actual[k] <= a[0],
                            oracle_cost[k] <= b[0],
                            "best promotion {k}"
                        );
                    }
                    a[j] = actual[k];
                    b[j] = oracle_cost[k];
                    if oracle_cost[k] <= b[0] {
                        promote(&mut a, &mut b);
                    }
                }
            }
            let mut divergence = None;
            let mut eval = 0;
            let real = differential_evolution(
                |x| {
                    if divergence.is_none()
                        && all_x.get(eval * bounds.len()..(eval + 1) * bounds.len()) != Some(x)
                    {
                        divergence = Some(eval);
                    }
                    eval += 1;
                    s.cost(
                        &input,
                        &search.unpack(&input, x),
                        indiv.as_deref(),
                        bus.as_deref(),
                    )
                },
                &bounds,
                x0.as_deref(),
                options,
            )
            .unwrap();
            let got = flat(&search.unpack(&input, &real.x));
            let expected = flat(&search.unpack(&input, &replay.x));
            let delta = got
                .iter()
                .zip(&expected)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0, f64::max);
            println!(
                "MEASURE {} pass={pass} ba_cost max_rel={max:e} near_ties={near} first_divergence={divergence:?} final_rows_max_abs={delta:e}",
                fx.id
            );
            if divergence.is_none() {
                assert_eq!(real.x, replay.x);
                compare("matched DE rows", &got, &expected, 1e-12);
            } else {
                println!(
                    "PLATFORM {} pass{pass} global DE report-only: objective SVD/FFT roundoff",
                    fx.id
                );
            }
        }
        if search.skip {
            compare(
                "skipped search rows",
                &flat(&search.unpack(&input, &vec![0.0; search.bounds.len()])),
                &flat(&rows(&o["aligned_rows"])),
                1e-12,
            );
        }
        out.rows = rows(&o["aligned_rows"]);
        let iterations = two_way(&input, &h, &mut out).unwrap();
        assert_eq!(iterations, o["two_way"].as_array().unwrap().len());
        compare(
            &format!("{} two-way rows", fx.id),
            &flat(&out.rows),
            &flat(&rows(&o["rows"])),
            1e-12,
        );
        out.sub_sum_by_pos = sums(
            &h,
            &case.config.subs,
            &out.rows,
            &(0..BINS).collect::<Vec<_>>(),
            &complete(&h, &case.config.subs),
        );
        let indices: Vec<_> = fx
            .f64s(&fx.inputs()["samples"])
            .iter()
            .map(|v| *v as usize)
            .collect();
        for expected in o["raw_sums"].as_array().unwrap() {
            let p = integer(&expected[0]) as u32;
            let sum = &out
                .sub_sum_by_pos
                .iter()
                .find(|(pos, _)| *pos == p)
                .unwrap()
                .1;
            let got = gather(sum, &indices);
            let re = fx.f64s(&expected[1]["re"]);
            let im = fx.f64s(&expected[1]["im"]);
            compare(
                "sub sum real",
                &got.iter().map(|v| v.re).collect::<Vec<_>>(),
                &re,
                1e-12,
            );
            compare(
                "sub sum imag",
                &got.iter().map(|v| v.im).collect::<Vec<_>>(),
                &im,
                1e-12,
            );
        }
        let lvl = level(&input, &out.sub_sum_by_pos);
        compare(
            "sub level",
            &[lvl - input.ref_level],
            &[number(&o["sub_level_db"])],
            1e-12,
        );
        out.bus_gain = if out.plan.as_ref().is_some_and(|p| p.gain_anchored) {
            1.0
        } else {
            10f64.powf((input.ref_level - lvl) / 20.0)
        };
        compare("bus gain", &[out.bus_gain], &[number(&o["bus_gain"])], 1e-9);
        for (_, h) in &mut out.sub_sum_by_pos {
            *h = h.iter().map(|v| *v * C::new(out.bus_gain, 0.0)).collect();
        }
        let last = calls.iter().rfind(|c| c["stage"] == "bus").unwrap();
        let mut fitter = replay_fitter(
            vec![read_recorded_fit(&fx, &last["inputs"], &last["result"])],
            1e-12,
        );
        out.bus_peq = bus_fit(&input, &out.sub_sum_by_pos, &mut fitter).0;
        let p = response::peq_response(design_freqs(), &out.bus_peq);
        for (_, h) in &mut out.sub_sum_by_pos {
            *h = h.iter().zip(&p).map(|(v, p)| v * p).collect();
        }
        let mut delay = (0.0, 1);
        if !o["bus_delay"].is_null() {
            let b = &o["bus_delay"];
            let mains: Vec<_> = case
                .config
                .mains
                .iter()
                .map(|r| {
                    let peq = cases::pairs(&b["top_peq"])
                        .find(|(s, _)| s == r)
                        .map(|(_, p)| cases::peq(p))
                        .unwrap_or_default();
                    let response = response::peq_response(design_freqs(), &peq);
                    (
                        r.clone(),
                        prep.h
                            .positions()
                            .iter()
                            .filter_map(|&pos| {
                                prep.h.get(pos, r).map(|h| {
                                    (
                                        pos,
                                        h.iter()
                                            .zip(&response)
                                            .map(|(v, p)| v * p)
                                            .collect::<Arc<[C]>>(),
                                    )
                                })
                            })
                            .collect(),
                    )
                })
                .collect();
            let mut candidates = vec![];
            delay = bus_delay_observe(
                &out,
                &mains,
                &prep.align_delay_ms,
                &xo,
                number(&b["lo"]),
                &|| false,
                &mut |p, d, c| candidates.extend([f64::from(p), d, c]),
            )
            .unwrap();
            compare(
                &format!("{} bus delay costs", fx.id),
                &candidates,
                &fx.f64s(&b["candidate_values"]),
                1e-10,
            );
            assert_eq!(
                delay,
                (number(&o["bus_d"]), o["bus_pol"].as_i64().unwrap() as i8)
            );
        }
        compare(
            "folded rows",
            &flat(&fold(&out.rows, out.bus_gain, delay.0, delay.1)),
            &flat(&rows(&o["folded_rows"])),
            1e-12,
        );
        // Run the actual orchestrator, cutting only optimizer decisions, not
        // stage arithmetic. This includes all bus-aware second-pass fits.
        let fits = calls
            .iter()
            .map(|c| read_recorded_fit(&fx, &c["inputs"], &c["result"]))
            .collect();
        let mut replay = replay_fitter(fits, 1e-9);
        let mut fit_count = 0;
        let mut fitter = |req: &crate::fit::FitRequest<'_>| {
            fit_count += 1;
            replay(req)
        };
        let mut pass = 0;
        let mut solver = |cost: &mut dyn FnMut(&[f64]) -> f64,
                          bounds: &[(f64, f64)],
                          x0: Option<&[f64]>,
                          options: DifferentialEvolutionOptions| {
            let e = &o["searches"][pass];
            pass += 1;
            assert_eq!(bounds.len(), e["bounds"].as_array().unwrap().len());
            assert_eq!(options.seed as u64, integer(&e["options"]["seed"]));
            assert_eq!(
                x0.map(|x| x.to_vec()),
                (!e["x0"].is_null()).then(|| fx.f64s(&e["x0"]))
            );
            let x = fx.f64s(&e["x"]);
            let fun = cost(&x);
            compare("orchestrator DE cost", &[fun], &[number(&e["fun"])], 1e-13);
            Ok(secs_dsp::optimize::DifferentialEvolutionResult {
                x,
                fun,
                nit: integer(&e["nit"]) as usize,
                nfev: integer(&e["nfev"]) as usize,
                success: false,
                message: String::new(),
            })
        };
        let got = align_with(&input, &mut fitter, &mut solver).unwrap();
        assert_eq!(fit_count, calls.len());
        assert_eq!(pass, o["searches"].as_array().unwrap().len());
        compare(
            "orchestrated rows",
            &flat(&got.rows),
            &flat(&out.rows),
            1e-12,
        );
        compare("orchestrated gain", &[got.bus_gain], &[out.bus_gain], 1e-9);
        assert_eq!(got.bus_peq, out.bus_peq);
    }
}

#[test]
fn ba_bus_delay_stops_at_every_cancellation_check() {
    // TD-49: the bus-delay grid search polls the token once per main position
    // while it gathers the crossover bands and once per candidate delay.
    let case = cases::load("room");
    let setup = Setup::new(&case.config);
    let prep = crate::measurement::prepare(&case.measurements, &case.config, &setup).unwrap();
    let cancel = CancelToken::new();
    let input = BaInput {
        measurements: &case.measurements,
        prepared: &prep,
        setup: &setup,
        config: &case.config,
        tuning: &case.tuning,
        xo_of: &[],
        ref_level: 0.0,
        cancel: &cancel,
    };
    let mut alignment = empty(&input);
    let rows: Vec<_> = case
        .config
        .subs
        .iter()
        .map(|_| row(0.0, 0.0, 1, vec![]))
        .collect();
    alignment.sub_sum_by_pos = sums(
        &prep.h,
        &case.config.subs,
        &rows,
        &(0..BINS).collect::<Vec<_>>(),
        &complete(&prep.h, &case.config.subs),
    );
    let mains: Vec<_> = case
        .config
        .mains
        .iter()
        .map(|r| {
            let cells: Vec<_> = prep
                .h
                .positions()
                .iter()
                .filter_map(|&p| prep.h.get(p, r).map(|h| (p, Arc::clone(h))))
                .collect();
            (r.clone(), cells)
        })
        .collect();
    let xo: Vec<_> = case
        .config
        .mains
        .iter()
        .map(|r| (r.clone(), 80.0))
        .collect();
    let plain = bus_delay(&alignment, &mains, &prep.align_delay_ms, &xo, -5.0, &|| {
        false
    })
    .unwrap();
    let (delay, polls) = crate::tests::cancel::assert_cancels("bus_delay", 1, |cancelled| {
        bus_delay(
            &alignment,
            &mains,
            &prep.align_delay_ms,
            &xo,
            -5.0,
            cancelled,
        )
    });
    assert_eq!(delay, plain);
    // Twelve main cells, then two polarities of the 81 delays -5..=15 ms.
    assert_eq!(polls, 12 + 2 * 81);
}

#[test]
fn ba_cancelled_before_work() {
    let case = cases::load("room");
    let setup = Setup::new(&case.config);
    let prep = crate::measurement::prepare(&case.measurements, &case.config, &setup).unwrap();
    let cancel = CancelToken::new();
    cancel.cancel();
    let input = BaInput {
        measurements: &case.measurements,
        prepared: &prep,
        setup: &setup,
        config: &case.config,
        tuning: &case.tuning,
        xo_of: &[],
        ref_level: 0.0,
        cancel: &cancel,
    };
    assert!(matches!(
        align(&input, &mut |_| panic!("cancelled fit")),
        Err(EngineError::Cancelled)
    ));
}

#[test]
fn ba_single_sub_and_inflight_cancellation() {
    let mut case = cases::load("room");
    case.config.subs.truncate(1);
    case.tuning.sub_allpass = SubAllpass::Full;
    case.tuning.xo_stage = false;
    let setup = Setup::new(&case.config);
    let prep = crate::measurement::prepare(&case.measurements, &case.config, &setup).unwrap();
    let cancel = CancelToken::new();
    let input = BaInput {
        measurements: &case.measurements,
        prepared: &prep,
        setup: &setup,
        config: &case.config,
        tuning: &case.tuning,
        xo_of: &[],
        ref_level: 0.0,
        cancel: &cancel,
    };
    let result = align(&input, &mut |_| panic!("disabled fitter")).unwrap();
    assert!(result.rows[0].allpass.is_empty());
    assert_eq!(result.rows[0].delay_ms, base(&input, &case.config.subs[0]));
    let s = BaSetup::new(&input, &prep.h, 150.0);
    let mut evaluations = 0;
    let start = std::time::Instant::now();
    differential_evolution(
        |_| {
            evaluations += 1;
            if evaluations == 20 {
                cancel.cancel();
            }
            s.cost(&input, &result.rows, None, None)
        },
        &[(-1.0, 1.0); 14],
        None,
        DifferentialEvolutionOptions::default(),
    )
    .unwrap();
    assert!(start.elapsed().as_secs_f64() < 1.0);
    assert!(matches!(check(&input), Err(EngineError::Cancelled)));
    assert_eq!(s.cost(&input, &result.rows, None, None), 1e30);
}
