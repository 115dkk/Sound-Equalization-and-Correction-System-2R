use crate::{C, DesignRequest, Note, Spectra, grid::design_freqs, test_support};
use secs_compute::Context;
use secs_types::chain::{MimoFilter, MimoPath, MimoPaths};

fn bits(a: &[C], b: &[C]) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert_eq!(
            (a.re.to_bits(), a.im.to_bits()),
            (b.re.to_bits(), b.im.to_bits())
        );
    }
}

fn spectra_bits(a: &Spectra, b: &Spectra) {
    assert_eq!(a.positions(), b.positions());
    for &p in a.positions() {
        assert_eq!(
            a.roles_at(p).collect::<Vec<_>>(),
            b.roles_at(p).collect::<Vec<_>>()
        );
        for role in a.roles_at(p) {
            bits(a.get(p, role).unwrap(), b.get(p, role).unwrap());
        }
    }
}

fn filtered_error(a: &[C], b: &[C], hp: &[C]) -> f64 {
    a.iter()
        .zip(b)
        .zip(hp)
        .map(|((a, b), hp)| {
            let expected = a * hp;
            let error = (b - expected).norm() / expected.norm().max(1e-30);
            assert!(error.is_finite());
            error
        })
        .fold(0.0, f64::max)
}

#[test]
fn input_hp_keeps_rc_ba_bits_and_filters_export_and_mimo_once() {
    for name in ["room", "room_filters"] {
        let case = test_support::load(name);
        let run = |hp| {
            let mut config = case.config.clone();
            config.input_hp_hz = hp;
            let replay = test_support::Replay::for_case(name);
            let design = crate::design(
                &DesignRequest {
                    measurements: &case.measurements,
                    config: &config,
                    tuning: &case.tuning,
                    subs_only: false,
                    adapter_notes: &[],
                },
                &Context::serial(),
                &*replay.factory(),
            )
            .unwrap();
            replay.finish();
            design
        };
        let off = run(None);
        let on = run(Some(27.0));
        assert_eq!(on.chain.input_hp_hz, Some(27.0));
        assert_eq!(on.replay.config().input_hp_hz, Some(27.0));
        assert_eq!(on.replay.setup().input_hp_hz, Some(27.0));
        let mut stripped = on.chain.clone();
        stripped.input_hp_hz = None;
        assert_eq!(
            serde_json::to_vec(&stripped).unwrap(),
            serde_json::to_vec(&off.chain).unwrap()
        );
        assert_eq!(
            on.notes
                .iter()
                .filter(|note| matches!(note, Note::InputHp { hz } if *hz == 27.0))
                .count(),
            1
        );
        assert_eq!(
            on.notes
                .iter()
                .filter(|note| !matches!(note, Note::InputHp { .. }))
                .collect::<Vec<_>>(),
            off.notes.iter().collect::<Vec<_>>()
        );
        spectra_bits(&off.spectra.raw, &on.spectra.raw);
        spectra_bits(&off.spectra.used, &on.spectra.used);
        for (a, b) in off.mains.iter().zip(&on.mains) {
            bits(&a.response, &b.response);
            bits(&a.after, &b.after);
            bits(&a.before, &b.before);
            bits(&a.combined, &b.combined);
        }
        let a = off.subs.as_ref().unwrap();
        let b = on.subs.as_ref().unwrap();
        assert_eq!(a.rows, b.rows);
        bits(&a.sum_at_reference, &b.sum_at_reference);
        if let (Some(a), Some(b)) = (&a.bus_response, &b.bus_response) {
            bits(a, b);
        }
        for (a, b) in off.scores.iter().zip(&on.scores) {
            for ((_, a), (_, b)) in super::design::metrics(&a.before)
                .iter()
                .chain(&super::design::metrics(&a.after))
                .zip(
                    super::design::metrics(&b.before)
                        .iter()
                        .chain(&super::design::metrics(&b.after)),
                )
            {
                assert_eq!(a.to_bits(), b.to_bits());
            }
            for (a, b) in a
                .spl_before
                .iter()
                .chain(&a.spl_after)
                .chain(&a.gd_before)
                .chain(&a.gd_after)
                .zip(
                    b.spl_before
                        .iter()
                        .chain(&b.spl_after)
                        .chain(&b.gd_before)
                        .chain(&b.gd_after),
                )
            {
                assert_eq!(a.to_bits(), b.to_bits());
            }
        }
        let hp = secs_dsp::response::band_filter(design_freqs(), 27.0, "HP", "BW48").unwrap();
        let a = off.replay.export_view(&off).unwrap();
        let b = on.replay.export_view(&on).unwrap();
        assert!(b.has_sub_filters);
        let mut max_relative = 0.0_f64;
        for &position in a.spectra.positions() {
            for role in a.spectra.roles_at(position) {
                max_relative = max_relative.max(filtered_error(
                    a.spectra.get(position, role).unwrap(),
                    b.spectra.get(position, role).unwrap(),
                    &hp,
                ));
            }
        }
        eprintln!("input_hp engine {name} export max_relative={max_relative:.12e}");
        assert!(max_relative < 1e-9);
        // Fresh preparation and retained replay share exactly the same cut.
        let mut different_config = on.replay.config().clone();
        different_config.peq_bands = Some(1);
        let fresh = crate::export_view(&on, &case.measurements, &different_config).unwrap();
        spectra_bits(&b.spectra, &fresh.spectra);
        let ctx = Context::serial();
        let prepared =
            crate::mimo::prepared::Prepared::new(&on, &case.measurements, on.replay.config(), &ctx)
                .unwrap();
        spectra_bits(&b.spectra, &prepared.spectra);
        let paths = MimoPaths {
            fsiso: 150.0,
            paths: vec![MimoPath {
                input: on.chain.mains[0].role.clone(),
                support: on.chain.subs[0].role.clone(),
                filter: MimoFilter::Fir {
                    taps_48k: vec![0.2, -0.01, 0.005],
                },
                low_hz: 30.0,
                delay_ms: on.chain.subs[0].delay_ms,
            }],
        };
        let before = off.replay.mimo_responses(&off, &paths).unwrap();
        let after = on.replay.mimo_responses(&on, &paths).unwrap();
        let mut max_relative = 0.0_f64;
        for ((role, a), (other, b)) in before.iter().zip(&after) {
            assert_eq!(role, other);
            max_relative = max_relative.max(filtered_error(a, b, &hp));
        }
        eprintln!("input_hp engine {name} cached MIMO max_relative={max_relative:.12e}");
        assert!(max_relative < 1e-9);
        for row in crate::late_energy(&on).unwrap() {
            let before = b.spectra.get(row.position, &row.role).unwrap();
            let after = crate::stage_responses(&on, &row.role, &b.spectra, row.position)
                .unwrap()
                .output;
            assert_eq!(
                row.before_db.to_bits(),
                crate::decay::late_db_20_150(before).to_bits()
            );
            assert_eq!(
                row.after_db.to_bits(),
                crate::decay::late_db_20_150(&after).to_bits()
            );
        }
    }
}

#[test]
fn input_hp_setting_reaches_engine_without_changing_other_fields() {
    let case = test_support::load("room");
    let view = crate::calibration::ProjectView {
        groups: &case.config.groups,
        project_xo: &case.config.xo,
        target: &case.config.target,
    };
    let settings: secs_types::calibration::CalibrationSettings =
        serde_json::from_value(case.settings).unwrap();
    let off = crate::calibration::engine_inputs(&view, &case.measurements, &settings).unwrap();
    let mut settings = settings;
    settings.input_hp_hz = Some(27.0);
    let mut on = crate::calibration::engine_inputs(&view, &case.measurements, &settings).unwrap();
    assert_eq!(on.config.input_hp_hz, Some(27.0));
    on.config.input_hp_hz = None;
    assert_eq!(on.config, off.config);
}
