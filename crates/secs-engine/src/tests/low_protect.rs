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
        .filter(|(f, _)| **f < hz)
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
fn low_protect_realized_guard_scales_leaking_boosts_and_reports_infeasible() {
    let unity = vec![C::new(1.0, 0.0); BINS];
    let mut peq = vec![(30.0, 1.0, 6.0)];
    assert!(max_below(&response::peq_response(design_freqs(), &peq), 25.0) > 5.0);
    assert!(low_protect::limit(&mut peq, &unity, 25.0));
    let maximum = max_below(&response::peq_response(design_freqs(), &peq), 25.0);
    assert!(maximum <= 0.5 && maximum > 0.499999, "{maximum}");
    assert!(peq[0].2 > 0.0 && peq[0].2 < 6.0);
    let original = vec![(30.0, 1.0, 6.0), (200.0, 1.0, 2.0), (18.0, 1.0, -1.0)];
    let mut same = original.clone();
    assert!(!low_protect::limit(
        &mut same,
        &vec![C::new(2.0, 0.0); BINS],
        25.0
    ));
    assert_eq!(same, original);
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
