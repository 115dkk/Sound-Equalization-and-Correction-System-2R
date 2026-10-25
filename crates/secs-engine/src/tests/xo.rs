use super::{cases, design::replay};
use crate::xo::{XO_CANDIDATES, XoRow, XoScores, pick_xo, recommend_xo, xo_groups, xo_scores};
use secs_compute::Context;
use secs_oracle::{Fixture, PLATFORM_RTOL, errors, number};
use serde_json::Value;

fn scores(v: &Value) -> XoScores {
    XoScores {
        flat: number(&v["flat"]),
        gd: number(&v["gd"]),
        late: number(&v["late"]),
    }
}
fn table(v: &Value) -> Vec<(String, Vec<XoRow>)> {
    cases::pairs(v)
        .map(|(g, rows)| {
            (
                g.into(),
                rows.as_array()
                    .unwrap()
                    .iter()
                    .map(|row| XoRow {
                        fc: number(&row[0]),
                        scores: scores(&row[1]),
                        score: number(&row[1]["score"]),
                    })
                    .collect(),
            )
        })
        .collect()
}
fn values(s: XoScores) -> [f64; 3] {
    [s.flat, s.gd, s.late]
}

#[test]
fn xo_scores_replayed_designs() {
    for name in cases::names() {
        let case = cases::load(&name);
        let design = replay(&name);
        let fx = Fixture::load("engine_xo", &format!("{name}_scores"));
        let expected: Vec<_> = cases::pairs(&fx.outputs()["scores"])
            .map(|(g, v)| (g, scores(v)))
            .collect();
        let got = xo_scores(&design, &case.measurements, &case.config);
        assert_eq!(got.len(), expected.len());
        assert_eq!(
            xo_groups(&case.config),
            got.iter().map(|(g, _)| g.clone()).collect::<Vec<_>>()
        );
        for ((group, actual), (expected_group, expected)) in got.iter().zip(expected) {
            assert_eq!(group, expected_group);
            let error = errors(&values(*actual), &values(expected));
            eprintln!(
                "MEASURE xo_scores/{name}/{group} max_abs={:e} max_rel={:e} Rust={actual:?} reference={expected:?}",
                error.0, error.1
            );
            // Every score depends on the BA rows. `room`'s BA differential
            // evolution diverges from the oracle at evaluation 3713 (P10), and
            // off the oracle libm it can diverge on any case (macOS:
            // room_filters), so those are reported, as the replayed design
            // test does after BA.
            if name == "room" || !secs_oracle::oracle_libm() {
                eprintln!("XO_REPORT xo_scores/{name}/{group} max_rel={:e}", error.1);
                continue;
            }
            secs_oracle::assert_close(
                &format!("xo/{name}/{group}"),
                &values(*actual),
                &values(expected),
                0.0,
                (1.5 * 1.175_413_288_588_782e-13_f64).max(PLATFORM_RTOL),
            );
        }
    }
}

#[test]
fn xo_pick_recorded_tables() {
    for name in ["room", "dolby"] {
        let runs = Fixture::load("engine_xo", &format!("{name}_recommend")).calls();
        for run in runs {
            let mut actual = table(&run.outputs()["table"]);
            let expected = actual.clone();
            for (_, rows) in &mut actual {
                for row in rows {
                    row.score = f64::NAN;
                }
            }
            let best = pick_xo(&mut actual);
            assert_eq!(actual, expected);
            assert_eq!(serde_json::to_value(best).unwrap(), run.outputs()["best"]);
        }
    }
    let s = XoScores {
        flat: 90.0,
        gd: 2.0,
        late: -20.0,
    };
    let mut tie = vec![(
        "front".into(),
        vec![
            XoRow {
                fc: 120.0,
                scores: s,
                score: 0.0,
            },
            XoRow {
                fc: 60.0,
                scores: s,
                score: 0.0,
            },
        ],
    )];
    assert_eq!(pick_xo(&mut tie), vec![("front".into(), 60.0)]);
}

#[test]
fn xo_parallel_order_progress_and_cancellation() {
    use secs_compute::{CancelToken, MemoryBudget, Progress, ProgressSink};
    use std::sync::{Arc, Mutex};
    #[derive(Debug, Default)]
    struct Sink(Mutex<Vec<Progress>>);
    impl ProgressSink for Sink {
        fn report(&self, p: &Progress) {
            self.0.lock().unwrap().push(p.clone());
        }
    }
    let case = cases::load("room");
    let settings = serde_json::from_value(case.settings.clone()).unwrap();
    let view = crate::ProjectView {
        groups: &case.config.groups,
        project_xo: &case.config.xo,
        target: &case.config.target,
    };
    let sink = Arc::new(Sink::default());
    let cancel = CancelToken::new();
    let context = Context::new(2, MemoryBudget::new(u64::MAX), sink.clone(), cancel.clone());
    let candidates = [120.0, 60.0, 120.0];
    let parallel = recommend_xo(
        &view,
        &case.measurements,
        &settings,
        &case.tuning,
        &candidates,
        &context,
    )
    .unwrap();
    let serial = recommend_xo(
        &view,
        &case.measurements,
        &settings,
        &case.tuning,
        &candidates,
        &Context::serial(),
    )
    .unwrap();
    assert_eq!(parallel, serial);
    assert_eq!(parallel.candidates, candidates);
    assert!(
        parallel
            .table
            .iter()
            .all(|(_, rows)| rows.iter().map(|r| r.fc).eq([120.0, 60.0]))
    );
    // StageProgress throttles reports that come close together (two
    // candidates can finish within its interval), so check the endpoints and
    // monotonicity, not every intermediate count.
    let reports = sink.0.lock().unwrap();
    let done: Vec<_> = reports.iter().map(|p| p.done).collect();
    assert_eq!(done.first(), Some(&0), "{done:?}");
    assert_eq!(done.last(), Some(&3), "{done:?}");
    assert!(done.windows(2).all(|w| w[0] <= w[1]), "{done:?}");
    assert!(
        reports
            .iter()
            .all(|p| p.total == 3 && p.stage == "engine.xo_recommendation")
    );
    cancel.cancel();
    assert!(matches!(
        recommend_xo(
            &view,
            &case.measurements,
            &settings,
            &case.tuning,
            &candidates,
            &context
        ),
        Err(crate::EngineError::Cancelled)
    ));
    let empty = recommend_xo(
        &view,
        &case.measurements,
        &settings,
        &case.tuning,
        &[],
        &Context::serial(),
    )
    .unwrap();
    assert!(empty.table.is_empty() && empty.best.is_empty());
    let groups: Vec<_> = case
        .config
        .groups
        .iter()
        .filter(|(g, _)| g != "sub")
        .cloned()
        .collect();
    let view = crate::ProjectView {
        groups: &groups,
        ..view
    };
    assert!(matches!(
        recommend_xo(
            &view,
            &case.measurements,
            &settings,
            &case.tuning,
            &[],
            &Context::serial()
        ),
        Err(crate::EngineError::Input(_))
    ));
}

#[test]
fn xo_recommendation_reports_a_position_missing_a_sub() {
    // TD-48: BA and RC only use complete positions, so the candidate designs
    // succeed; scoring replays every position where a main was measured and
    // used to panic there instead of reporting the incomplete measurement.
    let mut case = cases::load("room");
    let (_, roles) = case
        .measurements
        .positions
        .iter_mut()
        .find(|(p, _)| *p == 2)
        .unwrap();
    roles.retain(|(r, _)| r != "SW2");
    let settings = serde_json::from_value(case.settings.clone()).unwrap();
    let view = crate::ProjectView {
        groups: &case.config.groups,
        project_xo: &case.config.xo,
        target: &case.config.target,
    };
    let result = recommend_xo(
        &view,
        &case.measurements,
        &settings,
        &case.tuning,
        &[80.0],
        &Context::serial(),
    );
    assert!(
        matches!(&result, Err(crate::EngineError::Input(message))
            if message == "position 2 L: missing SW2 for crossover scoring"),
        "{result:?}"
    );
}

#[test]
fn xo_candidates_share_the_job_budget() {
    use secs_compute::{CancelToken, MemoryBudget, NoProgress};
    use std::sync::Arc;
    let case = cases::load("room");
    let settings: secs_types::calibration::CalibrationSettings =
        serde_json::from_value(case.settings.clone()).unwrap();
    let view = crate::ProjectView {
        groups: &case.config.groups,
        project_xo: &case.config.xo,
        target: &case.config.target,
    };
    let inputs = crate::engine_inputs(&view, &case.measurements, &settings).unwrap();
    let (shared, candidate) = crate::xo::xo_bytes(&inputs.measurements, &inputs.config);
    let candidates = [60.0, 120.0];
    let expected = recommend_xo(
        &view,
        &case.measurements,
        &settings,
        &case.tuning,
        &candidates,
        &Context::serial(),
    )
    .unwrap();
    let budget = |bytes| {
        Arc::new(Context::new(
            4,
            MemoryBudget::new(bytes),
            Arc::new(NoProgress),
            CancelToken::new(),
        ))
    };
    // Room for one candidate at a time on four threads: it must finish, give
    // the unbounded result, and return every reserved byte.
    let ctx = budget(shared + candidate);
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = {
        let ctx = Arc::clone(&ctx);
        let (case, settings) = (cases::load("room"), settings.clone());
        std::thread::spawn(move || {
            let view = crate::ProjectView {
                groups: &case.config.groups,
                project_xo: &case.config.xo,
                target: &case.config.target,
            };
            let result = recommend_xo(
                &view,
                &case.measurements,
                &settings,
                &case.tuning,
                &candidates,
                &ctx,
            );
            let _ = tx.send(result);
        })
    };
    let result = rx.recv_timeout(std::time::Duration::from_secs(300));
    ctx.cancel().cancel();
    worker.join().unwrap();
    let actual = result
        .expect("XO recommendation timed out with a one-candidate budget")
        .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(ctx.budget().available(), ctx.budget().total());
    // One byte less does not fit the shared preparation and one candidate:
    // refuse before any spectra are built.
    let ctx = budget(shared + candidate - 1);
    let built = crate::measurement::SPECTRA_BUILT.with(std::cell::Cell::get);
    let result = recommend_xo(
        &view,
        &case.measurements,
        &settings,
        &case.tuning,
        &candidates,
        &ctx,
    );
    assert!(
        matches!(&result, Err(crate::EngineError::Input(message))
            if message.contains("exceeds memory budget")),
        "{result:?}"
    );
    assert_eq!(
        crate::measurement::SPECTRA_BUILT.with(std::cell::Cell::get),
        built,
        "XO built spectra before refusing its budget"
    );
    assert_eq!(ctx.budget().available(), ctx.budget().total());
}

fn binomial_quantile(n: usize, p: f64) -> usize {
    let mut probability = (1.0 - p).powi(n as i32);
    let mut cumulative = probability;
    for k in 0..n {
        if cumulative >= 0.999 {
            return k;
        }
        probability *= (n - k) as f64 / (k + 1) as f64 * p / (1.0 - p);
        cumulative += probability;
    }
    n
}

#[test]
fn xo_recommendation_production_envelope() {
    let mut failures = Vec::new();
    for name in ["room", "dolby"] {
        let case = cases::load(name);
        let settings: secs_types::calibration::CalibrationSettings =
            serde_json::from_value(case.settings.clone()).unwrap();
        let view = crate::ProjectView {
            groups: &case.config.groups,
            project_xo: &case.config.xo,
            target: &case.config.target,
        };
        let got = recommend_xo(
            &view,
            &case.measurements,
            &settings,
            &case.tuning,
            &XO_CANDIDATES,
            &Context::serial(),
        )
        .unwrap();
        let runs = Fixture::load("engine_xo", &format!("{name}_recommend")).calls();
        let tables: Vec<_> = runs.iter().map(|r| table(&r.outputs()["table"])).collect();
        assert_eq!(tables.len(), 8);
        let (mut count, mut worse_all, mut varying, mut worse_median) = (0, 0, 0, 0);
        assert_eq!(got.candidates, XO_CANDIDATES);
        assert_eq!(got.table.len(), tables[0].len());
        for (gi, (group, rows)) in got.table.iter().enumerate() {
            assert_eq!(group, &tables[0][gi].0);
            assert_eq!(rows.len(), XO_CANDIDATES.len());
            for (ri, row) in rows.iter().enumerate() {
                assert_eq!(row.fc, XO_CANDIDATES[ri]);
                let actual = [row.scores.flat, row.scores.gd, row.scores.late, row.score];
                for (vi, (metric, direction)) in
                    [("flat", 1.0), ("gd", -1.0), ("late", -1.0), ("score", 1.0)]
                        .into_iter()
                        .enumerate()
                {
                    let all: Vec<_> = tables
                        .iter()
                        .map(|t| {
                            let r = &t[gi].1[ri];
                            [r.scores.flat, r.scores.gd, r.scores.late, r.score][vi]
                        })
                        .collect();
                    let reference = all[0];
                    let low = all.iter().copied().reduce(f64::min).unwrap();
                    let high = all.iter().copied().reduce(f64::max).unwrap();
                    let width = high - low;
                    let quantum = if metric == "flat" {
                        100.0
                            / crate::grid::design_freqs()
                                .iter()
                                .filter(|f| (20.0..=200.0).contains(*f))
                                .count() as f64
                    } else {
                        1e-9 * reference.abs().max(1.0)
                    };
                    let worse = direction * (reference - actual[vi]);
                    let pass = actual[vi].is_finite() && worse <= 3.0 * width.max(quantum);
                    eprintln!(
                        "XO_TABLE {name}/{group}/{} {metric} Rust={:.12e} reference={reference:.12e} envelope=[{low:.12e},{high:.12e}] pass={pass}",
                        row.fc, actual[vi]
                    );
                    if worse > (1.25 * width).max(1e-9 * reference.abs().max(1.0)) {
                        eprintln!(
                            "XO_125 {name}/{group}/{} {metric} Rust={} all={all:?}",
                            row.fc, actual[vi]
                        );
                    }
                    // ADR 0004 "다른 libm": the eight runs sample the oracle's
                    // libm only, so off it a single value is reported; the
                    // binomial checks and the winner rule stay asserted.
                    if !pass && secs_oracle::oracle_libm() {
                        failures.push(format!(
                            "{name}/{group}/{} {metric}: {} vs {all:?}",
                            row.fc, actual[vi]
                        ));
                    } else if !pass {
                        eprintln!(
                            "XO_OFFLIBM {name}/{group}/{} {metric}: {} vs {all:?}",
                            row.fc, actual[vi]
                        );
                    }
                    count += 1;
                    worse_all += usize::from(if direction > 0.0 {
                        actual[vi] < low
                    } else {
                        actual[vi] > high
                    });
                    if width > 0.0 {
                        varying += 1;
                        let mut sorted = all.clone();
                        sorted.sort_by(f64::total_cmp);
                        worse_median += usize::from(
                            direction * ((sorted[3] + sorted[4]) / 2.0 - actual[vi]) > 0.0,
                        );
                    }
                }
            }
            let mut order: Vec<_> = (0..rows.len()).collect();
            order.sort_by(|&a, &b| {
                tables[0][gi].1[b]
                    .score
                    .total_cmp(&tables[0][gi].1[a].score)
                    .then(rows[a].fc.total_cmp(&rows[b].fc))
            });
            let (first, second) = (order[0], order[1]);
            let gaps: Vec<_> = tables
                .iter()
                .map(|t| t[gi].1[first].score - t[gi].1[second].score)
                .collect();
            let width = gaps.iter().copied().reduce(f64::max).unwrap()
                - gaps.iter().copied().reduce(f64::min).unwrap();
            let reference = tables[0][gi].1[first].fc;
            let actual = got.best.iter().find(|(g, _)| g == group).unwrap().1;
            let stable = gaps[0] > 3.0 * width;
            eprintln!(
                "XO_BEST {name}/{group} Rust={actual} reference={reference} gap={} gap_width={width} stable={stable}",
                gaps[0]
            );
            if stable && actual != reference {
                failures.push(format!(
                    "{name}/{group}: stable best {reference}, Rust {actual}"
                ));
            }
        }
        let all_bound = binomial_quantile(count, 1.0 / 9.0);
        let median_bound = binomial_quantile(varying, 0.5);
        eprintln!(
            "XO_BINOMIAL {name} worse_all={worse_all}/{count} bound={all_bound} worse_median={worse_median}/{varying} bound={median_bound}"
        );
        if worse_all > all_bound || worse_median > median_bound {
            failures.push(format!("{name} binomial gate"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
