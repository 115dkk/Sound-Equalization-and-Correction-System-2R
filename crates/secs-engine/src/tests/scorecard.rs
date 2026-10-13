use super::{cases, decay::recipe};
use crate::{grid::design_freqs, measurement::prepare, scorecard::metrics, setup::Setup};
use secs_oracle::{Fixture, PLATFORM_RTOL, assert_close, integer, number, oracle_libm};
use std::collections::BTreeMap;

#[test]
fn metrics_match_oracle() {
    let mut cache = BTreeMap::new();
    let mut results: BTreeMap<&str, (Vec<f64>, Vec<f64>)> = BTreeMap::new();
    for fx in Fixture::all_calls("engine_decay", "metrics_") {
        let i = fx.inputs();
        let f = design_freqs();
        let (h, target) = if !i["recipe"].is_null() {
            let points = i["target"].as_array().unwrap();
            let x: Vec<_> = points.iter().map(|p| number(&p[0])).collect();
            let y: Vec<_> = points.iter().map(|p| number(&p[1])).collect();
            (
                recipe(&i["recipe"]),
                f.iter()
                    .map(|f| secs_dsp::interp(f.max(1.0), &x, &y))
                    .collect::<Vec<_>>(),
            )
        } else {
            let name = i["case"].as_str().unwrap();
            let (prep, target) = cache.entry(name.to_owned()).or_insert_with(|| {
                let case = cases::load(name);
                let setup = Setup::new(&case.config);
                (
                    prepare(&case.measurements, &case.config, &setup).unwrap(),
                    crate::levels::target_db(
                        &case.config.target,
                        &f.iter().map(|f| f.max(1.0)).collect::<Vec<_>>(),
                    ),
                )
            });
            (
                prep.h0
                    .get(integer(&i["position"]) as u32, i["role"].as_str().unwrap())
                    .unwrap()
                    .to_vec(),
                target.clone(),
            )
        };
        let m = metrics(
            &h,
            f,
            &target,
            (number(&i["norm"][0]), number(&i["norm"][1])),
        );
        for (key, v) in [
            ("in1_20", m.in1_20),
            ("in2_20", m.in2_20),
            ("in1_200", m.in1_200),
            ("in2_200", m.in2_200),
            ("in1_2000", m.in1_2000),
            ("in2_2000", m.in2_2000),
        ] {
            assert_eq!(v, number(&fx.outputs()[key]), "{} {key}", fx.id);
        }
        if !oracle_libm() && i["recipe"]["bands"].as_array().is_some_and(Vec::is_empty) {
            println!(
                "MEASURE {} gd24_abs_hz report-only flat-response argmax: {} vs {}",
                fx.id,
                m.gd24_abs_hz,
                fx.outputs()["gd24_abs_hz"]
            );
        } else {
            assert_eq!(
                m.gd24_abs_hz,
                number(&fx.outputs()["gd24_abs_hz"]),
                "{} gd24_abs_hz",
                fx.id
            );
        }
        for (key, v) in [
            ("level", m.level),
            ("turns", m.turns),
            ("gd_20", m.gd_20),
            ("gd_100", m.gd_100),
            ("gd_500", m.gd_500),
            ("gd24_abs", m.gd24_abs),
            ("gd24_max", m.gd24_max),
            ("gd24_span", m.gd24_span),
            ("gd_30", m.gd_30),
            ("pre_db", m.pre_db),
        ] {
            let expected_value = number(&fx.outputs()[key]);
            // At ~−300 dB the logarithm magnifies different FFT roundoff
            // floors (also in windowed real IRs with no pre-energy).
            // Keep that budget separate from measurable pre-ringing.
            let key = if key == "pre_db" && expected_value < -250.0 {
                "pre_db_floor"
            } else {
                key
            };
            let (got, expected) = results.entry(key).or_default();
            got.push(v);
            expected.push(expected_value);
        }
    }
    assert_eq!(results.len(), 11);
    for (key, (got, expected)) in results {
        if key == "pre_db_floor" {
            // Below −250 dB the pre-ringing energy is FFT roundoff (the
            // formula floors at −300 dB): its value differs with each FFT's
            // rounding (macOS differs from the oracle by 3e-5 relative), so
            // the meaningful check is that the Rust value is at the floor too.
            let (abs, rel) = secs_oracle::errors(&got, &expected);
            println!(
                "MEASURE engine_scorecard.pre_db_floor n={} max_abs={abs:e} max_rel={rel:e} (report-only)",
                got.len()
            );
            for (g, e) in got.iter().zip(&expected) {
                assert!(
                    *g < -250.0,
                    "pre_db at the roundoff floor: got {g}, oracle {e}"
                );
            }
            continue;
        }
        let measured = 6.155272160143477e-14_f64;
        assert_close(
            &format!("engine_scorecard.{key}"),
            &got,
            &expected,
            0.0,
            (1.5 * measured).max(PLATFORM_RTOL),
        );
    }
}
