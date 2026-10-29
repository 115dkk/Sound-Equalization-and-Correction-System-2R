use secs_types::calibration::CalibrationSettings;

use crate::calibration::{ProjectView, engine_inputs};

use super::cases;

#[test]
fn engine_inputs_match_constructor_goldens() {
    for name in cases::names() {
        let case = cases::load(&name);
        let settings: CalibrationSettings = serde_json::from_value(case.settings.clone()).unwrap();
        let groups = cases::pairs(&case.project["groups"])
            .map(|(group, roles)| {
                (
                    group.to_owned(),
                    roles
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|role| role.as_str().unwrap().to_owned())
                        .collect(),
                )
            })
            .collect::<Vec<_>>();
        let project_xo = cases::pairs(&case.project["xo"])
            .map(|(group, value)| (group.to_owned(), secs_oracle::number(value)))
            .collect::<Vec<_>>();
        let target = case.config.target.clone();
        let resolved = engine_inputs(
            &ProjectView {
                groups: &groups,
                project_xo: &project_xo,
                target: &target,
            },
            &case.measurements,
            &settings,
        )
        .unwrap();
        assert_config(&resolved.config, &case.config, &name);
        assert!(resolved.notes.is_empty(), "{name}");
    }
}

fn assert_config(actual: &crate::EngineConfig, expected: &crate::EngineConfig, case: &str) {
    assert_eq!(actual.mains, expected.mains, "{case} mains");
    assert_eq!(actual.subs, expected.subs, "{case} subs");
    assert_eq!(actual.groups, expected.groups, "{case} groups");
    assert_eq!(actual.target, expected.target, "{case} target");
    assert_eq!(actual.xo, expected.xo, "{case} xo");
    assert_eq!(actual.main_position, expected.main_position, "{case}");
    assert_eq!(actual.taps, expected.taps, "{case}");
    assert_eq!(actual.lift_ms, expected.lift_ms, "{case}");
    assert_eq!(actual.max_boost_db, expected.max_boost_db, "{case}");
    assert_eq!(actual.peq_bands, expected.peq_bands, "{case}");
    assert_eq!(actual.max_latency_ms, expected.max_latency_ms, "{case}");
    assert_eq!(actual.rc_range, expected.rc_range, "{case}");
    assert_eq!(actual.room_volume_m3, expected.room_volume_m3, "{case}");
    assert_eq!(actual.phase_frac, expected.phase_frac, "{case}");
    assert_eq!(actual.sub_manual, expected.sub_manual, "{case}");
    assert_eq!(
        actual.sub_main_delay_ms, expected.sub_main_delay_ms,
        "{case}"
    );
    assert_eq!(actual.bundle_layouts, expected.bundle_layouts, "{case}");
    assert_eq!(
        actual.infra_boost_max_db, expected.infra_boost_max_db,
        "{case}"
    );
    assert_eq!(actual.xo_peq_on, expected.xo_peq_on, "{case}");
    assert_eq!(actual.xo_peq_lo_hz, expected.xo_peq_lo_hz, "{case}");
    assert_eq!(actual.xo_peq_hi_hz, expected.xo_peq_hi_hz, "{case}");
}

#[test]
fn calibration_replay_matches_assembled_design_bit_for_bit() {
    use crate::{
        CalibrationRequest, DesignRequest, Tuning, calibrate_with, design, normalize_measurements,
    };
    use secs_compute::Context;

    let mut case = cases::load("dolby_linear");
    // Resolution must retain its adapter note, but not the biamp measurement.
    let measurement = case.measurements.positions[0].1[0].1.clone();
    case.measurements.positions[0]
        .1
        .push(("L:LF".into(), measurement));
    let settings: CalibrationSettings = serde_json::from_value(case.settings).unwrap();
    let view = ProjectView {
        groups: &case.config.groups,
        project_xo: &case.config.xo,
        target: &case.config.target,
    };
    let ctx = Context::serial();
    let inputs = engine_inputs(&view, &case.measurements, &settings).unwrap();
    let measurements =
        normalize_measurements(&inputs.measurements, settings.sample_rate_policy, &ctx).unwrap();
    let replay = cases::Replay::for_case("dolby_linear");
    let expected = design(
        &DesignRequest {
            measurements: &measurements,
            config: &inputs.config,
            tuning: &Tuning::default(),
            subs_only: false,
            adapter_notes: &inputs.notes,
        },
        &ctx,
        &*replay.factory(),
    )
    .unwrap();
    replay.finish();
    let replay = cases::Replay::for_case("dolby_linear");
    let actual = calibrate_with(
        &CalibrationRequest {
            view: &view,
            irs: &case.measurements,
            settings: &settings,
            subs_only: false,
        },
        &ctx,
        &*replay.factory(),
    )
    .unwrap();
    replay.finish();
    assert_eq!(actual.resampled, 0);
    assert_eq!(actual.config.mains, inputs.config.mains);
    assert_eq!(
        actual.measurements.positions.len(),
        measurements.positions.len()
    );
    let actual = actual.design;
    assert_eq!(actual.chain, expected.chain);
    assert_eq!(actual.notes, expected.notes);
    assert_eq!(actual.adapter_notes, ["바이앰프 대역 측정 제외: L:LF"]);
    assert_eq!(actual.adapter_notes, expected.adapter_notes);
    assert_eq!(actual.ref_level.to_bits(), expected.ref_level.to_bits());
    assert_eq!(actual.rc_hi.to_bits(), expected.rc_hi.to_bits());
    assert_eq!(actual.mains.len(), expected.mains.len());
    for (a, b) in actual.mains.iter().zip(&expected.mains) {
        assert_eq!(a.role, b.role);
        for (a, b) in [
            (&a.before, &b.before),
            (&a.combined, &b.combined),
            (&a.after, &b.after),
        ] {
            assert_eq!(a.len(), b.len());
            assert!(
                a.iter()
                    .zip(b.iter())
                    .all(|(a, b)| (a.re.to_bits(), a.im.to_bits())
                        == (b.re.to_bits(), b.im.to_bits()))
            );
        }
    }
    assert_eq!(actual.scores.len(), expected.scores.len());
    for (a, b) in actual.scores.iter().zip(&expected.scores) {
        assert_eq!(a.role, b.role);
        assert_eq!(a.before, b.before);
        assert_eq!(a.after, b.after);
        for (a, b) in [
            (&a.spl_before, &b.spl_before),
            (&a.spl_after, &b.spl_after),
            (&a.gd_before, &b.gd_before),
            (&a.gd_after, &b.gd_after),
        ] {
            assert_eq!(a.len(), b.len());
            assert!(a.iter().zip(b).all(|(a, b)| a.to_bits() == b.to_bits()));
        }
    }
}
