#![forbid(unsafe_code)]
use secs_dsp::{
    Complex64 as C, fir, grid, iir, lowrate, minphase, peaks, response,
    smooth::{self, Fraction},
};
use secs_oracle::{Fixture, assert_close, assert_same, integer, number, rtol};
use serde_json::Value;
fn complex(fx: &Fixture, v: &Value) -> Vec<C> {
    fx.c128s(v)
        .into_iter()
        .map(|(re, im)| C::new(re, im))
        .collect()
}
fn flatten(x: &[C]) -> Vec<f64> {
    x.iter().flat_map(|z| [z.re, z.im]).collect()
}
fn close(name: &str, a: &[f64], b: &[f64], budget: f64) {
    assert_close(name, a, b, 0., rtol(budget));
}
fn floats(fx: &Fixture, key: &str) -> Vec<f64> {
    fx.f64s(&fx.inputs()[key])
}
fn selected(fx: &Fixture, x: Vec<f64>) -> Vec<f64> {
    if fx.inputs()["indices"].is_null() {
        x
    } else {
        fx.f64s(&fx.inputs()["indices"])
            .iter()
            .map(|&i| x[i as usize])
            .collect()
    }
}

#[test]
fn golden_grid() {
    for fx in Fixture::all("dsp", "grid_") {
        let i = fx.inputs();
        let got = grid::freqs(integer(&i["n"]) as usize, number(&i["sr"]));
        if fx.outputs()["error"].is_string() {
            assert!(got.is_err());
        } else {
            assert_same(&fx.id, &got.unwrap(), &fx.f64s(&fx.outputs()["value"]));
        }
    }
    for fx in Fixture::all("dsp", "frac_") {
        assert_same(
            &fx.id,
            &grid::frac_for(&floats(&fx, "f")),
            &fx.f64s(&fx.outputs()["value"]),
        );
    }
}
#[test]
fn golden_smoothing() {
    for fx in Fixture::all("dsp", "smooth_") {
        let f = floats(&fx, "f");
        let frac = floats(&fx, "frac");
        let frac = if frac.len() == 1 {
            Fraction::Scalar(frac[0])
        } else {
            Fraction::PerBin(&frac)
        };
        if fx.inputs()["complex"] == true {
            let out = smooth::smooth_complex(&f, &complex(&fx, &fx.inputs()["h"]), frac).unwrap();
            let out: Vec<_> = if fx.inputs()["indices"].is_null() {
                out
            } else {
                fx.f64s(&fx.inputs()["indices"])
                    .iter()
                    .map(|&i| out[i as usize])
                    .collect()
            };
            assert_same(
                &fx.id,
                &flatten(&out),
                &flatten(&complex(&fx, &fx.outputs()["value"])),
            );
        } else {
            let got = smooth::smooth_power(&f, &floats(&fx, "power"), frac);
            if fx.outputs()["error"].is_string() {
                assert!(got.is_err(), "{}", fx.id);
            } else {
                assert_same(
                    &fx.id,
                    &selected(&fx, got.unwrap()),
                    &fx.f64s(&fx.outputs()["value"]),
                );
            }
        }
    }
}
#[test]
fn golden_minimum_phase() {
    for fx in Fixture::all("dsp", "minphase_") {
        let got = minphase::minimum_phase(&floats(&fx, "mag"));
        if fx.outputs()["error"].is_string() {
            assert!(got.is_err());
        } else {
            close(
                &fx.id,
                &flatten(&got.unwrap()),
                &flatten(&complex(&fx, &fx.outputs()["value"])),
                1.5 * 7.884298807167249e-15,
            );
        }
    }
}
#[test]
fn golden_responses() {
    for fx in Fixture::all("dsp", "response_") {
        let i = fx.inputs();
        let f = floats(&fx, "f");
        let got = match i["fn"].as_str().unwrap() {
            "band" => response::band_filter(
                &f,
                number(&i["fc"]),
                i["kind"].as_str().unwrap(),
                i["typ"].as_str().unwrap(),
            )
            .unwrap(),
            "allpass" => response::allpass2(&f, number(&i["f0"]), number(&i["q"])),
            "hp2" => response::hp2(&f, number(&i["f0"])),
            "peaking" => response::peaking_response(
                &f,
                number(&i["f0"]),
                number(&i["q"]),
                number(&i["gain"]),
                number(&i["sr"]),
            ),
            "peaking_z" => {
                let z: Vec<_> = f
                    .iter()
                    .map(|f| C::from_polar(1., (-2. * std::f64::consts::PI * f) * (1. / 48000.)))
                    .collect();
                response::peaking_response_z(
                    &z,
                    number(&i["f0"]),
                    number(&i["q"]),
                    number(&i["gain"]),
                    number(&i["sr"]),
                )
            }
            "peq" => {
                let bands: Vec<_> = i["bands"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| (number(&v[0]), number(&v[1]), number(&v[2])))
                    .collect();
                response::peq_response(&f, &bands)
            }
            x => panic!("{x}"),
        };
        assert_same(
            &fx.id,
            &flatten(&got),
            &flatten(&complex(&fx, &fx.outputs()["value"])),
        );
    }
    for fx in Fixture::all("dsp", "gd_") {
        let got = response::gd_curve(
            &complex(&fx, &fx.inputs()["h"]),
            &floats(&fx, "f"),
            &floats(&fx, "g"),
            Fraction::Scalar(number(&fx.inputs()["frac"])),
        )
        .unwrap();
        // FFT peak selection can differ in last bits; fixed floor for a zero measured error.
        close(&fx.id, &got, &fx.f64s(&fx.outputs()["value"]), 1e-15);
    }
}
#[test]
fn golden_iir() {
    for fx in Fixture::all("dsp", "sos_edge_") {
        let sos = iir::Sos(
            floats(&fx, "sos")
                .chunks(6)
                .map(|x| x.try_into().unwrap())
                .collect(),
        );
        let got = iir::sosfilt(&sos, &floats(&fx, "x"));
        if fx.outputs()["error"].is_string() {
            assert!(got.is_err());
        } else {
            assert_same(&fx.id, &got.unwrap(), &fx.f64s(&fx.outputs()["value"]));
        }
    }
    for fx in Fixture::all("dsp", "iir_") {
        let i = fx.inputs();
        let kind = match i["kind"].as_str().unwrap() {
            "lowpass" => iir::BType::Lowpass,
            "highpass" => iir::BType::Highpass,
            _ => iir::BType::Bandpass,
        };
        let sos = iir::butter(
            integer(&i["order"]) as usize,
            &floats(&fx, "cutoff"),
            kind,
            number(&i["sr"]),
        )
        .unwrap();
        close(
            &format!("{} coefficients", fx.id),
            &sos.0.iter().flatten().copied().collect::<Vec<_>>(),
            &fx.f64s(&fx.outputs()["sos"]),
            1.5 * 1.2876814476858912e-16,
        );
        let oracle_sos = iir::Sos(
            fx.f64s(&fx.outputs()["sos"])
                .chunks(6)
                .map(|x| x.try_into().unwrap())
                .collect(),
        );
        assert_same(
            &format!("{} oracle SOS filter", fx.id),
            &iir::sosfilt(&oracle_sos, &floats(&fx, "x")).unwrap(),
            &fx.f64s(&fx.outputs()["filtered"]),
        );
        close(
            &format!("{} filter", fx.id),
            &iir::sosfilt(&sos, &floats(&fx, "x")).unwrap(),
            &fx.f64s(&fx.outputs()["filtered"]),
            1.5 * 2.2778003441747963e-15,
        );
        close(
            &format!("{} response", fx.id),
            &flatten(&iir::sosfreqz(&sos, &floats(&fx, "f"), number(&i["sr"])).unwrap()),
            &flatten(&complex(&fx, &fx.outputs()["response"])),
            1.5 * 2.120525977034049e-14,
        );
    }
}
#[test]
fn golden_firwin() {
    for fx in Fixture::all("dsp", "fir_") {
        let i = fx.inputs();
        let got = fir::firwin(
            integer(&i["taps"]) as usize,
            number(&i["fc"]),
            number(&i["fs"]),
        );
        if fx.outputs()["error"].is_string() {
            assert!(got.is_err());
        } else {
            assert_same(&fx.id, &got.unwrap(), &fx.f64s(&fx.outputs()["value"]));
        }
    }
}
#[test]
fn golden_find_peaks() {
    for fx in Fixture::all("dsp", "peaks_") {
        let height = if fx.inputs()["height"].is_null() {
            None
        } else {
            Some(number(&fx.inputs()["height"]))
        };
        let got = peaks::find_peaks(&floats(&fx, "x"), height);
        let expected: Vec<_> = fx
            .f64s(&fx.outputs()["value"])
            .into_iter()
            .map(|x| x as usize)
            .collect();
        assert_eq!(got, expected, "{}", fx.id);
    }
}
#[test]
fn golden_lowrate() {
    for fx in Fixture::all("dsp", "lowrate_") {
        let got = lowrate::to_low(
            &complex(&fx, &fx.inputs()["h"]),
            &floats(&fx, "f"),
            integer(&fx.inputs()["n"]) as usize,
        )
        .unwrap();
        close(
            &fx.id,
            &got,
            &fx.f64s(&fx.outputs()["value"]),
            1.5 * 8.326672684688674e-17,
        );
    }
    for fx in Fixture::all("dsp", "conv_") {
        let i = fx.inputs();
        let got = lowrate::conv_matrix(
            &floats(&fx, "g"),
            integer(&i["taps"]) as usize,
            integer(&i["n"]) as usize,
        );
        if fx.outputs()["error"].is_string() {
            assert!(got.is_err(), "{}", fx.id);
        } else {
            assert_same(&fx.id, &got.unwrap(), &fx.f64s(&fx.outputs()["value"]));
        }
    }
}
