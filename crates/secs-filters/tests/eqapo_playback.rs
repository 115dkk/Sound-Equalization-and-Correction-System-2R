#![forbid(unsafe_code)]

use secs_camilla::pyfmt::format_fixed;
use secs_dsp::Complex64;
use secs_dsp::fft::{irfft, rfft};
use secs_filters::{
    FilterPlan, FilterRun, Op, export_eqapo, make_plan, plan_responses, windows_channels,
};
use secs_oracle::Fixture;
use secs_types::chain::{
    BandType, MainChannel, MimoFilter, MimoPath, MimoPaths, SignalChain, SubChannel,
};
use serde_json::Value;
use std::f64::consts::PI;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const SAMPLE_RATE: u32 = 48_000;
const PERIOD: usize = 131_072;
const REL: f64 = 2e-3;
const ABS: f64 = 1e-4;
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "secs_eqapo_playback_{label}_{}_{id}",
            std::process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn number(value: &Value) -> f64 {
    value.as_f64().unwrap()
}

fn peqs(value: &Value) -> Vec<(f64, f64, f64)> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|band| {
            let band = band.as_array().unwrap();
            (number(&band[0]), number(&band[1]), number(&band[2]))
        })
        .collect()
}

fn allpasses(value: &Value) -> Vec<(f64, f64)> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|band| {
            let band = band.as_array().unwrap();
            (number(&band[0]), number(&band[1]))
        })
        .collect()
}

fn band(value: &Value) -> BandType {
    BandType::from_name(value.as_str().unwrap())
}

fn chain_value(fixture: &Fixture, value: &Value) -> SignalChain {
    SignalChain {
        sample_rate: value["sample_rate"].as_u64().unwrap() as u32,
        mains: value["mains"]
            .as_array()
            .unwrap()
            .iter()
            .map(|main| MainChannel {
                role: main["role"].as_str().unwrap().into(),
                input_peq: peqs(&main["input_peq"]),
                fir: fixture.f64s(&main["fir"]),
                gain_db: number(&main["gain_db"]),
                delay_ms: number(&main["delay_ms"]),
                xo_hz: main["xo_hz"].as_f64(),
                top_peq: peqs(&main["top_peq"]),
                align_delay_ms: number(&main["align_delay_ms"]),
            })
            .collect(),
        subs: value["subs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|sub| SubChannel {
                role: sub["role"].as_str().unwrap().into(),
                gain_db: number(&sub["gain_db"]),
                delay_ms: number(&sub["delay_ms"]),
                polarity: sub["polarity"].as_i64().unwrap() as i8,
                allpass: allpasses(&sub["allpass"]),
                hp_hz: number(&sub["hp_hz"]),
                hp_type: band(&sub["hp_type"]),
                lp_hz: number(&sub["lp_hz"]),
                lp_type: band(&sub["lp_type"]),
                peq: peqs(&sub["peq"]),
                mode: sub["mode"].as_str().unwrap().into(),
            })
            .collect(),
        sub_bus_peq: peqs(&value["sub_bus_peq"]),
        main_extra_ms: number(&value["main_extra_ms"]),
        lift_ms: number(&value["lift_ms"]),
        latency_ms: number(&value["latency_ms"]),
    }
}

fn chain(fixture: &Fixture) -> SignalChain {
    chain_value(fixture, &fixture.inputs()["chain"])
}

fn mimo_paths(fixture: &Fixture) -> MimoPaths {
    let value = &fixture.outputs()["paths"];
    MimoPaths {
        fsiso: number(&value["fsiso"]),
        paths: value["paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|path| {
                let value = &path["filter"];
                let filter = if let Some(iir) = value.get("Iir") {
                    MimoFilter::Iir {
                        gain_db: number(&iir["gain_db"]),
                        delay_ms: number(&iir["delay_ms"]),
                        polarity: iir["polarity"].as_i64().unwrap() as i8,
                        peq: peqs(&iir["peq"]),
                        allpass: allpasses(&iir["allpass"]),
                    }
                } else {
                    MimoFilter::Fir {
                        taps_48k: fixture.f64s(&value["Fir"]["taps_48k"]),
                    }
                };
                MimoPath {
                    input: path["input"].as_str().unwrap().into(),
                    support: path["support"].as_str().unwrap().into(),
                    filter,
                    low_hz: number(&path["low_hz"]),
                    delay_ms: number(&path["delay_ms"]),
                }
            })
            .collect(),
    }
}

struct PlaybackCase {
    name: &'static str,
    chain: SignalChain,
    mimo: Option<MimoPaths>,
}

fn playback_cases() -> Vec<PlaybackCase> {
    let fir = Fixture::load("export_mimo", "room_dba_fir");
    let iir = Fixture::load("export_mimo", "room_dba_iir");
    let filters = Fixture::load("filters", "computed_dolby");
    vec![
        PlaybackCase {
            name: "baseline_room_dba",
            chain: chain(&fir),
            mimo: None,
        },
        PlaybackCase {
            name: "mimo_fir_room_dba",
            chain: chain(&fir),
            mimo: Some(mimo_paths(&fir)),
        },
        PlaybackCase {
            name: "mimo_iir_room_dba",
            chain: chain(&iir),
            mimo: Some(mimo_paths(&iir)),
        },
        PlaybackCase {
            name: "dolby_computed",
            chain: chain(&filters),
            mimo: None,
        },
    ]
}

fn rounded(value: f64, places: usize) -> f64 {
    format_fixed(value, places, false).parse().unwrap()
}

fn as_written(plan: &FilterPlan) -> FilterPlan {
    let mut result = plan.clone();
    let round_ops = |ops: &mut Vec<Op>| {
        for op in ops {
            match op {
                Op::Peq {
                    frequency,
                    q,
                    gain_db,
                    ..
                } => {
                    *frequency = rounded(*frequency, 2);
                    *gain_db = rounded(*gain_db, 2);
                    *q = rounded(*q, 4);
                }
                Op::Fir { taps, .. } => {
                    taps.iter_mut()
                        .for_each(|tap| *tap = f64::from(*tap as f32));
                }
                Op::Gain { db, .. } => {
                    *db = if db.abs() < 0.005 {
                        0.0
                    } else {
                        rounded(*db, 2)
                    };
                }
                Op::Hp { frequency, .. } | Op::Lp { frequency, .. } => {
                    *frequency = rounded(*frequency, 2);
                    // Equalizer APO receives the writer's rounded SECTION_Q values. The
                    // 0.7071 versus 1/sqrt(2) distinction is below this test's bound.
                }
                Op::Ap { frequency, q, .. } => {
                    *frequency = rounded(*frequency, 2);
                    *q = rounded(*q, 4);
                }
                Op::Delay { ms, .. } => {
                    if *ms <= 0.0 {
                        *ms = 0.0;
                    } else {
                        let samples = (rounded(*ms, 3) * 48.0 + 0.5).floor();
                        *ms = samples / 48.0;
                    }
                }
                Op::Invert { .. } => {}
            }
        }
    };
    for (_, ops) in &mut result.input_ops {
        round_ops(ops);
    }
    for (_, ops) in &mut result.main_ops {
        round_ops(ops);
    }
    for (_, ops) in &mut result.send_ops {
        round_ops(ops);
    }
    round_ops(&mut result.lfe_ops);
    for (_, feed) in &mut result.subs {
        // The written bus factor is rounded to six places. Its maximum amplitude
        // error is 5e-7, well below ABS, so the reference deliberately keeps it.
        round_ops(&mut feed.ops);
    }
    for feed in &mut result.mimo {
        round_ops(&mut feed.ops);
    }
    result
}

fn excitation(seed_index: usize) -> Vec<f32> {
    let mut state = 0x6a09_e667_f3bc_c909_u64 ^ (seed_index as u64 + 1);
    let mut spectrum = vec![Complex64::new(0.0, 0.0); PERIOD / 2 + 1];
    for value in &mut spectrum[1..PERIOD / 2] {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        let random = state.wrapping_mul(0x2545_f491_4f6c_dd1d);
        let phase = 2.0 * PI * (random as f64 / u64::MAX as f64);
        *value = Complex64::from_polar(1.0, phase);
    }
    let mut samples = irfft(&spectrum, PERIOD);
    let rms = (samples.iter().map(|value| value * value).sum::<f64>() / PERIOD as f64).sqrt();
    samples
        .iter_mut()
        .map(|value| (*value * 0.05 / rms) as f32)
        .collect()
}

fn write_float_wav(path: &Path, channels: usize, frames: &[f32]) {
    assert_eq!(frames.len() % channels, 0);
    let data_len = (frames.len() * 4) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&3_u16.to_le_bytes());
    bytes.extend_from_slice(&(channels as u16).to_le_bytes());
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * channels as u32 * 4).to_le_bytes());
    bytes.extend_from_slice(&(channels as u16 * 4).to_le_bytes());
    bytes.extend_from_slice(&32_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in frames {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    fs::write(path, bytes).unwrap();
}

#[derive(Debug)]
struct Wav {
    channels: usize,
    rate: u32,
    samples: Vec<f64>,
}

fn read_u16(cursor: &mut Cursor<&[u8]>) -> u16 {
    let mut bytes = [0; 2];
    cursor.read_exact(&mut bytes).unwrap();
    u16::from_le_bytes(bytes)
}

fn read_u32(cursor: &mut Cursor<&[u8]>) -> u32 {
    let mut bytes = [0; 4];
    cursor.read_exact(&mut bytes).unwrap();
    u32::from_le_bytes(bytes)
}

fn read_wav_bytes(bytes: &[u8]) -> Result<Wav, String> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a RIFF WAVE file".into());
    }
    let mut offset = 12;
    let mut format = None;
    let mut channels = 0;
    let mut rate = 0;
    let mut bits = 0;
    let mut data = None;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let start = offset + 8;
        let end = start
            .checked_add(size)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| "truncated WAV chunk".to_owned())?;
        if id == b"fmt " {
            if size < 16 {
                return Err("short fmt chunk".into());
            }
            let mut cursor = Cursor::new(&bytes[start..end]);
            let tag = read_u16(&mut cursor);
            channels = read_u16(&mut cursor) as usize;
            rate = read_u32(&mut cursor);
            let _byte_rate = read_u32(&mut cursor);
            let _block_align = read_u16(&mut cursor);
            bits = read_u16(&mut cursor);
            format = Some(if tag == 0xfffe {
                if size < 40 {
                    return Err("short extensible fmt chunk".into());
                }
                let _extension_size = read_u16(&mut cursor);
                let _valid_bits = read_u16(&mut cursor);
                let _channel_mask = read_u32(&mut cursor);
                read_u16(&mut cursor)
            } else {
                tag
            });
        } else if id == b"data" {
            data = Some(&bytes[start..end]);
        }
        offset = end + size % 2;
    }
    let format = format.ok_or_else(|| "missing fmt chunk".to_owned())?;
    let data = data.ok_or_else(|| "missing data chunk".to_owned())?;
    if channels == 0 || rate == 0 {
        return Err("invalid WAV channel count or rate".into());
    }
    let samples = match (format, bits) {
        (1, 16) => {
            let (samples, rest) = data.as_chunks::<2>();
            if !rest.is_empty() {
                return Err("PCM16 data is not whole samples".into());
            }
            samples
                .iter()
                .map(|bytes| f64::from(i16::from_le_bytes(*bytes)) / 32768.0)
                .collect()
        }
        (3, 32) => {
            let (samples, rest) = data.as_chunks::<4>();
            if !rest.is_empty() {
                return Err("float32 data is not whole samples".into());
            }
            samples
                .iter()
                .map(|bytes| f64::from(f32::from_le_bytes(*bytes)))
                .collect()
        }
        _ => return Err(format!("unsupported WAV format {format:#x}/{bits}")),
    };
    Ok(Wav {
        channels,
        rate,
        samples,
    })
}

fn read_wav(path: &Path) -> Result<Wav, String> {
    read_wav_bytes(&fs::read(path).map_err(|error| error.to_string())?)
}

fn output_channel(wav: &Wav, channel: usize, start: usize, count: usize) -> Vec<f64> {
    (start..start + count)
        .map(|frame| wav.samples[frame * wav.channels + channel])
        .collect()
}

fn estimate_response(input: &[f32], output: &[f64]) -> Vec<Complex64> {
    let input = input
        .iter()
        .map(|value| f64::from(*value))
        .collect::<Vec<_>>();
    let x = rfft(&input);
    let y = rfft(output);
    x.iter()
        .zip(y)
        .map(|(input, output)| output / input)
        .collect()
}

fn preamp_gain(config: &Path) -> f64 {
    let text = fs::read_to_string(config).unwrap();
    let line = text
        .lines()
        .find(|line| line.trim_start().starts_with("Preamp:"))
        .expect("config.txt Preamp line");
    let db: f64 = line
        .split_whitespace()
        .nth(1)
        .expect("Preamp value")
        .parse()
        .expect("numeric Preamp value");
    10_f64.powf(db / 20.0)
}

fn clear_folder(path: &Path) {
    fs::create_dir_all(path).unwrap();
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            fs::remove_dir_all(path).unwrap();
        } else {
            fs::remove_file(path).unwrap();
        }
    }
}

fn db(value: f64) -> f64 {
    if value > 0.0 {
        20.0 * value.log10()
    } else {
        f64::NEG_INFINITY
    }
}

fn phase_error(measured: Complex64, reference: Complex64) -> f64 {
    if measured.norm() == 0.0 || reference.norm() == 0.0 {
        0.0
    } else {
        (measured / reference).arg().to_degrees()
    }
}

fn residual_delay(measured: &[Complex64], reference: &[Complex64], peak: f64) -> f64 {
    let mut points = Vec::new();
    let mut previous = 0.0;
    let mut first = true;
    for (index, (got, expected)) in measured.iter().zip(reference).enumerate() {
        let frequency = index as f64 * SAMPLE_RATE as f64 / PERIOD as f64;
        if !(10.0..=20_000.0).contains(&frequency) || expected.norm() <= 0.1 * peak {
            continue;
        }
        let mut phase = (*got / *expected).arg();
        if first {
            first = false;
        } else {
            while phase - previous > PI {
                phase -= 2.0 * PI;
            }
            while phase - previous < -PI {
                phase += 2.0 * PI;
            }
        }
        previous = phase;
        points.push((2.0 * PI * index as f64 / PERIOD as f64, phase));
    }
    if points.len() < 2 {
        return 0.0;
    }
    let mean_x = points.iter().map(|(x, _)| x).sum::<f64>() / points.len() as f64;
    let mean_y = points.iter().map(|(_, y)| y).sum::<f64>() / points.len() as f64;
    let numerator = points
        .iter()
        .map(|(x, y)| (x - mean_x) * (y - mean_y))
        .sum::<f64>();
    let denominator = points
        .iter()
        .map(|(x, _)| (x - mean_x).powi(2))
        .sum::<f64>();
    -numerator / denominator
}

#[derive(Debug)]
struct ReportRow {
    case: String,
    input: String,
    output: String,
    output_number: usize,
    ratio: f64,
    frequency: f64,
    error_db: f64,
    measured_db: f64,
    reference_db: f64,
    phase_deg: f64,
    residual_samples: f64,
}

fn markdown_report(engine: &str, rows: &[ReportRow]) -> String {
    let mut text = format!(
        "# Equalizer APO playback — {engine}\n\n| Case | Input | Output | Worst err/bound | Hz | Error/peak dB | Measured dB | Reference dB | Phase Δ ° | Residual delay samples |\n|---|---|---:|---:|---:|---:|---:|---:|---:|---:|\n"
    );
    for row in rows {
        text.push_str(&format!(
            "| {} | {} | {} ({}) | {:.6} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.4} |\n",
            row.case,
            row.input,
            row.output,
            row.output_number,
            row.ratio,
            row.frequency,
            row.error_db,
            row.measured_db,
            row.reference_db,
            row.phase_deg,
            row.residual_samples
        ));
    }
    text
}

#[test]
#[ignore = "Equalizer APO Benchmark.exe가 필요합니다. ci.yml의 eqapo-playback이 돌립니다."]
fn exported_configs_play_as_designed() {
    let benchmark = PathBuf::from(
        std::env::var_os("SECS_EQAPO_BENCHMARK")
            .expect("SECS_EQAPO_BENCHMARK must name Equalizer APO Benchmark.exe"),
    );
    let benchmark = benchmark
        .canonicalize()
        .expect("SECS_EQAPO_BENCHMARK must exist");
    let external_config = std::env::var_os("SECS_EQAPO_CONFIG_DIR").map(PathBuf::from);
    let engine = std::env::var("SECS_EQAPO_ENGINE").unwrap_or_else(|_| "unspecified".into());
    let selected = std::env::var("SECS_EQAPO_CASES").ok().map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    });
    let root = TempDir::new("engine");
    let mut rows = Vec::new();
    let mut failures = Vec::new();
    for case in playback_cases().into_iter().filter(|case| {
        selected
            .as_ref()
            .is_none_or(|selected| selected.iter().any(|name| name == case.name))
    }) {
        let case_root = root.0.join(case.name);
        fs::create_dir_all(&case_root).unwrap();
        let config = external_config
            .clone()
            .unwrap_or_else(|| case_root.join("config"));
        clear_folder(&config);
        let run = FilterRun {
            chain: &case.chain,
            mimo: case.mimo.as_ref(),
            mode: "mixed",
            ultra_ms: 200.0,
            label: "playback",
            latency_ms: case.chain.latency_ms,
            rc_hi_hz: 100.0,
            rc_pieces: &[],
            stamp: "2026-10-23 00:00",
        };
        export_eqapo(&run, &config).unwrap();
        let preamp = preamp_gain(&config.join("config.txt"));
        let plan = make_plan(&case.chain, case.mimo.as_ref()).unwrap();
        let reference_plan = as_written(&plan);
        let (inputs, outputs) = windows_channels(&plan);
        let channels = inputs
            .iter()
            .chain(&outputs)
            .map(|(_, number)| *number)
            .max()
            .unwrap();
        let frequencies = (0..=PERIOD / 2)
            .map(|index| index as f64 * SAMPLE_RATE as f64 / PERIOD as f64)
            .collect::<Vec<_>>();
        for (input_index, (input, input_number)) in inputs.iter().enumerate() {
            let excitation = excitation(input_index);
            let mut input_samples = vec![0.0_f32; 2 * PERIOD * channels];
            for frame in 0..2 * PERIOD {
                input_samples[frame * channels + input_number - 1] = excitation[frame % PERIOD];
            }
            let input_path = case_root.join(format!("input_{input}.wav"));
            let output_path = case_root.join(format!("output_{input}.wav"));
            write_float_wav(&input_path, channels, &input_samples);
            let mut command = Command::new(&benchmark);
            command
                .current_dir(benchmark.parent().unwrap())
                .args(["--nopause", "--verbose", "--batchsize", "512", "-i"])
                .arg(&input_path)
                .arg("-o")
                .arg(&output_path);
            if external_config.is_none() {
                command.arg("--config-path").arg(&config);
            }
            let output = command.output().expect("run Equalizer APO Benchmark.exe");
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let error_line = stdout
                .lines()
                .chain(stderr.lines())
                .any(|line| line.contains("Error"));
            // Every case runs to the end and lands in the report, so one engine's
            // run says which cases it plays and which it does not.
            if !output.status.success() || error_line {
                failures.push(format!(
                    "{} / {input}: Benchmark failed ({:?})\nstdout:\n{stdout}\nstderr:\n{stderr}",
                    case.name,
                    output.status.code()
                ));
                continue;
            }
            let wav = match read_wav(&output_path) {
                Ok(wav) => wav,
                Err(error) => {
                    failures.push(format!(
                        "{} / {input}: {}: {error}",
                        case.name,
                        output_path.display()
                    ));
                    continue;
                }
            };
            let problem = if wav.channels != channels {
                Some(format!("{} channels, expected {channels}", wav.channels))
            } else if wav.rate != SAMPLE_RATE {
                Some(format!("sample rate {}", wav.rate))
            } else if wav.samples.len() / channels < 2 * PERIOD {
                Some("output shorter than two periods".to_owned())
            } else if wav.samples.iter().any(|sample| sample.abs() >= 0.9999) {
                Some("output clipped".to_owned())
            } else {
                None
            };
            if let Some(problem) = problem {
                failures.push(format!("{} / {input}: {problem}", case.name));
                continue;
            }
            let expected = plan_responses(&reference_plan, &frequencies, input, false).unwrap();
            let peak = expected
                .iter()
                .flat_map(|(_, response)| response.iter())
                .map(|value| value.norm() * preamp)
                .fold(0.0_f64, f64::max);
            assert!(peak > 0.0, "{} / {input}: zero reference", case.name);
            for (output_role, output_number) in &outputs {
                let output_samples = output_channel(&wav, output_number - 1, PERIOD, PERIOD);
                let measured = estimate_response(&excitation, &output_samples);
                let reference = expected
                    .iter()
                    .find(|(role, _)| role == output_role)
                    .map(|(_, response)| response.clone())
                    .unwrap_or_else(|| vec![Complex64::new(0.0, 0.0); measured.len()]);
                let reference = reference
                    .into_iter()
                    .map(|value| value * preamp)
                    .collect::<Vec<_>>();
                let mut worst = (0.0_f64, 0_usize, 0.0_f64);
                for (index, (got, expected)) in measured.iter().zip(&reference).enumerate() {
                    let frequency = frequencies[index];
                    if !(10.0..=20_000.0).contains(&frequency) {
                        continue;
                    }
                    let error = (*got - *expected).norm();
                    let bound = REL * expected.norm() + ABS * peak;
                    let ratio = error / bound;
                    if ratio > worst.0 {
                        worst = (ratio, index, error);
                    }
                }
                let index = worst.1;
                let row = ReportRow {
                    case: case.name.into(),
                    input: input.clone(),
                    output: output_role.clone(),
                    output_number: *output_number,
                    ratio: worst.0,
                    frequency: frequencies[index],
                    error_db: db(worst.2 / peak),
                    measured_db: db(measured[index].norm()),
                    reference_db: db(reference[index].norm()),
                    phase_deg: phase_error(measured[index], reference[index]),
                    residual_samples: residual_delay(&measured, &reference, peak),
                };
                if row.ratio > 1.0 {
                    failures.push(format!(
                        "{} / {} -> {} ({}): err/bound {:.6} at {:.3} Hz",
                        row.case,
                        row.input,
                        row.output,
                        row.output_number,
                        row.ratio,
                        row.frequency
                    ));
                }
                rows.push(row);
            }
        }
    }
    let mut report = markdown_report(&engine, &rows);
    if !failures.is_empty() {
        report.push_str("\n## Failures\n\n```text\n");
        report.push_str(&failures.join("\n"));
        report.push_str("\n```\n");
    }
    println!("{report}");
    if let Some(path) = std::env::var_os("SECS_EQAPO_REPORT") {
        fs::write(path, &report).unwrap();
    }
    if let Some(selected) = selected {
        let run = playback_cases()
            .iter()
            .map(|case| case.name)
            .collect::<Vec<_>>();
        for requested in selected {
            assert!(
                run.contains(&requested.as_str()),
                "unknown SECS_EQAPO_CASES entry {requested}"
            );
        }
    }
    assert!(
        failures.is_empty(),
        "{} playback checks failed (REL={REL}, ABS={ABS}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn wav_fixture(format: u16, bits: u16, extensible: bool, values: &[f64]) -> Vec<u8> {
    let mut data = Vec::new();
    match (format, bits) {
        (1, 16) => values.iter().for_each(|value| {
            data.extend_from_slice(&((*value * 32768.0).round() as i16).to_le_bytes());
        }),
        (3, 32) => values
            .iter()
            .for_each(|value| data.extend_from_slice(&(*value as f32).to_le_bytes())),
        _ => unreachable!(),
    }
    let fmt_size = if extensible { 40_u32 } else { 16_u32 };
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(4 + 8 + fmt_size + 8 + data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&fmt_size.to_le_bytes());
    bytes.extend_from_slice(&(if extensible { 0xfffe_u16 } else { format }).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2 * u32::from(bits / 8)).to_le_bytes());
    bytes.extend_from_slice(&(2 * bits / 8).to_le_bytes());
    bytes.extend_from_slice(&bits.to_le_bytes());
    if extensible {
        bytes.extend_from_slice(&22_u16.to_le_bytes());
        bytes.extend_from_slice(&bits.to_le_bytes());
        bytes.extend_from_slice(&3_u32.to_le_bytes());
        bytes.extend_from_slice(&format.to_le_bytes());
        bytes.extend_from_slice(&[0; 14]);
    }
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&data);
    bytes
}

#[test]
fn wav_write_and_read_cover_pcm_float_and_extensible() {
    let root = TempDir::new("wav");
    let path = root.0.join("float.wav");
    let values = [-0.5_f32, 0.25, 0.75, -0.125];
    write_float_wav(&path, 2, &values);
    let wav = read_wav(&path).unwrap();
    assert_eq!(wav.channels, 2);
    assert_eq!(wav.rate, SAMPLE_RATE);
    assert_eq!(wav.samples, values.map(f64::from));

    for (format, bits, extensible, tolerance) in [
        (1, 16, false, 1.0 / 32768.0),
        (3, 32, false, 1e-7),
        (1, 16, true, 1.0 / 32768.0),
        (3, 32, true, 1e-7),
    ] {
        let expected = [-0.5, 0.25, 0.75, -0.125];
        let wav = read_wav_bytes(&wav_fixture(format, bits, extensible, &expected)).unwrap();
        for (got, expected) in wav.samples.iter().zip(expected) {
            assert!((got - expected).abs() <= tolerance);
        }
    }
}

#[test]
fn as_written_rounds_equalizer_apo_values() {
    let fixture = Fixture::load("export_mimo", "room_dba_iir");
    let plan = make_plan(&chain(&fixture), Some(&mimo_paths(&fixture))).unwrap();
    let written = as_written(&plan);
    for op in written
        .input_ops
        .iter()
        .chain(&written.main_ops)
        .chain(&written.send_ops)
        .flat_map(|(_, ops)| ops)
        .chain(written.lfe_ops.iter())
        .chain(written.subs.iter().flat_map(|(_, feed)| &feed.ops))
        .chain(written.mimo.iter().flat_map(|feed| &feed.ops))
    {
        match op {
            Op::Peq {
                frequency,
                q,
                gain_db,
                ..
            } => {
                assert_eq!(*frequency, rounded(*frequency, 2));
                assert_eq!(*q, rounded(*q, 4));
                assert_eq!(*gain_db, rounded(*gain_db, 2));
            }
            Op::Ap { frequency, q, .. } => {
                assert_eq!(*frequency, rounded(*frequency, 2));
                assert_eq!(*q, rounded(*q, 4));
            }
            Op::Delay { ms, .. } => assert_eq!(*ms * 48.0, (*ms * 48.0).round()),
            Op::Fir { taps, .. } => assert!(taps.iter().all(|tap| *tap == f64::from(*tap as f32))),
            Op::Gain { db, .. } => assert!(*db == 0.0 || *db == rounded(*db, 2)),
            Op::Hp { frequency, .. } | Op::Lp { frequency, .. } => {
                assert_eq!(*frequency, rounded(*frequency, 2));
            }
            Op::Invert { .. } => {}
        }
    }
}

fn circular_output(input: &[f32], fir: &[f64], delay: usize) -> Vec<f64> {
    let input = input
        .iter()
        .map(|value| f64::from(*value))
        .collect::<Vec<_>>();
    let spectrum = rfft(&input);
    let mut padded = vec![0.0; PERIOD];
    padded[..fir.len()].copy_from_slice(fir);
    let response = rfft(&padded);
    let delayed = spectrum
        .iter()
        .zip(response)
        .enumerate()
        .map(|(index, (input, response))| {
            input
                * response
                * Complex64::from_polar(
                    1.0,
                    -2.0 * PI * index as f64 * delay as f64 / PERIOD as f64,
                )
        })
        .collect::<Vec<_>>();
    irfft(&delayed, PERIOD)
}

#[test]
fn response_estimator_recovers_fir_delay_and_pcm16_stays_in_bounds() {
    let input = excitation(19);
    let fir = [0.72, 0.08, -0.03, 0.01];
    let delay = 37;
    let output = circular_output(&input, &fir, delay);
    let measured = estimate_response(&input, &output);
    let mut impulse = vec![0.0; PERIOD];
    for (index, value) in fir.iter().enumerate() {
        impulse[(index + delay) % PERIOD] = *value;
    }
    let expected = rfft(&impulse);
    for (index, (got, expected)) in measured.iter().zip(&expected).enumerate() {
        let frequency = index as f64 * SAMPLE_RATE as f64 / PERIOD as f64;
        if (10.0..=20_000.0).contains(&frequency) {
            assert!((*got - *expected).norm() < 1e-9, "{frequency} Hz");
        }
    }

    let quantized = output
        .iter()
        .map(|sample| (sample * 32768.0).round().clamp(-32768.0, 32767.0) / 32768.0)
        .collect::<Vec<_>>();
    let measured = estimate_response(&input, &quantized);
    let peak = expected
        .iter()
        .map(|value| value.norm())
        .fold(0.0_f64, f64::max);
    for (index, (got, expected)) in measured.iter().zip(&expected).enumerate() {
        let frequency = index as f64 * SAMPLE_RATE as f64 / PERIOD as f64;
        if (10.0..=20_000.0).contains(&frequency) {
            let error = (*got - *expected).norm();
            assert!(
                error <= REL * expected.norm() + ABS * peak,
                "{frequency} Hz: {error}"
            );
        }
    }
}
