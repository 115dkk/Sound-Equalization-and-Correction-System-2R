//! `chain::term` and `chain::sub_sum` against `_term` and `_sub_sum`.

use super::cases;
use crate::C;
use crate::chain::{BaRow, gather, sub_sum, term};
use crate::grid::design_freqs;
use crate::measurement::prepare;
use crate::setup::Setup;
use secs_oracle::{Fixture, errors, integer, number};
use serde_json::Value;

fn row(v: &Value) -> BaRow {
    BaRow {
        gain_db: number(&v[0]),
        delay_ms: number(&v[1]),
        polarity: number(&v[2]) as i8,
        allpass: v[3]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| (number(&a[0]), number(&a[1])))
            .collect(),
    }
}

fn flat(v: &[C]) -> Vec<f64> {
    v.iter().flat_map(|c| [c.re, c.im]).collect()
}

/// Spectra both sides build exactly: re = 1 + i/4 for the i-th sub,
/// im = k·1e-6 − 0.5 at bin k.
fn synthetic(i: usize) -> Vec<C> {
    (0..design_freqs().len())
        .map(|k| C::new(1.0 + 0.25 * i as f64, k as f64 * 1e-6 - 0.5))
        .collect()
}

#[test]
fn sub_sum_and_term_match_numpy() {
    let fx = Fixture::load("engine_prep", "sub_sum");
    let case = cases::load("room");
    let setup = Setup::new(&case.config);
    let prepared = prepare(&case.measurements, &case.config, &setup).unwrap();
    let f = design_freqs();
    let full: Vec<usize> = (0..f.len()).collect();
    let indices =
        |v: &Value| -> Vec<usize> { fx.f64s(v).into_iter().map(|i| i as usize).collect() };
    let log = indices(&fx.inputs()["log_grid"]);
    let few = indices(&fx.inputs()["few"]);
    let subs: Vec<String> = fx.inputs()["subs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect();
    let synth: Vec<Vec<C>> = (0..subs.len()).map(synthetic).collect();
    let mut worst: f64 = 0.0;
    for s in fx.outputs()["sums"].as_array().unwrap() {
        let rows: Vec<BaRow> = s["rows"].as_array().unwrap().iter().map(row).collect();
        let full_grid = s["grid"] == "full";
        let idx = if full_grid { &full } else { &log };
        let fg = gather(f, idx);
        // Arithmetic: bit for bit on identical inputs (the oracle libm's
        // cos/sin/pow; elsewhere within 1e-12).
        let hs: Vec<Vec<C>> = synth.iter().map(|h| gather(h, idx)).collect();
        let terms: Vec<(&[C], &BaRow)> = hs.iter().map(Vec::as_slice).zip(&rows).collect();
        let got = flat(&sub_sum(&terms, &fg));
        for entry in s["synthetic"].as_array().unwrap() {
            let name = format!("synthetic sub_sum {} pos {}", s["grid"], entry[0]);
            if secs_oracle::oracle_libm() {
                fx.assert_digest(&name, &entry[1], &got);
            } else {
                let first = fx.f64s(&entry[1]["first"]);
                let (abs, _) = errors(&got[..first.len()], &first);
                println!("PLATFORM {name}: foreign libm, first values max_abs={abs:e}");
                assert!(abs <= 1e-12, "{name}: {abs:e}");
            }
        }
        // Real spectra: within the FFT difference of the inputs.
        for entry in s["real"].as_array().unwrap() {
            let pos = integer(&entry[0]) as u32;
            let hs: Vec<Vec<C>> = subs
                .iter()
                .map(|r| gather(prepared.h.get(pos, r).unwrap(), idx))
                .collect();
            let terms: Vec<(&[C], &BaRow)> = hs.iter().map(Vec::as_slice).zip(&rows).collect();
            let sum = sub_sum(&terms, &fg);
            let pick: Vec<C> = if full_grid { gather(&sum, &few) } else { sum };
            let expected: Vec<f64> = fx
                .c128s(&entry[1])
                .into_iter()
                .flat_map(|(re, im)| [re, im])
                .collect();
            let peak = expected.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
            let (abs, _) = errors(&flat(&pick), &expected);
            worst = worst.max(abs / peak);
            assert!(
                abs <= 1e-12 * peak,
                "real sub_sum pos {pos}: {abs:e} of {peak:e}"
            );
        }
    }
    println!("MEASURE sub_sum real max_rel_peak={worst:e} bound=1e-12");
    let t = &fx.outputs()["term"];
    let h = &synth[subs
        .iter()
        .position(|r| r == t["role"].as_str().unwrap())
        .unwrap()];
    let got = flat(&term(h, f, &row(&t["row"])));
    if secs_oracle::oracle_libm() {
        fx.assert_digest("term", &t["digest"], &got);
    }
}
