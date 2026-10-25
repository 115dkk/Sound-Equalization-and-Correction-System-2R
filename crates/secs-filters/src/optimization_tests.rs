use super::*;
use secs_types::chain::{MainChannel, MimoFilter, MimoPath, SubChannel};
use std::collections::BTreeMap;
use std::f64::consts::PI;

type C = Complex64;

fn sample() -> (SignalChain, MimoPaths) {
    let chain = SignalChain {
        sample_rate: SAMPLE_RATE,
        main_extra_ms: 2.0,
        lift_ms: 0.0,
        latency_ms: 2.0,
        mains: ["L", "R", "C"]
            .iter()
            .enumerate()
            .map(|(index, role)| MainChannel {
                role: (*role).into(),
                input_peq: vec![(70.123456, 1.234567, 0.812345)],
                fir: vec![0.8, 0.13, -0.02],
                gain_db: -0.345678 * index as f64,
                delay_ms: 3.0104166667,
                xo_hz: Some(if index < 2 { 80.12345 } else { 90.0 }),
                top_peq: vec![(130.654321, 0.876543, -0.765432)],
                align_delay_ms: 1.0,
            })
            .collect(),
        subs: ["SW1", "SW2"]
            .iter()
            .enumerate()
            .map(|(index, role)| SubChannel {
                role: (*role).into(),
                gain_db: -3.123456 - index as f64,
                delay_ms: 5.123456 + index as f64,
                polarity: if index == 0 { 1 } else { -1 },
                allpass: vec![(70.876543, 0.987654)],
                hp_hz: 22.0,
                hp_type: BandType::Bw12,
                lp_hz: 230.0,
                lp_type: BandType::Lr24,
                peq: vec![(38.654321, 1.345678, -1.876543)],
                mode: "auto".into(),
            })
            .collect(),
        sub_bus_peq: vec![(45.123456, 1.345678, -0.987654)],
    };
    let mut paths = Vec::new();
    for (index, input) in ["L", "R"].iter().enumerate() {
        paths.push(MimoPath {
            input: (*input).into(),
            support: "SW1".into(),
            low_hz: 30.123456,
            delay_ms: 5.0104166667,
            filter: MimoFilter::Fir {
                taps_48k: vec![0.1, -0.02, 0.001],
            },
        });
        paths.push(MimoPath {
            input: (*input).into(),
            support: "SW2".into(),
            low_hz: 40.123456,
            delay_ms: 6.0104166667,
            filter: MimoFilter::Iir {
                gain_db: -9.876543,
                delay_ms: index as f64 * 0.03125,
                polarity: if index == 0 { 1 } else { -1 },
                peq: vec![(111.654321, 0.987654, -0.456789)],
                allpass: vec![(125.987654, 1.765432)],
            },
        });
    }
    (
        chain,
        MimoPaths {
            fsiso: 150.123456,
            paths,
        },
    )
}

fn line_response(line: &str, f: f64, taps: &BTreeMap<String, Vec<f64>>) -> C {
    let p = line.split_whitespace().collect::<Vec<_>>();
    let parse = |i: usize| p[i].parse::<f64>().unwrap();
    if p[0] == "Preamp:" {
        return C::new(10_f64.powf(parse(1) / 20.0), 0.0);
    }
    if p[0] == "Delay:" {
        return C::from_polar(1.0, -2.0 * PI * f * parse(1) / 1000.0);
    }
    if p[0] == "Copy:" {
        return C::new(-1.0, 0.0);
    } // single-op polarity response
    if p[0] == "Convolution:" {
        return taps[p[1]]
            .iter()
            .enumerate()
            .map(|(i, tap)| {
                f64::from(*tap as f32)
                    * C::from_polar(1.0, -2.0 * PI * f * i as f64 / f64::from(SAMPLE_RATE))
            })
            .sum();
    }
    assert_eq!(p[0], "Filter:");
    let frequency = parse(4);
    let q = if p[2] == "PK" { parse(10) } else { parse(7) };
    let w = 2.0 * PI * frequency / f64::from(SAMPLE_RATE);
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
        other => panic!("unexpected filter {other}"),
    };
    let z = C::from_polar(1.0, -2.0 * PI * f / f64::from(SAMPLE_RATE));
    (b[0] + b[1] * z + b[2] * z * z) / (a[0] + a[1] * z + a[2] * z * z)
}

fn interpret(
    text: &str,
    source: &str,
    f: f64,
    plan: &FilterPlan,
    taps: &BTreeMap<String, Vec<f64>>,
) -> BTreeMap<String, C> {
    let mut values = plan
        .inputs
        .iter()
        .map(|role| {
            (
                input_channel(role),
                C::new(if role == source { 1.0 } else { 0.0 }, 0.0),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut selected = String::new();
    for line in text
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        if let Some(channel) = line.strip_prefix("Channel: ") {
            selected = channel.into();
        } else if let Some(assignments) = line.strip_prefix("Copy: ") {
            let next = assignments
                .split_whitespace()
                .map(|assignment| {
                    let (destination, expression) = assignment.split_once('=').unwrap();
                    let value = expression
                        .split('+')
                        .map(|term| {
                            if let Some((factor, source)) = term.split_once('*') {
                                factor.parse::<f64>().unwrap() * values[source]
                            } else if term == "0.0" {
                                C::new(0.0, 0.0)
                            } else {
                                values[term]
                            }
                        })
                        .sum();
                    (destination.to_owned(), value)
                })
                .collect::<Vec<_>>();
            values.extend(next);
        } else {
            *values
                .get_mut(&selected)
                .expect("selected initialized channel") *= line_response(line, f, taps);
        }
    }
    values
}

#[test]
fn apo_factoring_preserves_rendered_transfer_and_gain_rounding() {
    let (chain, mimo) = sample();
    for use_mimo in [false, true] {
        for use_lfe in [false, true] {
            let plan = make_plan_with_lfe(
                &chain,
                use_mimo.then_some(&mimo),
                use_lfe.then_some(10.0),
                120.0,
            )
            .unwrap();
            let names = plan
                .firs()
                .iter()
                .map(|(name, _)| (name.to_string(), format!("{name}.wav")))
                .collect::<Vec<_>>();
            let taps = plan
                .firs()
                .into_iter()
                .map(|(name, taps)| (format!("{name}.wav"), taps.materialize().into_owned()))
                .collect();
            let text = eqapo_filters(&plan, &names, "");
            assert_eq!(text.matches("Filter: ON LPQ Fc 80.12 Hz").count(), 2);
            assert_eq!(text.matches("Filter: ON PK Fc 45.12 Hz").count(), 1);
            assert_eq!(text.matches("Filter: ON HPQ Fc 22.00 Hz").count(), 2);
            let (inputs, outputs) = windows_channels(&plan);
            let config = eqapo_config(&plan, "filters.txt", 0.0, &inputs, &outputs, None);
            assert!(!config.contains("+VP"));
            for source in &plan.inputs {
                for f in [11.0, 25.0, 40.0, 80.0, 150.0, 311.0, 1000.0, 17000.0] {
                    let response = |ops: &[Op]| {
                        eqapo_ops(ops, "V", &names)
                            .iter()
                            .map(|line| line_response(line, f, &taps))
                            .product::<C>()
                    };
                    let actual = interpret(&text, source, f, &plan, &taps);
                    let mut expected = plan
                        .outputs
                        .iter()
                        .map(|role| (role.as_str(), C::new(0.0, 0.0)))
                        .collect::<BTreeMap<_, _>>();
                    let pre = if source == "LFE" {
                        response(&plan.lfe_ops)
                    } else {
                        response(find_ops(&plan.input_ops, source).unwrap())
                    };
                    if source != "LFE" {
                        *expected.get_mut(source.as_str()).unwrap() =
                            pre * response(find_ops(&plan.main_ops, source).unwrap());
                    }
                    for (role, feed) in &plan.subs {
                        if feed.sources.contains(source) {
                            let send = if source == "LFE" {
                                C::new(1.0, 0.0)
                            } else {
                                response(find_ops(&plan.send_ops, source).unwrap())
                            };
                            let factor = format_fixed(
                                f64::from(feed.polarity) * 10_f64.powf(feed.gain_db / 20.0),
                                6,
                                false,
                            )
                            .parse::<f64>()
                            .unwrap();
                            *expected.get_mut(role.as_str()).unwrap() =
                                pre * send * factor * response(&feed.ops);
                        }
                    }
                    for path in plan.mimo.iter().filter(|path| path.input == *source) {
                        *expected.get_mut(path.support.as_str()).unwrap() += response(&path.ops);
                    }
                    for role in &plan.outputs {
                        let channel = virtual_channel(
                            if plan.subs.iter().any(|(name, _)| name == role) {
                                "sub"
                            } else {
                                "main"
                            },
                            role,
                        );
                        let wanted = expected[role.as_str()];
                        assert!(
                            (actual[&channel] - wanted).norm() <= 2e-12 * wanted.norm().max(1.0),
                            "source {source} output {role} frequency {f}: {} != {wanted}",
                            actual[&channel]
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn prepared_sub_responses_leave_headroom_bits_unchanged() {
    let (chain, mimo) = sample();
    let plan = make_plan(&chain, Some(&mimo)).unwrap();
    let f = (0..=32768)
        .map(|i| i as f64 * 48000.0 / 65536.0)
        .collect::<Vec<_>>();
    let mut peak = 0.0_f64;
    for input in plan.inputs.iter().filter(|input| input.as_str() != "LFE") {
        for (_, values) in plan_responses(&plan, &f, input, false).unwrap() {
            for (&f, value) in f.iter().zip(values) {
                if (20.0..=20_000.0).contains(&f) {
                    peak = peak.max(value.norm());
                }
            }
        }
    }
    let reference = (20.0 * peak.log10()).max(0.0);
    assert_eq!(headroom_db(&plan).unwrap().to_bits(), reference.to_bits());
}
