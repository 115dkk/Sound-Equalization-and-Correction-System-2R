use secs_dsp::Complex64 as C;
use secs_types::chain::BandType;
use serde_json::Value;
use std::{collections::BTreeMap, f64::consts::PI, path::Path};

const SAMPLE_RATE: f64 = 48_000.0;

fn biquad(b: [f64; 3], a: [f64; 3], f: f64) -> C {
    let z = C::from_polar(1.0, -2.0 * PI * f / SAMPLE_RATE);
    (b[0] + b[1] * z + b[2] * z * z) / (a[0] + a[1] * z + a[2] * z * z)
}

fn peak(f: f64, frequency: f64, q: f64, gain: f64) -> C {
    let a = 10_f64.powf(gain / 40.0);
    let w = 2.0 * PI * frequency / SAMPLE_RATE;
    let alpha = w.sin() / (2.0 * q);
    biquad(
        [1.0 + alpha * a, -2.0 * w.cos(), 1.0 - alpha * a],
        [1.0 + alpha / a, -2.0 * w.cos(), 1.0 - alpha / a],
        f,
    )
}

fn allpass(f: f64, frequency: f64, q: f64) -> C {
    let w = 2.0 * PI * frequency / SAMPLE_RATE;
    let alpha = w.sin() / (2.0 * q);
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
        BandType::Bw48 => &[
            0.5097955791041592,
            0.6013448869350453,
            0.8999762231364156,
            2.5629154477415055,
        ],
    };
    let w = 2.0 * PI * frequency / SAMPLE_RATE;
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

fn fir(f: f64, bytes: &[u8]) -> C {
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 3);
    assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 1);
    assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 32);
    let length = u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize;
    assert_eq!(bytes.len(), 44 + length);
    bytes[44..]
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .map(|(n, bytes)| {
            f64::from(f32::from_le_bytes(*bytes))
                * C::from_polar(1.0, -2.0 * PI * f * n as f64 / SAMPLE_RATE)
        })
        .sum()
}

/// YAML text and FIR WAV byte lookup to calculate input-to-all-output complex responses.
pub fn transfer(yaml: &str, wav: &dyn Fn(&str) -> Vec<u8>, input: usize, f: f64) -> Vec<C> {
    let mut filters = BTreeMap::new();
    let mut mixers = BTreeMap::new();
    let mut pipeline = Vec::new();
    let mut section = "";
    for line in yaml.lines() {
        if ["filters:", "mixers:", "pipeline:"].contains(&line) {
            section = line;
            continue;
        }
        if let Some(row) = line.strip_prefix("  - ") {
            if section == "pipeline:" {
                pipeline.push(serde_json::from_str::<Value>(row).unwrap());
            }
        } else if let Some(row) = line.strip_prefix("  ")
            && let Some((name, json)) = row.split_once(": ")
        {
            if section == "filters:" {
                filters.insert(name, serde_json::from_str::<Value>(json).unwrap());
            } else if section == "mixers:" {
                mixers.insert(name, serde_json::from_str::<Value>(json).unwrap());
            }
        }
    }
    let route_in = &mixers["route_in"];
    let n_input = route_in["channels"]["in"].as_u64().unwrap() as usize;
    let mut channels = vec![C::new(0.0, 0.0); n_input];
    channels[input] = C::new(1.0, 0.0);
    for step in pipeline {
        match step["type"].as_str().unwrap() {
            "Mixer" => {
                let mixer = &mixers[step["name"].as_str().unwrap()];
                assert_eq!(
                    mixer["channels"]["in"].as_u64().unwrap() as usize,
                    channels.len()
                );
                let mut next =
                    vec![C::new(0.0, 0.0); mixer["channels"]["out"].as_u64().unwrap() as usize];
                for row in mixer["mapping"].as_array().unwrap() {
                    let dest = row["dest"].as_u64().unwrap() as usize;
                    for source in row["sources"].as_array().unwrap() {
                        next[dest] += channels[source["channel"].as_u64().unwrap() as usize]
                            * 10_f64.powf(source["gain"].as_f64().unwrap() / 20.0)
                            * if source["inverted"] == true {
                                -1.0
                            } else {
                                1.0
                            };
                    }
                }
                channels = next;
            }
            "Filter" => {
                for name in step["names"].as_array().unwrap() {
                    let spec = &filters[name.as_str().unwrap()];
                    let p = &spec["parameters"];
                    let get = |key: &str| p[key].as_f64().unwrap();
                    let kind = spec["type"].as_str().unwrap();
                    let parameter_type = p.get("type").and_then(Value::as_str);
                    let allowed: &[&str] = match (kind, parameter_type) {
                        ("Gain", None) => &["gain", "inverted"],
                        ("Delay", None) => &["delay", "unit", "subsample"],
                        ("Biquad", Some("Peaking")) => &["type", "freq", "q", "gain"],
                        ("Biquad", Some("Allpass")) => &["type", "freq", "q"],
                        ("BiquadCombo", Some(_)) => &["type", "freq", "order"],
                        ("Conv", Some("Wav")) => &["type", "filename"],
                        other => panic!("unexpected filter parameters {other:?}: {p}"),
                    };
                    for key in p.as_object().unwrap().keys() {
                        assert!(
                            allowed.contains(&key.as_str()),
                            "unexpected {kind} parameter {key}: {p}"
                        );
                    }
                    assert_eq!(
                        p.as_object().unwrap().len(),
                        allowed.len(),
                        "missing {kind} parameter: {p}"
                    );
                    let h = match kind {
                        "Gain" => C::new(
                            10_f64.powf(get("gain") / 20.0)
                                * if p["inverted"] == true { -1.0 } else { 1.0 },
                            0.0,
                        ),
                        "Delay" => {
                            assert_eq!(p["unit"], "samples");
                            assert_eq!(p["subsample"], false);
                            C::from_polar(1.0, -2.0 * PI * f * get("delay") / SAMPLE_RATE)
                        }
                        "Biquad" if p["type"] == "Peaking" => {
                            peak(f, get("freq"), get("q"), get("gain"))
                        }
                        "Biquad" if p["type"] == "Allpass" => allpass(f, get("freq"), get("q")),
                        "BiquadCombo" => {
                            let filter_type = p["type"].as_str().unwrap();
                            let order = p["order"].as_i64().unwrap();
                            let (high, kind) = match (filter_type, order) {
                                ("LinkwitzRileyLowpass", 2) => (false, BandType::Lr12),
                                ("LinkwitzRileyLowpass", 4) => (false, BandType::Lr24),
                                ("LinkwitzRileyLowpass", 8) => (false, BandType::Lr48),
                                ("LinkwitzRileyHighpass", 2) => (true, BandType::Lr12),
                                ("LinkwitzRileyHighpass", 4) => (true, BandType::Lr24),
                                ("LinkwitzRileyHighpass", 8) => (true, BandType::Lr48),
                                ("ButterworthLowpass", 2) => (false, BandType::Bw12),
                                ("ButterworthLowpass", 4) => (false, BandType::Bw24),
                                ("ButterworthHighpass", 2) => (true, BandType::Bw12),
                                ("ButterworthHighpass", 4) => (true, BandType::Bw24),
                                other => panic!("unsupported BiquadCombo {other:?}"),
                            };
                            band(f, get("freq"), high, kind)
                        }
                        "Conv" if p["type"] == "Wav" => {
                            let name = Path::new(p["filename"].as_str().unwrap())
                                .file_name()
                                .unwrap()
                                .to_str()
                                .unwrap();
                            fir(f, &wav(name))
                        }
                        kind => panic!("unexpected filter {kind}: {p}"),
                    };
                    for channel in step["channels"].as_array().unwrap() {
                        channels[channel.as_u64().unwrap() as usize] *= h;
                    }
                }
            }
            kind => panic!("unexpected pipeline step {kind}"),
        }
    }
    channels
}
