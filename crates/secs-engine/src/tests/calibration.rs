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
