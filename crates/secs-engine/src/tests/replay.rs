use std::sync::Arc;

use crate::{C, Samples, Spectra, measurement, setup::Setup, test_support};

#[test]
fn design_replay_xo_preview_matches_fresh_preparation_bits() {
    for name in ["dolby", "room_filters"] {
        let case = test_support::replay_case(name);
        let prepared =
            measurement::prepare(&case.measurements, &case.config, &Setup::new(&case.config))
                .unwrap();
        let fitters = test_support::Replay::for_case(name);
        let fresh = crate::design::design_prepared(
            &crate::DesignRequest {
                measurements: &case.measurements,
                config: &case.config,
                tuning: &test_support::load(name).tuning,
                subs_only: false,
                adapter_notes: &[],
            },
            &secs_compute::Context::serial(),
            &*fitters.factory(),
            &prepared,
            &|_, _| {},
        )
        .unwrap();
        fitters.finish();
        let replay_preview = crate::preview::XoPreview::new(&case.design);
        let fresh_preview = crate::preview::XoPreview::new(&fresh);
        for (group, roles) in case.config.groups.iter().filter(|(g, _)| g != "sub") {
            for corrected in [false, true] {
                for role in std::iter::once(None).chain(roles.iter().map(|r| Some(r.as_str()))) {
                    let actual = replay_preview.curves(&case.design, group, 80.0, corrected, role);
                    let expected = fresh_preview.curves(&fresh, group, 80.0, corrected, role);
                    for (a, b) in [
                        (&actual.grid, &expected.grid),
                        (&actual.main, &expected.main),
                        (&actual.sub, &expected.sub),
                        (&actual.sum, &expected.sum),
                        (&actual.target, &expected.target),
                    ] {
                        assert_eq!(
                            a.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                            b.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn design_replay_mimo_responses_match_fresh_preparation_bits() {
    use crate::mimo::run::{self, Support};
    for name in ["room_dba", "room_filters"] {
        let case = test_support::replay_case(name);
        let ctx = secs_compute::Context::serial();
        let prepared = crate::mimo::prepared::Prepared::new(
            &case.design,
            &case.measurements,
            &case.config,
            &ctx,
        )
        .unwrap();
        for support in [
            Support::Iir(crate::IirSupport {
                gain_db: -20.0,
                delay_ms: 3.0,
                polarity: -1,
                peq: vec![(70.0, 2.0, -3.0)],
                allpass: vec![(90.0, 0.7)],
            }),
            Support::Fir {
                taps: vec![0.1, -0.05, 0.02],
                fs_low: 600.0,
            },
        ] {
            let key = crate::MimoKey {
                fsiso: 150.0,
                f_low: None,
                exclude: vec![],
                max_supports: 0,
                fir: matches!(support, Support::Fir { .. }),
            };
            let supports = vec![(
                case.config.mains[0].clone(),
                vec![(case.config.subs[0].clone(), support)],
            )];
            let ideal = crate::MimoDesign::default();
            let expected = run::assemble(
                &case.design,
                &case.measurements,
                &case.config,
                &ideal,
                supports.clone(),
                key.clone(),
                &ctx,
            )
            .unwrap();
            let actual =
                run::assemble_prepared(&case.design, &prepared, &ideal, supports, key, &ctx)
                    .unwrap();
            assert_eq!(actual.paths, expected.paths);
            for (a, b) in [
                (&actual.final_resp, &expected.final_resp),
                (&actual.applied, &expected.applied),
            ] {
                assert_eq!(a.len(), b.len());
                for ((role, a), (other, b)) in a.iter().zip(b) {
                    assert_eq!(role, other);
                    spectrum_bits(a, b);
                }
            }
        }
    }
}

fn spectrum_bits(actual: &[C], expected: &[C]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(
            (actual.re.to_bits(), actual.im.to_bits()),
            (expected.re.to_bits(), expected.im.to_bits())
        );
    }
}

fn spectra_bits(actual: &Spectra, expected: &Spectra) {
    assert_eq!(actual.positions(), expected.positions());
    for &position in expected.positions() {
        assert_eq!(
            actual.roles_at(position).collect::<Vec<_>>(),
            expected.roles_at(position).collect::<Vec<_>>()
        );
        for role in expected.roles_at(position) {
            spectrum_bits(
                actual.get(position, role).unwrap(),
                expected.get(position, role).unwrap(),
            );
        }
    }
}

#[test]
fn design_replay_export_matches_fresh_preparation_bits() {
    for name in ["dolby", "room_filters"] {
        let case = test_support::replay_case(name);
        let design = &case.design;
        let replay = &design.replay;
        let fresh =
            measurement::prepare(&case.measurements, &case.config, &Setup::new(&case.config))
                .unwrap();
        assert_eq!(replay.base_s().to_bits(), fresh.base_s.to_bits());
        assert_eq!(replay.config().main_position, case.config.main_position);
        for (position, roles) in &case.measurements.positions {
            for (role, measurement) in roles {
                let retained = replay.measurements().get(*position, role).unwrap();
                assert_eq!(retained.sample_rate, measurement.sample_rate);
                assert_eq!(retained.t0.to_bits(), measurement.t0.to_bits());
                match (&retained.samples, &measurement.samples) {
                    (Samples::F32(a), Samples::F32(b)) => assert!(Arc::ptr_eq(a, b)),
                    (Samples::F64(a), Samples::F64(b)) => assert!(Arc::ptr_eq(a, b)),
                    _ => panic!("replay changed sample storage"),
                }
            }
        }
        let expected =
            crate::export_view::export_view_prepared(design, &case.measurements, &fresh, &|| false)
                .unwrap();
        let builds = measurement::SPECTRA_BUILT.with(|count| count.get());
        let actual = replay.export_view(design).unwrap();
        assert_eq!(
            measurement::SPECTRA_BUILT.with(|count| count.get()) - builds,
            1,
            "export builds only the long window, not another preparation"
        );
        assert_eq!(actual.base_s.to_bits(), expected.base_s.to_bits());
        assert_eq!(actual.has_sub_filters, expected.has_sub_filters);
        spectra_bits(&actual.spectra, &expected.spectra);
        let ctx = secs_compute::Context::serial();
        let builds = measurement::SPECTRA_BUILT.with(|count| count.get());
        let mimo =
            crate::mimo::prepared::Prepared::new(design, &case.measurements, &case.config, &ctx)
                .unwrap();
        assert_eq!(
            measurement::SPECTRA_BUILT.with(|count| count.get()) - builds,
            1,
            "MIMO builds only the long window, not another preparation"
        );
        spectra_bits(&mimo.spectra, &expected.spectra);
        assert_eq!(mimo.align_delay_ms.len(), fresh.align_delay_ms.len());
        for ((role, actual), (expected_role, expected)) in
            mimo.align_delay_ms.iter().zip(&fresh.align_delay_ms)
        {
            assert_eq!(role, expected_role);
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
        eprintln!(
            "REPLAY_MEMORY {name}: metadata reservation={} bytes, sample copies=0 bytes",
            crate::DesignReplay::bytes(&case.measurements, &case.config, &Setup::new(&case.config))
        );
    }
}
