use std::sync::Arc;

use secs_compute::Context;
use secs_dsp::{
    fft,
    iir::{BType, butter, sosfilt},
    response,
};
use secs_types::{
    calibration::{CalibrationSettings, Mode, SubEntry},
    chain::BandType,
    target::TargetCurve,
};

use crate::{
    C, DesignRequest, EngineConfig, Measurement, MeasurementSet, Note, Samples, Tuning,
    calibration::{ProjectView, engine_inputs},
    grid::{BINS, N_FFT, SR, SR_F, design_freqs},
    low_protect,
    rc::{self, RcInput},
    setup::Setup,
};

fn ir(hp: f64) -> Measurement {
    let mut samples = vec![0.0; N_FFT];
    samples[0] = 1.0;
    if hp > 0.0 {
        samples = sosfilt(&butter(4, &[hp], BType::Highpass, SR_F).unwrap(), &samples).unwrap();
    }
    Measurement {
        samples: Samples::F64(samples.into()),
        sample_rate: SR,
        t0: 0.0,
    }
}

fn spectrum(hp: f64) -> Arc<[C]> {
    let m = ir(hp);
    // The synthetic measurement is transformed, not an analytic magnitude stub.
    let Samples::F64(samples) = m.samples else {
        unreachable!()
    };
    fft::rfft(&samples).into()
}

fn inputs(
    main_hp: f64,
    sub_hp: Option<f64>,
    settings: &CalibrationSettings,
) -> (MeasurementSet, EngineConfig) {
    let mut roles = vec![("L".into(), ir(main_hp))];
    let mut groups = vec![("front".into(), vec!["L".into()])];
    if let Some(hp) = sub_hp {
        roles.push(("SW1".into(), ir(hp)));
        groups.push(("sub".into(), vec!["SW1".into()]));
    }
    let set = MeasurementSet {
        positions: vec![(1, roles)],
    };
    let config = engine_inputs(
        &ProjectView {
            groups: &groups,
            project_xo: &[("front".into(), 80.0)],
            target: &TargetCurve::new(vec![(10.0, 0.0), (20000.0, 0.0)]),
        },
        &set,
        settings,
    )
    .unwrap()
    .config;
    (set, config)
}

fn max_below(h: &[C], hz: f64) -> f64 {
    design_freqs()
        .iter()
        .zip(h)
        .filter(|(f, _)| **f >= 10.0 && **f < hz)
        .map(|(_, &h)| 20.0 * response::magnitude(h).max(1e-30).log10())
        .fold(f64::NEG_INFINITY, f64::max)
}

#[test]
fn low_protect_rolloff_uses_weighted_power_and_filtered_passband() {
    let h = spectrum(30.0);
    let hz = low_protect::rolloff(&[(1, h.clone())], 1, Some((0.0, 0.0))).unwrap();
    // A fourth-order Butterworth has |H|²=1/(1+(fc/f)^8).
    // At -10 dB: f=30/9^(1/8)=22.795070570 Hz. Smoothing and the
    // finite reference band shift this by less than 1 Hz.
    let expected = 30.0 / 9.0_f64.powf(1.0 / 8.0);
    eprintln!("30 Hz BW24: rolloff={hz:.9}, expected={expected:.9}, tolerance=1 Hz");
    assert!((hz - expected).abs() < 1.0);
    assert!(low_protect::rolloff(&[(1, spectrum(0.0))], 1, Some((0.0, 0.0))).is_none());
    let anti: Arc<[C]> = h.iter().map(|v| -*v).collect();
    assert_eq!(
        low_protect::rolloff(&[(1, h.clone()), (2, anti)], 1, Some((0.0, 0.0))),
        Some(hz)
    );
    let flat = spectrum(0.0);
    let weighted = low_protect::rolloff(&[(1, h.clone()), (2, flat.clone())], 1, None);
    let repeated = low_protect::rolloff(
        &[
            (1, h.clone()),
            (2, h.clone()),
            (3, h.clone()),
            (4, h.clone()),
            (5, h),
            (6, flat),
        ],
        99,
        None,
    );
    assert_eq!(weighted, repeated);
    // A high user corner needs the passband maximum, not the 40–80 Hz median.
    let hz = low_protect::rolloff(&[(1, spectrum(100.0))], 1, Some((100.0, 0.0))).unwrap();
    assert!(hz > 65.0 && hz < 85.0, "{hz}");
}

#[test]
fn low_protect_realized_guard_reduces_boosts_and_cuts_fixed_leakage() {
    let unity = vec![C::new(1.0, 0.0); BINS];
    let mut peq = vec![(30.0, 1.0, 6.0)];
    let before = max_below(&response::peq_response(design_freqs(), &peq), 25.0);
    assert!(before > 5.0);
    let outcome = low_protect::limit(&mut peq, &unity, 25.0);
    assert_eq!(
        outcome,
        low_protect::LimitOutcome {
            reduced: 1,
            cuts: 0
        }
    );
    let maximum = max_below(&response::peq_response(design_freqs(), &peq), 25.0);
    eprintln!("leaking boost: before={before:.9}, after={maximum:.9}, {outcome:?}");
    assert!(maximum <= 0.5 && maximum > 0.499999, "{maximum}");
    assert!(peq[0].2 > 0.0 && peq[0].2 < 6.0);

    let original = vec![(30.0, 1.0, 6.0), (200.0, 1.0, 2.0), (18.0, 1.0, -1.0)];
    // Fixed low-frequency leakage cannot be removed by lowering fitted boosts.
    // Unlike constant gain, this response has unity DC and can be cut by PEQ.
    let fixed = response::peq_response(design_freqs(), &[(22.0, 1.4, 6.0)]);
    let combined = |peq: &[_]| {
        response::peq_response(design_freqs(), peq)
            .iter()
            .zip(&fixed)
            .map(|(a, b)| a * b)
            .collect::<Vec<_>>()
    };
    let before = max_below(&combined(&original), 25.0);
    let mut guarded = original.clone();
    let outcome = low_protect::limit(&mut guarded, &fixed, 25.0);
    let maximum = max_below(&combined(&guarded), 25.0);
    let limited = low_protect::limited(&guarded, &fixed, 25.0);
    eprintln!(
        "fixed leakage: before={before:.9}, after={maximum:.9}, limited={limited}, {outcome:?}"
    );
    assert!(outcome.cuts > 0 && outcome.cuts <= 4);
    assert!(guarded[original.len()..].iter().all(|band| band.0 >= 10.0));
    assert!(limited);
    assert!(maximum <= 0.5);
    assert_eq!(guarded[2], original[2]);
    assert_ne!(guarded, original);
}

#[test]
fn low_protect_keeps_boosts_that_do_not_leak() {
    let mut peq = vec![(22.0, 1.7, 5.2), (60.0, 10.0, 6.0)];
    let original = peq.clone();
    let rolloff = 21.0;
    let leakage = max_below(&response::peq_response(design_freqs(), &peq[1..]), rolloff);
    assert!(leakage < 0.05, "{leakage}");
    let outcome = low_protect::limit(&mut peq, &vec![C::new(1.0, 0.0); BINS], rolloff);
    let maximum = max_below(&response::peq_response(design_freqs(), &peq), rolloff);
    eprintln!(
        "targeted reduction: before={original:?}, after={peq:?}, 60 Hz leakage={leakage:.9}, max={maximum:.9}, {outcome:?}"
    );
    assert_eq!(
        outcome,
        low_protect::LimitOutcome {
            reduced: 1,
            cuts: 0
        }
    );
    assert!(peq[0].2 < original[0].2);
    assert_eq!(peq[1].0.to_bits(), original[1].0.to_bits());
    assert_eq!(peq[1].1.to_bits(), original[1].1.to_bits());
    assert_eq!(peq[1].2.to_bits(), original[1].2.to_bits());
    assert!(maximum <= 0.5);
}

#[test]
fn low_protect_dc_does_not_stop_leaking_boost_reduction() {
    let mut fixed = vec![C::new(1.0, 0.0); BINS];
    fixed[0] = C::new(10.0, 0.0);
    let mut peq = vec![(30.0, 1.0, 6.0)];
    let outcome = low_protect::limit(&mut peq, &fixed, 25.0);
    assert_eq!(outcome.reduced, 1);
    assert_eq!(outcome.cuts, 0);
    assert!(low_protect::limited(&peq, &fixed, 25.0));
    assert!(peq[0].2 > 0.0 && peq[0].2 < 6.0);
    // The FIR owns DC correction; a PEQ check must not stop on unchangeable DC.
    assert_eq!(fixed[0], C::new(10.0, 0.0));
}

#[test]
fn low_protect_rc_realized_response_in_peq_and_fir_modes() {
    for mode in [Mode::Zero, Mode::Mixed, Mode::Linear, Mode::Ultra] {
        let settings = CalibrationSettings {
            low_protect: true,
            mode,
            rc_hi: Some(500.0),
            ..Default::default()
        };
        let (_, mut config) = inputs(40.0, None, &settings);
        config.peq_bands = Some(2);
        let setup = Setup::new(&config);
        let tuning = Tuning {
            gd_cuts: false,
            ..Default::default()
        };
        let comb = [(1, spectrum(40.0))];
        let input = RcInput {
            bus_rolloff_hz: None,
            role: "L",
            comb: &comb,
            main_position: 1,
            level_db: 0.0,
            setup: &setup,
            config: &config,
            tuning: &tuning,
            positions: &[1],
        };
        let out = rc::channel(&input, &mut crate::fit::fit_peq, &|| false).unwrap();
        let (hz, limited) = out
            .out
            .notes
            .iter()
            .find_map(|n| match n {
                Note::LowProtect {
                    rolloff_hz,
                    hp_hz,
                    limited,
                    ..
                } => {
                    assert_eq!(*hp_hz, None);
                    Some((*rolloff_hz, *limited))
                }
                _ => None,
            })
            .unwrap();
        eprintln!(
            "main 40 Hz mode={mode:?}, rolloff={hz:.9}, max={:.9}, limited={limited}",
            max_below(&out.response, hz)
        );
        assert!((hz - 40.0 / 9.0_f64.powf(1.0 / 8.0)).abs() < 1.0);
        assert!(
            out.out
                .design_db
                .iter()
                .zip(design_freqs())
                .filter(|(_, f)| **f < hz)
                .all(|(db, _)| *db <= 0.0)
        );
        assert!(limited);
        assert!(20.0 * out.out.fir.iter().sum::<f64>().abs().max(1e-30).log10() <= 0.5 + 1e-10);
        assert!(max_below(&out.response, hz) <= 0.5 + 1e-10);
        assert!(
            out.out
                .peq
                .iter()
                .filter(|(f, _, _)| *f < hz)
                .all(|(_, _, g)| *g <= 0.0)
        );
    }
}

#[test]
fn low_protect_sub_export_bus_bundle_and_user_highpass() {
    for (user_hp, bundle) in [(false, false), (true, false), (false, true)] {
        let mut settings = CalibrationSettings {
            mode: Mode::Zero,
            low_protect: true,
            xo_peq_on: false,
            ..Default::default()
        };
        if user_hp || bundle {
            settings.subs_manual.push((
                "SW1".into(),
                SubEntry {
                    hp_hz: if user_hp { 30.0 } else { 0.0 },
                    hp_type: "LR24".into(),
                    mode: if bundle { "bundle" } else { "auto" }.into(),
                    bundle: "A".into(),
                    ..Default::default()
                },
            ));
        }
        if bundle {
            settings.bundle_layouts.push((
                "A".into(),
                secs_types::calibration::BundleLayoutEntry {
                    kind: "multi".into(),
                    rear_trim_ms: 0.0,
                },
            ));
        }
        let (set, config) = inputs(0.0, Some(if user_hp { 0.0 } else { 30.0 }), &settings);
        let tuning = Tuning {
            gd_cuts: false,
            ..Default::default()
        };
        // The leakage regression is injected at every fit. This verifies that
        // bus and bundle fit callers enforce the actual returned response.
        let fits = |stage: crate::FitStage<'_>| -> Box<crate::fit::Fitter<'_>> {
            let ba = matches!(stage, crate::FitStage::Ba);
            Box::new(move |request| {
                if ba {
                    assert!(request.lo > 20.0);
                }
                vec![(30.0, 1.0, 6.0)]
            })
        };
        let design = crate::design(
            &DesignRequest {
                measurements: &set,
                config: &config,
                tuning: &tuning,
                subs_only: false,
                adapter_notes: &[],
            },
            &Context::serial(),
            &fits,
        )
        .unwrap();
        let sub = &design.chain.subs[0];
        let hz = design
            .notes
            .iter()
            .find_map(|n| match n {
                Note::LowProtect {
                    role,
                    rolloff_hz,
                    hp_hz: Some(hp),
                    ..
                } if role == "SW1" => {
                    assert_eq!(*hp, sub.hp_hz);
                    Some(*rolloff_hz)
                }
                _ => None,
            })
            .unwrap();
        eprintln!(
            "sub user_hp={user_hp} bundle={bundle}: rolloff={hz:.9}, hp={:.9}",
            sub.hp_hz
        );
        if user_hp {
            assert_eq!(sub.hp_hz, 30.0);
        } else {
            assert_eq!(sub.hp_hz, hz / 2.0_f64.sqrt());
        }
        assert_eq!(sub.hp_type, BandType::Lr24);
        // BW2 squared is the existing LR24 export representation.
        let hp = response::band_filter(&[sub.hp_hz], sub.hp_hz, "HP", sub.hp_type.name()).unwrap();
        assert!((20.0 * response::magnitude(hp[0]).log10() + 6.020599913).abs() < 1e-8);
        // The bus sees the measured sub with any bundle PEQ, but no automatic HP.
        let used = design.spectra.used.get(1, "SW1").unwrap();
        let bus_rolloff = low_protect::rolloff(&[(1, used.clone())], 1, Some((0.0, 0.0))).unwrap();
        if !user_hp {
            let raw = design.spectra.raw.get(1, "SW1").unwrap();
            let peq_response = response::peq_response(design_freqs(), &sub.peq);
            for ((&raw, &used), p) in raw.iter().zip(used.iter()).zip(peq_response) {
                assert!((response::magnitude(raw * p) - response::magnitude(used)).abs() < 1e-10);
            }
        }
        if bundle {
            assert!(!sub.peq.is_empty());
        }
        for main in &design.mains {
            if let Some((rolloff, limited)) = design.notes.iter().find_map(|note| match note {
                Note::LowProtect {
                    role,
                    rolloff_hz,
                    limited,
                    ..
                } if role == &main.role => Some((*rolloff_hz, *limited)),
                _ => None,
            }) {
                assert!(limited);
                assert!(max_below(&main.response, rolloff) <= 0.5 + 1e-9);
            }
        }
        let baseline = if user_hp {
            let raw: Arc<[C]> = response::lr4(design_freqs(), 30.0, "HP").into();
            low_protect::rolloff(&[(1, raw)], 1, Some((0.0, 0.0))).unwrap()
        } else {
            low_protect::rolloff(&[(1, spectrum(30.0))], 1, Some((0.0, 0.0))).unwrap()
        };
        if let Some(bus) = &design.subs.as_ref().unwrap().bus_response {
            assert!(max_below(bus, bus_rolloff) <= 0.5 + 1e-9);
        }
        assert!(
            max_below(&response::peq_response(design_freqs(), &sub.peq), baseline) <= 0.5 + 1e-9
        );
    }
}

#[test]
fn low_protect_flat_channels_leave_the_design_unchanged() {
    let settings = CalibrationSettings {
        mode: Mode::Zero,
        ..Default::default()
    };
    let (set, config) = inputs(0.0, Some(0.0), &settings);
    let tuning = Tuning {
        gd_cuts: false,
        ..Default::default()
    };
    let fits = |_: crate::FitStage<'_>| -> Box<crate::fit::Fitter<'_>> { Box::new(|_| vec![]) };
    let run = |config: &EngineConfig| {
        crate::design(
            &DesignRequest {
                measurements: &set,
                config,
                tuning: &tuning,
                subs_only: false,
                adapter_notes: &[],
            },
            &Context::serial(),
            &fits,
        )
        .unwrap()
    };
    let off = run(&config);
    let mut on_config = config;
    on_config.low_protect = true;
    let on = run(&on_config);
    assert_eq!(off.chain, on.chain);
    assert_eq!(off.notes, on.notes);
}

#[test]
fn low_protect_mains_use_the_bus_rolloff() {
    let settings = CalibrationSettings {
        mode: Mode::Zero,
        low_protect: true,
        ..Default::default()
    };
    let (_, mut config) = inputs(0.0, Some(30.0), &settings);
    config.peq_bands = Some(2);
    assert_eq!(config.xo[0].1, 80.0);
    let comb = [(1, spectrum(0.0))];
    assert!(low_protect::rolloff(&comb, 1, None).is_none());
    let bus = low_protect::rolloff(&[(1, spectrum(30.0))], 1, Some((0.0, 0.0))).unwrap();
    let setup = Setup::new(&config);
    let tuning = Tuning {
        gd_cuts: false,
        ..Default::default()
    };
    let input = RcInput {
        role: "L",
        comb: &comb,
        main_position: 1,
        level_db: 0.0,
        setup: &setup,
        config: &config,
        tuning: &tuning,
        positions: &[1],
        bus_rolloff_hz: Some(bus),
    };
    let mut fitter = |_: &crate::fit::FitRequest<'_>| vec![(30.0, 1.0, 6.0)];
    let unprotected = rc::channel(
        &RcInput {
            bus_rolloff_hz: None,
            ..input
        },
        &mut fitter,
        &|| false,
    )
    .unwrap();
    let protected = rc::channel(&input, &mut fitter, &|| false).unwrap();
    let before = max_below(&unprotected.response, bus);
    let after = max_below(&protected.response, bus);
    eprintln!(
        "main without own rolloff: bus={bus:.9}, without bus bound={before:.9}, with bus bound={after:.9}"
    );
    assert!(before > 0.5);
    assert!(after <= 0.5);
    assert!(
        protected
            .out
            .design_db
            .iter()
            .zip(design_freqs())
            .filter(|(_, f)| **f < bus)
            .all(|(db, _)| *db <= 0.0)
    );
    assert!(
        protected
            .out
            .notes
            .iter()
            .any(|n| matches!(n, Note::LowProtect {
        role, rolloff_hz, limited: true, reduced, ..
    } if role == "L" && *rolloff_hz == bus && *reduced > 0))
    );
}

fn bass_db(h: &[C]) -> Vec<f64> {
    let power: Vec<_> = h.iter().map(|&h| response::magnitude(h).powi(2)).collect();
    let smoothed = secs_dsp::smooth::smooth_power(
        design_freqs(),
        &power,
        secs_dsp::smooth::Fraction::Scalar(6.0),
    )
    .unwrap();
    [50.0, 60.0, 80.0]
        .into_iter()
        .map(|hz| 10.0 * (secs_dsp::interp(hz, design_freqs(), &smoothed) + 1e-30).log10())
        .collect()
}

#[test]
fn low_protect_room_bass_survives() {
    let case = super::cases::load("room");
    let base: CalibrationSettings = serde_json::from_value(case.settings.clone()).unwrap();
    let view = ProjectView {
        groups: &case.config.groups,
        project_xo: &case.config.xo,
        target: &case.config.target,
    };
    let mut failures = Vec::new();
    for rc_lo in [20.0, 40.0] {
        let run = |low_protect| {
            let settings = CalibrationSettings {
                mode: Mode::Linear,
                rc_lo,
                low_protect,
                ..base.clone()
            };
            let inputs = engine_inputs(&view, &case.measurements, &settings).unwrap();
            let cancel = secs_compute::CancelToken::new();
            crate::design(
                &DesignRequest {
                    measurements: &inputs.measurements,
                    config: &inputs.config,
                    tuning: &case.tuning,
                    subs_only: false,
                    adapter_notes: &[],
                },
                &Context::serial(),
                &*crate::design::production_fitters(&cancel),
            )
            .unwrap()
        };
        let off = run(false);
        let on = run(true);
        for (tag, design) in [("off", &off), ("on", &on)] {
            for main in &design.chain.mains {
                let fir = crate::measurement::spectrum(&main.fir);
                let peq = response::peq_response(design_freqs(), &main.input_peq);
                let rolloff = on
                    .notes
                    .iter()
                    .find_map(|note| match note {
                        Note::LowProtect {
                            role, rolloff_hz, ..
                        } if role == &main.role => Some(*rolloff_hz),
                        _ => None,
                    })
                    .expect("protected main must report its rolloff");
                let response_db = |h: C| 20.0 * response::magnitude(h).max(1e-30).log10();
                let (worst_hz, fight) = design_freqs()
                    .iter()
                    .enumerate()
                    .filter(|(_, hz)| **hz >= rolloff && **hz <= 100.0)
                    .map(|(i, &hz)| {
                        let fight = response_db(fir[i]).abs() + response_db(peq[i]).abs()
                            - response_db(fir[i] * peq[i]).abs();
                        (hz, fight)
                    })
                    .max_by(|a, b| a.1.total_cmp(&b.1))
                    .unwrap();
                eprintln!(
                    "room rc_lo={rc_lo} {} {tag} fight={fight:.9} dB at {worst_hz:.9} Hz",
                    main.role
                );
                if tag == "on" && fight > 6.0 {
                    failures.push(format!(
                        "rc_lo={rc_lo} {} PEQ/FIR fight={fight:.9} dB at {worst_hz:.9} Hz",
                        main.role
                    ));
                }
                let db = |h: &[C]| {
                    let values: Vec<_> = h
                        .iter()
                        .map(|&h| 20.0 * response::magnitude(h).max(1e-30).log10())
                        .collect();
                    [20.0, 25.0, 30.0, 40.0, 50.0, 60.0, 80.0]
                        .map(|hz| secs_dsp::interp(hz, design_freqs(), &values))
                };
                eprintln!(
                    "room rc_lo={rc_lo} {} {tag} @20/25/30/40/50/60/80 FIR={:.6?} PEQ={:.6?} bands={:?}",
                    main.role,
                    db(&fir),
                    db(&peq),
                    main.input_peq
                );
            }
        }
        let check_bass = |role: &str, before: &[C], after: &[C]| {
            let off = bass_db(before);
            let on = bass_db(after);
            let diff: Vec<_> = on.iter().zip(&off).map(|(on, off)| on - off).collect();
            eprintln!(
                "room rc_lo={rc_lo} {role} @50/60/80 Hz: off={off:.6?}, on={on:.6?}, delta={diff:.6?}"
            );
            diff.iter().all(|d| d.abs() <= 1.5)
        };
        let bus_role = case.config.subs.join(" + ");
        let (bus_hz, reduced, cuts) = on
            .notes
            .iter()
            .find_map(|n| match n {
                Note::LowProtect {
                    role,
                    rolloff_hz,
                    limited,
                    reduced,
                    cuts,
                    hp_hz: None,
                } if role == &bus_role => {
                    assert!(limited);
                    Some((*rolloff_hz, *reduced, *cuts))
                }
                _ => None,
            })
            .expect("changed bus must report the guard");
        assert!(reduced + cuts > 0);
        let bus_max = max_below(
            &response::peq_response(design_freqs(), &on.chain.sub_bus_peq),
            bus_hz,
        );
        eprintln!(
            "room rc_lo={rc_lo} bus rolloff={bus_hz:.9}, max={bus_max:.9}, reduced={reduced}, cuts={cuts}"
        );
        assert!(bus_max <= 0.5);
        for (before, after) in off.mains.iter().zip(&on.mains) {
            let (hz, limited) = on.notes.iter().find_map(|n| match n {
                Note::LowProtect { role, rolloff_hz, limited, reduced, cuts, .. } if role == &after.role => {
                    eprintln!("room rc_lo={rc_lo} {role} rolloff={rolloff_hz:.9}, max={:.9}, reduced={reduced}, cuts={cuts}, limited={limited}", max_below(&after.response, *rolloff_hz));
                    Some((*rolloff_hz, *limited))
                }
                _ => None,
            }).expect("protected main must report its rolloff");
            assert!(hz >= bus_hz);
            let fir = &on
                .chain
                .mains
                .iter()
                .find(|main| main.role == after.role)
                .unwrap()
                .fir;
            let dc_db = 20.0 * fir.iter().sum::<f64>().abs().max(1e-30).log10();
            eprintln!("room rc_lo={rc_lo} {} FIR DC={dc_db:.9} dB", after.role);
            assert!(dc_db <= 0.5 + 1e-10, "{} DC={dc_db}", after.role);
            if !limited || max_below(&after.response, hz) > 0.5 + 1e-10 {
                let worst = after
                    .response
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| design_freqs()[*i] >= 10.0 && design_freqs()[*i] < hz)
                    .max_by(|(_, a), (_, b)| {
                        response::magnitude(**a).total_cmp(&response::magnitude(**b))
                    })
                    .unwrap();
                failures.push(format!(
                    "rc_lo={rc_lo} {} bound: {:.9} dB at {:.9} Hz",
                    after.role,
                    max_below(&after.response, hz),
                    design_freqs()[worst.0]
                ));
            }
            if !check_bass(&after.role, &before.after, &after.after) {
                failures.push(format!(
                    "rc_lo={rc_lo} {} bass changed by more than 1.5 dB",
                    after.role
                ));
            }
        }
        if !check_bass(
            "sub sum",
            &off.subs.as_ref().unwrap().sum_at_reference,
            &on.subs.as_ref().unwrap().sum_at_reference,
        ) {
            failures.push(format!(
                "rc_lo={rc_lo} sub sum bass changed by more than 1.5 dB"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        failures.join(
            "
"
        )
    );
}

#[test]
fn low_protect_main_fit_floor_uses_the_protection_rolloff() {
    for mode in [Mode::Zero, Mode::Linear] {
        for low_protect in [false, true] {
            let settings = CalibrationSettings {
                mode,
                low_protect,
                rc_hi: Some(500.0),
                ..Default::default()
            };
            let (_, mut config) = inputs(40.0, Some(30.0), &settings);
            config.peq_bands = Some(2);
            let setup = Setup::new(&config);
            let tuning = Tuning {
                gd_cuts: false,
                ..Default::default()
            };
            let mut floors = Vec::new();
            rc::channel(
                &RcInput {
                    role: "L",
                    comb: &[(1, spectrum(40.0))],
                    main_position: 1,
                    level_db: 0.0,
                    setup: &setup,
                    config: &config,
                    tuning: &tuning,
                    positions: &[1],
                    bus_rolloff_hz: Some(60.0),
                },
                &mut |request| {
                    floors.push(request.lo);
                    vec![]
                },
                &|| false,
            )
            .unwrap();
            let expected = if low_protect {
                60.0
            } else if mode == Mode::Zero {
                15.0
            } else {
                20.0
            };
            eprintln!("main fit floors mode={mode:?} protect={low_protect}: {floors:?}");
            assert_eq!(floors[0], expected);
            if mode == Mode::Zero {
                assert_eq!(floors[1], 400.0);
            } else {
                assert!(floors.len() >= 2);
                assert!(floors.iter().all(|&lo| lo == expected));
            }
        }
    }
}
