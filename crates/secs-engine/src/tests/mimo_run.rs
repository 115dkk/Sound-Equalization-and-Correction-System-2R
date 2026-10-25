//! P17a: recorded-support assembly isolates P16 fitting from run/export parity.
use super::{cases, design};
use crate::{
    C, MimoDesign, Note, Tuning,
    mimo::{
        self, IirSupport,
        run::{self, Support, Supports},
    },
};
use secs_compute::{CancelToken, Context, MemoryBudget, Progress, ProgressSink};
use secs_dsp::fft;
use secs_oracle::{Fixture, number, oracle_libm};
use secs_types::{
    calibration::{CalibrationSettings, SubEntry},
    chain::{MimoFilter, MimoPaths},
};
use serde_json::Value;
use std::sync::{Arc, Mutex};

fn pairs(v: &Value) -> impl Iterator<Item = (&str, &Value)> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| (v[0].as_str().unwrap(), &v[1]))
}
fn ideal(fx: &Fixture) -> MimoDesign {
    let v = &fx.outputs()["ideal"];
    MimoDesign {
        fs_low: number(&v["fs_low"]),
        ranges: pairs(&v["ranges"])
            .map(|(r, v)| {
                (
                    r.into(),
                    (!v.is_null()).then(|| (number(&v[0]), number(&v[1]))),
                )
            })
            .collect(),
        supports: pairs(&v["supports"])
            .map(|(r, v)| {
                (
                    r.into(),
                    pairs(v)
                        .map(|(r, taps)| (r.into(), fx.f64s(taps)))
                        .collect(),
                )
            })
            .collect(),
        level_db: pairs(&v["level_db"])
            .map(|(r, v)| (r.into(), number(v)))
            .collect(),
        notes: v["notes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .filter(|n| !n.starts_with("FIR 서포트") && !n.starts_with("입력당 서포트"))
            .map(|text| {
                let note = if text.starts_with("서포트 대역 ") {
                    crate::MimoNote::SupportRanges(
                        pairs(&v["ranges"])
                            .map(|(role, range)| {
                                (
                                    role.into(),
                                    (!range.is_null())
                                        .then(|| (number(&range[0]), number(&range[1]))),
                                )
                            })
                            .collect(),
                    )
                } else {
                    // The recorded-support fixture carries only rounded legacy
                    // guard text; this adapter is test-only, never service code.
                    let (role, text) = text.split_once(": ").unwrap();
                    if text == "MIMO 끔(악화)" {
                        crate::MimoNote::GuardDisabled { role: role.into() }
                    } else {
                        let gain_db = text
                            .strip_prefix("대역 늦은 에너지 악화로 서포트 ")
                            .unwrap()
                            .strip_suffix(" dB")
                            .unwrap()
                            .parse()
                            .unwrap();
                        crate::MimoNote::GuardReduced {
                            role: role.into(),
                            gain_db,
                        }
                    }
                };
                assert_eq!(note.oracle_text(), text);
                Note::Mimo(note)
            })
            .collect(),
    }
}
fn supports(fx: &Fixture, ideal: &MimoDesign) -> Supports {
    if fx.inputs()["fir"].as_bool().unwrap() {
        return run::fir_supports(ideal);
    }
    pairs(&fx.outputs()["supports"])
        .map(|(r, v)| {
            (
                r.into(),
                pairs(v)
                    .map(|(r, s)| {
                        (
                            r.into(),
                            Support::Iir(IirSupport {
                                gain_db: number(&s["gain_db"]),
                                delay_ms: number(&s["delay_ms"]),
                                polarity: s["polarity"].as_i64().unwrap() as i8,
                                peq: serde_json::from_value(s["peq"].clone()).unwrap(),
                                allpass: serde_json::from_value(s["allpass"].clone()).unwrap(),
                            }),
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}
fn paths(fx: &Fixture) -> MimoPaths {
    let mut v = fx.outputs()["paths"].clone();
    for path in v["paths"].as_array_mut().unwrap() {
        if let Some(fir) = path["filter"].get_mut("Fir") {
            fir["taps_48k"] = serde_json::json!(fx.f64s(&fir["taps_48k"]));
        }
    }
    serde_json::from_value(v).unwrap()
}
fn settings(fir: bool) -> CalibrationSettings {
    CalibrationSettings {
        mimo_on: true,
        mimo_fir: fir,
        ..CalibrationSettings::default()
    }
}
fn error(got: &[f64], expected: &[f64]) -> f64 {
    assert_eq!(got.len(), expected.len());
    got.iter()
        .zip(expected)
        .map(|(a, b)| (a - b).abs())
        .fold(0., f64::max)
}
fn relative(got: &[f64], expected: &[f64]) -> f64 {
    error(got, expected) / expected.iter().map(|v| v.abs()).fold(1e-30, f64::max)
}
fn measure(label: &str, got: f64, limit: f64, downstream: bool) {
    eprintln!(
        "MEASURE MIMO_RUN {label} error={got:.12e} limit={limit:.12e}{}",
        if downstream && !oracle_libm() {
            " REPORT_OFFLIBM"
        } else {
            ""
        }
    );
    assert!(got.is_finite());
    if !downstream || oracle_libm() {
        assert!(got <= limit, "{label}: {got} > {limit}");
    }
}
fn compare_paths(got: &MimoPaths, expected: &MimoPaths, tolerance: f64, downstream: bool) {
    assert_eq!(got.fsiso, expected.fsiso);
    if downstream && !oracle_libm() {
        eprintln!(
            "MIMO_RUN_OFFLIBM path order got={:?} expected={:?}",
            got.paths
                .iter()
                .map(|p| (&p.input, &p.support))
                .collect::<Vec<_>>(),
            expected
                .paths
                .iter()
                .map(|p| (&p.input, &p.support))
                .collect::<Vec<_>>()
        );
        return;
    }
    assert_eq!(got.paths.len(), expected.paths.len());
    for (g, e) in got.paths.iter().zip(&expected.paths) {
        assert_eq!((&g.input, &g.support), (&e.input, &e.support));
        measure("path low", (g.low_hz - e.low_hz).abs(), 1e-12, downstream);
        measure("path delay", (g.delay_ms - e.delay_ms).abs(), 1e-12, true);
        match (&g.filter, &e.filter) {
            (MimoFilter::Fir { taps_48k: g }, MimoFilter::Fir { taps_48k: e }) => {
                measure("path FIR", relative(g, e), tolerance, downstream)
            }
            (MimoFilter::FirLowRate { taps_low, fs_low }, MimoFilter::Fir { taps_48k }) => {
                let materialized = secs_dsp::fir::lowrate_to_48k(taps_low, *fs_low);
                measure(
                    "path FIR",
                    relative(&materialized, taps_48k),
                    tolerance,
                    downstream,
                )
            }
            (g, e) => assert_eq!(g, e),
        }
    }
}
fn compare_curves(fx: &Fixture, run: &mimo::MimoRun, tolerance: f64) {
    let indices: Vec<_> = fx
        .f64s(&fx.inputs()["indices"])
        .iter()
        .map(|v| *v as usize)
        .collect();
    for (name, got) in [("final", &run.final_resp), ("applied", &run.applied)] {
        let rows: Vec<_> = pairs(&fx.outputs()[name]).collect();
        assert_eq!(rows.len(), got.len());
        for ((r, h), (er, ev)) in got.iter().zip(rows) {
            assert_eq!(r, er);
            let e: Vec<C> = fx
                .c128s(ev)
                .iter()
                .map(|&(re, im)| C::new(re, im))
                .collect();
            let scale = e.iter().map(|v| v.norm()).fold(1e-30, f64::max);
            let diff = indices
                .iter()
                .zip(&e)
                .map(|(&k, e)| (h[k] - e).norm())
                .fold(0., f64::max)
                / scale;
            measure(
                &format!("{} {name} {r}", fx.inputs()["case"]),
                diff,
                tolerance,
                true,
            );
        }
    }
    for ((r, h), (er, ev)) in run.spl_mimo.iter().zip(pairs(&fx.outputs()["spl_mimo"])) {
        assert_eq!(r, er);
        measure(
            &format!("{} SPL {r}", fx.inputs()["case"]),
            error(h, &fx.f64s(ev)),
            tolerance,
            true,
        );
    }
}

#[test]
fn mimo_run_assemble_recorded_supports_matches_oracle() {
    for name in ["room_dba", "room_filters"] {
        let case = cases::load(name);
        let d = design::replay(name);
        for fir in [false, true] {
            let fx = Fixture::load(
                "engine_mimo_run",
                &format!("{name}_{}", mimo::kind_text(fir).to_lowercase()),
            );
            let ideal = ideal(&fx);
            let key = mimo::MimoKey::from_settings(&settings(fir));
            let run = run::assemble(
                &d,
                &case.measurements,
                &case.config,
                &ideal,
                supports(&fx, &ideal),
                key,
                &Context::serial(),
            )
            .unwrap();
            compare_paths(&run.paths, &paths(&fx), 1e-12, false);
            compare_curves(&fx, &run, 1e-9);
            assert_eq!(
                serde_json::json!([
                    run.key.fsiso,
                    run.key.f_low,
                    run.key.exclude,
                    run.key.max_supports,
                    run.key.fir
                ]),
                fx.outputs()["key"]
            );
            let notes: Vec<_> = run
                .notes
                .iter()
                .map(|n| match n {
                    Note::Mimo(note) => note.oracle_text(),
                    _ => panic!("unexpected note"),
                })
                .collect();
            assert_eq!(serde_json::json!(notes), fx.outputs()["notes"]);
            for row in fx.outputs()["budgets"].as_array().unwrap() {
                assert_eq!(
                    mimo::fir_budget_text(&run, row[0].as_u64().unwrap() as usize),
                    row[1].as_str().unwrap()
                );
            }
        }
    }
}

#[derive(Debug, Default)]
struct Events(Mutex<Vec<Progress>>);
impl ProgressSink for Events {
    fn report(&self, p: &Progress) {
        self.0.lock().unwrap().push(p.clone());
    }
}
#[test]
fn mimo_run_compute_fir_and_iir_end_to_end() {
    for name in ["room_dba", "room_filters"] {
        let case = cases::load(name);
        let d = design::replay(name);
        for fir in [false, true] {
            let fx = Fixture::load(
                "engine_mimo_run",
                &format!("{name}_{}", mimo::kind_text(fir).to_lowercase()),
            );
            let events = Arc::new(Events::default());
            let ctx = Context::new(
                1,
                MemoryBudget::new(512 * 1024 * 1024),
                events.clone(),
                CancelToken::new(),
            );
            // Invalid IIR tuning makes any accidental FIR-mode IIR call fail.
            let tuning = Tuning {
                mimo_max_nfev: if fir {
                    0
                } else {
                    Tuning::default().mimo_max_nfev
                },
                ..Tuning::default()
            };
            let run = mimo::compute(
                &d,
                &case.measurements,
                &case.config,
                &settings(fir),
                &tuning,
                &ctx,
            )
            .unwrap();
            let expected = paths(&fx);
            if fir {
                // P14 uses 3 * max(spread, 1e-9 * max(1, |reference|)).
                // These reference-only run fixtures use that 3e-9 absolute floor
                // for ideal taps; response comparisons allow its propagated error.
                let recorded = ideal(&fx);
                if oracle_libm() {
                    assert_eq!(run.ideal.ranges, recorded.ranges);
                }
                for ((r, s), (er, es)) in run.ideal.supports.iter().zip(&recorded.supports) {
                    assert_eq!(r, er);
                    if oracle_libm() {
                        assert_eq!(s.len(), es.len());
                        for ((r, t), (er, et)) in s.iter().zip(es) {
                            assert_eq!(r, er);
                            measure("computed ideal FIR", error(t, et), 3e-9, true);
                        }
                    }
                }
                compare_paths(&run.paths, &expected, 3e-7, true);
                compare_curves(&fx, &run, 3e-7);
                assert!(run.paths.paths.iter().all(
                    |p| matches!(&p.filter, MimoFilter::FirLowRate { taps_low, fs_low } if taps_low.len() == 100 && *fs_low == 600.0)
                ));
                assert!(
                    run.paths
                        .paths
                        .iter()
                        .all(|p| p.filter.fir_len_48k() == 8000)
                );
            } else {
                let got: Vec<_> = run
                    .paths
                    .paths
                    .iter()
                    .map(|p| (&p.input, &p.support))
                    .collect();
                let want: Vec<_> = expected
                    .paths
                    .iter()
                    .map(|p| (&p.input, &p.support))
                    .collect();
                // IIR post-processing drops paths by fitted values, which match
                // SECS 2 within its spread (P16's gate), not bit for bit, so the
                // set is reported. Asserted: every path is one of its input's
                // ideal supports, in the ideal design's order.
                eprintln!("MEASURE MIMO_RUN {name} IIR paths got={got:?} reference={want:?}");
                let ideal: Vec<_> = run
                    .ideal
                    .supports
                    .iter()
                    .flat_map(|(i, s)| s.iter().map(move |(j, _)| (i, j)))
                    .collect();
                let mut rest = ideal.iter();
                for path in &got {
                    assert!(
                        rest.any(|p| p == path),
                        "{name}: IIR path {path:?} is not an ideal support in order {ideal:?}"
                    );
                }
            }
            assert_eq!(
                run.applied.iter().map(|(r, _)| r).collect::<Vec<_>>(),
                d.chain.mains.iter().map(|m| &m.role).collect::<Vec<_>>()
            );
            assert!(
                run.applied
                    .iter()
                    .flat_map(|(_, v)| v)
                    .all(|v| v.re.is_finite() && v.im.is_finite())
            );
            assert!(
                run.spl_mimo
                    .iter()
                    .flat_map(|(_, v)| v)
                    .all(|v| v.is_finite())
            );
            for ((r, a), (_, b)) in run.applied.iter().zip(&run.final_resp) {
                if !run.paths.paths.iter().any(|p| p.input == *r) {
                    assert_eq!(a, b);
                }
            }
            let events = events.0.lock().unwrap();
            assert!(
                events
                    .iter()
                    .any(|p| p.stage == "engine.mimo" && p.done == 2 && p.total == 2)
            );
            if fir {
                assert!(!events.iter().any(|p| p.stage == "engine.mimo_iir"));
            }
        }
    }
}

#[test]
fn mimo_run_fir_to_48k_and_response_match_oracle() {
    let fx = Fixture::load("engine_mimo_run", "fir");
    let taps = fx.f64s(&fx.inputs()["taps"]);
    assert_eq!(taps.len(), 100);
    let h = mimo::fir_to_48k(&taps, 600.);
    assert_eq!(h.len(), 8000);
    measure(
        "fir_to_48k",
        relative(&h, &fx.f64s(&fx.outputs()["taps_48k"])),
        1e-12,
        false,
    );
    let f: Vec<_> = (0..=16384).map(|k| k as f64 * 48000. / 32768.).collect();
    let response = run::fir_response(&taps, 600., &f);
    let expected = fx.c128s(&fx.outputs()["response"]);
    let diff = response
        .iter()
        .zip(&expected)
        .map(|(g, &(re, im))| (*g - C::new(re, im)).norm())
        .fold(0., f64::max);
    measure("FIR DTFT", diff, 1e-12, false);
    let mut padded = h.clone();
    padded.resize(32768, 0.0);
    let want = fft::rfft(&padded);
    let scale = want
        .iter()
        .zip(&f)
        .filter(|(_, f)| **f < 250.)
        .map(|(h, _)| h.norm())
        .fold(0., f64::max);
    let diff = response
        .iter()
        .zip(&want)
        .zip(&f)
        .filter(|(_, f)| **f < 250.)
        .map(|((a, b), _)| (*a - *b).norm())
        .fold(0., f64::max)
        / scale;
    measure("FIR DTFT vs exported FFT below 250Hz", diff, 0.01, false);
    assert!(
        response
            .iter()
            .zip(&f)
            .filter(|(_, f)| **f >= 300.)
            .all(|(h, _)| *h == C::new(0., 0.))
    );
}

#[test]
fn mimo_run_helpers_settings_and_cancellation() {
    let fx = Fixture::load("engine_mimo_run", "helpers");
    for n in 0..5 {
        assert_eq!(
            serde_json::json!(mimo::availability(n)),
            fx.outputs()["availability"][n]
        );
    }
    for v in fx.outputs()["caps"].as_array().unwrap() {
        assert_eq!(
            mimo::cap_text(v[0].as_u64().unwrap() as usize),
            v[1].as_str().unwrap()
        );
    }
    for (k, fir) in [false, true].iter().enumerate() {
        assert_eq!(
            mimo::kind_text(*fir),
            fx.outputs()["kinds"][k].as_str().unwrap()
        );
    }
    for v in fx.outputs()["estimates"].as_array().unwrap() {
        assert_eq!(
            mimo::estimate_seconds(
                v[0].as_u64().unwrap() as usize,
                v[1].as_u64().unwrap() as usize,
                v[2].as_u64().unwrap() as usize,
                v[3].as_bool().unwrap()
            ),
            number(&v[4])
        );
    }
    for (raw, want) in [(-3, 0), (0, 0), (4, 4), (99, 12)] {
        let mut s = settings(true);
        s.mimo_max_supports = raw;
        let key = mimo::MimoKey::from_settings(&s);
        assert_eq!(key.max_supports, want);
        assert_eq!(key.request().max_supports, (want != 0).then_some(want));
    }
    let mut s = settings(true);
    s.fsiso = 200.;
    s.mimo_low = Some(1.);
    s.subs_manual = vec![
        (
            "SW2".into(),
            SubEntry {
                mimo_support: false,
                ..SubEntry::default()
            },
        ),
        (
            "SW1".into(),
            SubEntry {
                mimo_support: false,
                ..SubEntry::default()
            },
        ),
    ];
    let key = mimo::MimoKey::from_settings(&s);
    assert_eq!(
        (key.fsiso, key.f_low, key.exclude),
        (150., Some(15.), vec!["SW1".to_string(), "SW2".to_string()])
    );
    let case = cases::load("room_dba");
    let mut d = design::replay("room_dba");
    let ctx = Context::serial();
    let off = CalibrationSettings::default();
    assert!(
        mimo::compute(
            &d,
            &case.measurements,
            &case.config,
            &off,
            &Tuning::default(),
            &ctx
        )
        .is_err()
    );
    d.spectra.positions.truncate(2);
    assert!(
        mimo::compute(
            &d,
            &case.measurements,
            &case.config,
            &s,
            &Tuning::default(),
            &ctx
        )
        .is_err()
    );
    ctx.cancel().cancel();
    assert!(matches!(
        mimo::compute(
            &d,
            &case.measurements,
            &case.config,
            &s,
            &Tuning::default(),
            &ctx
        ),
        Err(crate::EngineError::Cancelled)
    ));
    assert!(matches!(
        run::assemble(
            &d,
            &case.measurements,
            &case.config,
            &MimoDesign::default(),
            Vec::new(),
            mimo::MimoKey::from_settings(&s),
            &ctx
        ),
        Err(crate::EngineError::Cancelled)
    ));
}

#[test]
fn mimo_run_empty_supports_preserve_final_and_fallback_paths() {
    let case = cases::load("room_dba");
    let d = design::replay("room_dba");
    let key = mimo::MimoKey::from_settings(&settings(false));
    let ctx = Context::serial();
    let run = run::assemble(
        &d,
        &case.measurements,
        &case.config,
        &MimoDesign::default(),
        Vec::new(),
        key.clone(),
        &ctx,
    )
    .unwrap();
    assert_eq!(run.final_resp, run.applied);
    let sp = Support::Iir(IirSupport {
        gain_db: -20.,
        delay_ms: 3.,
        polarity: -1,
        peq: vec![],
        allpass: vec![],
    });
    let supports = vec![(
        "R".into(),
        vec![("SW2".into(), sp.clone()), ("L".into(), sp)],
    )];
    let run = run::assemble(
        &d,
        &case.measurements,
        &case.config,
        &MimoDesign::default(),
        supports,
        key,
        &ctx,
    )
    .unwrap();
    assert_eq!(
        run.paths
            .paths
            .iter()
            .map(|p| p.support.as_str())
            .collect::<Vec<_>>(),
        vec!["SW2", "L"]
    );
    assert!(run.paths.paths.iter().all(|p| p.low_hz == 50.));
    for ((r, a), (_, b)) in run.applied.iter().zip(&run.final_resp) {
        if r != "R" {
            assert_eq!(a, b);
        }
    }
}

#[test]
fn mimo_compute_shared_preparation_bit_identical() {
    let case = cases::load("room_dba");
    let d = design::replay("room_dba");
    let ctx = Context::serial();
    for fir in [false, true] {
        let settings = settings(fir);
        let key = mimo::MimoKey::from_settings(&settings);
        let req = key.request();
        let ideal = mimo::design::design_mimo_before(
            &d,
            &case.measurements,
            &case.config,
            &req,
            &ctx,
            None,
        )
        .unwrap()
        .0;
        let supports = if fir {
            run::fir_supports(&ideal)
        } else {
            let prepared =
                mimo::prepared::Prepared::before(&d, &case.measurements, &case.config).unwrap();
            mimo::iir::design_mimo_iir_prepared(&d, &prepared, &ideal, &req, &case.tuning, &ctx)
                .unwrap()
                .supports
                .into_iter()
                .map(|(r, s)| {
                    (
                        r,
                        s.into_iter().map(|(r, s)| (r, Support::Iir(s))).collect(),
                    )
                })
                .collect()
        };
        let before = run::assemble(
            &d,
            &case.measurements,
            &case.config,
            &ideal,
            supports,
            key,
            &ctx,
        )
        .unwrap();
        let after = mimo::compute(
            &d,
            &case.measurements,
            &case.config,
            &settings,
            &case.tuning,
            &ctx,
        )
        .unwrap();
        assert_eq!(before, after, "fir={fir}");
        assert_eq!(run_bits(&before), run_bits(&after), "fir={fir} bits");
    }
}

fn run_bits(run: &mimo::MimoRun) -> Vec<u64> {
    let mut bits = super::mimo::design_bits(&run.ideal);
    let mut values = vec![run.key.fsiso, run.paths.fsiso];
    values.extend(run.key.f_low);
    for path in &run.paths.paths {
        values.extend([path.low_hz, path.delay_ms]);
        match &path.filter {
            MimoFilter::Fir { taps_48k } => values.extend(taps_48k),
            MimoFilter::FirLowRate { taps_low, fs_low } => {
                values.push(*fs_low);
                values.extend(taps_low);
            }
            MimoFilter::Iir {
                gain_db,
                delay_ms,
                peq,
                allpass,
                ..
            } => {
                values.extend([*gain_db, *delay_ms]);
                for &(f, q, g) in peq {
                    values.extend([f, q, g]);
                }
                for &(f, q) in allpass {
                    values.extend([f, q]);
                }
            }
        }
    }
    for (_, response) in run.final_resp.iter().chain(&run.applied) {
        for v in response {
            values.extend([v.re, v.im]);
        }
    }
    for (_, spl) in &run.spl_mimo {
        values.extend(spl);
    }
    bits.extend(values.into_iter().map(f64::to_bits));
    bits
}

#[test]
fn iir_acceleration_request_keeps_cpu_target_and_outputs_bit_identical() {
    use secs_types::compute::{ComputeBackend, ComputePreference, ComputeReason, ComputeStatus};
    #[derive(Debug)]
    struct MustNotFactor;
    impl mimo::RidgeBackend for MustNotFactor {
        fn factor<'a>(
            &'a self,
            _: &'a [f64],
            _: &'a [f64],
            _: &CancelToken,
        ) -> Result<Box<dyn mimo::ShiftedSystem + 'a>, mimo::AccelerationError> {
            panic!("IIR must retain the CPU reference target");
        }
    }
    let case = cases::load("room");
    let d = design::replay("room");
    let ctx = Context::serial();
    let cpu = mimo::AccelerationSelection {
        status: ComputeStatus {
            preference: ComputePreference::Cpu,
            reason: ComputeReason::UserCpu,
            ..ComputeStatus::default()
        },
        backend: None,
    };
    let expected = mimo::compute_with_acceleration(
        &d,
        &case.measurements,
        &case.config,
        &settings(false),
        &case.tuning,
        &ctx,
        &cpu,
    )
    .unwrap();
    let gpu = mimo::AccelerationSelection {
        status: ComputeStatus {
            preference: ComputePreference::Gpu,
            selected: ComputeBackend::Cuda,
            gpu_available: true,
            reason: ComputeReason::UserGpu,
            ..ComputeStatus::default()
        },
        backend: Some(Arc::new(MustNotFactor)),
    };
    let mut actual = mimo::compute_with_acceleration(
        &d,
        &case.measurements,
        &case.config,
        &settings(false),
        &case.tuning,
        &ctx,
        &gpu,
    )
    .unwrap();
    assert_eq!(actual.compute.requested, ComputePreference::Gpu);
    assert_eq!(actual.compute.backend, ComputeBackend::Cpu);
    assert_eq!(actual.compute.reason, Some(ComputeReason::IirReference));
    assert_eq!(actual.compute.accelerated_systems, 0);
    assert_eq!(actual.compute.cpu_systems, expected.compute.cpu_systems);
    assert!(actual.compute.cpu_systems > 0);
    actual.compute = expected.compute.clone();
    assert_eq!(actual, expected);
    assert_eq!(run_bits(&actual), run_bits(&expected));
}

#[test]
fn mimo_preparation_budget_refuses_before_spectra() {
    let case = cases::load("room_dba");
    let d = design::replay("room_dba");
    let (peak, held) = mimo::prepared::Prepared::bytes(&case.measurements, &case.config);
    assert!(held > 0 && peak > 2 * held);
    let ctx = Context::new(
        2,
        MemoryBudget::new(peak - 1),
        Arc::new(secs_compute::NoProgress),
        CancelToken::new(),
    );
    let built = crate::measurement::SPECTRA_BUILT.with(std::cell::Cell::get);
    let result = mimo::compute(
        &d,
        &case.measurements,
        &case.config,
        &settings(true),
        &case.tuning,
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
        "MIMO built spectra before refusing its preparation"
    );
    assert_eq!(ctx.budget().available(), ctx.budget().total());
}
