use std::{f64::consts::PI, sync::Arc};

use secs_dsp::{fft, response, smooth::Fraction};
use secs_types::calibration::Mode;

use super::lf_align::{arrival, assert_bits, config, correct, synthetic};
use crate::{C, Note, grid::SR_F, grid::design_freqs, rc::RcOutput, setup::Setup};

fn room(index: usize) -> Arc<[C]> {
    match index {
        0 => synthetic(80.0, 1.0),
        1 => synthetic(60.0, 2.0),
        2 => synthetic(45.0, 3.0)
            .iter()
            .zip(response::allpass2(design_freqs(), 120.0, 1.0))
            .map(|(h, a)| h * a)
            .collect(),
        _ => unreachable!(),
    }
}

fn phase(out: &RcOutput, lift: f64, i: usize) -> f64 {
    (out.response[i] * C::from_polar(1.0, 2.0 * PI * design_freqs()[i] * lift))
        .arg()
        .to_degrees()
}

fn phase_range(out: &RcOutput, lift: f64, lo: f64, hi: f64) -> (f64, f64) {
    let values: Vec<_> = design_freqs()
        .iter()
        .enumerate()
        .filter(|(_, f)| **f >= lo && **f <= hi)
        .map(|(i, _)| phase(out, lift, i))
        .collect();
    assert!(!values.is_empty());
    assert!(values.iter().all(|v| v.is_finite()));
    (
        values.iter().copied().fold(f64::INFINITY, f64::min),
        values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    )
}

fn angular_difference(a: f64, b: f64) -> f64 {
    ((a - b + 180.0).rem_euclid(360.0) - 180.0).abs()
}

fn pre_peak_energy(out: &RcOutput) -> f64 {
    let peak = out
        .fir
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))
        .unwrap()
        .0;
    out.fir[..peak.saturating_sub(48)]
        .iter()
        .map(|v| v * v)
        .sum()
}

fn corrected(h: &[C], out: &RcOutput) -> Vec<C> {
    h.iter().zip(&out.response).map(|(h, r)| h * r).collect()
}

/// The corrected IR's peak, found as `response::gd_curve` finds it.
fn ir_peak(h: &[C], out: &RcOutput) -> usize {
    let ir = fft::irfft(&corrected(h, out), 2 * (h.len() - 1));
    let mut peak = 0;
    for i in 1..ir.len() {
        if ir[i].abs() > ir[peak].abs() {
            peak = i;
        }
    }
    peak
}

/// Median 60–100 Hz group delay in ms against the fixed time `peak`.
/// `gd_curve` alone removes each response's own IR peak, which can move by a
/// sample between runs, so both runs use the off run's peak here.
fn absolute_lf_delay(h: &[C], out: &RcOutput, peak: usize) -> f64 {
    let band: Vec<_> = (0..101).map(|i| 60.0 + 40.0 * i as f64 / 100.0).collect();
    crate::decay::median(
        &response::gd_curve_with_peak(
            &corrected(h, out),
            design_freqs(),
            &band,
            Fraction::Scalar(24.0),
            peak,
        )
        .unwrap(),
    )
}

fn pre_arrival_energy(out: &RcOutput, lift: f64) -> f64 {
    let end = ((lift * SR_F).round() as usize).saturating_sub(48);
    out.fir[..end].iter().map(|v| v * v).sum()
}

fn max_extra_delay_ms(rotation: f64, lift: f64, lo: f64, hi: f64) -> f64 {
    // Excess and applied are zero above the highest fade. Reconstruct the
    // realized delay increment from the selected rotation and discrete weights.
    let weights: Vec<_> = design_freqs()
        .iter()
        .filter(|&&f| f >= lo && f <= hi)
        .map(|&f| {
            let x = (f / lo).ln() / (hi / lo).ln();
            0.5 * (1.0 - (2.0 * PI * x).cos())
        })
        .collect();
    let mut weight_sum = 0.0;
    for &weight in &weights {
        weight_sum += weight;
    }
    let correction = rotation.to_radians().rem_euclid(2.0 * PI) / (2.0 * PI * design_freqs()[1]);
    weights
        .iter()
        .map(|w| -(-correction * w / weight_sum).clamp(-0.005, lift) * 1000.0)
        .fold(0.0, f64::max)
}

fn check_case(index: usize, mode: Mode, rc_hi: f64, extra: bool) -> Vec<String> {
    let h = room(index);
    let mut config = config(&h, mode);
    config.rc_range.1 = Some(rc_hi);
    if extra {
        config.rc_bands = vec![(900.0, 1200.0)];
    }
    let setup = Setup::new(&config);
    let off = correct(h.clone(), &config, &setup);
    assert!(off.hf_phase.is_none());
    config.hf_phase = true;
    let on_setup = Setup::new(&config);
    assert!(on_setup.hf_phase);
    let on = correct(h.clone(), &config, &on_setup);
    let f1 = if extra { 1200.0 } else { rc_hi } * 2_f64.sqrt();
    let f2 = (8.0 * f1).min(0.45 * SR_F);
    let lift = setup.lift_ms / 1000.0;
    let high = 0.45 * SR_F;
    let off_hf = phase_range(&off, lift, 1.06 * f2, high);
    let on_hf = phase_range(&on, lift, 1.06 * f2, high);
    let arrival_off = arrival(&h, &off, 60.0, 100.0);
    let arrival_on = arrival(&h, &on, 60.0, 100.0);
    let arrival_difference = (arrival_on - arrival_off).abs();
    let peak = ir_peak(&h, &off);
    let absolute_off = absolute_lf_delay(&h, &off, peak);
    let absolute_on = absolute_lf_delay(&h, &on, peak);
    let absolute_difference = (absolute_on - absolute_off).abs();
    let pre_off = pre_arrival_energy(&off, lift);
    let pre_on = pre_arrival_energy(&on, lift);
    let pre_difference_db = 10.0 * (pre_on / pre_off).log10();
    let magnitude_difference = design_freqs()
        .iter()
        .enumerate()
        .filter(|(_, f)| **f >= f1 && **f <= f2)
        .map(|(i, _)| (20.0 * (on.response[i].norm() / off.response[i].norm()).log10()).abs())
        .fold(0.0, f64::max);
    let off_energy = pre_peak_energy(&off);
    let on_energy = pre_peak_energy(&on);
    let outcome = on.hf_phase.as_ref().unwrap();
    let rotation = outcome.rotation_deg.unwrap();
    let extra_delay = max_extra_delay_ms(rotation, lift, f1, f2);
    let note_error = design_freqs()
        .iter()
        .enumerate()
        .filter(|(_, f)| **f >= 1.06 * f2 && **f <= high)
        .map(|(i, _)| angular_difference(rotation, phase(&off, lift, i)))
        .fold(0.0, f64::max);
    let notes: Vec<_> = on
        .notes
        .iter()
        .filter(|n| matches!(n, Note::HfPhase { .. }))
        .collect();
    assert_eq!(notes.len(), 1);
    assert_eq!(
        notes[0],
        &Note::HfPhase {
            role: "L".into(),
            rotation_deg: Some(rotation),
            limited: outcome.limited,
        }
    );
    let label = format!(
        "room={} mode={mode:?} rc_hi={rc_hi} extra={extra}",
        index + 1
    );
    eprintln!(
        "HF {label}: f1={f1:.6} f2={f2:.6} Hz; off=[{:.6}, {:.6}] on=[{:.6}, {:.6}] deg; arrival off={arrival_off:.6} on={arrival_on:.6} delta={arrival_difference:.6} ms; magnitude delta={magnitude_difference:.6} dB; pre-peak off={off_energy:.12e} on={on_energy:.12e} ratio={:.9}; strength off={:.1} on={:.1}; note rotation={rotation:.6} deg limited={} note_error={note_error:.6} deg; max extra delay={extra_delay:.9} ms",
        off_hf.0,
        off_hf.1,
        on_hf.0,
        on_hf.1,
        on_energy / off_energy,
        off.phase_strength,
        on.phase_strength,
        outcome.limited,
    );
    eprintln!(
        "HF {label}: absolute LF off={absolute_off:.9} on={absolute_on:.9} delta={absolute_difference:.9} ms; lift-anchored pre off={pre_off:.12e} on={pre_on:.12e} delta={pre_difference_db:.9} dB"
    );
    let mut failures = Vec::new();
    let mut check = |passed: bool, condition: &str| {
        if !passed {
            failures.push(format!("{label}: {condition}"));
        }
    };
    check(
        on_hf.0.abs().max(on_hf.1.abs()) <= 1.0,
        "HF phase exceeds 1 degree",
    );
    if index == 2 {
        check(
            off_hf.0.abs().max(off_hf.1.abs()) > 10.0,
            "off HF phase must exceed 10 degrees",
        );
    }
    check(
        absolute_difference <= 0.1,
        "absolute LF delay differs by more than 0.1 ms",
    );
    check(
        off.phase_strength == on.phase_strength,
        "phase strength changed",
    );
    check(
        magnitude_difference <= 0.1,
        "region magnitude differs by more than 0.1 dB",
    );
    check(
        pre_difference_db <= 0.25,
        "lift-anchored pre-ringing increased by more than 0.25 dB",
    );
    check(
        note_error <= 1.0,
        "note rotation differs from off phase by more than 1 degree",
    );
    check(!outcome.limited, "unwind unexpectedly limited");
    if extra {
        let gap_difference = design_freqs()
            .iter()
            .enumerate()
            .filter(|(_, f)| **f >= 1000.0 && **f <= 1600.0)
            .map(|(i, _)| angular_difference(phase(&on, lift, i), phase(&off, lift, i)))
            .fold(0.0, f64::max);
        eprintln!("HF {label}: 1000..1600 Hz phase delta={gap_difference:.6} deg");
        check(
            gap_difference <= 0.5,
            "extra-band phase differs by more than 0.5 degrees",
        );
    }
    failures
}

#[test]
fn hf_phase_linear_rooms() {
    let mut failures = Vec::new();
    for rc_hi in [1000.0, 250.0] {
        for index in 0..3 {
            failures.extend(check_case(index, Mode::Linear, rc_hi, false));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn hf_phase_mixed_room() {
    let failures = check_case(2, Mode::Mixed, 1000.0, false);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn hf_phase_extra_band_starts_above_highest_fade() {
    let failures = check_case(2, Mode::Linear, 250.0, true);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn hf_phase_unavailable_region_is_bit_identical() {
    let h = room(2);
    let mut config = config(&h, Mode::Linear);
    config.rc_range.1 = Some(8000.0);
    let off = correct(h.clone(), &config, &Setup::new(&config));
    config.hf_phase = true;
    let mut on = correct(h, &config, &Setup::new(&config));
    let outcome = on.hf_phase.as_ref().unwrap();
    assert_eq!(outcome.rotation_deg, None);
    assert!(!outcome.limited);
    let notes: Vec<_> = on
        .notes
        .iter()
        .filter(|n| matches!(n, Note::HfPhase { .. }))
        .collect();
    assert_eq!(
        notes,
        vec![&Note::HfPhase {
            role: "L".into(),
            rotation_deg: None,
            limited: false,
        }]
    );
    // The enabled unavailable-region note is the only intentional difference.
    on.notes.retain(|n| !matches!(n, Note::HfPhase { .. }));
    assert_bits(&off, &on);
    eprintln!("HF rc_hi=8000: rotation=None limited=false; off/on float bits identical");
}

#[test]
fn hf_phase_zero_mode_is_bit_identical_without_note() {
    let h = room(2);
    let mut config = config(&h, Mode::Zero);
    let off = correct(h.clone(), &config, &Setup::new(&config));
    config.hf_phase = true;
    let on = correct(h, &config, &Setup::new(&config));
    assert!(on.hf_phase.is_none());
    assert!(!on.notes.iter().any(|n| matches!(n, Note::HfPhase { .. })));
    assert_bits(&off, &on);
    eprintln!("HF Zero: no outcome or note; off/on float bits identical");
}
