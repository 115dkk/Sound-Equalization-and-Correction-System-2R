#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Electrical transfer checks for the serialized, factored Camilla graph.
//! These are separate from the unchanged upstream formatting goldens.
mod support;

use secs_camilla::{
    DesignLabels, Device, DeviceValue, Options, make_config, pyfmt::round_ndigits,
    round_ties_even_i64, write_config,
};
use secs_dsp::Complex64 as C;
use secs_types::chain::{
    BandType, MainChannel, MimoFilter, MimoPath, MimoPaths, Peq, SignalChain, SubChannel,
};
use std::{f64::consts::PI, path::Path};

fn biquad(b: [f64; 3], a: [f64; 3], f: f64) -> C {
    let z = C::from_polar(1.0, -2.0 * PI * f / 48_000.0);
    (b[0] + b[1] * z + b[2] * z * z) / (a[0] + a[1] * z + a[2] * z * z)
}
fn peak(f: f64, frequency: f64, q: f64, gain: f64) -> C {
    let a = 10_f64.powf(round_ndigits(gain, 4) / 40.0);
    let w = 2.0 * PI * round_ndigits(frequency, 4) / 48_000.0;
    let alpha = w.sin() / (2.0 * round_ndigits(q, 5));
    biquad(
        [1.0 + alpha * a, -2.0 * w.cos(), 1.0 - alpha * a],
        [1.0 + alpha / a, -2.0 * w.cos(), 1.0 - alpha / a],
        f,
    )
}
fn ap(f: f64, frequency: f64, q: f64) -> C {
    let w = 2.0 * PI * round_ndigits(frequency, 4) / 48_000.0;
    let alpha = w.sin() / (2.0 * round_ndigits(q, 5));
    biquad(
        [1.0 - alpha, -2.0 * w.cos(), 1.0 + alpha],
        [1.0 + alpha, -2.0 * w.cos(), 1.0 - alpha],
        f,
    )
}
fn band(f: f64, frequency: f64, high: bool, kind: BandType) -> C {
    let q: &[f64] = match kind {
        BandType::Lr12 => &[0.5],
        BandType::Lr24 => &[std::f64::consts::FRAC_1_SQRT_2; 2],
        BandType::Lr48 => &[
            0.541196100146197,
            1.306562964876377,
            0.541196100146197,
            1.306562964876377,
        ],
        BandType::Bw12 => &[std::f64::consts::FRAC_1_SQRT_2],
        BandType::Bw24 => &[0.541196100146197, 1.306562964876377],
    };
    let w = 2.0 * PI * frequency / 48_000.0;
    q.iter()
        .map(|q| {
            let alpha = w.sin() / (2.0 * q);
            let c = w.cos();
            let b = if high {
                [(1.0 + c) / 2.0, -1.0 - c, (1.0 + c) / 2.0]
            } else {
                [(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0]
            };
            biquad(b, [1.0 + alpha, -2.0 * c, 1.0 - alpha], f)
        })
        .product()
}
fn peqs(f: f64, values: &[Peq]) -> C {
    values
        .iter()
        .map(|&(frequency, q, gain)| peak(f, frequency, q, gain))
        .product()
}
fn gain(db: f64) -> f64 {
    10_f64.powf(round_ndigits(db, 4) / 20.0)
}
fn delay(f: f64, ms: f64) -> C {
    C::from_polar(
        1.0,
        -2.0 * PI * f * round_ties_even_i64(ms * 48_000.0 / 1000.0) as f64 / 48_000.0,
    )
}
fn fir(f: f64, taps: &[f64]) -> C {
    taps.iter()
        .enumerate()
        .map(|(n, tap)| {
            f64::from(*tap as f32) * C::from_polar(1.0, -2.0 * PI * f * n as f64 / 48_000.0)
        })
        .sum()
}
fn fixed(f: f64, sub: &SubChannel) -> C {
    let mut h = peqs(f, &sub.peq);
    if sub.hp_hz != 0.0 {
        h *= band(f, sub.hp_hz, true, sub.hp_type);
    }
    if sub.lp_hz != 0.0 {
        h *= band(f, sub.lp_hz, false, sub.lp_type);
    }
    h
}

fn sample() -> (SignalChain, MimoPaths) {
    let mains = ["L", "R", "C"]
        .iter()
        .enumerate()
        .map(|(i, role)| MainChannel {
            role: (*role).into(),
            input_peq: vec![(73.123456, 1.432198, -1.01347)],
            fir: vec![0.8, 0.11, -0.031, 0.008],
            gain_db: -0.37 * i as f64,
            delay_ms: 3.010416666666667 + i as f64 / 48.0,
            xo_hz: Some(if i < 2 { 80.12345 } else { 95.0 }),
            top_peq: vec![(120.987654, 0.812345, 0.37)],
            align_delay_ms: 2.0,
        })
        .collect();
    let subs = ["SW1", "SW2"]
        .iter()
        .enumerate()
        .map(|(i, role)| SubChannel {
            role: (*role).into(),
            gain_db: -3.123456 - i as f64,
            delay_ms: 4.32 + i as f64,
            polarity: if i == 0 { 1 } else { -1 },
            allpass: vec![(65.124987, 1.765432)],
            hp_hz: 22.0,
            hp_type: BandType::Bw12,
            lp_hz: 210.0,
            lp_type: BandType::Lr24,
            peq: vec![(41.765432, 2.123456, -2.345678)],
            mode: "auto".into(),
        })
        .collect();
    let chain = SignalChain {
        sample_rate: 48_000,
        mains,
        subs,
        sub_bus_peq: vec![
            (36.654321, 1.123456, -0.234567),
            (98.012345, 1.987654, 0.765432),
        ],
        main_extra_ms: 2.0,
        lift_ms: 0.0,
        latency_ms: 2.0,
    };
    let iir = |input: &str, support: &str, path_delay: f64, internal_delay: f64| MimoPath {
        input: input.into(),
        support: support.into(),
        low_hz: 31.123456,
        delay_ms: path_delay,
        filter: MimoFilter::Iir {
            gain_db: -11.123456,
            delay_ms: internal_delay,
            polarity: if input == "R" { -1 } else { 1 },
            peq: vec![(85.123456, 1.314159, -0.712345)],
            allpass: vec![(111.987654, 0.812345)],
        },
    };
    let mut paths = vec![
        iir("L", "SW1", 5.427083333333334, 0.0),
        iir("R", "SW1", 5.447916666666666, 0.0),
        iir("C", "SW1", 4.0, 0.13541666666666666),
        iir("L", "R", 3.0, 0.03125),
    ];
    // A mixed FIR/IIR support and unequal HP limits must remain distinct groups.
    for (input, low) in [("L", 43.123456), ("R", 43.123456), ("C", 51.0)] {
        paths.push(MimoPath {
            input: input.into(),
            support: "SW2".into(),
            low_hz: low,
            delay_ms: 5.010416666666667,
            filter: MimoFilter::Fir {
                taps_48k: vec![0.14, -0.021, 0.0034, 0.0012],
            },
        });
    }
    (
        chain,
        MimoPaths {
            fsiso: 150.12345,
            paths,
        },
    )
}

fn expected(chain: &SignalChain, mimo: &MimoPaths, input: usize, f: f64, lfe: bool) -> Vec<C> {
    let n_main = chain.mains.len();
    let mut out = vec![C::new(0.0, 0.0); n_main + chain.subs.len()];
    let mut bass = C::new(0.0, 0.0);
    if input < n_main {
        let main = &chain.mains[input];
        let pre = peqs(f, &main.input_peq)
            * if main.fir.len() > 1 {
                fir(f, &main.fir)
            } else {
                C::new(1.0, 0.0)
            };
        out[input] = pre * gain(main.gain_db) * peqs(f, &main.top_peq) * delay(f, main.delay_ms);
        if let Some(xo) = main.xo_hz.filter(|xo| *xo != 0.0) {
            out[input] *= band(f, xo, true, BandType::Lr24);
            bass = pre * band(f, xo, false, BandType::Lr24);
        }
    } else if lfe {
        bass = gain(10.0) * band(f, 120.0, false, BandType::Lr24);
    }
    for (i, sub) in chain.subs.iter().enumerate() {
        out[n_main + i] = bass
            * gain(sub.gain_db)
            * f64::from(sub.polarity)
            * peqs(f, &chain.sub_bus_peq)
            * fixed(f, sub)
            * sub
                .allpass
                .iter()
                .map(|&(frequency, q)| ap(f, frequency, q))
                .product::<C>()
            * delay(f, sub.delay_ms);
    }
    if input < n_main {
        for path in mimo
            .paths
            .iter()
            .filter(|path| path.input == chain.mains[input].role)
        {
            let mut h = band(f, round_ndigits(path.low_hz, 3), true, BandType::Lr24);
            h *= match &path.filter {
                MimoFilter::Iir {
                    gain_db,
                    delay_ms,
                    polarity,
                    peq,
                    allpass,
                } => {
                    gain(*gain_db)
                        * f64::from(*polarity)
                        * peqs(f, peq)
                        * allpass
                            .iter()
                            .map(|&(frequency, q)| ap(f, frequency, q))
                            .product::<C>()
                        * band(f, mimo.fsiso, false, BandType::Lr24)
                        * delay(f, path.delay_ms + delay_ms)
                }
                MimoFilter::Fir { taps_48k } => fir(f, taps_48k) * delay(f, path.delay_ms),
                MimoFilter::FirLowRate { taps_low, fs_low } => {
                    fir(f, &secs_dsp::fir::lowrate_to_48k(taps_low, *fs_low))
                        * delay(f, path.delay_ms)
                }
            };
            let output = chain
                .mains
                .iter()
                .position(|main| main.role == path.support)
                .unwrap_or_else(|| {
                    n_main
                        + chain
                            .subs
                            .iter()
                            .position(|sub| sub.role == path.support)
                            .unwrap()
                });
            if output >= n_main {
                h *= fixed(f, &chain.subs[output - n_main]);
            }
            out[output] += h;
        }
    }
    out
}

#[test]
fn factored_graph_preserves_every_input_to_output_transfer_and_integer_delay() {
    let (chain, mimo) = sample();
    let mut max_expected = 0.0_f64;
    for include_mimo in [false, true] {
        for lfe in [false, true] {
            let options = Options {
                lfe_gain_db: lfe.then_some(10.0),
                ..Options::default()
            };
            let selected = if include_mimo {
                mimo.clone()
            } else {
                MimoPaths {
                    fsiso: mimo.fsiso,
                    paths: vec![],
                }
            };
            let config = make_config(
                &chain,
                Path::new("/tmp/graph"),
                Path::new("/tmp/graph/configs"),
                "secs_",
                &options,
                Some(&selected),
            )
            .unwrap();
            assert_eq!(config.yaml.matches("  bass_xo").count(), 2);
            assert_eq!(config.yaml.matches("  bass_bus_peq").count(), 2);
            for input in 0..chain.mains.len() + usize::from(lfe) {
                for f in [
                    7.0, 20.0, 31.0, 43.0, 80.0, 120.0, 173.0, 350.0, 1000.0, 8765.0, 20_000.0,
                ] {
                    let expected = expected(&chain, &selected, input, f, lfe);
                    let actual = support::camilla_graph::transfer(
                        &config.yaml,
                        &|name| {
                            config
                                .coeffs
                                .iter()
                                .find(|(path, _)| path.file_name().unwrap() == name)
                                .unwrap()
                                .1
                                .clone()
                        },
                        input,
                        f,
                    );
                    assert_eq!(actual.len(), expected.len());
                    for (output, (got, wanted)) in actual.iter().zip(&expected).enumerate() {
                        max_expected = max_expected.max(wanted.norm());
                        assert!(
                            (*got - *wanted).norm() <= 2e-12 * wanted.norm().max(1.0),
                            "input {input} output {output} f {f}: {got} != {wanted}"
                        );
                    }
                }
            }
        }
    }
    assert!(
        max_expected >= 1e-3,
        "expected optimized graph responses are all silent ({max_expected})"
    );
}

#[test]
fn controller_metadata_headroom_bypass_labels_and_devices_follow_v4_schema() {
    let (chain, mimo) = sample();
    let out = std::env::temp_dir().join(format!("secs-camilla-controller-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let design = DesignLabels {
        project_name: Some("Oracle room".into()),
        design_name: "저지연".into(),
        design_id: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
        bypass: false,
    };
    let baseline_options = Options {
        headroom: true,
        ..Options::default()
    };
    let baseline = make_config(
        &chain,
        &out,
        &out.join("configs"),
        "secs_",
        &baseline_options,
        Some(&mimo),
    )
    .unwrap();
    let options = Options {
        capture: Some(Device(vec![
            ("type".into(), DeviceValue::Str("Wasapi".into())),
            ("channels".into(), DeviceValue::Int(4)),
            ("device".into(), DeviceValue::Str("Loopback".into())),
            ("format".into(), DeviceValue::Str("F32".into())),
            ("exclusive".into(), DeviceValue::Bool(false)),
            ("loopback".into(), DeviceValue::Bool(true)),
        ])),
        playback: Some(Device(vec![
            ("type".into(), DeviceValue::Str("Asio".into())),
            ("channels".into(), DeviceValue::Int(5)),
            ("device".into(), DeviceValue::Str("Test ASIO".into())),
        ])),
        design: Some(design),
        headroom: true,
        labels: true,
        bypass: true,
        ..Options::default()
    };
    let configured = make_config(
        &chain,
        &out,
        &out.join("configs"),
        "secs_",
        &options,
        Some(&mimo),
    )
    .unwrap();
    assert!(
        configured
            .yaml
            .contains("title: \"SECS 2R \\u00b7 Oracle room \\u00b7 \\uc800\\uc9c0\\uc5f0\"")
    );
    assert!(configured.yaml.contains("secs-design 0123456789abcdef"));
    assert!(
        configured
            .yaml
            .contains("\"labels\": [\"L\", \"R\", \"C\", \"LFE\"]")
    );
    assert!(configured.yaml.contains("\"type\": \"Wasapi\""));
    assert!(configured.yaml.contains("\"format\": \"F32\""));
    assert!(configured.yaml.contains("\"loopback\": true"));
    assert!(configured.yaml.contains("\"type\": \"Asio\""));
    assert!(
        configured
            .yaml
            .contains("camilladsp-windows-asio-amd64.zip")
    );

    let pipeline = configured
        .yaml
        .lines()
        .filter_map(|line| line.strip_prefix("  - "))
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(pipeline[0]["description"], "secs:headroom");
    assert_eq!(pipeline[0]["names"][0], "secs_headroom");
    let correction = pipeline
        .iter()
        .filter(|step| step["description"] == "secs:correction")
        .collect::<Vec<_>>();
    assert!(!correction.is_empty());
    assert!(correction.iter().all(|step| step["bypassed"] == false));
    assert!(
        correction
            .iter()
            .all(|step| step["names"].as_array().unwrap().len() == 1)
    );
    let support = pipeline
        .iter()
        .filter(|step| step["description"] == "secs:mimo")
        .collect::<Vec<_>>();
    assert!(!support.is_empty());
    assert!(support.iter().all(|step| step.get("bypassed").is_none()));

    for input in 0..4 {
        for frequency in [20.0, 80.0, 1000.0, 20_000.0] {
            let before = support::camilla_graph::transfer(
                &baseline.yaml,
                &|name| {
                    baseline
                        .coeffs
                        .iter()
                        .find(|(path, _)| path.file_name().unwrap() == name)
                        .unwrap()
                        .1
                        .clone()
                },
                input,
                frequency,
            );
            let after = support::camilla_graph::transfer(
                &configured.yaml,
                &|name| {
                    configured
                        .coeffs
                        .iter()
                        .find(|(path, _)| path.file_name().unwrap() == name)
                        .unwrap()
                        .1
                        .clone()
                },
                input,
                frequency,
            );
            assert_eq!(before, after);
        }
    }

    let written = write_config(&chain, &out, "oracle", &options, Some(&mimo)).unwrap();
    let bypass = written
        .iter()
        .find(|path| path.file_name().unwrap() == "oracle_bypass.yml")
        .unwrap();
    let bypass = std::fs::read_to_string(bypass).unwrap();
    assert!(bypass.contains(" (\\uc6b0\\ud68c)\""));
    assert!(bypass.contains(
        "secs-design 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef bypass"
    ));
    assert!(!bypass.contains("secs:correction"));
    assert!(!bypass.contains("secs:mimo"));
    assert!(bypass.contains("main_L_delay"));
    assert!(bypass.contains("main_L_hp"));
    assert!(bypass.contains("sub_SW1_delay"));
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn compact_fir_exports_the_compatible_full_rate_waveform() {
    let (chain, mut mimo) = sample();
    let taps_low = vec![0.0, 0.3, -0.08, 0.01, 0.0];
    let full = secs_dsp::fir::lowrate_to_48k(&taps_low, 600.0);
    mimo.paths.truncate(1);
    mimo.paths[0].filter = MimoFilter::FirLowRate {
        taps_low,
        fs_low: 600.0,
    };
    let out = Path::new("/tmp/compact");
    let low = make_config(
        &chain,
        out,
        &out.join("configs"),
        "secs_",
        &Options::default(),
        Some(&mimo),
    )
    .unwrap();
    mimo.paths[0].filter = MimoFilter::Fir { taps_48k: full };
    let expanded = make_config(
        &chain,
        out,
        &out.join("configs"),
        "secs_",
        &Options::default(),
        Some(&mimo),
    )
    .unwrap();
    assert_eq!(low.yaml, expanded.yaml);
    assert_eq!(low.coeffs, expanded.coeffs);
}
