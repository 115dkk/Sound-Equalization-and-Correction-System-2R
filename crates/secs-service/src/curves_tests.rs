use super::*;
use secs_oracle::{Fixture, number};

fn compare(label: &str, actual: &[f64], expected: &[f64]) {
    // ADR 0004: 1.5 × measured maximum (GD: 1.2522605175036006e-10),
    // with a 2e-10 absolute floor; optimizer variability is gated by the engine.
    secs_oracle::assert_close(label, actual, expected, 2e-10, secs_oracle::PLATFORM_RTOL);
}

#[test]
fn sub_gain_warning_codes_match_settings_edit_oracle() {
    let fx = Fixture::load("service_settings_edit", "gain_warnings");
    let mut case = secs_engine_testkit::replay_case("dolby");
    let template = case.design.chain.subs[0].clone();
    case.design.chain.subs = fx.inputs()["gains"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, gain)| {
            let mut sub = template.clone();
            sub.role = format!("SW{}", i + 1);
            sub.gain_db = number(gain);
            sub
        })
        .collect();
    let rendered: Vec<_> = secs_export::sub_gain_warnings(&case.design)
        .into_iter()
        .map(warning)
        .map(|notice| {
            assert_eq!(notice.code, "sub_high_gain");
            let gain: f64 = notice.params["gain_db"].parse().unwrap();
            format!(
                "{} 게인 {gain:+.1} dB — 너무 큽니다. 서브 필터(HP, LP) 설정을 확인하세요",
                notice.params["role"]
            )
        })
        .collect();
    assert_eq!(
        serde_json::to_value(rendered).unwrap(),
        fx.outputs()["warnings"]
    );
}

#[test]
fn result_display_matches_replayed_oracle() {
    for name in ["dolby", "room_dba"] {
        let case = secs_engine_testkit::replay_case(name);
        let fx = Fixture::load("service_curves", name);
        let out = fx.outputs();
        let offset = offset(&case.design);
        compare("offset", &[offset], &[number(&out["offset"])]);
        for score in &case.design.scores {
            let spl = out["spl"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["role"] == score.role)
                .unwrap();
            let gd = out["gd"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["role"] == score.role)
                .unwrap();
            compare(
                &format!("{name}/{}/spl_before", score.role),
                &series("before", None, &score.spl_before, offset).values,
                &fx.f64s(&spl["before"]),
            );
            compare(
                &format!("{name}/{}/spl_after", score.role),
                &series("after", None, &score.spl_after, offset).values,
                &fx.f64s(&spl["after"]),
            );
            compare(
                &format!("{name}/{}/gd_before", score.role),
                &score.gd_before,
                &fx.f64s(&gd["before"]),
            );
            compare(
                &format!("{name}/{}/gd_after", score.role),
                &score.gd_after,
                &fx.f64s(&gd["after"]),
            );
            let both: Vec<_> = score
                .gd_before
                .iter()
                .chain(&score.gd_after)
                .copied()
                .collect();
            compare(
                "gd_bounds",
                &[
                    (display::percentile(&both, 1.0) - 5.0).clamp(-50.0, -5.0),
                    (display::percentile(&both, 99.0) + 10.0).clamp(30.0, 400.0),
                ],
                &fx.f64s(&gd["bounds"]),
            );
            let line = out["lines"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["role"] == score.role)
                .unwrap();
            compare(
                "metrics",
                &[score.before.in1_20, score.after.in1_20],
                &fx.f64s(&line["within_1db_pct"]),
            );
            compare(
                "gd_spread",
                &[score.before.gd_20, score.after.gd_20],
                &fx.f64s(&line["gd_spread_ms"]),
            );
            let main = case
                .design
                .chain
                .mains
                .iter()
                .find(|m| m.role == score.role)
                .unwrap();
            assert_eq!(
                main.input_peq.len() as u64,
                line["filters"]["peq"].as_u64().unwrap()
            );
            assert_eq!(
                if main.fir.len() > 1 {
                    main.fir.len()
                } else {
                    0
                } as u64,
                line["filters"]["fir_taps"].as_u64().unwrap()
            );
        }
        let cache = XoPreview::new(&case.design);
        for request in out["xo"].as_array().unwrap() {
            let view = cache.curves(
                &case.design,
                request["group"].as_str().unwrap(),
                number(&request["hz"]),
                request["corrected"].as_bool().unwrap(),
                request["role"].as_str(),
            );
            for (field, values) in [
                ("target", view.target),
                ("main", view.main),
                ("sub", view.sub),
                ("sum", view.sum),
            ] {
                compare(
                    &format!("{name}/xo/{field}"),
                    &series(field, None, &values, offset).values,
                    &fx.f64s(&request["curves"][field]),
                );
            }
        }
        let ba = sub_combine(&case.design).unwrap();
        for (field, values) in [("sum_before", &ba.sum_before), ("sum_after", &ba.sum_after)] {
            compare(
                &format!("{name}/ba/{field}"),
                &series(field, None, values, offset).values,
                &fx.f64s(&out["ba"]["curves"][field]),
            );
        }
        for (field, curves) in [("before", &ba.before), ("after", &ba.after)] {
            for (role, values) in curves {
                let pair = out["ba"]["curves"][field]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|p| p[0] == *role)
                    .unwrap();
                compare(
                    &format!("{name}/ba/{field}/{role}"),
                    &series(field, Some(role), values, offset).values,
                    &fx.f64s(&pair[1]),
                );
            }
        }
        compare(
            "flatness",
            &[ba.flat.0, ba.flat.1],
            &fx.f64s(&out["ba"]["flat"]),
        );
        if let Some((before, after)) = ba.seat {
            compare("seat", &[before, after], &fx.f64s(&out["ba"]["seat"]));
        }
        let mut settings: CalibrationSettings =
            serde_json::from_value(out["take_before"].clone()).unwrap();
        take_values(&mut settings, &case.design);
        let expected: CalibrationSettings =
            serde_json::from_value(out["take_after"].clone()).unwrap();
        for ((role, actual), (want_role, want)) in
            settings.subs_manual.iter().zip(&expected.subs_manual)
        {
            assert_eq!(role, want_role);
            assert_eq!(actual.mode, want.mode);
            assert_eq!(actual.mimo_support, want.mimo_support);
            compare(
                "take_values",
                &[actual.gain_db, actual.delay_ms],
                &[want.gain_db, want.delay_ms],
            );
            assert_eq!(actual.polarity, want.polarity);
            compare(
                "take_allpass",
                &actual
                    .allpass
                    .iter()
                    .flat_map(|(f, q)| [*f, *q])
                    .collect::<Vec<_>>(),
                &want
                    .allpass
                    .iter()
                    .flat_map(|(f, q)| [*f, *q])
                    .collect::<Vec<_>>(),
            );
        }
        compare(
            "take_main_delay",
            &[settings.sub_main_delay_ms],
            &[expected.sub_main_delay_ms],
        );
        for row in &ba.values {
            let value = out["ba"]["values"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["role"] == row.role)
                .unwrap();
            assert_eq!(row.mode, value["mode"].as_str().unwrap());
            assert_eq!(i64::from(row.polarity), value["polarity"].as_i64().unwrap());
            compare(
                "table",
                &[row.gain_db, row.delay_ms],
                &[number(&value["gain_db"]), number(&value["delay_ms"])],
            );
        }
    }
}
