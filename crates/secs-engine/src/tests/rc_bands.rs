use std::sync::Arc;

use secs_compute::Context;
use secs_dsp::{fft, response, smooth::Fraction};
use secs_types::{
    calibration::{CalibrationSettings, Mode, RcBand, RcHiBasis},
    target::TargetCurve,
};

use crate::{
    C, DesignRequest, EngineConfig, Measurement, MeasurementSet, Note, Samples, Tuning,
    calibration::{ProjectView, engine_inputs},
    fit::{self, FitRequest},
    grid::{N_FFT, SR, SR_F, design_freqs},
    rc::{self, RcInput},
    setup::Setup,
};

fn extra() -> Vec<RcBand> {
    vec![RcBand {
        lo_hz: 15000.0,
        hi_hz: 20000.0,
    }]
}

fn synthetic(high_gain: f64, phase: bool) -> Arc<[C]> {
    let f = design_freqs();
    let h = response::peq_response(
        f,
        &[
            (100.0, 2.0, -6.0),
            (5000.0, 3.0, 6.0),
            (16000.0, 3.0, high_gain),
        ],
    );
    if phase {
        h.iter()
            .zip(response::allpass2(f, 17500.0, 4.0))
            .map(|(h, p)| h * p)
            .collect()
    } else {
        h.into()
    }
}

fn inputs(h: &[C], mode: Mode, extras: bool, positions: u32) -> (MeasurementSet, EngineConfig) {
    let measurement = Measurement {
        samples: Samples::F64(fft::irfft(h, N_FFT).into()),
        sample_rate: SR,
        t0: 0.0,
    };
    let set = MeasurementSet {
        positions: (1..=positions)
            .map(|p| (p, vec![("L".into(), measurement.clone())]))
            .collect(),
    };
    let mut config = engine_inputs(
        &ProjectView {
            groups: &[("front".into(), vec!["L".into()])],
            project_xo: &[],
            target: &TargetCurve::new(vec![(10.0, 0.0), (20000.0, 0.0)]),
        },
        &set,
        &CalibrationSettings {
            mode,
            rc_hi: Some(300.0),
            rc_bands: if extras { extra() } else { vec![] },
            ..Default::default()
        },
    )
    .unwrap()
    .config;
    config.peq_bands = Some(4);
    (set, config)
}

fn correct(h: Arc<[C]>, config: &EngineConfig) -> rc::RcOutput {
    rc::correction(
        &RcInput {
            role: "L",
            comb: &[(1, h)],
            main_position: 1,
            level_db: 0.0,
            setup: &Setup::new(config),
            config,
            tuning: &Tuning::default(),
            positions: &[1],
        },
        config.phase_frac,
        &mut fit::fit_peq,
    )
}

/// One recorded PEQ fit: its interval, the weighting union it was given and its result.
type RecordedFit = (f64, f64, Option<Vec<(f64, f64)>>, Vec<(f64, f64, f64)>);

/// `correct`, recording every PEQ fit it runs.
fn recorded_fits(h: Arc<[C]>, config: &EngineConfig) -> Vec<RecordedFit> {
    let mut fits = Vec::new();
    rc::correction(
        &RcInput {
            role: "L",
            comb: &[(1, h)],
            main_position: 1,
            level_db: 0.0,
            setup: &Setup::new(config),
            config,
            tuning: &Tuning::default(),
            positions: &[1],
        },
        config.phase_frac,
        &mut |request: &FitRequest<'_>| {
            let peq = fit::fit_peq(request);
            fits.push((
                request.lo,
                request.hi,
                request.bands.map(<[_]>::to_vec),
                peq.clone(),
            ));
            peq
        },
    );
    fits
}

fn peq_bits(peq: &[(f64, f64, f64)]) -> Vec<[u64; 3]> {
    peq.iter()
        .map(|&(f, q, gain)| [f.to_bits(), q.to_bits(), gain.to_bits()])
        .collect()
}

/// A response whose phase wanders at random between `lo` and `hi`: its
/// group delay spikes there, as a noisy measurement's does.
fn noisy_phase(lo: f64, hi: f64) -> Arc<[C]> {
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    design_freqs()
        .iter()
        .map(|&f| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let phase = if (lo..=hi).contains(&f) {
                (state >> 11) as f64 / (1u64 << 53) as f64 * 2.0 * std::f64::consts::PI
            } else {
                0.0
            };
            C::from_polar(1.0, phase)
        })
        .collect()
}

fn bin(hz: f64) -> usize {
    (hz / design_freqs()[1]).round() as usize
}

fn db(h: C) -> f64 {
    20.0 * response::magnitude(h).log10()
}

#[test]
fn rc_band_set_sorts_merges_overlaps_and_touching_edges() {
    assert_eq!(rc::band_set(20.0, 300.0, &[]), [(20.0, 300.0)]);
    // Legacy inverted bounds remain unchanged without extras.
    assert_eq!(rc::band_set(200.0, 100.0, &[]), [(200.0, 100.0)]);
    assert_eq!(
        rc::band_set(
            20.0,
            300.0,
            &[
                (15000.0, 20000.0),
                (600.0, 900.0),
                (250.0, 500.0),
                (500.0, 600.0),
                (40.0, 80.0),
            ]
        ),
        [(20.0, 900.0), (15000.0, 20000.0)]
    );
    assert_eq!(
        rc::band_set(100.0, 300.0, &[(20.0, 60.0), (60.0, 100.0)]),
        [(20.0, 300.0)]
    );
}

#[test]
fn rc_bands_empty_masks_are_bit_identical_and_extra_ramps_use_max() {
    let h = synthetic(6.0, false);
    let (_, config) = inputs(&h, Mode::Mixed, false, 1);
    let setup = Setup::new(&config);
    let bands = rc::band_set(setup.rc_lo, setup.rc_hi, &[]);
    let hi_edge = (setup.rc_hi * 2_f64.sqrt()).min(SR_F / 2.0 * 0.99);
    let fade_hz = setup.rc_hi * 2_f64.sqrt();
    for &f in design_freqs() {
        let lf = f.max(1e-3).log2();
        let magnitude = ((lf - (setup.rc_lo / 2_f64.sqrt()).log2()) / 0.5).clamp(0.0, 1.0)
            * ((hi_edge.log2() - lf) / (hi_edge / setup.rc_hi).log2().max(1e-3)).clamp(0.0, 1.0);
        let phase = if f < 15.0 {
            (f / 15.0).clamp(0.0, 1.0)
        } else {
            ((fade_hz.log2() - f.max(1e-3).log2()) / 0.5).clamp(0.0, 1.0)
        };
        assert_eq!(rc::magnitude_mask(f, &bands).to_bits(), magnitude.to_bits());
        assert_eq!(rc::phase_fade(f, &setup, &bands).to_bits(), phase.to_bits());
    }
    let mut multi = setup;
    multi.rc_bands = vec![(15000.0, 20000.0)];
    let bands = rc::band_set(multi.rc_lo, multi.rc_hi, &multi.rc_bands);
    for f in [3000.0, 5000.0, 10000.0] {
        assert_eq!(rc::magnitude_mask(f, &bands), 0.0);
        assert_eq!(rc::phase_fade(f, &multi, &bands), 0.0);
    }
    assert_eq!(rc::magnitude_mask(16000.0, &bands), 1.0);
    assert_eq!(rc::phase_fade(16000.0, &multi, &bands), 1.0);
    assert_eq!(rc::magnitude_mask(SR_F / 2.0 * 0.99, &bands), 0.0);
    let overlap = [(20.0, 300.0), (400.0, 600.0)];
    let f = 350.0;
    assert_eq!(
        rc::magnitude_mask(f, &overlap),
        rc::magnitude_mask(f, &overlap[..1]).max(rc::magnitude_mask(f, &overlap[1..]))
    );
}

#[test]
fn rc_bands_fir_corrects_high_bump_not_gap_and_retains_boost_cap() {
    let h = synthetic(6.0, false);
    let (_, config) = inputs(&h, Mode::Mixed, false, 1);
    let primary = correct(h.clone(), &config);
    let (_, config) = inputs(&h, Mode::Mixed, true, 1);
    let multi = correct(h, &config);
    let high = bin(16000.0);
    let gap = bin(5000.0);
    assert_eq!(primary.design_db[high], 0.0);
    assert!(db(primary.response[high]).abs() < 0.1);
    assert!(multi.design_db[high] < -4.0);
    assert!(db(multi.response[high]) < -3.5);
    assert_eq!(multi.design_db[gap], 0.0);
    assert!(db(multi.response[gap]).abs() < 0.15);
    assert!(multi.design_db[bin(100.0)] > 3.0);

    let boosted = correct(synthetic(-6.0, false), &config);
    for (i, &f) in design_freqs()
        .iter()
        .enumerate()
        .filter(|(_, f)| **f > 16000.0)
    {
        assert!(boosted.design_db[i] <= 2.0, "{f}: {}", boosted.design_db[i]);
    }
    assert!(boosted.design_db[bin(17000.0)] > 1.0);
    assert!(db(boosted.response[bin(17000.0)]) <= 2.1);
}

#[test]
fn rc_bands_zero_peq_centres_stay_in_the_union() {
    let h = synthetic(6.0, false);
    let (_, config) = inputs(&h, Mode::Zero, true, 1);
    let out = correct(h.clone(), &config);
    assert!(
        out.peq
            .iter()
            .any(|&(f, _, gain)| (15000.0..=20000.0).contains(&f) && gain < -1.0),
        "{:?}",
        out.peq
    );
    assert!(db(out.response[bin(16000.0)]) < -3.0);
    assert!(db(out.response[bin(5000.0)]).abs() < 0.5);
    // The high fit keeps its centres where the primary band and the extra
    // band reach (up to ×√2 past their edges), out of the gap between them.
    let fits = recorded_fits(h, &config);
    let reach = [
        (400.0, 300.0 * 2_f64.sqrt()),
        (15000.0 / 2_f64.sqrt(), 20000.0 * 2_f64.sqrt()),
    ];
    assert_eq!(fits[1].1, 20000.0);
    assert_eq!(fits[1].2.as_deref(), Some(&reach[..]));
    assert!(
        fits[1].3.iter().all(|&(f, _, _)| rc::in_bands(f, &reach)),
        "{:?}",
        fits[1].3
    );
}

#[test]
fn rc_bands_high_extra_leaves_the_low_zero_fit_as_before() {
    // The primary band 20–300 Hz (an automatic upper limit resolves to the
    // same value before RC runs); an extra band at 15 kHz must not change
    // the low fit (15–400 Hz) at all.
    let h = synthetic(6.0, false);
    let (_, primary) = inputs(&h, Mode::Zero, false, 1);
    let (_, multi) = inputs(&h, Mode::Zero, true, 1);
    let before = recorded_fits(h.clone(), &primary);
    let after = recorded_fits(h, &multi);
    assert_eq!((before[0].0, before[0].1), (15.0, 400.0));
    assert_eq!((after[0].0, after[0].1), (15.0, 400.0));
    assert_eq!(before[0].2, None);
    assert_eq!(after[0].2, None, "the low fit keeps its legacy weights");
    assert!(!after[0].3.is_empty());
    assert_eq!(peq_bits(&before[0].3), peq_bits(&after[0].3));
}

#[test]
fn rc_bands_extra_inside_the_low_zero_fit_keeps_the_gap_out_of_seeds() {
    // Primary 20–100 Hz (its ramp reaches 141 Hz) and an extra band
    // 250–400 Hz (its ramp starts at 177 Hz): the low fit weighs both
    // reaches and leaves the gap between them out.
    let h = synthetic(6.0, false);
    let (_, mut config) = inputs(&h, Mode::Zero, false, 1);
    config.rc_range.1 = Some(100.0);
    config.rc_bands = vec![(250.0, 400.0)];
    let sqrt2 = 2_f64.sqrt();
    let gap = (100.0 * sqrt2, 250.0 / sqrt2);
    let fits = recorded_fits(h, &config);
    let (lo, hi, bands, peq) = &fits[0];
    assert_eq!((*lo, *hi), (15.0, 400.0));
    let bands = bands.as_deref().expect("an extra band inside the low fit");
    assert_eq!(bands, [(15.0, gap.0), (gap.1, 400.0 * sqrt2)]);
    assert!(
        peq.iter().all(|&(f, _, _)| f <= gap.0 || f >= gap.1),
        "{peq:?}"
    );
    // A correction that asks for a cut only inside the gap seeds nothing,
    // and one beside it seeds there.
    let g = [100.0, 160.0, 300.0];
    let request = FitRequest {
        g: &g,
        corr_db: &[0.0, -12.0, 0.0],
        n: 2,
        lo: *lo,
        hi: *hi,
        boost: 10.0,
        ring_ms: None,
        f_max: None,
        bands: Some(bands),
    };
    assert!(fit::seed(&request).bands.is_empty());
    let seed = fit::seed(&FitRequest {
        corr_db: &[0.0, -12.0, -6.0],
        ..request
    });
    assert_eq!(seed.bands.len(), 1);
    assert_eq!(seed.bands[0].0, 300.0);
}

#[test]
fn rc_bands_phase_error_band_keeps_the_primary_from_20_hz() {
    let (_, mut config) = inputs(&synthetic(6.0, false), Mode::Mixed, true, 1);
    config.rc_range.0 = 40.0;
    let multi = Setup::new(&config);
    let mut legacy = multi.clone();
    legacy.rc_bands.clear();
    // The mode's own FIR length (lo_fir 35 Hz) and a longer one (lo_fir 9 Hz).
    for taps in [config.taps, 8192] {
        let lo_fir = 1.5 * SR_F / taps as f64;
        let start = 20_f64.max(lo_fir);
        assert!(start < 40.0);
        // Without extra bands it is the legacy band at every bin.
        for &f in design_freqs() {
            assert_eq!(
                rc::phase_error_band(f, lo_fir, &legacy),
                f >= start && f <= 300.0 * 2_f64.sqrt(),
                "{f}"
            );
        }
        // With one, the primary part is still the legacy band (from 20 Hz
        // or lo_fir, not from rc_lo) and the extra band adds its own reach.
        for (f, inside) in [
            (start - 0.5, false),
            (start, true),
            ((start + 40.0) / 2.0, true),
            (40.0, true),
            (424.0, true),
            (425.0, false),
            (10000.0, false),
            (15000.0, true),
            (23000.0, true),
            (23800.0, false),
        ] {
            assert_eq!(
                rc::phase_error_band(f, lo_fir, &multi),
                inside,
                "{taps}: {f}"
            );
        }
    }
}

#[test]
fn rc_bands_inverted_primary_grids_skip_it_and_gd_cuts_stay_in_the_bands() {
    // rc_lo 150 Hz above an automatic upper limit of 126 Hz.
    let bands = rc::band_set(150.0, 126.0, &[(15000.0, 20000.0)]);
    assert_eq!(bands, [(150.0, 126.0), (15000.0, 20000.0)]);
    let h = noisy_phase(100.0, 200.0);
    let cuts = fit::fit_gd_cuts_bands_until(design_freqs(), &h, &bands, 6, 3.0, &|| false);
    assert!(
        cuts.iter()
            .all(|&(f, _, _)| (15000.0..=20000.0).contains(&f)),
        "{cuts:?}"
    );

    // The same through the engine's RC with GD cuts on.
    let (_, mut config) = inputs(&h, Mode::Zero, true, 1);
    config.rc_range = (150.0, Some(126.0));
    let tuning = Tuning {
        gd_cuts: true,
        ..Tuning::default()
    };
    let out = rc::channel(
        &RcInput {
            role: "L",
            comb: &[(1, h)],
            main_position: 1,
            level_db: 0.0,
            setup: &Setup::new(&config),
            config: &config,
            tuning: &tuning,
            positions: &[1],
        },
        &mut fit::fit_peq,
        &|| false,
    )
    .unwrap();
    assert!(
        out.gd_cuts
            .iter()
            .all(|&(f, _, _)| (15000.0..=20000.0).contains(&f)),
        "{:?}",
        out.gd_cuts
    );

    // The grids leave the inverted primary out ...
    let grid = rc::band_grid(&bands, 20.0, 1500);
    assert_eq!(grid.len(), 1500);
    assert!(grid.iter().all(|&f| rc::in_bands(f, &bands)));
    // ... but alone it keeps its legacy (falling) grid.
    let legacy = rc::band_grid(&[(150.0, 126.0)], 20.0, 1200);
    assert_eq!(
        (legacy.len(), legacy[0], legacy[1199]),
        (1200, 150.0, 126.0)
    );
}

#[test]
fn rc_bands_engine_inputs_drop_invalid_extras_sort_and_keep_four() {
    let h = synthetic(6.0, false);
    let band = |lo_hz, hi_hz| RcBand { lo_hz, hi_hz };
    let settings = CalibrationSettings {
        rc_bands: vec![
            band(15000.0, 20000.0),
            band(10.0, 50.0),
            band(300.0, 200.0),
            band(400.0, 400.0),
            band(f64::NAN, 100.0),
            band(100.0, f64::INFINITY),
            band(19000.0, 20001.0),
            band(5000.0, 6000.0),
            band(30.0, 60.0),
            band(1000.0, 2000.0),
            band(500.0, 600.0),
        ],
        ..Default::default()
    };
    let measurement = Measurement {
        samples: Samples::F64(fft::irfft(&h, N_FFT).into()),
        sample_rate: SR,
        t0: 0.0,
    };
    let set = MeasurementSet {
        positions: [(1, vec![("L".into(), measurement)])].into_iter().collect(),
    };
    let config = engine_inputs(
        &ProjectView {
            groups: &[("front".into(), vec!["L".into()])],
            project_xo: &[],
            target: &TargetCurve::new(vec![(10.0, 0.0), (20000.0, 0.0)]),
        },
        &set,
        &settings,
    )
    .unwrap()
    .config;
    assert_eq!(
        config.rc_bands,
        [
            (30.0, 60.0),
            (500.0, 600.0),
            (1000.0, 2000.0),
            (5000.0, 6000.0)
        ]
    );
}

#[test]
fn rc_bands_fit_seeds_and_refine_weights_exclude_gaps() {
    let g = [100.0, 1000.0, 16000.0];
    let corr = [0.0, 20.0, -6.0];
    let bands = [(20.0, 300.0), (15000.0, 20000.0)];
    let request = FitRequest {
        g: &g,
        corr_db: &corr,
        n: 1,
        lo: 20.0,
        hi: 20000.0,
        boost: 10.0,
        ring_ms: None,
        f_max: None,
        bands: Some(&bands),
    };
    let seed = fit::seed(&request);
    assert_eq!(seed.bands[0].0, 16000.0);
    let residual = fit::residual(&request, &[]);
    assert!((residual[1] + 3.0).abs() < 1e-10);
    assert!((residual[2] - 6.0).abs() < 1e-10);
    assert_eq!(seed.lb[0], 15000.0_f64.ln());
    assert_eq!(seed.ub[0], 20000.0_f64.ln());
    let narrow = [(15999.9999, 16000.0001)];
    let seed = fit::seed(&FitRequest {
        bands: Some(&narrow),
        ..request
    });
    assert!(seed.x0[0] > seed.lb[0] && seed.x0[0] < seed.ub[0]);
}

#[test]
fn rc_bands_fir_excess_phase_moves_high_gd_toward_reference() {
    let h = synthetic(6.0, true);
    let (_, config) = inputs(&h, Mode::Linear, false, 1);
    let primary = correct(h.clone(), &config);
    let (_, config) = inputs(&h, Mode::Linear, true, 1);
    let multi = correct(h.clone(), &config);
    let grid: Vec<_> = (0..101).map(|i| 15000.0 + i as f64 * 50.0).collect();
    let reference_grid = [3000.0, 4000.0, 5000.0, 6000.0];
    let error = |out: &rc::RcOutput| {
        let corrected: Vec<_> = h.iter().zip(&out.response).map(|(a, b)| a * b).collect();
        let gd =
            response::gd_curve(&corrected, design_freqs(), &grid, Fraction::Scalar(24.0)).unwrap();
        let reference = response::gd_curve(
            &corrected,
            design_freqs(),
            &reference_grid,
            Fraction::Scalar(24.0),
        )
        .unwrap();
        let reference = crate::levels::median(reference);
        gd.iter().map(|v| (v - reference).powi(2)).sum::<f64>() / gd.len() as f64
    };
    let (before, after) = (error(&primary), error(&multi));
    eprintln!(
        "high GD mean-square error: primary={before:e}, extra={after:e}, strength={}",
        multi.phase_strength
    );
    assert!(multi.phase_strength > 0.0);
    assert!(after < before * 0.5, "primary={before}, extra={after}");
}

#[test]
fn rc_bands_agreement_probe_ignores_extras_and_reports_merged_bands() {
    let h = synthetic(6.0, false);
    let (set, mut config) = inputs(&h, Mode::Zero, false, 3);
    config.rc_range.1 = None;
    config.rc_hi_basis = RcHiBasis::Agreement;
    let ctx = Context::serial();
    let requests = std::sync::Mutex::new(Vec::new());
    let fitters = |_: crate::FitStage<'_>| -> Box<fit::Fitter<'_>> {
        Box::new(|request| {
            requests
                .lock()
                .unwrap()
                .push((request.hi, request.bands.map(<[_]>::to_vec)));
            fit::fit_peq(request)
        })
    };
    let run = |config: &EngineConfig| {
        crate::design(
            &DesignRequest {
                measurements: &set,
                config,
                tuning: &Tuning::default(),
                subs_only: false,
                adapter_notes: &[],
            },
            &ctx,
            &fitters,
        )
        .unwrap()
    };
    let primary = run(&config);
    assert!(primary.rc_bands.is_empty());
    assert!(
        !primary
            .notes
            .iter()
            .any(|n| matches!(n, Note::RcBands { .. }))
    );
    requests.lock().unwrap().clear();
    config.rc_bands = vec![(15000.0, 18000.0), (18000.0, 20000.0)];
    let multi = run(&config);
    assert_eq!(primary.rc_hi.to_bits(), multi.rc_hi.to_bits());
    assert_eq!(multi.rc_bands, [(20.0, multi.rc_hi), (15000.0, 20000.0)]);
    assert!(
        multi
            .notes
            .iter()
            .any(|n| matches!(n, Note::RcBands { bands } if bands == &multi.rc_bands))
    );
    assert_eq!(multi.replay.config(), &config);
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0], (400.0, None));
    assert_eq!(requests[1], (2000.0, None));
    // The extra bands lie above the low fit, which keeps its legacy weights;
    // the high fit weighs where the primary band and the extra bands reach.
    assert_eq!(requests[2], (400.0, None));
    let sqrt2 = 2_f64.sqrt();
    let mut reach = Vec::new();
    if multi.rc_hi * sqrt2 > 400.0 {
        reach.push((400.0, multi.rc_hi * sqrt2));
    }
    reach.push((15000.0 / sqrt2, 20000.0 * sqrt2));
    assert_eq!(requests[3], (20000.0, Some(reach)));
}
