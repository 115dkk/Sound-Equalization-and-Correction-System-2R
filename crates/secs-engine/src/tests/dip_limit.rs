use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use secs_compute::Context;
use secs_dsp::{
    fft,
    smooth::{self, Fraction},
};
use secs_types::{
    calibration::{CalibrationSettings, Mode, RcHiBasis},
    target::TargetCurve,
};

use super::lf_align::{config, correct};
use crate::{
    C, DesignRequest, FitStage, Measurement, MeasurementSet, Note, Samples, Tuning,
    calibration::{ProjectView, engine_inputs},
    grid::{BINS, N_FFT, SR, design_freqs},
    rc,
    setup::Setup,
};

fn bin(hz: f64) -> usize {
    design_freqs().partition_point(|f| *f < hz)
}

#[test]
fn dip_limit_fills_narrow_dips_partly_and_broad_loss_fully() {
    let target = vec![0.0; BINS];

    let power = vec![1.0; BINS];
    let mut sm = vec![0.0; BINS];
    let centre = 100.0;
    let lo = bin(centre / 2_f64.powf(1.0 / 8.0));
    let hi = bin(centre * 2_f64.powf(1.0 / 8.0));
    sm[lo..hi].fill(-8.0);
    let mut narrow: Vec<_> = sm.iter().map(|value| -value).collect();
    let unlimited = narrow.clone();
    let bins = rc::limit_dip_fills(&power, &sm, &target, 0.0, &mut narrow);
    let dips = rc::dip_notes(&bins, &narrow, &unlimited);
    assert_eq!(narrow[bin(centre)], 5.0);
    assert_eq!(dips, vec![(design_freqs()[lo], 8.0, 5.0)]);

    let broad_centre = 100.0;
    let broad_lo = broad_centre / 2.0;
    let broad_hi = broad_centre * 2.0;
    let broad_power: Vec<_> = design_freqs()
        .iter()
        .map(|&hz| {
            if (broad_lo..=broad_hi).contains(&hz) {
                10_f64.powf(-6.0 / 10.0)
            } else {
                1.0
            }
        })
        .collect();
    let broad_sm: Vec<_> =
        smooth::smooth_power(design_freqs(), &broad_power, Fraction::Scalar(1.0))
            .unwrap()
            .into_iter()
            .map(|value| 10.0 * (value + 1e-30).log10())
            .collect();
    let mut broad: Vec<_> = broad_sm.iter().map(|value| -value).collect();
    let before = broad.clone();
    let bins = rc::limit_dip_fills(&broad_power, &broad_sm, &target, 0.0, &mut broad);
    assert!(rc::dip_notes(&bins, &broad, &before).is_empty());
    assert!(
        broad
            .iter()
            .zip(before)
            .all(|(actual, expected)| (actual - expected).abs() < 1e-12)
    );
    assert!((broad[bin(broad_centre)] - 6.0).abs() < 0.05);
}

#[test]
fn dip_limit_keeps_the_cut_floor_on_peaks() {
    // A broad +30 dB peak has no dip: the target correction already sits on
    // the -25 dB cut floor and the dip cap must not lower it further.
    let target = vec![0.0; BINS];
    let power = vec![10_f64.powf(30.0 / 10.0); BINS];
    let sm = vec![30.0; BINS];
    let mut corr = vec![rc::CUT_FLOOR_DB; BINS];
    let unlimited = corr.clone();
    let bins = rc::limit_dip_fills(&power, &sm, &target, 0.0, &mut corr);
    assert!(rc::dip_notes(&bins, &corr, &unlimited).is_empty());
    assert!(corr.iter().all(|value| *value == rc::CUT_FLOOR_DB));
}

#[test]
fn dip_limit_off_and_golden_rooms_are_bit_identical() {
    for name in ["room_filters", "room"] {
        let result = super::design::replay(name);
        assert!(!result.config().dip_limit);
        for main in &result.mains {
            let fixture =
                secs_oracle::Fixture::load("engine_design", &format!("{name}_{}", main.role));
            let expected = fixture.c128s(&fixture.outputs()["F"]);
            let indices: Vec<_> = design_freqs()
                .iter()
                .enumerate()
                .filter(|(index, hz)| {
                    ((15.0..=600.0).contains(*hz) && index % 8 == 0) || index % 257 == 0
                })
                .map(|(index, _)| index)
                .collect();
            // The response follows the design stage test's rule (tests/design.rs): it is
            // within the platform tolerance of the golden for room_filters, and report-only
            // for room, whose downstream DE comparisons can flip on the last bits. glibc
            // lands a few ULP away from the recorded bits (PR #189), so bits are not asked;
            // the test keeps its registered name (features.toml, DEVIATIONS D56).
            let got: Vec<_> = indices
                .iter()
                .flat_map(|&index| [main.response[index].re, main.response[index].im])
                .collect();
            let expected: Vec<_> = expected.into_iter().flat_map(|(re, im)| [re, im]).collect();
            let label = format!("{name}/{}/F", main.role);
            if name != "room" && secs_oracle::oracle_libm() {
                secs_oracle::assert_close(&label, &got, &expected, 0.0, secs_oracle::PLATFORM_RTOL);
            } else {
                let (_, relative) = secs_oracle::errors(&got, &expected);
                eprintln!("MEASURE {label} report-only max_rel={relative:e}");
            }
            let before = fixture.c128s(&fixture.outputs()["before"]);
            let got: Vec<_> = indices
                .iter()
                .flat_map(|&index| [main.before[index].re, main.before[index].im])
                .collect();
            let expected: Vec<_> = before.into_iter().flat_map(|(re, im)| [re, im]).collect();
            // The design stage test gates `before` on every platform within the same tolerance.
            secs_oracle::assert_close(
                &format!("{name}/{}/before", main.role),
                &got,
                &expected,
                0.0,
                secs_oracle::PLATFORM_RTOL,
            );
        }
    }
}

#[test]
fn dip_limit_notes_list_the_deepest_limited_dips() {
    let power = vec![1.0; BINS];
    let target = vec![0.0; BINS];
    let mut sm = vec![0.0; BINS];
    let dips_by_centre = [
        (40.0, 4.0),
        (55.0, 5.0),
        (75.0, 6.0),
        (100.0, 7.0),
        (140.0, 7.5),
        (200.0, 9.0),
        (300.0, 10.0),
    ];
    for (centre, depth) in dips_by_centre {
        let at = bin(centre);
        sm[at..at + 2].fill(-depth);
    }
    let mut corr: Vec<_> = sm.iter().map(|value| -value).collect();
    let unlimited = corr.clone();
    let bins = rc::limit_dip_fills(&power, &sm, &target, 0.0, &mut corr);
    let dips = rc::dip_notes(&bins, &corr, &unlimited);
    assert_eq!(dips.len(), 6);

    // A dip another limit removes anyway (the band mask zeroes both targets)
    // is not a limited dip: only the cap's own reduction counts.
    let masked_out = bin(300.0);
    let mut limited = corr.clone();
    let mut without = unlimited.clone();
    for value in &mut limited[masked_out..masked_out + 2] {
        *value = 0.0;
    }
    for value in &mut without[masked_out..masked_out + 2] {
        *value = 0.0;
    }
    let kept = rc::dip_notes(&bins, &limited, &without);
    assert_eq!(kept.len(), 6);
    assert!(kept.iter().all(|&(hz, _, _)| hz < 290.0));
    assert!(dips.windows(2).all(|pair| pair[0].1 >= pair[1].1));
    assert_eq!(dips[0].1, 10.0);
    assert_eq!(dips[0].2, 6.0);
    assert_eq!(dips.last().unwrap().1, 5.0);
    let seven_point_five = dips
        .iter()
        .find(|(_, depth, _)| (*depth - 7.5).abs() < 0.05)
        .unwrap();
    assert_eq!(seven_point_five.2, 4.75);
    let ten = dips
        .iter()
        .find(|(_, depth, _)| (*depth - 10.0).abs() < 0.05)
        .unwrap();
    assert_eq!(ten.2, 6.0);

    let tolerance = secs_oracle::PLATFORM_RTOL;
    for (index, (&limited, &original)) in corr.iter().zip(&unlimited).enumerate() {
        let changed = (limited - original).abs() > tolerance * original.abs().max(1.0);
        if changed {
            assert!(bins[index].0 >= 4.0, "unreported changed bin {index}");
        }
    }

    let spectrum: Arc<[C]> = design_freqs()
        .iter()
        .map(|&hz| {
            let octave = (hz.max(1.0) / 100.0).log2();
            let db = -20.0 * (-0.5 * (octave / 0.12).powi(2)).exp();
            C::new(10_f64.powf(db / 20.0), 0.0)
        })
        .collect();
    let mut engine_config = config(&spectrum, Mode::Zero);
    engine_config.dip_limit = true;
    let out = correct(spectrum, &engine_config, &Setup::new(&engine_config));
    let note = out
        .notes
        .iter()
        .find(|note| matches!(note, Note::DipLimit { .. }))
        .unwrap();
    let Note::DipLimit { role, dips } = note else {
        unreachable!()
    };
    assert_eq!(role, "L");
    assert!(!dips.is_empty());
    assert!(dips.len() <= 6);
    assert!((90.0..=110.0).contains(&dips[0].hz));
    assert!(
        dips.windows(2)
            .all(|pair| pair[0].depth_db >= pair[1].depth_db)
    );
    assert_eq!(dips[0].fill_db, 6.0);
    assert!(
        dips.iter()
            .all(|dip| dip.depth_db >= 4.0 && dip.fill_db <= 6.0)
    );
}

fn agreement_room() -> (MeasurementSet, crate::EngineConfig) {
    let spectrum: Vec<_> = design_freqs()
        .iter()
        .map(|&hz| {
            let octave = (hz.max(1.0) / 100.0).log2();
            let db = -20.0 * (-0.5 * (octave / 0.12).powi(2)).exp();
            C::new(10_f64.powf(db / 20.0), 0.0)
        })
        .collect();
    let measurement = Measurement {
        samples: Samples::F64(fft::irfft(&spectrum, N_FFT).into()),
        sample_rate: SR,
        t0: 0.0,
    };
    let set = MeasurementSet {
        positions: (1..=3)
            .map(|position| (position, vec![("L".into(), measurement.clone())]))
            .collect(),
    };
    let config = engine_inputs(
        &ProjectView {
            groups: &[("front".into(), vec!["L".into()])],
            project_xo: &[],
            target: &TargetCurve::new(vec![(10.0, 0.0), (20_000.0, 0.0)]),
            inverted: &[],
        },
        &set,
        &CalibrationSettings {
            mode: Mode::Zero,
            rc_hi_basis: RcHiBasis::Agreement,
            dip_limit: true,
            ..Default::default()
        },
    )
    .unwrap()
    .config;
    (set, config)
}

#[test]
fn dip_limit_stays_on_in_the_agreement_probe() {
    let (set, config) = agreement_room();
    let rc_stage = Arc::new(AtomicUsize::new(0));
    let first_requests = Arc::new(Mutex::new(Vec::<(usize, f64)>::new()));
    let fitters = {
        let rc_stage = Arc::clone(&rc_stage);
        let first_requests = Arc::clone(&first_requests);
        move |stage: FitStage<'_>| -> Box<crate::fit::Fitter<'_>> {
            let index = match stage {
                FitStage::Rc(_) => Some(rc_stage.fetch_add(1, Ordering::Relaxed)),
                _ => None,
            };
            let first_requests = Arc::clone(&first_requests);
            let mut first = true;
            Box::new(move |request| {
                if let Some(index) = index
                    && first
                {
                    let at = request
                        .g
                        .partition_point(|hz| *hz < 100.0)
                        .min(request.g.len() - 1);
                    first_requests
                        .lock()
                        .unwrap()
                        .push((index, request.corr_db[at]));
                    first = false;
                }
                Vec::new()
            })
        }
    };
    crate::design(
        &DesignRequest {
            measurements: &set,
            config: &config,
            tuning: &Tuning::default(),
            subs_only: false,
            adapter_notes: &[],
        },
        &Context::serial(),
        &fitters,
    )
    .unwrap();
    let requests = first_requests.lock().unwrap();
    assert!(
        requests.len() >= 2,
        "probe and final RC requests: {requests:?}"
    );
    assert_eq!(requests[0].0, 0);
    assert!(
        requests[0].1 < 9.0,
        "agreement probe did not limit the dip: {requests:?}"
    );
}
