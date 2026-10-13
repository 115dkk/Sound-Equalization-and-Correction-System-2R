use super::cases;
use crate::{C, decay::*, grid::design_freqs, measurement::*, setup::Setup};
use secs_oracle::{Fixture, PLATFORM_RTOL, assert_close, integer, number, oracle_libm};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) fn recipe(v: &Value) -> Vec<C> {
    let peq = secs_dsp::response::peq_response(design_freqs(), &cases::peq(&v["bands"]));
    delay_response(number(&v["delay_ms"]))
        .iter()
        .zip(peq)
        .map(|(d, h)| d * h)
        .collect()
}

#[test]
fn ba_late_matches_oracle() {
    let mut got = Vec::new();
    let mut expected = Vec::new();
    for fx in Fixture::all_calls("engine_decay", "ba_") {
        let grid = BaLateGrid::new(number(&fx.inputs()["hi_hz"]));
        let o = fx.outputs();
        assert_eq!(grid.lin, integer(&o["lin"]) as usize);
        assert_close(
            "ba.band",
            &grid.band,
            &fx.f64s(&o["band"]),
            0.0,
            PLATFORM_RTOL,
        );
        assert_eq!(grid.fs_eff, number(&o["fs_eff"]));
        for sample in o["samples"].as_array().unwrap() {
            let spec: Vec<_> = fx
                .c128s(&sample["spec"])
                .iter()
                .map(|&(re, im)| C::new(re, im))
                .collect();
            got.push(ba_late_db(&grid, &spec));
            expected.push(number(&sample["late_db"]));
            if integer(&sample["row"]) == 0 {
                assert_eq!(
                    number(&sample["late_db"]),
                    number(&o["raw_late"][integer(&sample["position"]).to_string()])
                );
            }
        }
    }
    assert!(!got.is_empty());
    assert_close(
        "engine_decay.ba_late_db",
        &got,
        &expected,
        0.0,
        (1.5 * 4.3379872724354655e-16_f64).max(PLATFORM_RTOL),
    );
}

#[test]
fn calibration_late_matches_oracle() {
    let mut cache = BTreeMap::new();
    let mut got = Vec::new();
    let mut expected = Vec::new();
    for fx in Fixture::all_calls("engine_decay", "late_") {
        let i = fx.inputs();
        let h = if !i["recipe"].is_null() {
            recipe(&i["recipe"])
        } else {
            let name = i["case"].as_str().unwrap();
            let spectra = cache.entry(name.to_owned()).or_insert_with(|| {
                let case = cases::load(name);
                let setup = Setup::new(&case.config);
                let prep = prepare(&case.measurements, &case.config, &setup).unwrap();
                long_spectra(&case.measurements, &prep, &setup.sub_filters, 2000.0).unwrap()
            });
            spectra
                .get(integer(&i["position"]) as u32, i["role"].as_str().unwrap())
                .unwrap()
                .to_vec()
        };
        got.push(late_db_20_150(&h));
        expected.push(number(fx.outputs()));
    }
    assert!(!got.is_empty());
    assert_close(
        "engine_decay.late_db_20_150",
        &got,
        &expected,
        0.0,
        (1.5 * 3.743228207063849e-16_f64).max(PLATFORM_RTOL),
    );
}

#[test]
fn mode_t60_matches_oracle() {
    let mut got = Vec::new();
    let mut expected = Vec::new();
    let mut none = 0;
    for fx in Fixture::all_calls("engine_decay", "mode_") {
        let i = fx.inputs();
        let actual = mode_t60(
            &fx.f64s(&i["ir"]),
            number(&i["rate"]),
            number(&i["center_hz"]),
            number(&i["half_band"]),
        );
        if fx.outputs().is_null() {
            assert!(actual.is_none(), "{}", fx.id);
            none += 1;
        } else {
            got.push(actual.unwrap());
            expected.push(number(fx.outputs()));
        }
    }
    assert!(none > 0 && !got.is_empty());
    assert_close(
        "engine_decay.mode_t60",
        &got,
        &expected,
        0.0,
        (1.5 * 2.55351295663786e-15_f64).max(PLATFORM_RTOL),
    );
}

#[test]
fn late_peaks_match_oracle() {
    let mut n = 0;
    for fx in Fixture::all_calls("engine_decay", "peaks_") {
        let i = fx.inputs();
        let xs: Vec<_> = i["xs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| fx.f64s(x))
            .collect();
        let slices: Vec<_> = xs.iter().map(Vec::as_slice).collect();
        let got = late_peaks(&slices, integer(&i["count"]) as usize);
        let expected = fx.f64s(fx.outputs());
        // A late single spike has a theoretically flat spectrum: FFT roundoff
        // alone determines the selected peaks. Only this foreign-libm case reports.
        if !oracle_libm()
            && xs
                .iter()
                .all(|x| x.iter().filter(|v| **v != 0.0).count() <= 1)
        {
            println!(
                "MEASURE {} report-only single-spike FFT ties: {got:?} vs {expected:?}",
                fx.id
            );
        } else {
            assert_eq!(got, expected, "{}", fx.id);
        }
        n += got.len();
    }
    println!("MEASURE engine_decay.late_peaks n={n} exact");
}
