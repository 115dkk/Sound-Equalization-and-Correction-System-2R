//! RC replay gates rebuild spectra from P05, not interpolated golden samples.
use super::{cases, fit::read_recorded_fit};
use crate::{
    C,
    chain::{self, BaRow},
    fit::replay_fitter,
    grid::design_freqs,
    measurement,
    rc::{self, RcInput, Trace},
    setup::Setup,
};
use secs_dsp::{pairwise_sum, response};
use secs_oracle::{Fixture, PLATFORM_RTOL, errors, integer, number, oracle_libm};
use serde_json::Value;
use std::sync::Arc;

fn check(label: &str, got: &[f64], expected: &[f64]) {
    let error = errors(got, expected);
    println!(
        "MEASURE {label} n={} max_abs={:e} max_rel={:e}",
        got.len(),
        error.0,
        error.1
    );
    // Largest measured relative error: response 5.924982726668304e-12.
    // All individual 1.5× budgets are below the mandated platform floor.
    let tolerance = (1.5_f64 * 5.924982726668304e-12).max(PLATFORM_RTOL);
    assert!(error.1 <= tolerance, "{label}: {error:?}");
}
fn complex(fx: &Fixture, label: &str, got: &[C], v: &Value) {
    check(
        label,
        &got.iter().map(|h| h.re).collect::<Vec<_>>(),
        &fx.f64s(&v["re"]),
    );
    check(
        label,
        &got.iter().map(|h| h.im).collect::<Vec<_>>(),
        &fx.f64s(&v["im"]),
    );
}
fn ranges(mask: &[bool]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, m) in mask.iter().copied().chain([false]).enumerate() {
        if m && start.is_none() {
            start = Some(i);
        }
        if !m && let Some(s) = start.take() {
            out.push((s, i));
        }
    }
    out
}
fn expected_ranges(v: &Value) -> Vec<(usize, usize)> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| (integer(&v[0]) as usize, integer(&v[1]) as usize))
        .collect()
}
fn optional(v: &Value) -> Option<f64> {
    (!v.is_null()).then(|| number(v))
}

#[test]
fn rc_recorded_candidates_and_channel() {
    for fx in Fixture::all("engine_rc", "") {
        let i = fx.inputs();
        let o = fx.outputs();
        let recipe = &i["recipe"];
        let case = cases::load(i["case"].as_str().unwrap());
        let mut tuning = case.tuning.clone();
        tuning.gd_cuts = i["gd_cuts"].as_bool().unwrap();
        let mut setup = Setup::new(&case.config);
        setup.rc_hi = number(&recipe["rc_hi"]);
        let prep = measurement::prepare(&case.measurements, &case.config, &setup).unwrap();
        let role = fx.id.rsplit('_').next().unwrap();
        // engine_rc's sampling rule is documented once in oracle/README.md.
        let indices: Vec<_> = design_freqs()
            .iter()
            .enumerate()
            .filter(|(i, f)| (15.0..=600.0).contains(*f) || i % 257 == 0)
            .map(|(i, _)| i)
            .collect();
        let positions: Vec<_> = recipe["positions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| integer(v) as u32)
            .collect();
        let rows: Vec<_> = recipe["sub_rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| BaRow {
                gain_db: number(&v[0]),
                delay_ms: number(&v[1]),
                polarity: v[2].as_i64().unwrap() as i8,
                allpass: v[3]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| (number(&v[0]), number(&v[1])))
                    .collect(),
            })
            .collect();
        let subs: Vec<_> = recipe["subs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let bus = response::peq_response(design_freqs(), &cases::peq(&recipe["bus_peq"]));
        let top = response::peq_response(design_freqs(), &cases::peq(&recipe["top_peq"]));
        let comb: Vec<(u32, Arc<[C]>)> = positions
            .iter()
            .map(|&pos| {
                let terms: Vec<_> = subs
                    .iter()
                    .zip(&rows)
                    .filter_map(|(r, row)| prep.h.get(pos, r).map(|h| (h.as_ref(), row)))
                    .collect();
                let mut sub = chain::sub_sum(&terms, design_freqs());
                for (v, p) in sub.iter_mut().zip(&bus) {
                    *v = *v * C::new(number(&recipe["bus_gain"]), 0.0) * p;
                }
                let main: Vec<_> = prep
                    .h
                    .get(pos, role)
                    .unwrap()
                    .iter()
                    .zip(&top)
                    .map(|(h, p)| h * p)
                    .collect();
                (
                    pos,
                    rc::combine(
                        &main,
                        number(&recipe["align_delay_ms"]),
                        number(&recipe["gain_db"]),
                        optional(&recipe["xo"]),
                        (!terms.is_empty()).then_some(sub.as_slice()),
                        recipe["bus_polarity"].as_i64().unwrap() as i8,
                        number(&recipe["bus_delay_ms"]),
                    )
                    .into(),
                )
            })
            .collect();
        let input = RcInput {
            role,
            comb: &comb,
            main_position: case.config.main_position,
            level_db: number(&recipe["level_db"]),
            setup: &setup,
            config: &case.config,
            tuning: &tuning,
            positions: prep.h.positions(),
        };
        let main = &comb.iter().find(|v| v.0 == input.main_position).unwrap().1;
        complex(
            &fx,
            &format!("{} combine", fx.id),
            &chain::gather(main, &indices),
            &o["combined"],
        );
        let candidates = o["candidates"].as_array().unwrap();
        for candidate in candidates {
            let calls: Vec<_> = candidate["fits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| read_recorded_fit(&fx, &v["inputs"], &v["result"]))
                .collect();
            let count = calls.len();
            let mut seen = 0;
            let mut replay = replay_fitter(calls, PLATFORM_RTOL);
            let mut fitter = |req: &crate::fit::FitRequest<'_>| {
                seen += 1;
                replay(req)
            };
            let mut trace = Trace::default();
            let out = rc::correction_traced(
                &input,
                number(&candidate["fraction"]),
                &mut fitter,
                &mut trace,
            );
            assert_eq!(seen, count);
            assert_eq!(
                ranges(&trace.no_boost),
                expected_ranges(&candidate["no_boost"])
            );
            assert_eq!(
                ranges(&trace.long_mode),
                expected_ranges(&candidate["long_mode"])
            );
            let peaks: Vec<_> = candidate["peaks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| (integer(&v[0]) as u32, integer(&v[1]) as usize))
                .collect();
            if oracle_libm() {
                assert_eq!(trace.peaks, peaks);
                assert_eq!(out.phase_strength, number(&candidate["strength"]));
            } else {
                println!(
                    "PLATFORM {} peaks={:?}/{peaks:?} strength={}/{}",
                    fx.id,
                    trace.peaks,
                    out.phase_strength,
                    number(&candidate["strength"])
                );
            }
            assert_eq!(out.peq, cases::peq(&candidate["peq"]));
            if oracle_libm() {
                assert_eq!(out.phase_band, optional(&candidate["band"]));
            }
            let expected_ladder = candidate["ladder"].as_array().unwrap();
            if oracle_libm() {
                assert_eq!(
                    trace.ladder.iter().map(|v| v.0).collect::<Vec<_>>(),
                    expected_ladder
                        .iter()
                        .map(|v| number(&v[0]))
                        .collect::<Vec<_>>()
                );
            }
            let label = format!("{} fraction={}", fx.id, number(&candidate["fraction"]));
            let chosen = o["chosen"].is_null() || candidate["fraction"] == o["chosen"];
            assert_eq!(candidate.get("design_db").is_some(), chosen);
            assert_eq!(candidate.get("response").is_some(), chosen);
            if chosen {
                check(
                    &format!("{label} design"),
                    &chain::gather(&out.design_db, &indices),
                    &fx.f64s(&candidate["design_db"]),
                );
                complex(
                    &fx,
                    &format!("{label} response"),
                    &chain::gather(&out.response, &indices),
                    &candidate["response"],
                );
            }
            let fir = &candidate["fir"];
            assert_eq!(out.fir.len(), integer(&fir["length"]) as usize);
            let peak = integer(&fir["peak"]) as usize;
            let actual_peak =
                crate::decay::first_max(&out.fir.iter().map(|v| v.abs()).collect::<Vec<_>>());
            if oracle_libm() {
                assert_eq!(actual_peak, peak);
            } else {
                println!("PLATFORM {label} FIR peak={actual_peak}/{peak}");
            }
            let full = fir["full"].as_bool().unwrap();
            let run = fx
                .id
                .strip_prefix("engine_rc/")
                .unwrap()
                .rsplit_once('_')
                .unwrap()
                .0;
            assert_eq!(
                full,
                chosen && role == "L" && matches!(run, "dolby" | "dolby_linear" | "dolby_ultra")
            );
            let samples: Vec<_> = out
                .fir
                .iter()
                .enumerate()
                .filter(|(i, _)| full || i.abs_diff(peak) <= 512 || i % 64 == 0)
                .map(|(_, &v)| v)
                .collect();
            check(&format!("{label} fir"), &samples, &fx.f64s(&fir["values"]));
            check(
                &format!("{label} fir sum"),
                &[pairwise_sum(&out.fir)],
                &[number(&fir["sum"])],
            );
            check(
                &format!("{label} fir sum_squares"),
                &[pairwise_sum(
                    &out.fir.iter().map(|v| v * v).collect::<Vec<_>>(),
                )],
                &[number(&fir["sum_squares"])],
            );
            if let Some(score) = candidate.get("gd_score") {
                check(
                    &format!("{label} GD score"),
                    &[rc::phase_score(&input, &out)],
                    &[number(score)],
                );
            } else {
                assert_eq!(candidates.len(), 1);
            }
            let expected: Vec<_> = candidate["ladder"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|v| [number(&v[0]), number(&v[1]), number(&v[2])])
                .collect();
            check(
                &format!("{label} ladder"),
                &trace
                    .ladder
                    .iter()
                    .flat_map(|v| [v.0, v.1, v.2])
                    .collect::<Vec<_>>(),
                &expected,
            );
            println!(
                "RC {label} strength={} bands={}",
                out.phase_strength,
                out.peq.len()
            );
        }
        let calls: Vec<_> = candidates
            .iter()
            .flat_map(|c| c["fits"].as_array().unwrap())
            .map(|v| read_recorded_fit(&fx, &v["inputs"], &v["result"]))
            .collect();
        let count = calls.len();
        let mut seen = 0;
        let mut gd_seen = 0;
        let mut replay = replay_fitter(calls, PLATFORM_RTOL);
        let mut fitter = |req: &crate::fit::FitRequest<'_>| {
            seen += 1;
            replay(req)
        };
        // P07 gates fit_gd_cuts' inner fits on SECS 2's spread. This seam
        // checks the corrected input and application of the recorded cuts,
        // not the optimizer again (Fitter itself accepts only PEQ requests).
        let mut cuts = |h: &[C], lo: f64, hi: f64| {
            gd_seen += 1;
            let v = &o["gd"];
            assert_eq!(lo, number(&v["lo"]));
            assert_eq!(hi, number(&v["hi"]));
            complex(
                &fx,
                &format!("{} GD input", fx.id),
                &chain::gather(h, &indices),
                &v["h"],
            );
            cases::peq(&v["result"])
        };
        let out = if tuning.gd_cuts {
            rc::channel_with_cuts(&input, &mut fitter, &mut cuts)
        } else {
            rc::channel(&input, &mut fitter)
        };
        assert_eq!(seen, count);
        assert_eq!(gd_seen, usize::from(tuning.gd_cuts));
        if oracle_libm() {
            assert_eq!(out.phase_frac_used, optional(&o["chosen"]));
        } else {
            println!(
                "PLATFORM {} GD candidate choice {:?}/{:?}",
                fx.id,
                out.phase_frac_used,
                optional(&o["chosen"])
            );
        }
        let mut all = out.out.peq.clone();
        all.extend(out.gd_cuts.clone());
        assert_eq!(all, cases::peq(&o["final_peq"]));
        let expected_notes = candidates
            .iter()
            .map(|c| c["notes"].as_array().unwrap().len())
            .sum::<usize>()
            + usize::from(!out.gd_cuts.is_empty());
        assert_eq!(
            out.out.notes.len(),
            expected_notes,
            "losing candidate notes survive"
        );
        complex(
            &fx,
            &format!("{} channel", fx.id),
            &chain::gather(&out.response, &indices),
            &o["final_response"],
        );
        println!(
            "RC {} chosen={:?} bands={} cuts={}",
            fx.id,
            out.phase_frac_used,
            out.out.peq.len(),
            out.gd_cuts.len()
        );
    }
}
