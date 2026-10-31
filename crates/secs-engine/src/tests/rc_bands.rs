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
    let out = correct(h, &config);
    assert!(
        out.peq
            .iter()
            .any(|&(f, _, gain)| (15000.0..=20000.0).contains(&f) && gain < -1.0),
        "{:?}",
        out.peq
    );
    assert!(
        out.peq
            .iter()
            .all(|&(f, _, _)| (20.0..=300.0).contains(&f) || (15000.0..=20000.0).contains(&f)),
        "{:?}",
        out.peq
    );
    assert!(db(out.response[bin(16000.0)]) < -3.0);
    assert!(db(out.response[bin(5000.0)]).abs() < 0.5);
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
    assert_eq!(requests[2], (400.0, Some(multi.rc_bands.clone())));
    assert_eq!(requests[3], (20000.0, Some(multi.rc_bands)));
}
