use super::cases::{self, pairs};
use crate::{C, EngineError, Samples, grid::*, input::*, levels::*, measurement::*, setup::*};
use secs_oracle::{Fixture, PLATFORM_RTOL, assert_close, integer, number};
use serde_json::Value;
use std::sync::Arc;

fn close(name: &str, got: f64, expected: &Value) {
    assert_close(name, &[got], &[number(expected)], 0.0, PLATFORM_RTOL);
}

fn ordered_values(name: &str, got: &[(String, f64)], expected: &Value) {
    let expected: Vec<_> = pairs(expected).collect();
    assert_eq!(got.len(), expected.len());
    for ((r, v), (er, ev)) in got.iter().zip(expected) {
        assert_eq!(r, er);
        close(name, *v, ev);
    }
}

#[test]
fn setup_matches_oracle() {
    for name in cases::names() {
        let case = cases::load(&name);
        let fx = Fixture::load("engine_prep", &name);
        let o = &fx.outputs()["setup"];
        let setup = Setup::new(&case.config);
        for (key, value) in [
            ("lift_ms", setup.lift_ms),
            ("max_latency_ms", setup.max_latency_ms),
            ("rc_lo", setup.rc_lo),
            ("rc_hi", setup.rc_hi),
        ] {
            close(key, value, &o[key]);
        }
        assert_eq!(
            setup.lift_capped_from_ms.is_some(),
            o["lift_note"].as_bool().unwrap()
        );
        assert_eq!(setup.rc_hi_manual, o["rc_hi_manual"].as_f64());
        assert_eq!(setup.peq_bands, integer(&o["peq_bands"]) as usize);
        assert_eq!(
            setup.sub_manual,
            pairs(&o["sub_manual"])
                .map(|(r, v)| (r.to_owned(), cases::manual(v)))
                .collect::<Vec<_>>()
        );
        let expected: Vec<_> = pairs(&o["sub_filters"])
            .map(|(r, v)| {
                (
                    r.to_owned(),
                    SubFilter {
                        hp_hz: number(&v["hp"]),
                        hp_type: BandType::from_name(v["hp_type"].as_str().unwrap()),
                        lp_hz: number(&v["lp"]),
                        lp_type: BandType::from_name(v["lp_type"].as_str().unwrap()),
                        peq: cases::peq(&v["peq"]),
                    },
                )
            })
            .collect();
        assert_eq!(setup.sub_filters, expected);
        let expected: Vec<_> = pairs(&o["layouts"])
            .map(|(r, v)| {
                (
                    r.to_owned(),
                    Layout {
                        kind: if v["kind"] == "dba" {
                            LayoutKind::Dba
                        } else {
                            LayoutKind::Multi
                        },
                        rear_trim_ms: number(&v["rear_trim_ms"]),
                        members: v["members"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_str().unwrap().to_owned())
                            .collect(),
                    },
                )
            })
            .collect();
        assert_eq!(setup.layouts, expected);
    }
}

#[derive(Default)]
struct ErrorMeasure {
    n: usize,
    abs: f64,
    relative_peak: f64,
}
impl ErrorMeasure {
    fn spectra(&mut self, got: &Spectra, descriptors: &Value, bins: &[usize], bank: &[(f64, f64)]) {
        let expected = descriptors.as_array().unwrap();
        let cells: Vec<_> = got
            .positions()
            .iter()
            .flat_map(|&p| got.roles_at(p).map(move |r| (p, r)))
            .collect();
        assert_eq!(cells.len(), expected.len());
        for ((p, role), cell) in cells.iter().zip(expected) {
            assert_eq!(*p, integer(&cell[0]) as u32);
            assert_eq!(*role, cell[1].as_str().unwrap());
            let start = integer(&cell[2]) as usize * bins.len();
            let expected = &bank[start..start + bins.len()];
            let peak = expected
                .iter()
                .map(|(re, im)| re.hypot(*im))
                .fold(0.0, f64::max);
            let h = got.get(*p, role).unwrap();
            assert_eq!(h.len(), BINS);
            for (&bin, &(re, im)) in bins.iter().zip(expected) {
                let error = (h[bin] - C::new(re, im)).norm();
                self.n += 1;
                self.abs = self.abs.max(error);
                self.relative_peak = self.relative_peak.max(error / peak.max(f64::MIN_POSITIVE));
                // Initial max error / sampled spectrum peak: 5.368509846704912e-16.
                // ADR 0004: max(1.5 × measured, 1e-12), hence the floor wins.
                // Using the sampled peak is stricter than using the full-grid peak.
                assert!(
                    error <= (1.5 * 5.368509846704912e-16_f64).max(1e-12) * peak,
                    "{p}/{role} bin {bin}: {error:e} / peak {peak:e}"
                );
            }
        }
    }
}

#[test]
fn preparation_and_levels_match_oracle() {
    let bank_fx = Fixture::load("engine_prep", "spectra");
    let bank = bank_fx.c128s(&bank_fx.outputs()["values"]);
    let mut measure = ErrorMeasure::default();
    for name in cases::names() {
        let case = cases::load(&name);
        let fx = Fixture::load("engine_prep", &name);
        let o = fx.outputs();
        let bins: Vec<_> = fx
            .f64s(&fx.inputs()["bins"])
            .into_iter()
            .map(|v| v as usize)
            .collect();
        let expected_bins: Vec<_> = design_freqs()
            .iter()
            .enumerate()
            .filter(|(i, f)| (15.0..=300.0).contains(*f) || i % 257 == 0)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(bins, expected_bins);
        assert_eq!(
            bins.len(),
            integer(&bank_fx.inputs()["bins_per_spectrum"]) as usize
        );
        let setup = Setup::new(&case.config);
        let prep = prepare(&case.measurements, &case.config, &setup).unwrap();
        ordered_values("arrival", &prep.arrival_s, &o["arrival_s"]);
        ordered_values("alignment", &prep.align_delay_ms, &o["align_delay_ms"]);
        close("base", prep.base_s, &o["base_s"]);
        for position in o["starts"].as_array().unwrap() {
            let p = integer(&position[0]) as u32;
            for row in position[1].as_array().unwrap() {
                let role = row[0].as_str().unwrap();
                let m = case.measurements.get(p, role).unwrap();
                let onset = onset_index(&m.samples);
                assert_eq!(onset, integer(&row[1]) as usize);
                assert_eq!(windowed(&m.samples, onset).1, integer(&row[2]) as usize);
                assert!(Arc::ptr_eq(
                    prep.h.get(p, role).unwrap(),
                    prep.h0.get(p, role).unwrap()
                ));
            }
        }
        assert!(o["h0_diff"].as_array().unwrap().is_empty());
        measure.spectra(&prep.h, &o["h"], &bins, &bank);
        for (window, key) in [(2000.0, "long_2000"), (500.0, "long_500")] {
            let spectra =
                long_spectra(&case.measurements, &prep, &setup.sub_filters, window).unwrap();
            measure.spectra(&spectra, &o[key], &bins, &bank);
        }
        let w = weights(prep.h.positions(), case.config.main_position);
        for ((&p, &weight), row) in prep
            .h
            .positions()
            .iter()
            .zip(&w)
            .zip(o["weights"].as_array().unwrap())
        {
            assert_eq!(p, integer(&row[0]) as u32);
            close("weight", weight, &row[1]);
        }
        let fw = freq_weights(
            prep.h.positions(),
            case.config.main_position,
            case.tuning.main_weight_lo,
        );
        for (got, row) in fw.iter().zip(o["freq_weights"].as_array().unwrap()) {
            let got: Vec<_> = bins.iter().map(|&i| got[i]).collect();
            assert_close("freq weights", &got, &fx.f64s(&row[1]), 0.0, PLATFORM_RTOL);
        }
        let mut reference = f64::INFINITY;
        for (role, expected) in pairs(&o["levels"]) {
            let level = band_level(&prep.h, role, &w, &case.config.target, 300.0, 3000.0);
            close("band level", level, expected);
            reference = reference.min(level);
        }
        close("reference", reference, &o["reference_db"]);
        match schroeder_hz(&case.measurements, &case.config) {
            Some(hz) => close("schroeder", hz, &o["schroeder_hz"]),
            None => assert!(o["schroeder_hz"].is_null()),
        }
        let f: Vec<_> = bins.iter().map(|&i| design_freqs()[i]).collect();
        assert_close(
            "target design",
            &target_db(&case.config.target, &f),
            &fx.f64s(&o["target_design"]),
            0.0,
            PLATFORM_RTOL,
        );
        assert_close(
            "target log",
            &target_db(&case.config.target, &fx.f64s(&fx.inputs()["log_grid"])),
            &fx.f64s(&o["target_log"]),
            0.0,
            PLATFORM_RTOL,
        );
    }
    println!(
        "MEASURE engine_prep spectra n={} max_abs={:.17e} max_rel_peak={:.17e} bound=1e-12",
        measure.n, measure.abs, measure.relative_peak
    );
}

#[test]
fn rejects_invalid_measurements_at_every_position_d2() {
    let case = cases::load("room");
    let setup = Setup::new(&case.config);
    let mut set = case.measurements.clone();
    set.positions[1].1[0].1.sample_rate = 44100;
    let position = set.positions[1].0;
    let role = set.positions[1].1[0].0.clone();
    let error = prepare(&set, &case.config, &setup).unwrap_err();
    assert!(matches!(
        &error,
        EngineError::MeasurementSampleRate {
            position: p, role: r, actual_hz: 44100, expected_hz: 48000,
        } if *p == position && r == &role
    ));
    assert_eq!(
        error.to_string(),
        format!("invalid input: position {position} {role}: sample rate 44100 (expected 48000)")
    );
    let prepared = prepare(&case.measurements, &case.config, &setup).unwrap();
    assert!(matches!(
        long_spectra(&set, &prepared, &[], 2000.0),
        Err(EngineError::MeasurementSampleRate {
            position: p, role: r, actual_hz: 44100, expected_hz: 48000,
        }) if p == position && r == role
    ));
    let mut config = case.config.clone();
    config.main_position = 99;
    let error = prepare(&case.measurements, &config, &setup).unwrap_err();
    assert!(matches!(
        error,
        EngineError::MissingReferencePosition { position: 99 }
    ));
    assert_eq!(error.to_string(), "invalid input: missing main position");
    config = case.config.clone();
    config.mains = vec!["absent".into()];
    let error = prepare(&case.measurements, &config, &setup).unwrap_err();
    assert!(matches!(
        error,
        EngineError::MissingMainMeasurement { position } if position == config.main_position
    ));
    assert_eq!(error.to_string(), "invalid input: no measured main");
    for samples in [vec![], vec![f64::NAN], vec![f64::INFINITY]] {
        set = case.measurements.clone();
        set.positions[0].1[0].1.samples = Samples::F64(samples.into());
        let position = set.positions[0].0;
        let role = &set.positions[0].1[0].0;
        let error = prepare(&set, &case.config, &setup).unwrap_err();
        assert!(matches!(
            &error,
            EngineError::InvalidMeasurement { position: p, role: r } if *p == position && r == role
        ));
        assert_eq!(
            error.to_string(),
            format!("invalid input: position {position} {role}: empty or nonfinite IR")
        );
    }
    for t0 in [f64::NAN, f64::INFINITY] {
        set = case.measurements.clone();
        set.positions[0].1[0].1.t0 = t0;
        assert!(matches!(
            prepare(&set, &case.config, &setup),
            Err(EngineError::InvalidMeasurement { position, role })
                if position == set.positions[0].0 && role == set.positions[0].1[0].0
        ));
    }
    set = case.measurements.clone();
    let duplicate = set.positions[1].1[0].clone();
    let position = set.positions[1].0;
    let role = duplicate.0.clone();
    set.positions[1].1.push(duplicate);
    let error = prepare(&set, &case.config, &setup).unwrap_err();
    assert!(matches!(
        &error,
        EngineError::DuplicateMeasurement { position: p, role: r } if *p == position && r == &role
    ));
    assert_eq!(
        error.to_string(),
        format!("invalid input: position {position}: duplicate role {role}")
    );
    for ms in [0.0, -1.0, f64::NAN, f64::INFINITY, 0.01] {
        assert!(matches!(
            long_spectra(&case.measurements, &prepared, &[], ms),
            Err(EngineError::Input(_))
        ));
    }
}

#[test]
fn setup_caps_lift_and_preserves_dictionary_order() {
    let mut c = cases::load("room_filters").config;
    c.taps = 1024;
    c.lift_ms = 30.0;
    c.max_latency_ms = Some(40.0);
    c.rc_range.1 = Some(0.0);
    c.peq_bands = Some(0);
    c.sub_manual.reverse();
    for (_, m) in &mut c.sub_manual {
        m.mode = SubMode::Bundle;
        m.bundle = "A".into();
        m.peq.push((90.0, 1.0, 0.0));
    }
    c.bundle_layouts = vec![(
        "A".into(),
        BundleLayout {
            kind: LayoutKind::Dba,
            rear_trim_ms: -1.0,
        },
    )];
    let s = Setup::new(&c);
    assert_eq!(s.sub_filters[0].0, "SW2");
    assert_eq!(s.sub_filters[0].1, s.sub_filters[1].1);
    assert_eq!(s.layouts[0].1.members, ["SW1", "SW2"]);
    assert_eq!(s.lift_capped_from_ms, Some(30.0));
    assert_eq!(s.lift_ms, 0.7 * 1024.0 / SR_F * 1000.0);
    assert_eq!(s.max_latency_ms, 40.0 - (30.0 - s.lift_ms));
    assert_eq!(s.rc_hi, 500.0);
    assert_eq!(s.peq_bands, 0);
    let m = &mut c.sub_manual[1].1;
    m.hp_hz = 0.0;
    m.lp_hz = 0.0;
    m.peq.clear();
    assert!(Setup::new(&c).sub_filters.is_empty());
    c.max_latency_ms = None;
    assert_eq!(Setup::new(&c).max_latency_ms, s.lift_ms + 15.0);
}

#[test]
fn onset_windows_delay_and_spectra_edges() {
    let empty = Samples::F64(Arc::from([]));
    assert_eq!(onset_index(&empty), 0);
    assert!(windowed(&empty, usize::MAX).0.is_empty());
    let ir = Samples::F64(vec![0.0, -0.1, 0.0, 1.0, -1.0].into());
    assert_eq!(onset_index(&ir), 1);
    assert_eq!(windowed(&ir, 0).1, 0);
    assert_eq!(spectrum(&[1.0]), vec![C::new(1.0, 0.0); BINS]);
    for ms in [-3.25, 0.0, 12.5] {
        for (&f, h) in design_freqs().iter().zip(delay_response(ms)) {
            let angle = (0.0 + (-2.0 * std::f64::consts::PI * f) * ms) * 0.001;
            assert_eq!(h, C::new(angle.cos(), angle.sin()));
        }
    }
    let weights = freq_weights(&[1, 3, 8], 3, 2.0);
    assert_eq!(weights[1][0], 2.0);
    assert_eq!(weights[1][2000], 5.0);
    assert!(Arc::ptr_eq(&weights[0], &weights[2]));
    let mut spectra = Spectra::default();
    spectra.set(3, "R", vec![C::new(1.0, 0.0); BINS].into());
    spectra.set(1, "L", vec![C::new(1.0, 0.0); BINS].into());
    let old = spectra.clone();
    spectra.set(1, "L", vec![C::new(2.0, 0.0); BINS].into());
    assert_eq!(spectra.positions(), &[1, 3]);
    assert_eq!(old.get(1, "L").unwrap()[0].re, 1.0);
    assert_eq!(spectra.roles_at(99).count(), 0);
    let flat = secs_types::target::TargetCurve::new(vec![]);
    assert_close(
        "missing positions excluded from level denominator",
        &[band_level(&spectra, "L", &[5.0, 1.0], &flat, 300.0, 3000.0)],
        &[20.0 * 2.0_f64.log10()],
        0.0,
        PLATFORM_RTOL,
    );
    assert!(
        band_level(
            &spectra,
            "absent",
            &[5.0, 1.0],
            &Default::default(),
            300.0,
            3000.0
        )
        .is_nan()
    );
}

#[test]
fn delay_matches_numpy_bit_for_bit() {
    let fx = Fixture::load("engine_prep", "delay");
    let bins: Vec<usize> = fx
        .f64s(&fx.inputs()["bins"])
        .into_iter()
        .map(|b| b as usize)
        .collect();
    for d in fx.outputs()["delays"].as_array().unwrap() {
        let ms = secs_oracle::number(&d["ms"]);
        let got = delay_response(ms);
        let sampled: Vec<f64> = bins.iter().flat_map(|&b| [got[b].re, got[b].im]).collect();
        let expected: Vec<f64> = fx
            .c128s(&d["sampled"])
            .into_iter()
            .flat_map(|(re, im)| [re, im])
            .collect();
        let (abs, _) = secs_oracle::errors(&sampled, &expected);
        println!("MEASURE delay ms={ms} sampled max_abs={abs:e}");
        if secs_oracle::oracle_libm() {
            let flat: Vec<f64> = got.iter().flat_map(|c| [c.re, c.im]).collect();
            fx.assert_digest(&format!("delay {ms}"), &d["digest"], &flat);
        } else {
            println!("PLATFORM delay ms={ms}: foreign libm, digest not asserted");
            assert!(abs <= 1e-12, "delay {ms}: {abs:e}");
        }
    }
}
