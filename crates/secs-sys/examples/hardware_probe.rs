#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
// Adapted from Impulcifer-pip313 impulcifer-sys-win/examples/hardware_probe.rs,
// commit cac2217. MIT; Copyright (c) 2018- Jaakko Pasanen,
// 2024- 115dkk, 2025- LionLion123, SDC.
// See ../THIRD_PARTY_LICENSES.txt and docs/THIRD_PARTY.md.

use std::env;

use secs_audio::session::SessionEvent;
use secs_audio::{
    AudioBackend, CancelToken, Direction, Endpoint, MeasurementPlan, RoleOutput, ShareMode,
    SharePreference, StreamSpec, measure_position,
};

#[derive(Debug)]
struct Args {
    output: Option<String>,
    input: Option<String>,
    output_channel: u16,
    input_channel: u16,
    loopback_channel: Option<u16>,
    rate: u32,
    share: SharePreference,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            output: None,
            input: None,
            output_channel: 1,
            input_channel: 1,
            loopback_channel: None,
            rate: 48_000,
            share: SharePreference::Auto,
        }
    }
}

fn usage() -> &'static str {
    "hardware_probe [--output <substring>] [--input <substring>] [--channel N] \
     [--input-channel N] [--loopback N] [--rate 48000] \
     [--share auto|exclusive|shared]"
}

fn parse_args() -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" => parsed.output = Some(value(&mut args, "--output")?),
            "--input" => parsed.input = Some(value(&mut args, "--input")?),
            "--channel" => {
                parsed.output_channel = number(&value(&mut args, "--channel")?, "--channel")?;
            }
            "--input-channel" => {
                parsed.input_channel =
                    number(&value(&mut args, "--input-channel")?, "--input-channel")?;
            }
            "--loopback" => {
                parsed.loopback_channel =
                    Some(number(&value(&mut args, "--loopback")?, "--loopback")?);
            }
            "--rate" => parsed.rate = number(&value(&mut args, "--rate")?, "--rate")?,
            "--share" => {
                let text = value(&mut args, "--share")?;
                parsed.share = SharePreference::parse(&text)
                    .ok_or_else(|| "--share must be auto, exclusive or shared".to_string())?;
            }
            "--help" | "-h" => return Err(usage().into()),
            other => return Err(format!("unknown argument {other}\n{}", usage())),
        }
    }
    if parsed.rate == 0
        || parsed.output_channel == 0
        || parsed.input_channel == 0
        || parsed.loopback_channel == Some(0)
    {
        return Err("rates and channel numbers must be greater than zero".into());
    }
    if parsed.output.is_some() != parsed.input.is_some() {
        return Err("--output and --input must be supplied together".into());
    }
    Ok(parsed)
}

fn value(args: &mut impl Iterator<Item = String>, option: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{option} requires a value"))
}

fn number<T: std::str::FromStr>(text: &str, option: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{option} has an invalid number"))
}

fn channels(endpoint: &Endpoint, direction: Direction) -> u16 {
    match direction {
        Direction::Input => endpoint.max_input_channels,
        Direction::Output => endpoint.max_output_channels,
    }
}

fn cell(text: impl std::fmt::Display) -> String {
    text.to_string().replace(['|', '\n', '\r'], " ")
}

fn print_devices(endpoints: &[Endpoint]) {
    println!("# SECS 하드웨어 프로브 보고서\n");
    println!("## 장치\n");
    println!("| 방향 | 이름 | 호스트 API | 채널 | 기본 레이트 | 기본 장치 | 식별자 |");
    println!("|---|---|---|---:|---:|---|---|");
    for endpoint in endpoints {
        for dir in [Direction::Output, Direction::Input] {
            let channel_count = channels(endpoint, dir);
            if channel_count == 0 {
                continue;
            }
            let default = match dir {
                Direction::Input => endpoint.is_default_input,
                Direction::Output => endpoint.is_default_output,
            };
            println!(
                "| {dir:?} | {} | {} | {channel_count} | {:.0} | {} | {} |",
                cell(&endpoint.name),
                cell(&endpoint.host_api),
                endpoint.default_samplerate,
                if default { "예" } else { "아니요" },
                cell(&endpoint.id),
            );
        }
    }
    if endpoints.is_empty() {
        println!("| — | 장치 없음 | — | 0 | — | — | — |");
    }
}

fn matrix_channels(maximum: u16) -> Vec<u16> {
    let mut result = Vec::new();
    for requested in [1, 2, 8, 16] {
        let clipped = requested.min(maximum);
        if clipped > 0 && !result.contains(&clipped) {
            result.push(clipped);
        }
    }
    result
}

fn print_formats(backend: &dyn AudioBackend, endpoints: &[Endpoint]) {
    println!("\n## 형식 프로브\n");
    println!("| 방향 | 장치 | 레이트 | 채널 | 공유 모드 | 결과 | 실제 형식 | 상세 |");
    println!("|---|---|---:|---:|---|---|---|---|");
    for endpoint in endpoints {
        for dir in [Direction::Output, Direction::Input] {
            let maximum = channels(endpoint, dir);
            if maximum == 0 {
                continue;
            }
            for rate in [44_100, 48_000, 96_000] {
                for channel_count in matrix_channels(maximum) {
                    for mode in [ShareMode::Exclusive, ShareMode::SharedAutoConvert] {
                        let spec = StreamSpec {
                            sample_rate: rate,
                            channels: channel_count,
                        };
                        match backend.probe(endpoint, dir, spec, mode) {
                            Ok(result) => println!(
                                "| {dir:?} | {} | {rate} | {channel_count} | {mode:?} | {} | {} Hz / {} ch | {} |",
                                cell(&endpoint.name),
                                if result.supported { "지원" } else { "거부" },
                                result.native_sample_rate,
                                result.native_channels,
                                cell(result.detail),
                            ),
                            Err(error) => println!(
                                "| {dir:?} | {} | {rate} | {channel_count} | {mode:?} | 오류 | — | {} |",
                                cell(&endpoint.name),
                                cell(error),
                            ),
                        }
                    }
                }
            }
        }
    }
    if endpoints.is_empty() {
        println!("| — | 장치 없음 | — | — | — | — | — | — |");
    }
}

fn select_endpoint(
    endpoints: &[Endpoint],
    direction: Direction,
    filter: &str,
) -> Result<Endpoint, String> {
    let filter = filter.to_lowercase();
    endpoints
        .iter()
        .filter(|endpoint| channels(endpoint, direction) > 0)
        .filter(|endpoint| {
            endpoint.name.to_lowercase().contains(&filter)
                || endpoint.id.to_lowercase().contains(&filter)
        })
        .min_by_key(|endpoint| endpoint.name.len())
        .cloned()
        .ok_or_else(|| format!("no {direction:?} endpoint contains {filter:?}"))
}

fn run_measurement(
    backend: &dyn AudioBackend,
    endpoints: &[Endpoint],
    args: &Args,
) -> Result<(), String> {
    let output = select_endpoint(
        endpoints,
        Direction::Output,
        args.output.as_deref().expect("validated output"),
    )?;
    let input = select_endpoint(
        endpoints,
        Direction::Input,
        args.input.as_deref().expect("validated input"),
    )?;
    if args.output_channel > output.max_output_channels {
        return Err(format!(
            "output channel {} exceeds {}",
            args.output_channel, output.max_output_channels
        ));
    }
    if args.input_channel > input.max_input_channels
        || args
            .loopback_channel
            .is_some_and(|channel| channel > input.max_input_channels)
    {
        return Err(format!(
            "input or loopback channel exceeds {}",
            input.max_input_channels
        ));
    }

    let mut plan = MeasurementPlan::new(
        output.clone(),
        input.clone(),
        vec![RoleOutput {
            role: "probe".into(),
            channel: args.output_channel,
        }],
    );
    plan.input_channel = args.input_channel;
    plan.loopback_input_channel = args.loopback_channel;
    plan.sample_rate = args.rate;
    plan.sweep_length = "64k".into();
    plan.share = args.share;

    let mut input_mode = None;
    let mut output_mode = None;
    let mut underruns = 0;
    let mut drained = 0;
    let mut captured = 0;
    let measurement =
        measure_position(
            backend,
            &plan,
            1,
            &CancelToken::new(),
            &mut |event| match event {
                SessionEvent::InputReady { mode } => input_mode = Some(mode),
                SessionEvent::OutputStarted { mode } => output_mode = Some(mode),
                SessionEvent::OutputDrained(report) => {
                    underruns = report.underruns;
                    drained = report.frames_drained;
                }
                SessionEvent::InputStopped { frames } => captured = frames,
                SessionEvent::Progress { .. } => {}
            },
        )
        .map_err(|error| error.to_string())?;
    let role = measurement
        .roles
        .first()
        .ok_or_else(|| "measurement returned no role".to_string())?;

    println!("\n## 측정\n");
    println!("| 항목 | 값 |");
    println!("|---|---|");
    println!("| 출력 장치 | {} |", cell(output.name));
    println!("| 입력 장치 | {} |", cell(input.name));
    println!("| 출력 채널 | {} |", args.output_channel);
    println!("| 마이크 입력 채널 | {} |", args.input_channel);
    println!(
        "| 루프백 입력 채널 | {} |",
        args.loopback_channel
            .map_or_else(|| "없음".into(), |x| x.to_string())
    );
    println!("| 요청 모드 | {} |", args.share.as_str());
    println!("| 열린 출력 모드 | {:?} |", output_mode);
    println!("| 열린 입력 모드 | {:?} |", input_mode);
    println!("| 레이트 | {} Hz |", args.rate);
    println!(
        "| 루프백 레이턴시 | {} |",
        measurement.latency_samples.map_or_else(
            || "측정 안 함".into(),
            |samples| format!(
                "{samples} samples ({:.3} ms)",
                samples as f64 * 1000.0 / f64::from(args.rate)
            ),
        )
    );
    println!("| SNR | {:.2} dB |", role.snr_db);
    println!("| 출력 드롭아웃 | {underruns} |");
    println!("| 입력 불연속 | 0 (불연속이면 측정이 실패합니다) |");
    println!("| 출력 프레임 | {drained} |");
    println!("| 입력 프레임 | {captured} |");
    Ok(())
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let backend = secs_sys::audio::default_backend();
    let endpoints = backend.enumerate().map_err(|error| error.to_string())?;
    print_devices(&endpoints);
    print_formats(backend.as_ref(), &endpoints);
    if args.output.is_some() {
        run_measurement(backend.as_ref(), &endpoints, &args)?;
    } else {
        println!("\n## 측정\n\n장치 선택 인자가 없어 측정하지 않았습니다.");
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("hardware_probe: {error}");
        eprintln!("{}", usage());
        std::process::exit(2);
    }
}
