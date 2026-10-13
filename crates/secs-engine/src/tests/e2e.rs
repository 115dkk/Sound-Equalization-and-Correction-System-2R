use super::{cases, design::metrics};
use crate::design::{DesignRequest, design, production_fitters};
use secs_compute::Context;
use secs_oracle::{Fixture, number};
use std::{collections::BTreeMap, time::Instant};

#[test]
fn engine_e2e_spread() {
    let mut failures = Vec::new();
    for name in ["dolby", "room", "room_dba"] {
        let c = cases::load(name);
        let started = Instant::now();
        let context = Context::serial();
        let longest_fit = std::sync::Mutex::new(std::time::Duration::ZERO);
        let production = production_fitters(context.cancel());
        let timed: Box<crate::design::FitterFactory<'_>> = Box::new(|stage| {
            let mut fitter = production(stage);
            let longest = &longest_fit;
            Box::new(move |request| {
                let start = Instant::now();
                let out = fitter(request);
                let mut maximum = longest.lock().unwrap();
                *maximum = (*maximum).max(start.elapsed());
                out
            })
        });
        let d = design(
            &DesignRequest {
                measurements: &c.measurements,
                config: &c.config,
                tuning: &c.tuning,
                subs_only: false,
                adapter_notes: &[],
            },
            &context,
            &*timed,
        )
        .unwrap();
        let seconds = started.elapsed().as_secs_f64();
        if name == "dolby" {
            eprintln!(
                "MEASURE design dolby longest_fit={:?}",
                longest_fit.lock().unwrap()
            );
        }
        if name == "room_dba" {
            for (stage, elapsed) in &d.timings {
                eprintln!("MEASURE design room_dba stage={stage} wall={elapsed:?}");
            }
        }
        let runs = Fixture::load("engine_e2e", name).calls();
        let mut values: BTreeMap<String, (f64, Vec<f64>, i8)> = BTreeMap::new();
        let mut discrete: Vec<(String, serde_json::Value, Vec<serde_json::Value>)> = Vec::new();
        for (i, s) in d.scores.iter().enumerate() {
            for (when, m) in [("before", &s.before), ("after", &s.after)] {
                for (key, v) in metrics(m) {
                    if when == "before" && secs_oracle::oracle_libm() {
                        let expected = number(&runs[0].outputs()["roles"][i]["metrics"][when][key]);
                        let tolerance = if key == "pre_db" && expected < -250.0 {
                            1.5 * 1.4484069100264113e-5
                        } else {
                            secs_oracle::PLATFORM_RTOL
                        };
                        secs_oracle::assert_close(
                            &format!("e2e/{name}/{}/{key}/before", s.role),
                            &[v],
                            &[expected],
                            0.0,
                            tolerance,
                        );
                    }
                    // scorecard.py: percentages higher, phase/GD spans and pre-energy
                    // lower; level and the frequency of a peak are neutral descriptors.
                    let direction = if key.starts_with("in") {
                        1
                    } else if matches!(key, "level" | "gd24_abs_hz") {
                        0
                    } else {
                        -1
                    };
                    values.insert(
                        format!("{}/{when}/{key}", s.role),
                        (
                            v,
                            runs.iter()
                                .map(|r| number(&r.outputs()["roles"][i]["metrics"][when][key]))
                                .collect(),
                            direction,
                        ),
                    );
                }
            }
            let ch = &d.chain.mains[i];
            for (key, v) in [("gain_db", ch.gain_db), ("delay_ms", ch.delay_ms)] {
                values.insert(
                    format!("{}/{key}", s.role),
                    (
                        v,
                        runs.iter()
                            .map(|r| number(&r.outputs()["roles"][i][key]))
                            .collect(),
                        0,
                    ),
                );
            }
            for (key, v) in [
                ("xo_hz", serde_json::to_value(ch.xo_hz).unwrap()),
                ("peq", serde_json::json!(ch.input_peq.len())),
                (
                    "fir_taps",
                    serde_json::json!(if ch.fir.len() > 1 { ch.fir.len() } else { 0 }),
                ),
            ] {
                discrete.push((
                    format!("{}/{key}", s.role),
                    v,
                    runs.iter()
                        .map(|r| r.outputs()["roles"][i][key].clone())
                        .collect(),
                ));
            }
        }
        for (i, s) in d.chain.subs.iter().enumerate() {
            for (key, v) in [("gain_db", s.gain_db), ("delay_ms", s.delay_ms)] {
                values.insert(
                    format!("{}/{key}", s.role),
                    (
                        v,
                        runs.iter()
                            .map(|r| number(&r.outputs()["subs"][i][key]))
                            .collect(),
                        0,
                    ),
                );
            }
            for (key, v) in [
                ("polarity", serde_json::json!(s.polarity)),
                ("mode", serde_json::json!(s.mode)),
            ] {
                discrete.push((
                    format!("{}/{key}", s.role),
                    v,
                    runs.iter()
                        .map(|r| r.outputs()["subs"][i][key].clone())
                        .collect(),
                ));
            }
        }
        let sub = d.subs.as_ref().unwrap();
        for (key, v) in [
            ("bus_delay_ms", sub.bus_delay_ms),
            ("latency_ms", d.chain.latency_ms),
            ("main_extra_ms", d.chain.main_extra_ms),
            ("ref_level", d.ref_level),
            ("rc_hi", d.rc_hi),
        ] {
            values.insert(
                key.into(),
                (
                    v,
                    runs.iter().map(|r| number(&r.outputs()[key])).collect(),
                    0,
                ),
            );
        }
        discrete.push((
            "bus_polarity".into(),
            serde_json::json!(sub.bus_polarity),
            runs.iter()
                .map(|r| r.outputs()["bus_polarity"].clone())
                .collect(),
        ));
        let mut outside = 0;
        let (mut n, mut worse_all, mut varying, mut worse_median) = (0, 0, 0, 0);
        eprintln!("E2E {name} value | Rust | c0 | envelope | bound runs | pass");
        for (key, (v, all, direction)) in &values {
            let c0 = all[0];
            let (imin, min) = all
                .iter()
                .enumerate()
                .min_by(|a, b| a.1.total_cmp(b.1))
                .unwrap();
            let (imax, max) = all
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .unwrap();
            assert_eq!(all.len(), 8);
            let w = max - min;
            let floor = 1e-9 * c0.abs().max(1.0);
            let q = quantum(key, c0);
            let margin = 3.0 * w.max(q);
            let worse = match direction {
                1 => c0 - v,
                -1 => v - c0,
                _ => (v - c0).abs(),
            };
            if worse > (1.25 * w).max(floor) {
                eprintln!(
                    "E2E_125 {name} {key} Rust={v:.12e} c0={c0:.12e} spread={w:.12e} all={all:?}"
                );
            }
            // Neutral descriptors have no worse side: their two-sided cap is
            // still gated, but they cannot enter a directional rank test.
            if *direction != 0 {
                n += 1;
                worse_all += usize::from(if *direction == 1 { v < min } else { v > max });
                if w > 0.0 {
                    varying += 1;
                    let mut sorted = all.clone();
                    sorted.sort_by(f64::total_cmp);
                    let median = (sorted[3] + sorted[4]) / 2.0;
                    worse_median += usize::from(if *direction == 1 {
                        *v < median
                    } else {
                        *v > median
                    });
                }
            }
            let (lo, hi) = match direction {
                1 => (c0 - margin, f64::INFINITY),
                -1 => (f64::NEG_INFINITY, c0 + margin),
                _ => (c0 - margin, c0 + margin),
            };
            let pass = v.is_finite() && *v >= lo && *v <= hi;
            outside += usize::from(v < min || v > max);
            eprintln!(
                "E2E {name} {key} | {v:.12e} | {c0:.12e} | [{lo:.12e},{hi:.12e}] | {}..{} | {pass}",
                runs[imin].inputs()["run"],
                runs[imax].inputs()["run"]
            );
            // The eight runs sample the oracle's libm only. Another libm (macOS,
            // Windows) moves last bits everywhere at once and can flip a ladder
            // threshold or a phase candidate no eight-run sample flipped; there
            // a single value is reported and the binomial checks below, which
            // catch a Rust that is worse across the board, stay asserted.
            if !pass && secs_oracle::oracle_libm() {
                failures.push(format!(
                    "E2E FAIL {name} {key}: Rust={v:?}, all eight={all:?}"
                ));
            } else if !pass {
                eprintln!("E2E_OFFLIBM {name} {key}: Rust={v:?}, all eight={all:?}");
            }
        }
        let all_bound = binomial_quantile(n, 1.0 / 9.0);
        let median_bound = binomial_quantile(varying, 0.5);
        eprintln!(
            "E2E_SUMMARY {name} n={n} worse_all={worse_all} bound_all={all_bound} n_varying={varying} worse_median={worse_median} bound_median={median_bound} neutral={}",
            values.len() - n
        );
        if worse_all > all_bound || worse_median > median_bound {
            failures.push(format!("E2E FAIL {name} binomial counts {worse_all}/{all_bound}, {worse_median}/{median_bound}"));
        }
        for (key, v, all) in discrete {
            let pass = all.contains(&v);
            eprintln!("E2E {name} {key} | {v} | discrete | {all:?} | {pass}");
            if !pass {
                failures.push(format!(
                    "E2E FAIL {name} {key}: Rust={v}, all eight={all:?}"
                ));
            }
        }
        eprintln!(
            "E2E {name} outside={outside}/{} Rust={seconds:.3}s Python={:?}",
            values.len(),
            runs.iter()
                .map(|r| number(&r.outputs()["elapsed_s"]))
                .collect::<Vec<_>>()
        );
    }
    for failure in &failures {
        eprintln!("{failure}");
    }
    assert!(
        failures.is_empty(),
        "{} end-to-end values outside spread",
        failures.len()
    );
}

// scorecard.rs:108–115 counts design-grid bins, not the display log grid.
// turns (scorecard.rs:146–148) is continuous, not an integer count.
fn quantum(key: &str, reference: f64) -> f64 {
    let metric = key.rsplit('/').next().unwrap();
    if metric.starts_with("in") {
        let (lo, hi) = match metric.split('_').nth(1).unwrap() {
            "20" => (20.0, 200.0),
            "200" => (200.0, 2000.0),
            "2000" => (2000.0, 16000.0),
            _ => panic!("unknown percentage {metric}"),
        };
        let points = crate::grid::design_freqs()
            .iter()
            .filter(|f| (lo..=hi).contains(*f))
            .count();
        100.0 / points as f64
    } else {
        1e-9 * reference.abs().max(1.0)
    }
}

// Exact binomial CDF recurrence, not a Gaussian approximation. These cases
// have n <= 330, so the initial probability is comfortably representable.
fn binomial_quantile(n: usize, p: f64) -> usize {
    let mut probability = (1.0 - p).powi(n as i32);
    let mut cdf = probability;
    for k in 0..n {
        if cdf >= 0.999 {
            return k;
        }
        probability *= (n - k) as f64 / (k + 1) as f64 * p / (1.0 - p);
        cdf += probability;
    }
    n
}

#[test]
fn e2e_binomial_cdf_and_grid_quanta() {
    assert_eq!(binomial_quantile(0, 0.5), 0);
    assert_eq!(binomial_quantile(10, 0.5), 9);
    assert_eq!(binomial_quantile(100, 0.5), 65);
    assert_eq!(binomial_quantile(330, 1.0 / 9.0), 55);
    assert_eq!(quantum("L/after/in1_20", 0.0), 100.0 / 492.0);
    assert_eq!(quantum("L/after/turns", 2.0), 2e-9);
}
