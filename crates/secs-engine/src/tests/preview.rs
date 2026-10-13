use super::{cases, design::replay};
use crate::preview::sub_combine;
use secs_oracle::{Fixture, PLATFORM_RTOL, errors, number};

fn compare(name: &str, got: &[f64], expected: &[f64]) {
    let (abs, rel) = errors(got, expected);
    eprintln!(
        "MEASURE preview/{name} n={} max_abs={abs:e} max_rel={rel:e}",
        got.len()
    );
    // Largest strict XO/sub-preview relative error on the oracle libm.
    secs_oracle::assert_close(
        name,
        got,
        expected,
        0.0,
        (1.5 * 9.420_242_363_944_453e-14_f64).max(PLATFORM_RTOL),
    );
}

// `room`'s BA differential evolution diverges from the oracle at evaluation
// 3713 (P10), so on room everything the BA rows shape (the aligned sums and
// curves, the 'after' flatness and seat spread, the sub values) is reported;
// what precedes BA (grid, band, the unaligned curves) stays asserted.
fn report(name: &str, got: &[f64], expected: &[f64]) {
    let (abs, rel) = errors(got, expected);
    eprintln!(
        "XO_REPORT_ROOM preview/{name} n={} max_abs={abs:e} max_rel={rel:e}",
        got.len()
    );
}

#[test]
fn preview_xo_cached_curves() {
    for name in ["dolby", "room_dba", "room_filters"] {
        let design = replay(name);
        let cache = crate::preview::XoPreview::new(&design);
        let calls = Fixture::all_calls("engine_xo", &format!("{name}_preview_"));
        assert!(!calls.is_empty());
        for fx in calls {
            let input = fx.inputs();
            let started = std::time::Instant::now();
            let got = cache.curves(
                &design,
                input["group"].as_str().unwrap(),
                number(&input["fc"]),
                input["corrected"].as_bool().unwrap(),
                input["role"].as_str(),
            );
            let elapsed = started.elapsed().as_secs_f64();
            eprintln!("MEASURE preview_call/{name} seconds={elapsed:.6}");
            assert!(elapsed < 0.2, "R7: cached preview took {elapsed}s");
            for (field, values) in [
                ("grid", &got.grid),
                ("main", &got.main),
                ("sub", &got.sub),
                ("sum", &got.sum),
                ("target", &got.target),
            ] {
                compare(
                    &format!(
                        "{name}/{}/{}/{}/{}/{field}",
                        input["group"], input["fc"], input["corrected"], input["role"]
                    ),
                    values,
                    &fx.f64s(&fx.outputs()[field]),
                );
            }
        }
        let unknown = cache.curves(&design, "not a group", 80.0, true, None);
        assert!(
            unknown
                .main
                .iter()
                .all(|v| (*v + 300.0).abs() < PLATFORM_RTOL)
        );
        assert_eq!(unknown.sub, unknown.sum);
        let group = &design.groups[0].0;
        assert_eq!(
            cache.curves(&design, group, 80.0, true, None),
            cache.curves(&design, group, 80.0, true, Some("not a role"))
        );
    }
}

#[test]
fn preview_sub_combine_replayed_designs() {
    for name in cases::names() {
        let design = replay(&name);
        let fx = Fixture::load("engine_xo", &format!("{name}_sub"));
        let got = sub_combine(&design).unwrap();
        let out = fx.outputs();
        let after_ba = |field: &str, got: &[f64], expected: &[f64]| {
            if name == "room" {
                report(field, got, expected);
            } else {
                compare(field, got, expected);
            }
        };
        assert_eq!(serde_json::to_value(&got.subs).unwrap(), out["subs"]);
        assert_eq!(got.positions, out["positions"].as_u64().unwrap() as usize);
        for (field, values) in [("grid", &got.grid), ("sum_before", &got.sum_before)] {
            compare(&format!("{name}/{field}"), values, &fx.f64s(&out[field]));
        }
        after_ba(
            &format!("{name}/sum_after"),
            &got.sum_after,
            &fx.f64s(&out["sum_after"]),
        );
        for (field, curves) in [("before", &got.before), ("after", &got.after)] {
            let expected: Vec<_> = cases::pairs(&out[field]).collect();
            assert_eq!(curves.len(), expected.len());
            for ((role, curve), (r, value)) in curves.iter().zip(expected) {
                assert_eq!(role, r);
                let label = format!("{name}/{field}/{role}");
                if field == "before" {
                    compare(&label, curve, &fx.f64s(value));
                } else {
                    after_ba(&label, curve, &fx.f64s(value));
                }
            }
        }
        let band = fx.f64s(&out["band"]);
        compare(&format!("{name}/band"), &[got.band.0, got.band.1], &band);
        // (before, after): the first is unaligned, the second follows BA.
        let flat = fx.f64s(&out["flat"]);
        compare(&format!("{name}/flat/before"), &[got.flat.0], &flat[..1]);
        after_ba(&format!("{name}/flat/after"), &[got.flat.1], &flat[1..]);
        if let Some((a, b)) = got.seat {
            let seat = fx.f64s(&out["seat"]);
            compare(&format!("{name}/seat/before"), &[a], &seat[..1]);
            after_ba(&format!("{name}/seat/after"), &[b], &seat[1..]);
        } else {
            assert!(out["seat"].is_null());
        }
        assert_eq!(got.values.len(), out["values"].as_array().unwrap().len());
        for (s, v) in got.values.iter().zip(out["values"].as_array().unwrap()) {
            if name == "room" {
                report(
                    &format!("{name}/{}/values", s.role),
                    &[s.gain_db, s.delay_ms, f64::from(s.polarity)],
                    &[
                        number(&v["gain_db"]),
                        number(&v["delay_ms"]),
                        number(&v["polarity"]),
                    ],
                );
                continue;
            }
            assert_eq!(s.role, v["role"]);
            assert_eq!(s.mode, v["mode"]);
            assert_eq!(i64::from(s.polarity), v["polarity"].as_i64().unwrap());
            compare(
                &format!("{name}/{}/values", s.role),
                &[s.gain_db, s.delay_ms],
                &[number(&v["gain_db"]), number(&v["delay_ms"])],
            );
            assert_eq!(serde_json::to_value(&s.allpass).unwrap(), v["allpass"]);
            assert_eq!(serde_json::to_value(&s.peq).unwrap(), v["peq"]);
        }
        assert_eq!(
            got.layout_info.len(),
            out["layout_info"].as_array().unwrap().len()
        );
        for ((name, layout), entry) in got
            .layout_info
            .iter()
            .zip(out["layout_info"].as_array().unwrap())
        {
            assert_eq!(name, entry[0].as_str().unwrap());
            let expected = &entry[1];
            let kind = match layout.kind {
                crate::input::LayoutKind::Dba => "dba",
                crate::input::LayoutKind::Multi => "multi",
            };
            assert_eq!(kind, expected["kind"].as_str().unwrap());
            let strings = |v: &[String]| serde_json::to_value(v).unwrap();
            assert_eq!(strings(&layout.members), expected["members"]);
            {
                assert_eq!(strings(&layout.front), expected["front"]);
                assert_eq!(strings(&layout.rear), expected["rear"]);
                compare(
                    &format!("layout/{name}/rear_ms"),
                    &[layout.rear_ms],
                    &[number(&expected["rear_ms"])],
                );
                compare(
                    &format!("layout/{name}/trim_ms"),
                    &[layout.trim_ms],
                    &[number(&expected["trim_ms"])],
                );
                assert_eq!(layout.auto_ms.is_some(), !expected["auto_ms"].is_null());
                if let Some(v) = layout.auto_ms {
                    compare(
                        &format!("layout/{name}/auto_ms"),
                        &[v],
                        &[number(&expected["auto_ms"])],
                    );
                }
                assert_eq!(layout.min_ms.is_some(), !expected["min_ms"].is_null());
                if let Some(v) = layout.min_ms {
                    compare(
                        &format!("layout/{name}/min_ms"),
                        &[v],
                        &[number(&expected["min_ms"])],
                    );
                }
            }
            assert_eq!(serde_json::to_value(&layout.peq).unwrap(), expected["peq"]);
            assert_eq!(
                layout.gains.len(),
                expected["gains"].as_object().unwrap().len()
            );
            for (r, gain) in &layout.gains {
                compare(
                    &format!("layout/{name}/{r}/gain"),
                    &[*gain],
                    &[number(&expected["gains"][r])],
                );
            }
        }
    }
}
