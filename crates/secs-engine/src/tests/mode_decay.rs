use std::{f64::consts::PI, sync::Arc};

use secs_dsp::{
    fft,
    iir::{BType, butter, sosfilt},
    response,
};
use secs_types::calibration::Mode;

use super::lf_align::{assert_bits, config, correct};
use crate::{
    C, Note, decay,
    grid::{N_FFT, SR_F, design_freqs},
    rc,
    setup::Setup,
};

fn synthetic(hz: f64) -> Arc<[C]> {
    synthetic_with(hz, 1.2, 20.0 * 4_f64.log10())
}

fn synthetic_with(hz: f64, t: f64, gain: f64) -> Arc<[C]> {
    // An RBJ resonator with a 1.2 s pole decay, on shorter background modes.
    let q_m = PI * hz * t / 6.91;
    let mut bands = vec![(hz, q_m / 10_f64.powf(gain / 40.0), gain)];
    for fc in [200.0, 250.0, 315.0, 400.0, 500.0] {
        let a = 10_f64.powf(6.0 / 40.0);
        bands.push((fc, PI * fc * 0.3 / 6.91 / a, 6.0));
    }
    response::peq_response(design_freqs(), &bands).into()
}

fn t60(h: &[C], hz: f64) -> f64 {
    decay::mode_t60(&fft::irfft(h, N_FFT), SR_F, hz, 1.5).unwrap()
}

/// Same reverse-time band filter and energy-peak origin as mode_t60.
fn band_decay(h: &[C], hz: f64) -> (f64, f64) {
    let ir = fft::irfft(h, N_FFT);
    let sos = butter(2, &[hz - 1.5, hz + 1.5], BType::Bandpass, SR_F).unwrap();
    let mut padded: Vec<_> = ir.iter().rev().copied().collect();
    padded.resize(padded.len() + (2.0 * SR_F) as usize, 0.0);
    let filtered = sosfilt(&sos, &padded).unwrap();
    let energy: Vec<_> = filtered.iter().rev().map(|v| v * v).collect();
    let start = decay::first_max(&energy);
    let mut edc = energy[start..].to_vec();
    let mut total = 0.0;
    for e in edc.iter_mut().rev() {
        total += *e;
        *e = total;
    }
    let t20 = edc.iter().position(|e| *e <= 0.01 * edc[0]).unwrap() as f64 / SR_F;
    (t20, edc[(0.15 * SR_F) as usize])
}

#[test]
fn mode_decay_refines_frequency_and_decay_and_rejects_sidelobes() {
    let h = synthetic(50.0);
    let (hz, t60) = rc::refine_mode(&h, 49.951171875).unwrap();
    eprintln!("refined mode: {hz:.6} Hz, {t60:.6} s");
    assert!((hz - 50.0).abs() < 0.2);
    assert!((t60 - 1.2).abs() < 0.12);
    assert_eq!(rc::refine_mode(&h, 43.2), None);
}

#[test]
fn mode_decay_cut_matches_pole_replacement() {
    let cut = rc::decay_cut(50.0, 0.9, 0.3, 30.0, 3.0).unwrap();
    let a = 10_f64.powf(cut.2 / 40.0);
    assert!((a - (0.3_f64 / 0.9).sqrt()).abs() < 1e-14);
    assert!((cut.1 - a * PI * 50.0 * 0.9 / 6.91).abs() < 1e-14);
    assert_eq!(rc::decay_cut(50.0, 2.0, 0.25, 30.0, 3.0).unwrap().2, -12.0);
    assert_eq!(rc::decay_cut(50.0, 2.0, 0.25, 2.0, 3.0).unwrap().2, -5.0);
    assert_eq!(rc::decay_cut(50.0, 2.0, 0.25, -2.0, 3.0).unwrap().2, -3.0);
    assert!(rc::decay_cut(50.0, 2.0, 0.25, 0.0, 0.9).is_none());
    assert!(rc::decay_cut(50.0, 0.375, 0.25, 30.0, 3.0).is_none());
    assert!(rc::decay_cut(50.0, f64::NAN, 0.25, 30.0, 3.0).is_none());
}

#[test]
fn mode_decay_shortens_a_long_mode() {
    let h = synthetic(50.0);
    let config = config(&h, Mode::Linear);
    let mut setup = Setup::new(&config);
    setup.mode_decay = true;
    let out = correct(h.clone(), &config, &setup);
    let corrected: Vec<_> = h.iter().zip(&out.response).map(|(h, r)| h * r).collect();
    let before = t60(&h, 50.0);
    let after = t60(&corrected, 50.0);
    let outcome = out.mode_decay.as_ref().unwrap();
    eprintln!(
        "mode decay before={before:.6} s after={after:.6} s target={:.6} s cuts={:?}",
        outcome.target_t60_s, outcome.modes
    );
    assert!(!outcome.modes.is_empty());
    assert!(
        after <= 1.5 * outcome.target_t60_s || after <= 0.5 * before,
        "{before} -> {after}"
    );
    for (i, &(hz, _, depth)) in outcome.modes.iter().enumerate() {
        assert_eq!(out.peq[i].0, hz);
        assert_eq!(out.peq[i].2, -depth);
    }
    assert_eq!(
        out.notes
            .iter()
            .filter(|n| matches!(n, Note::ModeDecay { .. }))
            .count(),
        1
    );
}

#[test]
fn mode_decay_survives_a_misplaced_estimate() {
    let cut = rc::decay_cut(50.0, 1.2, 0.3, 30.0, 3.0).unwrap();
    let dr = response::peq_response(design_freqs(), &[cut]);
    let mut measured = Vec::new();
    for hz in [49.0, 50.0, 51.0] {
        let h = synthetic(hz);
        let corrected: Vec<_> = h.iter().zip(&dr).map(|(h, r)| h * r).collect();
        let before = t60(&h, hz);
        let after = t60(&corrected, hz);
        let depth = dr
            .iter()
            .map(|&h| -20.0 * response::magnitude(h).log10())
            .fold(0.0, f64::max);
        eprintln!(
            "misplaced mode {hz} Hz before={before:.6} s after={after:.6} s dip={depth:.6} dB"
        );
        if hz == 50.0 {
            assert!(after < before);
        }
        let (before_t20, before_late) = band_decay(&h, cut.0);
        let (after_t20, after_late) = band_decay(&corrected, cut.0);
        let late_db = 10.0 * (after_late / before_late).log10();
        eprintln!(
            "band {hz} Hz T20 {before_t20:.6} -> {after_t20:.6} s; late energy change {late_db:.6} dB"
        );
        measured.push((hz, before_t20, after_t20, late_db));
        assert!(depth <= -cut.2 + 1e-8);
    }
    // A cut 1 Hz off cannot cancel the pole, so the band's own decay slope
    // and its self-normalised −20 dB time stay about the same (each curve
    // starts from its own 0 dB, and the cut lowers the start too). What a
    // misplaced cut must still do is make the ringing quieter: the absolute
    // band energy after 150 ms drops. The exact cut also shortens the decay.
    assert!(
        measured.iter().all(|(_, _, _, late_db)| *late_db <= -3.0),
        "{measured:?}"
    );
    assert!(
        measured
            .iter()
            .filter(|(hz, ..)| *hz == 50.0)
            .all(|(_, before, after, _)| after < before),
        "{measured:?}"
    );
}

#[test]
fn mode_decay_zero_mode_carries_the_cut() {
    let h = synthetic(50.0);
    let mut config = config(&h, Mode::Zero);
    config.low_protect = true;
    let mut setup = Setup::new(&config);
    setup.mode_decay = true;
    let out = correct(h, &config, &setup);
    let outcome = out.mode_decay.as_ref().unwrap();
    assert!(!outcome.modes.is_empty());
    for (i, &(hz, t60, depth)) in outcome.modes.iter().enumerate() {
        let q = 10_f64.powf(-depth / 40.0) * PI * hz * t60 / 6.91;
        assert!((out.peq[i].1 - q).abs() < 1e-12);
        assert_eq!(out.peq[i].2, -depth);
    }
    assert_eq!(out.fir, [1.0]);
    assert_eq!(
        out.response,
        response::peq_response(design_freqs(), &out.peq)
    );
    assert!(out.lf_align.is_none());
    let cut = out.peq[0];
    let mut protected = vec![cut, (20.0, 1.0, 15.0)];
    assert!(crate::low_protect::limit(
        &mut protected,
        &vec![C::new(1.0, 0.0); design_freqs().len()],
        40.0,
    ));
    assert_eq!(protected[0], cut);
    assert!(protected[1].2 < 15.0);
}

#[test]
fn mode_decay_off_is_unchanged() {
    let h = synthetic(50.0);
    for mode in [Mode::Linear, Mode::Zero] {
        let mut config = config(&h, mode);
        let setup = Setup::new(&config);
        let off = correct(h.clone(), &config, &setup);
        config.mode_decay = true;
        let on_config_only = correct(h.clone(), &config, &setup);
        assert_bits(&off, &on_config_only);
        assert!(off.mode_decay.is_none());
    }
}

#[test]
fn mode_decay_empty_modes_still_reports_target() {
    let h: Arc<[C]> = vec![C::new(1.0, 0.0); design_freqs().len()].into();
    let config = config(&h, Mode::Zero);
    let mut setup = Setup::new(&config);
    setup.mode_decay = true;
    let out = correct(h, &config, &setup);
    let outcome = out.mode_decay.unwrap();
    assert!(outcome.modes.is_empty());
    assert!(outcome.target_t60_s >= 0.25);
    assert_eq!(
        out.notes
            .iter()
            .filter(|n| matches!(n, Note::ModeDecay { .. }))
            .count(),
        1
    );
}
