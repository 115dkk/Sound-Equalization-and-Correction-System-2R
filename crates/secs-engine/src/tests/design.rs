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
        .filter(|(i, f)| (15.0..=600.0).contains(*f) || i % 257 == 0)
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
pub(super) fn replay(name: &str) -> Design {
    let case = cases::load(name);
    let mut stages: BTreeMap<String, Vec<RecordedFit>> = BTreeMap::new();
    for call in Fixture::all_calls("engine_design", &format!("{name}_fits_")) {
        stages
            .entry(call.inputs()["stage"].as_str().unwrap().into())
            .or_default()
            .push(read_recorded_fit(
                &call,
                call.inputs(),
                &call.outputs()["result"],
            ));
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
        &Context::serial(),
        &*factory,
    )
    .unwrap();
    for (key, (actual, expected)) in counts.lock().unwrap().iter() {
        assert_eq!(actual, expected, "{name} {key} count");
    }
    result
}
#[test]
fn design_empty_subs_and_subs_only_keep_axes_and_shared_spectra() {
    let mut c = cases::load("room_manual");
    let notes = vec!["adapter note".to_owned()];
    c.config.subs.clear();
    c.config.sub_manual.clear();
    c.config.bundle_layouts.clear();
    let run = design::design(
        &DesignRequest {
            measurements: &c.measurements,
            config: &c.config,
            tuning: &c.tuning,
            subs_only: true,
            adapter_notes: &notes,
        },
        &Context::serial(),
        &*design::production_fitters(),
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
        thread,
        time::{Duration, Instant},
    };
    let case = cases::load("dolby");
    let token = CancelToken::new();
    let context = Context::new(
        2,
        MemoryBudget::new(u64::MAX),
        Arc::new(NoProgress),
        token.clone(),
    );
    thread::scope(|scope| {
        let canceller = scope.spawn(|| {
            thread::sleep(Duration::from_millis(150));
            token.cancel();
            Instant::now()
        });
        let result = design::design(
            &DesignRequest {
                measurements: &case.measurements,
                config: &case.config,
                tuning: &case.tuning,
                subs_only: false,
                adapter_notes: &[],
            },
            &context,
            &*design::production_fitters(),
        );
        let cancelled_at = canceller.join().unwrap();
        assert!(matches!(result, Err(crate::EngineError::Cancelled)));
        eprintln!("MEASURE design cancellation {:?}", cancelled_at.elapsed());
        assert!(cancelled_at.elapsed() < Duration::from_secs(1));
    });
}

#[test]
fn design_matches_recorded_stages() {
    for name in cases::names() {
        let started = std::time::Instant::now();
        let result = replay(&name);
        let fx = Fixture::load("engine_design", &name);
        let o = fx.outputs();
        let gate = name != "room" && oracle_libm();
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
            })
            .collect();
        if gate {
            assert_eq!(serde_json::to_value(kinds).unwrap(), o["note_kinds"]);
        }
        let sub = result.subs.as_ref().unwrap();
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
        for (s, expected) in result.chain.subs.iter().zip(o["subs"].as_array().unwrap()) {
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
            if let Some(p) = &main.top_response {
                complex(&role, "top_P", p, &out["top_P"], gate);
            }
            let channel = result
                .chain
                .mains
                .iter()
                .find(|c| c.role == main.role)
                .unwrap();
            if gate {
                assert_eq!(channel.xo_hz, out["channel"]["xo_hz"].as_f64());
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
                        gate || when == "before",
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
                    gate || key.ends_with("before"),
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
