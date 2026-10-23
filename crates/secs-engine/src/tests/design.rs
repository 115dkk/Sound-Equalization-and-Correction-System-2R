use super::{cases, fit::read_recorded_fit};
use crate::{
    C,
    design::{self, Design, DesignRequest, FitStage, FitterFactory},
    fit::{RecordedFit, replay_fitter},
    grid::design_freqs,
};
use secs_compute::Context;
use secs_oracle::{Fixture, PLATFORM_RTOL, errors, number, oracle_libm};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub(super) fn metrics(m: &crate::scorecard::Metrics) -> Vec<(&'static str, f64)> {
    vec![
        ("level", m.level),
        ("in1_20", m.in1_20),
        ("in2_20", m.in2_20),
        ("in1_200", m.in1_200),
        ("in2_200", m.in2_200),
        ("in1_2000", m.in1_2000),
        ("in2_2000", m.in2_2000),
        ("turns", m.turns),
        ("gd_20", m.gd_20),
        ("gd_100", m.gd_100),
        ("gd_500", m.gd_500),
        ("gd24_abs", m.gd24_abs),
        ("gd24_abs_hz", m.gd24_abs_hz),
        ("gd24_max", m.gd24_max),
        ("gd24_span", m.gd24_span),
        ("gd_30", m.gd_30),
        ("pre_db", m.pre_db),
    ]
}
fn note_value(note: &crate::notes::Note) -> Value {
    use crate::notes::Note;
    use serde_json::json;
    match note {
        Note::LiftCapped {
            from_ms,
            taps,
            lift_ms,
        } => json!({"LiftCapped": {"from_ms": from_ms, "taps": taps, "lift_ms": lift_ms}}),
        Note::SubFilters(filters) => json!({"SubFilters": filters.iter().map(|(r, f)| (r, json!({
            "hp_hz": f.hp_hz, "hp_type": f.hp_type.name(), "lp_hz": f.lp_hz,
            "lp_type": f.lp_type.name(), "peq": f.peq
        }))).collect::<Vec<_>>()}),
        Note::RcLimit {
            hz,
            auto,
            schroeder_hz,
        } => json!({"RcLimit": {"hz": hz, "auto": auto, "schroeder_hz": schroeder_hz}}),
        Note::Latency {
            latency_ms,
            lift_ms,
            main_extra_ms,
        } => {
            json!({"Latency": {"latency_ms": latency_ms, "lift_ms": lift_ms, "main_extra_ms": main_extra_ms}})
        }
        Note::PhaseStrength(v) => json!({"PhaseStrength": v}),
        Note::PhaseBand(v) => json!({"PhaseBand": v}),
        Note::LongModeBoost { cap_db, modes } => {
            json!({"LongModeBoost": {"cap_db": cap_db, "modes": modes}})
        }
        Note::GdCuts { role, cuts } => json!({"GdCuts": {"role": role, "cuts": cuts}}),
        Note::BaCost(v) => json!({"BaCost": v}),
        Note::BaBusPass {
            first,
            second,
            kept_first,
        } => json!({"BaBusPass": {"first": first, "second": second, "kept_first": kept_first}}),
        Note::BaManual => json!("BaManual"),
        Note::LayoutFallback(v) => json!({"LayoutFallback": v}),
        Note::Layout(v) => json!({"Layout": v}),
        Note::TwoWay {
            roles,
            gain_db,
            xo_hz,
        } => json!({"TwoWay": {"roles": roles, "gain_db": gain_db, "xo_hz": xo_hz}}),
        Note::BusGainAnchored(v) => json!({"BusGainAnchored": v}),
        Note::Mimo(v) => json!({"Mimo": v.oracle_text()}),
    }
}

// Structure, order, booleans and strings are exact; floats retain their
// stage's measured tolerance. Never zip arrays without checking their length.
fn compare_value(name: &str, got: &Value, expected: &Value, gate: bool) {
    match (got, expected) {
        (Value::Number(_), Value::Number(_)) => {
            compare(name, &[number(got)], &[number(expected)], gate)
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare_value(&format!("{name}/{i}"), a, b, gate);
            }
        }
        (Value::Object(a), Value::Object(b)) if a.keys().eq(b.keys()) => {
            for (key, a) in a {
                compare_value(&format!("{name}/{key}"), a, &b[key], gate);
            }
        }
        _ if gate => assert_eq!(got, expected, "{name}"),
        _ => eprintln!("MEASURE design {name} report-only got={got} expected={expected}"),
    }
}

fn compare(name: &str, got: &[f64], expected: &[f64], gate: bool) {
    let err = errors(got, expected);
    eprintln!("MEASURE design {name} {err:?}");
    if gate {
        let tolerance = if name.ends_with("/pre_db") && expected.iter().any(|v| *v < -250.0) {
            (1.5 * 1.4484069100264113e-5_f64).max(PLATFORM_RTOL)
        } else {
            PLATFORM_RTOL
        };
        secs_oracle::assert_close(name, got, expected, 0.0, tolerance);
    }
}
fn complex(fx: &Fixture, name: &str, h: &[C], expected: &Value, gate: bool) {
    let idx: Vec<_> = design_freqs()
        .iter()
        .enumerate()
        .filter(|(i, f)| ((15.0..=600.0).contains(*f) && i % 8 == 0) || i % 257 == 0)
        .map(|(i, _)| i)
        .collect();
    let got: Vec<_> = idx.iter().flat_map(|&i| [h[i].re, h[i].im]).collect();
    let expected: Vec<_> = fx
        .c128s(expected)
        .into_iter()
        .flat_map(|(r, i)| [r, i])
        .collect();
    compare(name, &got, &expected, gate);
}
fn verify_request_hash(fixture: &Fixture, fit: &RecordedFit, hashes: &Value) {
    for (key, values) in [("g", &fit.g), ("corr_db", &fit.corr_db)] {
        fixture.assert_digest(
            key,
            &serde_json::json!({
                "digest": hashes[key], "length": values.len(), "first": [], "last": []
            }),
            values,
        );
    }
}

pub(super) fn replay(name: &str) -> Design {
    replay_with_context(name, &Context::serial())
}

fn replay_with_context(name: &str, context: &Context) -> Design {
    let case = cases::load(name);
    let mut stages: BTreeMap<String, Vec<RecordedFit>> = BTreeMap::new();
    for call in Fixture::all_calls("engine_design", &format!("{name}_fits_")) {
        let recorded = if let Some(reference) = call.inputs().get("reference") {
            let source = Fixture::load(
                reference["family"].as_str().unwrap(),
                reference["fixture"].as_str().unwrap(),
            );
            let fit = if let Some(index) = reference["call"].as_u64() {
                let calls = source.calls();
                let c = &calls[index as usize];
                read_recorded_fit(c, c.inputs(), &c.outputs()["result"])
            } else {
                let fit = &source.outputs()["candidates"]
                    [reference["candidate"].as_u64().unwrap() as usize]["fits"]
                    [reference["fit"].as_u64().unwrap() as usize];
                read_recorded_fit(&source, &fit["inputs"], &fit["result"])
            };
            verify_request_hash(&source, &fit, &reference["request_sha256"]);
            fit
        } else {
            let fit = read_recorded_fit(&call, call.inputs(), &call.outputs()["result"]);
            verify_request_hash(&call, &fit, &call.inputs()["request_sha256"]);
            fit
        };
        stages
            .entry(call.inputs()["stage"].as_str().unwrap().into())
            .or_default()
            .push(recorded);
    }
    // The RC channel evaluates the phase-independent preparation once for
    // both phase candidates (ADR 0012); SECS 2 recorded it per candidate.
    for (key, calls) in &mut stages {
        if key.starts_with("Rc:") {
            let mut kept: Vec<RecordedFit> = Vec::new();
            for call in calls.drain(..) {
                if !kept
                    .iter()
                    .any(|earlier| super::fit::same_recorded_fit(earlier, &call))
                {
                    kept.push(call);
                }
            }
            *calls = kept;
        }
    }
    let counts: Arc<Mutex<BTreeMap<String, (usize, usize)>>> =
        Arc::new(Mutex::new(BTreeMap::new()));
    let seen = counts.clone();
    let factory: Box<FitterFactory<'_>> = Box::new(move |stage| {
        let key = match stage {
            FitStage::Ba => "Ba".into(),
            FitStage::Top(r) => format!("Top:{r}"),
            FitStage::Rc(r) => format!("Rc:{r}"),
        };
        let calls = stages.get(&key).cloned().unwrap_or_default();
        let expected = calls.len();
        let report_calls = (name == "room" || !oracle_libm()).then(|| calls.clone());
        let mut next = 0;
        let mut fit = replay_fitter(
            calls,
            if name == "room" || !oracle_libm() {
                f64::INFINITY
            } else {
                PLATFORM_RTOL
            },
        );
        let seen = seen.clone();
        seen.lock().unwrap().insert(key.clone(), (0, expected));
        Box::new(move |r| {
            seen.lock().unwrap().get_mut(&key).unwrap().0 += 1;
            if let Some(calls) = &report_calls {
                let expected = &calls[next];
                let (_, rel) = errors(r.corr_db, &expected.corr_db);
                eprintln!(
                    "MEASURE design {name}/{key}/fit{next} corr_db max_rel={rel:e} report-only (room BA or foreign-libm thresholds)"
                );
            }
            next += 1;
            fit(r)
        })
    });
    let result = design::design(
        &DesignRequest {
            measurements: &case.measurements,
            config: &case.config,
            tuning: &case.tuning,
            subs_only: false,
            adapter_notes: &[],
        },
        context,
        &*factory,
    )
    .unwrap();
    for (key, (actual, expected)) in counts.lock().unwrap().iter() {
        assert_eq!(actual, expected, "{name} {key} count");
    }
    result
}
#[test]
fn design_parallel_progress_counts_completed_channels() {
    use secs_compute::{CancelToken, MemoryBudget, Progress, ProgressSink};
    #[derive(Debug, Default)]
    struct Sink(Mutex<Vec<Progress>>);
    impl ProgressSink for Sink {
        fn report(&self, progress: &Progress) {
            self.0.lock().unwrap().push(progress.clone());
        }
    }
    let sink = Arc::new(Sink::default());
    let context = Context::new(
        3,
        MemoryBudget::new(u64::MAX),
        sink.clone(),
        CancelToken::new(),
    );
    let parallel = replay_with_context("room_dba", &context);
    let serial = replay("room_dba");
    assert_eq!(parallel.chain, serial.chain);
    assert_eq!(parallel.notes, serial.notes);
    assert_eq!(parallel.scores.len(), serial.scores.len());
    for (parallel, serial) in parallel.scores.iter().zip(&serial.scores) {
        assert_eq!(parallel.role, serial.role);
        for (a, b) in metrics(&parallel.before)
            .into_iter()
            .zip(metrics(&serial.before))
        {
            assert_eq!(a.0, b.0);
            assert_eq!(a.1.to_bits(), b.1.to_bits());
        }
        for (a, b) in metrics(&parallel.after)
            .into_iter()
            .zip(metrics(&serial.after))
        {
            assert_eq!(a.0, b.0);
            assert_eq!(a.1.to_bits(), b.1.to_bits());
        }
        for (a, b) in [
            (&parallel.spl_before, &serial.spl_before),
            (&parallel.spl_after, &serial.spl_after),
            (&parallel.gd_before, &serial.gd_before),
            (&parallel.gd_after, &serial.gd_after),
        ] {
            assert_eq!(a.len(), b.len());
            assert!(a.iter().zip(b).all(|(a, b)| a.to_bits() == b.to_bits()));
        }
    }
    let reports = sink.0.lock().unwrap();
    assert_eq!(reports.first().unwrap().done, 0);
    assert_eq!(reports.last().unwrap().done, 4);
    assert!(reports.iter().all(|p| p.total == 4));
    assert!(reports.windows(2).all(|p| p[0].done <= p[1].done));
}

#[test]
fn design_empty_subs_and_subs_only_keep_axes_and_shared_spectra() {
    let mut c = cases::load("room_manual");
    let notes = vec!["adapter note".to_owned()];
    c.config.subs.clear();
    c.config.sub_manual.clear();
    c.config.bundle_layouts.clear();
    let context = Context::serial();
    let run = design::design(
        &DesignRequest {
            measurements: &c.measurements,
            config: &c.config,
            tuning: &c.tuning,
            subs_only: true,
            adapter_notes: &notes,
        },
        &context,
        &*design::production_fitters(context.cancel()),
    )
    .unwrap();
    assert!(run.mains.is_empty() && run.chain.mains.is_empty() && run.scores.is_empty());
    assert!(run.subs.is_none() && run.chain.subs.is_empty());
    assert_eq!(run.chain.main_extra_ms, 0.0);
    assert_eq!(run.adapter_notes, notes);
    for &p in &run.spectra.positions {
        for role in run.spectra.raw.roles_at(p) {
            assert!(Arc::ptr_eq(
                run.spectra.raw.get(p, role).unwrap(),
                run.spectra.used.get(p, role).unwrap()
            ));
        }
    }
}

#[test]
fn design_cancellation_returns_within_one_second() {
    use secs_compute::{CancelToken, MemoryBudget, NoProgress};
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
        thread,
        time::{Duration, Instant},
    };
    let case = cases::load("dolby");
    let mut failures = Vec::new();
    let request = DesignRequest {
        measurements: &case.measurements,
        config: &case.config,
        tuning: &case.tuning,
        subs_only: false,
        adapter_notes: &[],
    };
    let two = |token: CancelToken| {
        Context::new(2, MemoryBudget::new(u64::MAX), Arc::new(NoProgress), token)
    };
    // Cancel inside the fit that starts as the n-th PEQ fit of the run. The
    // fit count does not depend on machine speed or load (wall-clock marks
    // did: a loaded runner measured a slow reference run and a later run had
    // finished before its last mark), and cancelling 20 ms into a fit
    // exercises the in-flight path.
    fn counting<'c>(
        cancel: &'c CancelToken,
        calls: &'c AtomicUsize,
        signal: Option<(usize, mpsc::Sender<()>)>,
    ) -> Box<design::FitterFactory<'c>> {
        let production = design::production_fitters(cancel);
        let signal = Arc::new(std::sync::Mutex::new(signal));
        Box::new(move |stage| {
            let mut fitter = production(stage);
            let signal = Arc::clone(&signal);
            Box::new(move |request| {
                let n = calls.fetch_add(1, Ordering::SeqCst) + 1;
                if let Some((target, tx)) = signal.lock().unwrap().as_ref()
                    && n == *target
                {
                    let _ = tx.send(());
                }
                fitter(request)
            })
        })
    }
    let calls = AtomicUsize::new(0);
    let total = {
        let context = two(CancelToken::new());
        let factory = counting(context.cancel(), &calls, None);
        let start = Instant::now();
        design::design(&request, &context, &*factory).expect("uncancelled design");
        eprintln!("MEASURE design uncancelled={:?}", start.elapsed());
        calls.swap(0, Ordering::SeqCst)
    };
    eprintln!("MEASURE design fit calls={total}");
    assert!(total > 10, "too few fits to place cancellations: {total}");
    for fraction in [0.03, 0.15, 0.35, 0.6, 0.85] {
        let target = ((total as f64 * fraction).ceil() as usize).max(1);
        calls.store(0, Ordering::SeqCst);
        let token = CancelToken::new();
        let context = two(token.clone());
        let (tx, rx) = mpsc::channel();
        let factory = counting(context.cancel(), &calls, Some((target, tx)));
        thread::scope(|scope| {
            let canceller = scope.spawn(move || {
                rx.recv_timeout(Duration::from_secs(600)).ok()?;
                thread::sleep(Duration::from_millis(20));
                let at = Instant::now();
                token.cancel();
                Some(at)
            });
            let result = design::design(&request, &context, &*factory);
            let returned = Instant::now();
            let cancelled_at = canceller.join().unwrap();
            let cancelled = matches!(result, Err(crate::EngineError::Cancelled));
            let latency = cancelled_at.map(|at| returned.saturating_duration_since(at));
            eprintln!(
                "MEASURE design cancellation fit={target}/{total} latency={latency:?} cancelled={cancelled}"
            );
            if !cancelled || latency.is_none_or(|l| l >= Duration::from_secs(1)) {
                failures.push(format!(
                    "fit {target}/{total}: cancelled={cancelled}, latency={latency:?}"
                ));
            }
        });
    }
    assert!(failures.is_empty(), "cancellation gate: {failures:?}");
}

#[test]
fn design_matches_recorded_stages() {
    for name in cases::names() {
        let started = std::time::Instant::now();
        let result = replay(&name);
        let fx = Fixture::load("engine_design", &name);
        let o = fx.outputs();
        let gate = name != "room" && oracle_libm();
        if !gate {
            eprintln!(
                "MEASURE design {name} REPORT_ONLY: BA rows/cost/bus gain/delay/polarity, layout winner, post-BA spectra and scores, RC masks/notes, phase ladder/fraction/FIR/response and selected input PEQ; foreign-libm before percentages (threshold), phase/GD/pre-energy metrics and GD curves (IR/peak argmax): downstream DE comparisons, threshold or argmax can flip on last bits. Pre-BA spectra/levels/target, configured XO/filter values, top PEQ and output-axis identities remain gated."
            );
        }
        assert_eq!(result.rc_auto, o["rc_auto"].as_bool().unwrap());
        compare(
            &format!("{name}/pre_ba"),
            &[result.ref_level, result.rc_hi],
            &[number(&o["ref_level"]), number(&o["rc_hi"])],
            true,
        );
        assert_eq!(
            serde_json::to_value(result.mains.iter().map(|m| &m.role).collect::<Vec<_>>()).unwrap(),
            o["roles"]
        );
        compare(
            &format!("{name}/chain"),
            &[
                result.chain.lift_ms,
                result.chain.latency_ms,
                result.chain.main_extra_ms,
            ],
            &[
                number(&o["lift_ms"]),
                number(&o["latency_ms"]),
                number(&o["main_extra_ms"]),
            ],
            gate,
        );
        compare(
            &format!("{name}/target_display"),
            &result.target_display,
            &fx.f64s(&o["target_display"]),
            true,
        );
        compare_value(
            &format!("{name}/schroeder_hz"),
            &serde_json::json!(result.schroeder_hz),
            &o["schroeder_hz"],
            true,
        );
        let target: Vec<_> = design_freqs()
            .iter()
            .enumerate()
            .filter(|(i, f)| ((15.0..=600.0).contains(*f) && i % 8 == 0) || i % 257 == 0)
            .map(|(i, _)| result.target_db[i])
            .collect();
        compare(
            &format!("{name}/target"),
            &target,
            &fx.f64s(&o["target"]),
            true,
        );
        assert_eq!(result.adapter_notes, Vec::<String>::new());
        compare_value(
            &format!("{name}/notes"),
            &Value::Array(result.notes.iter().map(note_value).collect()),
            &o["typed_notes"],
            gate,
        );
        // Pre-BA diagnostics do not depend on any optimizer decisions.
        for (i, note) in result.notes.iter().enumerate() {
            if matches!(
                note,
                crate::notes::Note::LiftCapped { .. }
                    | crate::notes::Note::SubFilters(_)
                    | crate::notes::Note::RcLimit { .. }
            ) {
                compare_value(
                    &format!("{name}/notes/{i}"),
                    &note_value(note),
                    &o["typed_notes"][i],
                    true,
                );
            }
        }
        assert_eq!(result.chain.sample_rate, 48000);
        if gate {
            assert_eq!(
                serde_json::json!(result.chain.sub_bus_peq),
                o["sub_bus_peq"]
            );
        }
        let kinds: Vec<_> = result
            .notes
            .iter()
            .map(|n| match n {
                crate::notes::Note::LiftCapped { .. } => "LiftCapped",
                crate::notes::Note::SubFilters(_) => "SubFilters",
                crate::notes::Note::RcLimit { .. } => "RcLimit",
                crate::notes::Note::Latency { .. } => "Latency",
                crate::notes::Note::PhaseStrength(_) => "PhaseStrength",
                crate::notes::Note::PhaseBand(_) => "PhaseBand",
                crate::notes::Note::LongModeBoost { .. } => "LongModeBoost",
                crate::notes::Note::GdCuts { .. } => "GdCuts",
                crate::notes::Note::BaCost(_) => "BaCost",
                crate::notes::Note::BaBusPass { .. } => "BaBusPass",
                crate::notes::Note::BaManual => "BaManual",
                crate::notes::Note::LayoutFallback(_) => "LayoutFallback",
                crate::notes::Note::Layout(_) => "Layout",
                crate::notes::Note::TwoWay { .. } => "TwoWay",
                crate::notes::Note::BusGainAnchored(_) => "BusGainAnchored",
                crate::notes::Note::Mimo(_) => "Mimo",
            })
            .collect();
        if gate {
            assert_eq!(serde_json::to_value(kinds).unwrap(), o["note_kinds"]);
        }
        let sub = result.subs.as_ref().unwrap();
        compare("bus_gain", &[sub.bus_gain], &[number(&o["bus_gain"])], gate);
        assert_eq!(sub.rows.len(), o["sub_rows"].as_array().unwrap().len());
        for ((role, row), expected) in sub.rows.iter().zip(o["sub_rows"].as_array().unwrap()) {
            assert!(result.chain.subs.iter().any(|s| &s.role == role));
            if gate {
                assert_eq!(i64::from(row.polarity), expected[2].as_i64().unwrap());
            }
            compare_value(
                "sub/internal_row",
                &serde_json::json!([row.gain_db, row.delay_ms, row.polarity, row.allpass]),
                expected,
                gate,
            );
        }
        let layouts = sub.layout_info.iter().map(|(name, info)| {
            serde_json::json!([name, {
                "kind": match info.kind { crate::input::LayoutKind::Dba => "dba", crate::input::LayoutKind::Multi => "multi" },
                "members": info.members, "gains": info.gains.iter().cloned().collect::<BTreeMap<_, _>>(),
                "front": info.front, "rear": info.rear, "rear_ms": info.rear_ms,
                "auto_ms": info.auto_ms, "trim_ms": info.trim_ms, "min_ms": info.min_ms, "peq": info.peq
            }])
        }).collect::<Vec<_>>();
        compare_value(
            "layout_info",
            &serde_json::json!(layouts),
            &o["layout_info"],
            gate,
        );
        assert_eq!(sub.plan.is_some(), !o["plan"].is_null());
        if let Some(plan) = &sub.plan {
            for (key, v) in [
                ("fixed", serde_json::json!(plan.fixed)),
                ("auto", serde_json::json!(plan.auto)),
                ("gain_anchored", serde_json::json!(plan.gain_anchored)),
                (
                    "bundles",
                    serde_json::json!(plan.bundles.iter().cloned().collect::<BTreeMap<_, _>>()),
                ),
            ] {
                assert_eq!(v, o["plan"][key], "{name}/plan/{key}");
            }
        }
        if gate {
            assert_eq!(
                i64::from(sub.bus_polarity),
                o["bus_polarity"].as_i64().unwrap()
            );
        }
        compare(
            "bus_delay",
            &[sub.bus_delay_ms],
            &[number(&o["bus_delay_ms"])],
            gate,
        );
        compare(
            "sub_level",
            &[sub.sub_level_db.unwrap()],
            &[number(&o["sub_level_db"])],
            gate,
        );
        complex(&fx, "sub_sum", &sub.sum_at_reference, &o["sub_sum"], gate);
        assert_eq!(result.chain.subs.len(), o["subs"].as_array().unwrap().len());
        for (s, expected) in result.chain.subs.iter().zip(o["subs"].as_array().unwrap()) {
            assert_eq!(s.hp_type.name(), expected["hp_type"].as_str().unwrap());
            assert_eq!(s.lp_type.name(), expected["lp_type"].as_str().unwrap());
            assert_eq!(s.hp_hz, number(&expected["hp_hz"]));
            assert_eq!(s.lp_hz, number(&expected["lp_hz"]));
            if gate {
                assert_eq!(serde_json::json!(s.peq), expected["peq"]);
            }
            compare_value(
                "sub/allpass",
                &serde_json::json!(s.allpass),
                &expected["allpass"],
                gate,
            );
            assert_eq!(s.role, expected["role"].as_str().unwrap());
            if gate {
                assert_eq!(s.mode, expected["mode"].as_str().unwrap());
                assert_eq!(
                    i64::from(s.polarity),
                    expected["polarity"].as_i64().unwrap()
                );
                assert_eq!(s.hp_type.name(), expected["hp_type"].as_str().unwrap());
                assert_eq!(s.lp_type.name(), expected["lp_type"].as_str().unwrap());
                assert_eq!(s.peq.len(), expected["peq"].as_array().unwrap().len());
                assert_eq!(
                    s.allpass.len(),
                    expected["allpass"].as_array().unwrap().len()
                );
            }
            compare(
                "sub scalars",
                &[s.gain_db, s.delay_ms, s.hp_hz, s.lp_hz],
                &[
                    number(&expected["gain_db"]),
                    number(&expected["delay_ms"]),
                    number(&expected["hp_hz"]),
                    number(&expected["lp_hz"]),
                ],
                gate,
            );
        }
        for main in &result.mains {
            let role = Fixture::load("engine_design", &format!("{name}_{}", main.role));
            let out = role.outputs();
            for (key, h) in [
                ("before", &main.before),
                ("combined", &main.combined),
                ("after", &main.after),
                ("F", &main.response),
            ] {
                complex(
                    &role,
                    &format!("{name}/{}/{key}", main.role),
                    h,
                    &out[key],
                    gate || key == "before",
                );
            }
            assert_eq!(main.top_response.is_some(), !out["top_P"].is_null());
            if let Some(p) = &main.top_response {
                complex(&role, "top_P", p, &out["top_P"], true);
            }
            let channel = result
                .chain
                .mains
                .iter()
                .find(|c| c.role == main.role)
                .unwrap();
            assert_eq!(channel.xo_hz, out["channel"]["xo_hz"].as_f64());
            assert_eq!(
                serde_json::json!(channel.top_peq),
                out["channel"]["top_peq"]
            );
            assert_eq!(main.xo_hz, channel.xo_hz);
            assert_eq!(main.align_delay_ms, channel.align_delay_ms);
            compare(
                "main/align_delay",
                &[main.align_delay_ms],
                &[number(&o["align_delay_ms"][&main.role])],
                true,
            );
            assert_eq!(main.gain_db, channel.gain_db);
            assert_eq!(
                channel.delay_ms,
                channel.align_delay_ms + result.chain.main_extra_ms
            );
            compare(
                "main/gain",
                &[channel.gain_db],
                &[number(&out["channel"]["gain_db"])],
                true,
            );
            compare(
                "main/delay",
                &[channel.delay_ms],
                &[number(&out["channel"]["delay_ms"])],
                gate,
            );
            compare(
                "main/phase_strength",
                &[main.phase_strength],
                &[number(&out["phase_strength"])],
                gate,
            );
            compare_value(
                "main/phase_band",
                &serde_json::json!(main.phase_band),
                &out["phase_band"],
                gate,
            );
            if gate {
                assert_eq!(serde_json::json!(channel.input_peq), out["channel"]["peq"]);
                assert_eq!(main.phase_frac_used, out["phase_frac_used"].as_f64());
                assert_eq!(
                    channel.fir.len(),
                    out["channel"]["fir"]["length"].as_u64().unwrap() as usize
                );
                assert_eq!(
                    channel.input_peq.len(),
                    out["channel"]["peq"].as_array().unwrap().len()
                );
            }
            let peak = out["channel"]["fir_peak"].as_u64().unwrap() as usize;
            if gate {
                let actual_peak = channel.fir.iter().enumerate().fold(0, |best, (i, value)| {
                    if value.abs() > channel.fir[best].abs() {
                        i
                    } else {
                        best
                    }
                });
                assert_eq!(actual_peak, peak, "{name}/{}/FIR peak", main.role);
            }
            let samples: Vec<_> = channel
                .fir
                .iter()
                .enumerate()
                .filter(|(i, _)| i.abs_diff(peak) <= 512 || i % 64 == 0)
                .map(|(_, v)| *v)
                .collect();
            compare(
                "fir",
                &samples,
                &role.f64s(&out["channel"]["fir_samples"]),
                gate,
            );
            let scores = result.scores.iter().find(|s| s.role == main.role).unwrap();
            for (when, m) in [("before", &scores.before), ("after", &scores.after)] {
                for (key, v) in metrics(m) {
                    compare(
                        &format!("{name}/{}/{when}/{key}", main.role),
                        &[v],
                        &[number(&out["metrics"][when][key])],
                        gate || (when == "before" && (oracle_libm() || key == "level")),
                    );
                }
            }
            for (key, v) in [
                ("spl_before", &scores.spl_before),
                ("spl_after", &scores.spl_after),
                ("gd_before", &scores.gd_before),
                ("gd_after", &scores.gd_after),
            ] {
                compare(
                    key,
                    v,
                    &role.f64s(&out["curves"][key]),
                    gate || key == "spl_before" || (key == "gd_before" && oracle_libm()),
                );
            }
        }
        eprintln!(
            "MEASURE design {name} elapsed={:?} gate={gate}",
            started.elapsed()
        );
    }
}
#[test]
fn simulator_matches_recorded_stages() {
    for name in ["dolby", "room_dba"] {
        let d = replay(name);
        let case = cases::load(name);
        let setup = crate::setup::Setup::new(&case.config);
        let prep = crate::measurement::prepare(&case.measurements, &case.config, &setup).unwrap();
        let filters = d
            .chain
            .subs
            .iter()
            .filter(|s| s.hp_hz > 0.0 || s.lp_hz > 0.0 || !s.peq.is_empty())
            .map(|s| {
                (
                    s.role.clone(),
                    crate::setup::SubFilter {
                        hp_hz: s.hp_hz,
                        hp_type: s.hp_type,
                        lp_hz: s.lp_hz,
                        lp_type: s.lp_type,
                        peq: s.peq.clone(),
                    },
                )
            })
            .collect::<Vec<_>>();
        let h =
            crate::measurement::long_spectra(&case.measurements, &prep, &filters, 500.0).unwrap();
        for call in Fixture::load("engine_design", &format!("{name}_stages")).calls() {
            let role = call.inputs()["role"].as_str().unwrap();
            let p = call.inputs()["position"].as_u64().unwrap() as u32;
            let s = crate::chain::stage_responses(&d, role, &h, p).unwrap();
            for (key, v) in [
                ("top_xo", &s.top_xo),
                ("combined", &s.combined),
                ("final", &s.output),
            ] {
                complex(&call, key, v, &call.outputs()[key], oracle_libm());
            }
            assert_eq!(s.sub_xo.is_some(), call.outputs().get("sub_xo").is_some());
            if let Some(sub) = s.sub_xo {
                complex(
                    &call,
                    "sub_xo",
                    &sub,
                    &call.outputs()["sub_xo"],
                    oracle_libm(),
                );
            }
            assert_eq!(
                crate::chain::input_response(&d, role, &h, p).unwrap(),
                s.output
            );
        }
        assert!(crate::chain::stage_responses(&d, "missing", &h, 1).is_none());
    }
}
