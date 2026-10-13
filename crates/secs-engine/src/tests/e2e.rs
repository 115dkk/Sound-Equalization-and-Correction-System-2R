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
        let d = design(
            &DesignRequest {
                measurements: &c.measurements,
                config: &c.config,
                tuning: &c.tuning,
                subs_only: false,
                adapter_notes: &[],
            },
            &Context::serial(),
            &*production_fitters(),
        )
        .unwrap();
        let seconds = started.elapsed().as_secs_f64();
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
            let margin = (1.25 * (max - min)).max(1e-9 * c0.abs().max(1.0));
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
            if !pass {
                failures.push(format!(
                    "E2E FAIL {name} {key}: Rust={v:?}, all eight={all:?}"
                ));
            }
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
