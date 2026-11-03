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

use super::lf_align::{assert_bits, config, correct, synthetic};
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
    let dips = rc::limit_dip_fills(&power, &sm, &target, 0.0, &mut narrow);
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
    assert!(rc::limit_dip_fills(&broad_power, &broad_sm, &target, 0.0, &mut broad).is_empty());
    assert!(
        broad
            .iter()
            .zip(before)
            .all(|(actual, expected)| (actual - expected).abs() < 1e-12)
    );
    assert!((broad[bin(broad_centre)] - 6.0).abs() < 0.05);
}

#[test]
fn dip_limit_off_and_golden_rooms_are_bit_identical() {
    for (hz, q) in [(45.0, 3.0), (60.0, 2.0), (80.0, 1.0)] {
        let h = synthetic(hz, q);
        let baseline = config(&h, Mode::Linear);
        assert!(!baseline.dip_limit);
        let expected = correct(h.clone(), &baseline, &Setup::new(&baseline));
        let mut explicit_off = baseline.clone();
        explicit_off.dip_limit = false;
        let actual = correct(h, &explicit_off, &Setup::new(&explicit_off));
        assert_bits(&expected, &actual);
    }
}

#[test]
fn dip_limit_notes_list_the_deepest_limited_dips() {
    let power = vec![1.0; BINS];
    let target = vec![0.0; BINS];
    let mut sm = vec![0.0; BINS];
    let centres = [40.0, 55.0, 75.0, 100.0, 140.0, 200.0, 300.0];
    for (index, centre) in centres.into_iter().enumerate() {
        let at = bin(centre);
        sm[at..at + 2].fill(-(4.0 + index as f64));
    }
    let mut corr: Vec<_> = sm.iter().map(|value| -value).collect();
    let dips = rc::limit_dip_fills(&power, &sm, &target, 0.0, &mut corr);
    assert_eq!(dips.len(), 6);
    assert!(dips.windows(2).all(|pair| pair[0].1 >= pair[1].1));
    assert_eq!(dips[0].1, 10.0);
    assert_eq!(dips[0].2, 6.0);
    assert_eq!(dips.last().unwrap().1, 5.0);
    for &(_, depth, fill) in &dips {
        assert_eq!(fill, (2.0 + (depth - 2.0) / 2.0).min(6.0));
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
    assert!((90.0..=110.0).contains(&dips[0].0));
    assert!(dips.windows(2).all(|pair| pair[0].1 >= pair[1].1));
    for &(_, depth, fill) in dips {
        assert!(depth >= 4.0);
        assert_eq!(fill, (2.0 + (depth - 2.0) / 2.0).min(6.0));
    }
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
