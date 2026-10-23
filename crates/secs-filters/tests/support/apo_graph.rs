use secs_dsp::Complex64 as C;
use std::{collections::BTreeMap, f64::consts::PI};

const SAMPLE_RATE: f64 = 48_000.0;
const DEVICE_CHANNELS: usize = 16;

fn biquad(b: [f64; 3], a: [f64; 3], f: f64) -> C {
    let z = C::from_polar(1.0, -2.0 * PI * f / SAMPLE_RATE);
    (b[0] + b[1] * z + b[2] * z * z) / (a[0] + a[1] * z + a[2] * z * z)
}

fn line_response(line: &str, f: f64, wav: &dyn Fn(&str) -> Vec<u8>) -> C {
    let p = line.split_whitespace().collect::<Vec<_>>();
    let parse = |i: usize| p[i].parse::<f64>().unwrap();
    match p[0] {
        "Preamp:" => C::new(10_f64.powf(parse(1) / 20.0), 0.0),
        "Delay:" => C::from_polar(1.0, -2.0 * PI * f * parse(1) / 1000.0),
        "Convolution:" => {
            let bytes = wav(p[1]);
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
                .map(|(i, tap)| {
                    f64::from(f32::from_le_bytes(*tap))
                        * C::from_polar(1.0, -2.0 * PI * f * i as f64 / SAMPLE_RATE)
                })
                .sum()
        }
        "Filter:" => {
            assert_eq!(p[1], "ON");
            let frequency = parse(4);
            let q = if p[2] == "PK" { parse(10) } else { parse(7) };
            let w = 2.0 * PI * frequency / SAMPLE_RATE;
            let alpha = w.sin() / (2.0 * q);
            let c = w.cos();
            let (b, a) = match p[2] {
                "PK" => {
                    let gain = 10_f64.powf(parse(7) / 40.0);
                    (
                        [1.0 + alpha * gain, -2.0 * c, 1.0 - alpha * gain],
                        [1.0 + alpha / gain, -2.0 * c, 1.0 - alpha / gain],
                    )
                }
                "HPQ" => (
                    [(1.0 + c) / 2.0, -1.0 - c, (1.0 + c) / 2.0],
                    [1.0 + alpha, -2.0 * c, 1.0 - alpha],
                ),
                "LPQ" => (
                    [(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0],
                    [1.0 + alpha, -2.0 * c, 1.0 - alpha],
                ),
                "AP" => (
                    [1.0 - alpha, -2.0 * c, 1.0 + alpha],
                    [1.0 + alpha, -2.0 * c, 1.0 - alpha],
                ),
                other => panic!("unexpected filter {other}: {line}"),
            };
            biquad(b, a, f)
        }
        command => panic!("unexpected response command {command}: {line}"),
    }
}

fn resolve(token: &str, vars: &BTreeMap<String, i64>) -> String {
    if let Some(variable) = token.strip_prefix('`').and_then(|v| v.strip_suffix('`')) {
        vars[variable].to_string()
    } else {
        token.to_owned()
    }
}

fn expression(expression: &str, channels: &BTreeMap<String, C>, vars: &BTreeMap<String, i64>) -> C {
    expression
        .replace('-', "+-")
        .split('+')
        .filter(|term| !term.is_empty())
        .map(|term| {
            let (factor, source) = term
                .split_once('*')
                .map_or((1.0, term), |(factor, source)| {
                    (factor.parse::<f64>().unwrap(), source)
                });
            let source = resolve(source, vars);
            // Copy constants require a decimal point; integers name device
            // channels, including Eval substitutions such as `inL`.
            let value = if source.contains('.') {
                C::new(source.parse().unwrap(), 0.0)
            } else {
                *channels
                    .get(&source)
                    .unwrap_or_else(|| panic!("unknown channel {source}"))
            };
            factor * value
        })
        .sum()
}

fn condition(text: &str, vars: &BTreeMap<String, i64>) -> bool {
    text.split(" && ").all(|comparison| {
        let parts = comparison.split_whitespace().collect::<Vec<_>>();
        assert_eq!(parts.len(), 3, "condition {comparison}");
        let value = |token: &str| match token {
            "inputChannelCount" | "outputChannelCount" => DEVICE_CHANNELS as i64,
            token => token.parse().unwrap_or_else(|_| vars[token]),
        };
        match parts[1] {
            ">=" => value(parts[0]) >= value(parts[2]),
            "<=" => value(parts[0]) <= value(parts[2]),
            op => panic!("unexpected comparison {op}"),
        }
    })
}

fn run(
    text: &str,
    include: &dyn Fn(&str) -> String,
    wav: &dyn Fn(&str) -> Vec<u8>,
    f: f64,
    vars: &mut BTreeMap<String, i64>,
    channels: &mut BTreeMap<String, C>,
    selected: &mut Vec<String>,
) {
    let mut enabled = vec![true];
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('\u{feff}') {
            continue;
        }
        if let Some(test) = line.strip_prefix("If: ") {
            let parent = *enabled.last().unwrap();
            enabled.push(parent && condition(test, vars));
            continue;
        }
        if line == "EndIf:" {
            assert!(enabled.pop().is_some());
            assert!(!enabled.is_empty());
            continue;
        }
        if !enabled.last().copied().unwrap() {
            continue;
        }
        if let Some(assignment) = line.strip_prefix("Eval: ") {
            let (name, value) = assignment.split_once(" = ").unwrap();
            assert!(!value.contains(';'));
            vars.insert(name.into(), value.parse().unwrap());
        } else if let Some(names) = line.strip_prefix("Channel: ") {
            *selected = if names == "all" {
                channels.keys().cloned().collect()
            } else {
                names
                    .split_whitespace()
                    .map(|name| resolve(name, vars))
                    .collect()
            };
        } else if let Some(assignments) = line.strip_prefix("Copy: ") {
            let next = assignments
                .split_whitespace()
                .map(|assignment| {
                    let (destination, value) = assignment.split_once('=').unwrap();
                    (
                        resolve(destination, vars),
                        expression(value, channels, vars),
                    )
                })
                .collect::<Vec<_>>();
            channels.extend(next);
        } else if let Some(path) = line.strip_prefix("Include: ") {
            run(&include(path), include, wav, f, vars, channels, selected);
        } else if matches!(
            line.split_whitespace().next().unwrap(),
            "Preamp:" | "Delay:" | "Filter:" | "Convolution:"
        ) {
            let h = line_response(line, f, wav);
            for channel in selected.iter() {
                *channels.entry(channel.clone()).or_default() *= h;
            }
        } else {
            panic!("unexpected command: {line}");
        }
    }
    assert_eq!(enabled.len(), 1);
}

/// Executes config.txt and its included filter body for one device input.
pub fn transfer(
    config: &str,
    include: &dyn Fn(&str) -> String,
    wav: &dyn Fn(&str) -> Vec<u8>,
    input: usize,
    f: f64,
) -> Vec<C> {
    let mut vars = BTreeMap::new();
    let mut channels = (1..=DEVICE_CHANNELS)
        .map(|channel| {
            (
                channel.to_string(),
                C::new(if channel == input + 1 { 1.0 } else { 0.0 }, 0.0),
            )
        })
        .collect();
    let mut selected = Vec::new();
    run(
        config,
        include,
        wav,
        f,
        &mut vars,
        &mut channels,
        &mut selected,
    );
    (1..=DEVICE_CHANNELS)
        .map(|channel| channels[&channel.to_string()])
        .collect()
}

#[test]
fn copy_resolves_device_channels_and_keeps_assignments_simultaneous() {
    let config = "Eval: inL = 1
Copy: VML=`inL` VMZERO=0.0
Copy: 1=2 2=1
Copy: 3=1 4=2
Copy: 1=0.0 2=0.0
Copy: 1=VML 2=VMZERO
";
    for input in 0..DEVICE_CHANNELS {
        let got = transfer(
            config,
            &|path| panic!("unexpected include {path}"),
            &|path| panic!("unexpected WAV {path}"),
            input,
            7.0,
        );
        let unit = |source| C::new(if input == source { 1.0 } else { 0.0 }, 0.0);
        assert_eq!(got[0], unit(0));
        assert_eq!(got[1], C::new(0.0, 0.0));
        assert_eq!(got[2], unit(1));
        assert_eq!(got[3], unit(0));
    }
}
