#![forbid(unsafe_code)]

use secs_apply::camilla::{Camilla, WsTransport};
use secs_camilla::pyfmt::round_ndigits;
use secs_camilla::{DesignLabels, Device, DeviceValue, Options, round_ties_even_i64, write_config};
use secs_dsp::Complex64;
use secs_dsp::fft::rfft;
use secs_filters::{FilterPlan, Op, headroom_db, make_plan, plan_responses, render_paths};
use secs_oracle::Fixture;
use secs_types::chain::{
    BandType, MainChannel, MimoFilter, MimoPath, MimoPaths, SignalChain, SubChannel,
};
use serde_json::Value;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

const SAMPLE_RATE: u32 = 48_000;
const FFT_LEN: usize = 131_072;
const MAX_RENDER_LEN: usize = 1 << 20;
const EXTRA_SAMPLES: usize = 2 * FFT_LEN;
const IMPULSE: f32 = 0.25;
const REL: f64 = 1e-2;
const ABS: f64 = 1e-3;
const RUN_TIMEOUT: Duration = Duration::from_secs(90);
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "secs_camilla_playback_{label}_{}_{id}",
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
        input_hp_hz: None,
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

fn fir_lowrate_case(fixture: &Fixture) -> PlaybackCase {
    let mut mimo = mimo_paths(fixture);
    for path in &mut mimo.paths {
        let MimoFilter::Fir { taps_48k } = &path.filter else {
            panic!("room_dba_fir path is not FIR");
        };
        assert_eq!(taps_48k.len(), 8_000);
        let taps_low = taps_48k
            .iter()
            .step_by(80)
            .map(|tap| tap * 80.0)
            .collect::<Vec<_>>();
        assert_eq!(taps_low.len(), 100);
        path.filter = MimoFilter::FirLowRate {
            taps_low,
            fs_low: 600.0,
        };
    }
    PlaybackCase {
        name: "mimo_fir_lowrate_room_dba",
        chain: chain(fixture),
        mimo: Some(mimo),
    }
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
        fir_lowrate_case(&fir),
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

fn bypass_chain(chain: &SignalChain) -> SignalChain {
    let mut result = chain.clone();
    for main in &mut result.mains {
        main.input_peq.clear();
        main.fir = vec![1.0];
    }
    result
}

fn headroom_gain(plan: &FilterPlan) -> f64 {
    let db = (headroom_db(plan).unwrap() * 2.0).ceil() / 2.0;
    10_f64.powf(-db / 20.0)
}

/// Values after CamillaDSP serialization: rounded biquad parameters and gains,
/// integer-sample delays, and float32 WAV coefficients. This is the same
/// analog-plan reference convention used by the Equalizer APO playback test.
fn as_written(plan: &FilterPlan) -> FilterPlan {
    let mut result = plan.clone();
    let round_ops = |ops: &mut Vec<Op>, mimo: bool| {
        for op in ops {
            match op {
                Op::Peq {
                    frequency,
                    q,
                    gain_db,
                    ..
                } => {
                    *frequency = round_ndigits(*frequency, 4);
                    *q = round_ndigits(*q, 5);
                    *gain_db = round_ndigits(*gain_db, 4);
                }
                Op::Fir { taps, .. } => taps
                    .iter_mut()
                    .for_each(|tap| *tap = f64::from(*tap as f32)),
                Op::FirLowRate {
                    name,
                    taps_low,
                    fs_low,
                    fixed,
                } => {
                    let taps = secs_dsp::fir::lowrate_to_48k(taps_low, *fs_low)
                        .into_iter()
                        .map(|tap| f64::from(tap as f32))
                        .collect();
                    *op = Op::Fir {
                        name: std::mem::take(name),
                        taps,
                        fixed: *fixed,
                    };
                }
                Op::Gain { db, .. } => *db = round_ndigits(*db, 4),
                Op::Hp { frequency, .. } if mimo => {
                    *frequency = round_ndigits(*frequency, 3);
                }
                Op::Hp { .. } | Op::Lp { .. } | Op::Invert { .. } => {}
                Op::Ap { frequency, q, .. } => {
                    *frequency = round_ndigits(*frequency, 4);
                    *q = round_ndigits(*q, 5);
                }
                Op::Delay { ms, .. } => {
                    let samples = round_ties_even_i64(*ms * f64::from(SAMPLE_RATE) / 1000.0);
                    *ms = samples as f64 * 1000.0 / f64::from(SAMPLE_RATE);
                }
            }
        }
    };
    for (_, ops) in &mut result.input_ops {
        round_ops(ops, false);
    }
    for (_, ops) in &mut result.main_ops {
        round_ops(ops, false);
    }
    for (_, ops) in &mut result.send_ops {
        round_ops(ops, false);
    }
    round_ops(&mut result.lfe_ops, false);
    for (_, feed) in &mut result.subs {
        feed.gain_db = round_ndigits(feed.gain_db, 4);
        round_ops(&mut feed.ops, false);
    }
    for feed in &mut result.mimo {
        round_ops(&mut feed.ops, true);
    }
    result
}

fn raw_device(kind: &str, filename: &Path, channels: usize, capture: bool) -> Device {
    let mut fields = vec![
        ("type".into(), DeviceValue::Str(kind.into())),
        (
            "filename".into(),
            DeviceValue::Str(filename.to_string_lossy().into_owned()),
        ),
        ("format".into(), DeviceValue::Str("F32_LE".into())),
        ("channels".into(), DeviceValue::Int(channels as i64)),
    ];
    if capture {
        fields.push((
            "extra_samples".into(),
            DeviceValue::Int(EXTRA_SAMPLES as i64),
        ));
    }
    Device(fields)
}

fn write_excitation(path: &Path, inputs: usize, slots: usize, slot_len: usize) {
    let file = File::create(path).unwrap();
    let mut writer = BufWriter::new(file);
    let zero = 0.0_f32.to_le_bytes();
    let impulse = IMPULSE.to_le_bytes();
    for slot in 0..slots {
        for frame in 0..slot_len {
            for channel in 0..inputs {
                writer
                    .write_all(if frame == 0 && channel == slot {
                        &impulse
                    } else {
                        &zero
                    })
                    .unwrap();
            }
        }
    }
    writer.flush().unwrap();
}

fn read_raw(path: &Path) -> Result<Vec<f32>, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let (samples, rest) = bytes.as_chunks::<4>();
    if !rest.is_empty() {
        return Err("float32 raw output ends in a partial sample".into());
    }
    Ok(samples
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect())
}

struct RunningEngine(Child);

impl Drop for RunningEngine {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn connect_engine(address: &str) -> Camilla<WsTransport> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        // The app's request timeout (secs-service apply.rs): since audit #155
        // TD-132 it bounds a whole request, including FIR loading on Reload.
        match WsTransport::connect(address, Duration::from_secs(2)) {
            Ok(transport) => return Camilla::new(transport),
            Err(error) if Instant::now() < deadline => {
                eprintln!("waiting for CamillaDSP websocket: {error}");
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => panic!("CamillaDSP websocket did not start: {error}"),
        }
    }
}

fn wait_for_file_playback(client: &mut Camilla<WsTransport>, output: &Path) {
    let deadline = Instant::now() + RUN_TIMEOUT;
    loop {
        if output.is_file()
            && fs::metadata(output).is_ok_and(|metadata| metadata.len() > 0)
            && client.status().is_ok_and(|status| {
                status.state == "Inactive" && status.stop_reason.as_deref() == Some("Done")
            })
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "CamillaDSP did not finish writing {}",
            output.display()
        );
        thread::sleep(Duration::from_millis(50));
    }
}

/// Applies a config and, when CamillaDSP does not report `Applied`, fails with
/// the outcome and the engine's own log so the CI run shows the reason.
fn expect_applied(
    client: &mut Camilla<WsTransport>,
    config: &Path,
    text: &str,
    label: &str,
    engine_log: &Path,
) {
    let outcome = client.apply(config.to_str().expect("UTF-8 test path"), text);
    if !matches!(
        outcome,
        Ok(secs_apply::camilla::ApplyOutcome::Applied { .. })
    ) {
        let log = fs::read_to_string(engine_log).unwrap_or_default();
        panic!("{label}: apply returned {outcome:?}\nCamillaDSP log:\n{log}");
    }
}

/// Waits until `frames` output frames exist while the pipeline is still up.
/// With its input paused at a slot boundary CamillaDSP reports `Stalled`,
/// which keeps the devices and filters as they are.
fn wait_for_live_frames(
    client: &mut Camilla<WsTransport>,
    output: &Path,
    channels: usize,
    frames: usize,
) {
    let required_bytes = (frames * channels * size_of::<f32>()) as u64;
    let deadline = Instant::now() + RUN_TIMEOUT;
    loop {
        let enough_output =
            fs::metadata(output).is_ok_and(|metadata| metadata.len() >= required_bytes);
        let live = client
            .status()
            .is_ok_and(|status| matches!(status.state.as_str(), "Running" | "Stalled"));
        if enough_output && live {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "CamillaDSP did not write {frames} live frames to {}",
            output.display()
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn wait_for_title(client: &mut Camilla<WsTransport>, title: &str) {
    let deadline = Instant::now() + RUN_TIMEOUT;
    loop {
        if client
            .status()
            .is_ok_and(|status| status.title.as_deref() == Some(title))
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "CamillaDSP never made {title:?} its active config"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

/// Writes six slots of one impulse each and pauses after slots 1 and 3 until
/// the test resumes it. Each slot goes out in one burst: CamillaDSP zero-fills
/// a chunk whose read times out half full, which would shift every later slot,
/// so the input may only pause at slot boundaries (whole chunks).
fn write_impulse_slots(
    input: impl Write,
    channels: usize,
    phase_ready: SyncSender<usize>,
    resume: Receiver<()>,
) -> Result<(), String> {
    const SLOTS: usize = 6;

    let mut writer = BufWriter::new(input);
    let zero = 0.0_f32.to_le_bytes();
    let impulse = IMPULSE.to_le_bytes();
    for slot in 0..SLOTS {
        for frame in 0..FFT_LEN {
            for channel in 0..channels {
                let sample = if frame == 0 && channel == 0 {
                    &impulse
                } else {
                    &zero
                };
                writer
                    .write_all(sample)
                    .map_err(|error| error.to_string())?;
            }
        }
        writer.flush().map_err(|error| error.to_string())?;
        if slot == 1 || slot == 3 {
            phase_ready
                .send(slot + 1)
                .map_err(|error| error.to_string())?;
            resume.recv().map_err(|error| error.to_string())?;
        }
    }
    // Dropping the writer closes stdin, which CamillaDSP reads as end of file.
    writer.flush().map_err(|error| error.to_string())
}

fn stdin_device(channels: usize) -> Device {
    Device(vec![
        ("type".into(), DeviceValue::Str("Stdin".into())),
        ("format".into(), DeviceValue::Str("F32_LE".into())),
        ("channels".into(), DeviceValue::Int(channels as i64)),
        (
            "extra_samples".into(),
            DeviceValue::Int(EXTRA_SAMPLES as i64),
        ),
    ])
}

fn run_engine(engine: &Path, config: &Path, log: &Path) -> Result<(), String> {
    let stdout = File::create(log).map_err(|error| error.to_string())?;
    let stderr = stdout.try_clone().map_err(|error| error.to_string())?;
    let mut child = Command::new(engine)
        .arg(config)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|error| format!("could not start {}: {error}", engine.display()))?;
    let deadline = Instant::now() + RUN_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err(format!("CamillaDSP exited with {status}"))
                };
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let kill = child.kill();
                let wait = child.wait();
                return Err(format!(
                    "CamillaDSP exceeded {} seconds; kill={kill:?}, wait={wait:?}",
                    RUN_TIMEOUT.as_secs()
                ));
            }
            Err(error) => {
                let kill = child.kill();
                let wait = child.wait();
                return Err(format!(
                    "could not wait for CamillaDSP: {error}; kill={kill:?}, wait={wait:?}"
                ));
            }
        }
    }
}

fn copy_file(from: &Path, to: &Path) {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::copy(from, to).unwrap();
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            copy_file(&path, &target);
        }
    }
}

fn response_for(
    responses: &[(String, Vec<Complex64>)],
    output: &str,
    bins: usize,
) -> Vec<Complex64> {
    responses
        .iter()
        .find(|(role, _)| role == output)
        .map(|(_, response)| response.clone())
        .unwrap_or_else(|| vec![Complex64::new(0.0, 0.0); bins])
}

fn rendered_response(
    rendered: &secs_filters::RenderedPaths,
    input: &str,
    output: &str,
    length: usize,
) -> Vec<Complex64> {
    let mut impulse = vec![0.0; length];
    if let Some(path) = rendered
        .paths
        .iter()
        .find(|path| path.input == input && path.output == output)
    {
        impulse[..path.response.len()].copy_from_slice(&path.response);
    }
    rfft(&impulse)
}

fn db(value: f64) -> f64 {
    if value > 0.0 {
        20.0 * value.log10()
    } else {
        f64::NEG_INFINITY
    }
}

#[derive(Clone)]
struct Worst {
    value: f64,
    error: f64,
    bound: f64,
    input: String,
    output: String,
    frequency: f64,
}

impl Default for Worst {
    fn default() -> Self {
        Self {
            value: 0.0,
            error: 0.0,
            bound: 0.0,
            input: String::new(),
            output: String::new(),
            frequency: 0.0,
        }
    }
}

struct LiveExpectation<'a> {
    label: &'a str,
    slot: usize,
    plan: &'a FilterPlan,
    gain: f64,
}

fn assert_live_slot_matches(samples: &[f32], expected: LiveExpectation<'_>) {
    let frequencies = (0..=FFT_LEN / 2)
        .map(|index| index as f64 * SAMPLE_RATE as f64 / FFT_LEN as f64)
        .collect::<Vec<_>>();
    let input = &expected.plan.inputs[0];
    let responses = plan_responses(expected.plan, &frequencies, input, false).unwrap();
    let analog_peak = responses
        .iter()
        .flat_map(|(_, response)| response)
        .map(|value| value.norm() * expected.gain)
        .fold(0.0_f64, f64::max);
    let frame_start = expected.slot * FFT_LEN;
    let required_frames = frame_start + FFT_LEN;
    let frames = samples.len() / expected.plan.outputs.len();
    assert_eq!(
        samples.len() % expected.plan.outputs.len(),
        0,
        "{}: partial output frame",
        expected.label
    );
    assert!(
        frames >= required_frames,
        "{}: output has {frames} frames, expected at least {required_frames}",
        expected.label
    );

    let mut worst = Worst::default();
    for (output_index, output) in expected.plan.outputs.iter().enumerate() {
        let measured_time = (0..FFT_LEN)
            .map(|offset| {
                let frame = frame_start + offset;
                f64::from(samples[frame * expected.plan.outputs.len() + output_index])
                    / f64::from(IMPULSE)
            })
            .collect::<Vec<_>>();
        assert!(
            measured_time.iter().all(|sample| sample.is_finite()),
            "{}/{input}->{output}: non-finite output",
            expected.label
        );
        let measured = rfft(&measured_time);
        let reference = response_for(&responses, output, measured.len());
        for (index, (got, reference)) in measured.iter().zip(reference).enumerate() {
            let frequency = frequencies[index];
            if !(10.0..=20_000.0).contains(&frequency) {
                continue;
            }
            let reference = reference * expected.gain;
            let error = (*got - reference).norm();
            let bound = REL * reference.norm() + ABS * analog_peak;
            let ratio = if bound > 0.0 {
                error / bound
            } else if error == 0.0 {
                0.0
            } else {
                f64::INFINITY
            };
            if ratio > worst.value {
                worst = Worst {
                    value: ratio,
                    error,
                    bound,
                    input: input.clone(),
                    output: output.clone(),
                    frequency,
                };
            }
        }
    }
    assert!(
        worst.value <= 1.0,
        "{} / {} -> {}: response error {:.6e} exceeds bound {:.6e} ({:.6}x) at {:.3} Hz",
        expected.label,
        worst.input,
        worst.output,
        worst.error,
        worst.bound,
        worst.value,
        worst.frequency
    );
}

struct ReportRow {
    case: String,
    config: &'static str,
    pairs: usize,
    reference: Option<Worst>,
    renderer: Option<Worst>,
    engine: String,
    status: String,
}

fn markdown_report(rows: &[ReportRow], failures: &[String]) -> String {
    let mut text = format!(
        "# CamillaDSP playback\n\nExcitation: one amplitude-{IMPULSE} impulse per input, each in its own {FFT_LEN}-sample slot. The slot is at least the renderer's complete -120 dB response length, so input-to-output responses do not overlap. RawFile adds {EXTRA_SAMPLES} samples per channel after EOF. Bound from 10 Hz through 20 kHz: {REL} x reference + {ABS} x the input's largest response.\n\n| Case | Config | Pairs | Worst reference err/bound | Reference pair @ Hz | Worst renderer abs | Renderer pair @ Hz | Engine | Status |\n|---|---|---:|---:|---|---:|---|---|---|\n"
    );
    for row in rows {
        let reference = row
            .reference
            .as_ref()
            .map_or_else(|| "-".into(), |worst| format!("{:.6}", worst.value));
        let reference_at = row.reference.as_ref().map_or_else(
            || "-".into(),
            |worst| {
                format!(
                    "{} -> {} @ {:.3}",
                    worst.input, worst.output, worst.frequency
                )
            },
        );
        let renderer = row
            .renderer
            .as_ref()
            .map_or_else(|| "-".into(), |worst| format!("{:.6e}", worst.error));
        let renderer_at = row.renderer.as_ref().map_or_else(
            || "-".into(),
            |worst| {
                format!(
                    "{} -> {} @ {:.3}",
                    worst.input, worst.output, worst.frequency
                )
            },
        );
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            row.case,
            row.config,
            row.pairs,
            reference,
            reference_at,
            renderer,
            renderer_at,
            row.engine,
            row.status
        ));
    }
    if !failures.is_empty() {
        text.push_str("\n## Failures\n\n```text\n");
        text.push_str(&failures.join("\n"));
        text.push_str("\n```\n");
    }
    text
}

fn compare_run(
    case: &str,
    config_name: &'static str,
    plan: &FilterPlan,
    gain: f64,
    output_path: &Path,
    engine: &str,
    failures: &mut Vec<String>,
) -> ReportRow {
    let frequencies = (0..=FFT_LEN / 2)
        .map(|index| index as f64 * SAMPLE_RATE as f64 / FFT_LEN as f64)
        .collect::<Vec<_>>();
    let rendered = render_paths(plan, MAX_RENDER_LEN).unwrap();
    assert!(
        rendered.length <= FFT_LEN,
        "{case}/{config_name}: renderer length {} exceeds slot {FFT_LEN}",
        rendered.length
    );
    let samples = match read_raw(output_path) {
        Ok(samples) => samples,
        Err(error) => {
            failures.push(format!(
                "{case} / {config_name}: cannot read {}: {error}",
                output_path.display()
            ));
            return ReportRow {
                case: case.into(),
                config: config_name,
                pairs: 0,
                reference: None,
                renderer: None,
                engine: engine.into(),
                status: "output error".into(),
            };
        }
    };
    let frames = samples.len() / plan.outputs.len();
    let required = plan.inputs.len() * FFT_LEN;
    if samples.len() % plan.outputs.len() != 0 || frames < required {
        failures.push(format!(
            "{case} / {config_name}: output has {frames} frames, expected at least {required}"
        ));
        return ReportRow {
            case: case.into(),
            config: config_name,
            pairs: 0,
            reference: None,
            renderer: None,
            engine: engine.into(),
            status: "short output".into(),
        };
    }

    let mut worst_reference = Worst::default();
    let mut worst_renderer = Worst::default();
    for (input_index, input) in plan.inputs.iter().enumerate() {
        let analog = plan_responses(plan, &frequencies, input, false).unwrap();
        let analog_peak = analog
            .iter()
            .flat_map(|(_, response)| response)
            .map(|value| value.norm() * gain)
            .fold(0.0_f64, f64::max);
        for (output_index, output) in plan.outputs.iter().enumerate() {
            let measured_time = (0..FFT_LEN)
                .map(|offset| {
                    let frame = input_index * FFT_LEN + offset;
                    f64::from(samples[frame * plan.outputs.len() + output_index])
                        / f64::from(IMPULSE)
                })
                .collect::<Vec<_>>();
            assert!(
                measured_time.iter().all(|sample| sample.is_finite()),
                "{case}/{config_name}/{input}->{output}: non-finite output"
            );
            let measured = rfft(&measured_time);
            let reference = response_for(&analog, output, measured.len())
                .into_iter()
                .map(|value| value * gain)
                .collect::<Vec<_>>();
            let renderer = rendered_response(&rendered, input, output, FFT_LEN)
                .into_iter()
                .map(|value| value * gain)
                .collect::<Vec<_>>();
            for (index, ((got, expected), digital)) in
                measured.iter().zip(&reference).zip(&renderer).enumerate()
            {
                let frequency = frequencies[index];
                if !(10.0..=20_000.0).contains(&frequency) {
                    continue;
                }
                let error = (*got - *expected).norm();
                let bound = REL * expected.norm() + ABS * analog_peak;
                let ratio = if bound > 0.0 {
                    error / bound
                } else if error == 0.0 {
                    0.0
                } else {
                    f64::INFINITY
                };
                if ratio > worst_reference.value {
                    worst_reference = Worst {
                        value: ratio,
                        error,
                        bound,
                        input: input.clone(),
                        output: output.clone(),
                        frequency,
                    };
                }
                let digital_error = (*got - *digital).norm();
                if digital_error > worst_renderer.error {
                    worst_renderer = Worst {
                        value: 0.0,
                        error: digital_error,
                        bound: 0.0,
                        input: input.clone(),
                        output: output.clone(),
                        frequency,
                    };
                }
            }
        }
    }
    if worst_reference.value > 1.0 {
        failures.push(format!(
            "{case} / {config_name} / {} -> {}: reference error {:.6e} exceeds bound {:.6e} ({:.6}x) at {:.3} Hz",
            worst_reference.input,
            worst_reference.output,
            worst_reference.error,
            worst_reference.bound,
            worst_reference.value,
            worst_reference.frequency
        ));
    }
    let passed = worst_reference.value <= 1.0;
    ReportRow {
        case: case.into(),
        config: config_name,
        pairs: plan.inputs.len() * plan.outputs.len(),
        reference: Some(worst_reference),
        renderer: Some(worst_renderer),
        engine: engine.into(),
        status: if passed { "pass".into() } else { "fail".into() },
    }
}

#[test]
#[ignore = "CamillaDSP 4.1.3 binary is required; CI runs this test with --ignored"]
fn camilla_exports_play_as_designed() {
    let Some(engine) = std::env::var_os("SECS_CAMILLADSP") else {
        println!("skipped: SECS_CAMILLADSP is unset; set it to the CamillaDSP 4.1.3 binary");
        return;
    };
    let engine = PathBuf::from(engine)
        .canonicalize()
        .expect("SECS_CAMILLADSP must name an existing CamillaDSP binary");
    let engine_version = std::env::var("SECS_CAMILLADSP_VERSION")
        .unwrap_or_else(|_| "unknown (SECS_CAMILLADSP_VERSION unset)".into());
    let artifacts = std::env::var_os("SECS_CAMILLA_ARTIFACTS").map(PathBuf::from);
    let root = TempDir::new("engine");
    let mut rows = Vec::new();
    let mut failures = Vec::new();

    for case in playback_cases() {
        let case_root = root.0.join(case.name);
        fs::create_dir_all(&case_root).unwrap();
        let main_plan = make_plan(&case.chain, case.mimo.as_ref()).unwrap();
        let main_written = as_written(&main_plan);
        let bypass_chain = bypass_chain(&case.chain);
        let bypass_plan = make_plan(&bypass_chain, None).unwrap();
        let bypass_written = as_written(&bypass_plan);
        let main_render = render_paths(&main_written, MAX_RENDER_LEN).unwrap();
        let bypass_render = render_paths(&bypass_written, MAX_RENDER_LEN).unwrap();
        assert!(
            main_render.length.max(bypass_render.length) <= FFT_LEN,
            "{} response needs {} samples, slot has {FFT_LEN}",
            case.name,
            main_render.length.max(bypass_render.length)
        );

        let excitation = case_root.join("excitation.f32le");
        let output = case_root.join("output.f32le");
        write_excitation(
            &excitation,
            main_plan.inputs.len(),
            main_plan.inputs.len(),
            FFT_LEN,
        );
        let options = Options {
            capture: Some(raw_device(
                "RawFile",
                &excitation,
                main_plan.inputs.len(),
                true,
            )),
            playback: Some(raw_device("File", &output, main_plan.outputs.len(), false)),
            design: Some(DesignLabels {
                project_name: Some("CamillaDSP playback check".into()),
                design_name: case.name.into(),
                design_id: "0".repeat(64),
                bypass: false,
            }),
            headroom: true,
            labels: true,
            bypass: true,
            ..Options::default()
        };
        write_config(
            &case.chain,
            &case_root,
            "playback",
            &options,
            case.mimo.as_ref(),
        )
        .unwrap();
        let main_config = case_root.join("configs/playback.yml");
        let bypass_config = case_root.join("configs/playback_bypass.yml");

        if let Some(artifacts) = &artifacts {
            let target = artifacts.join(case.name);
            copy_file(&excitation, &target.join("excitation.f32le"));
            copy_dir(&case_root.join("configs"), &target.join("configs"));
            copy_dir(&case_root.join("coeffs"), &target.join("coeffs"));
        }

        for (config_name, config, plan, gain) in [
            (
                "main",
                main_config.as_path(),
                &main_written,
                headroom_gain(&main_plan),
            ),
            (
                "bypass",
                bypass_config.as_path(),
                &bypass_written,
                headroom_gain(&bypass_plan),
            ),
        ] {
            let _ = fs::remove_file(&output);
            let log = case_root.join(format!("{config_name}.log"));
            if let Err(error) = run_engine(&engine, config, &log) {
                failures.push(format!(
                    "{} / {config_name}: {error}; log {}",
                    case.name,
                    log.display()
                ));
                rows.push(ReportRow {
                    case: case.name.into(),
                    config: config_name,
                    pairs: 0,
                    reference: None,
                    renderer: None,
                    engine: engine_version.clone(),
                    status: "engine error".into(),
                });
                continue;
            }
            rows.push(compare_run(
                case.name,
                config_name,
                plan,
                gain,
                &output,
                &engine_version,
                &mut failures,
            ));
            if let Some(artifacts) = &artifacts {
                let target = artifacts.join(case.name);
                copy_file(&output, &target.join(format!("{config_name}.f32le")));
                copy_file(&log, &target.join(format!("{config_name}.log")));
            }
        }
    }

    let report = markdown_report(&rows, &failures);
    println!("{report}");
    if let Some(path) = std::env::var_os("SECS_CAMILLA_REPORT") {
        let path = PathBuf::from(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, &report).unwrap();
    }
    assert!(
        failures.is_empty(),
        "{} CamillaDSP playback checks failed (REL={REL}, ABS={ABS}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
#[ignore = "CamillaDSP 4.1.3 binary is required; the camilla-playback CI job runs this test"]
fn one_running_pipeline_switches_generation_firs_by_unique_paths() {
    if !cfg!(target_os = "linux") {
        println!("skipped: the camilla-playback CI job runs this test on Linux");
        return;
    }
    let Some(engine) = std::env::var_os("SECS_CAMILLADSP") else {
        println!("skipped: SECS_CAMILLADSP is unset; set it to the CamillaDSP 4.1.3 binary");
        return;
    };
    let engine = PathBuf::from(engine)
        .canonicalize()
        .expect("SECS_CAMILLADSP must name an existing CamillaDSP binary");
    let root = TempDir::new("generation_reload");
    let fixture = Fixture::load("filters", "computed_dolby");
    let first_chain = chain(&fixture);
    let first_plan = make_plan(&first_chain, None).unwrap();
    let first_written = as_written(&first_plan);
    let first_gain = headroom_gain(&first_plan);
    let mut second_chain = first_chain.clone();
    second_chain.mains[0]
        .fir
        .iter_mut()
        .for_each(|tap| *tap *= 0.5);
    let second_plan = make_plan(&second_chain, None).unwrap();
    let second_written = as_written(&second_plan);
    let second_gain = headroom_gain(&second_plan);
    assert_eq!(first_plan.inputs, second_plan.inputs);
    assert_eq!(first_plan.outputs, second_plan.outputs);

    // The input is CamillaDSP's stdin, not a FIFO: config validation opens a
    // RawFile capture once and closes it, which breaks a FIFO's only writer.
    let output = root.0.join("continuous-output.f32le");
    let write_generation =
        |chain: &SignalChain, folder: &Path, design_name: &str, id_digit: char| {
            let options = Options {
                capture: Some(stdin_device(first_plan.inputs.len())),
                playback: Some(raw_device("File", &output, first_plan.outputs.len(), false)),
                design: Some(DesignLabels {
                    project_name: Some("CamillaDSP generation check".into()),
                    design_name: design_name.into(),
                    design_id: id_digit.to_string().repeat(64),
                    bypass: false,
                }),
                headroom: true,
                labels: true,
                bypass: false,
                ..Options::default()
            };
            write_config(chain, folder, "playback", &options, None).unwrap();
            folder.join("configs/playback.yml")
        };
    let first_folder = root.0.join("generation-1");
    let first_config = write_generation(&first_chain, &first_folder, "generation-one", '1');
    let second_folder = root.0.join("generation-2");
    let second_config = write_generation(&second_chain, &second_folder, "generation-two", '2');

    let probe = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let engine_log = root.0.join("camilladsp.log");
    let log = File::create(&engine_log).unwrap();
    let stderr = log.try_clone().unwrap();
    let mut child = Command::new(&engine)
        .args(["--port", &port.to_string(), "--wait"])
        .stdin(Stdio::piped())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("start CamillaDSP in websocket wait mode");
    let engine_input = child.stdin.take().expect("CamillaDSP stdin is piped");
    let mut process = RunningEngine(child);
    let address = format!("127.0.0.1:{port}");
    let mut client = connect_engine(&address);

    let (phase_ready_tx, phase_ready_rx) = std::sync::mpsc::sync_channel(0);
    let (resume_tx, resume_rx) = std::sync::mpsc::sync_channel(0);
    let input_channels = first_plan.inputs.len();
    let writer = thread::spawn(move || {
        write_impulse_slots(engine_input, input_channels, phase_ready_tx, resume_rx)
    });

    let first_text = fs::read_to_string(&first_config).unwrap();
    expect_applied(
        &mut client,
        &first_config,
        &first_text,
        "first apply",
        &engine_log,
    );
    assert_eq!(
        phase_ready_rx.recv_timeout(RUN_TIMEOUT).unwrap(),
        2,
        "writer paused after the first two impulses"
    );
    wait_for_live_frames(&mut client, &output, first_plan.outputs.len(), 2 * FFT_LEN);

    // An identical config is a no-op for CamillaDSP (config_diff finds no
    // change), so its filters keep the coefficients they loaded first.
    copy_dir(&second_folder.join("coeffs"), &first_folder.join("coeffs"));
    expect_applied(
        &mut client,
        &first_config,
        &first_text,
        "same-path reapply",
        &engine_log,
    );
    resume_tx.send(()).unwrap();
    assert_eq!(
        phase_ready_rx.recv_timeout(RUN_TIMEOUT).unwrap(),
        4,
        "writer paused after two same-path control impulses"
    );
    wait_for_live_frames(&mut client, &output, first_plan.outputs.len(), 4 * FFT_LEN);

    let second_text = fs::read_to_string(&second_config).unwrap();
    expect_applied(
        &mut client,
        &second_config,
        &second_text,
        "second generation apply",
        &engine_log,
    );
    // Reload answers once the config is queued; the controller hands it to
    // the pipeline later and only then reports its title as active.
    wait_for_title(
        &mut client,
        "SECS 2R · CamillaDSP generation check · generation-two",
    );
    resume_tx.send(()).unwrap();
    writer
        .join()
        .expect("paced input writer panicked")
        .expect("paced input writer failed");
    wait_for_file_playback(&mut client, &output);

    let samples = read_raw(&output).unwrap();
    assert_live_slot_matches(
        &samples,
        LiveExpectation {
            label: "generation 1 before reload",
            slot: 1,
            plan: &first_written,
            gain: first_gain,
        },
    );
    assert_live_slot_matches(
        &samples,
        LiveExpectation {
            label: "same Conv paths after coefficient replacement",
            slot: 3,
            plan: &first_written,
            gain: first_gain,
        },
    );
    assert_live_slot_matches(
        &samples,
        LiveExpectation {
            label: "generation 2 at unique Conv paths",
            slot: 5,
            plan: &second_written,
            gain: second_gain,
        },
    );

    client.exit().unwrap();
    let status = process.0.wait().unwrap();
    assert!(status.success());
}

#[test]
fn impulse_estimator_recovers_gain_and_delay() {
    let mut output = vec![0.0; FFT_LEN];
    output[37] = f64::from(IMPULSE) * 0.5;
    let measured = rfft(
        &output
            .iter()
            .map(|sample| sample / f64::from(IMPULSE))
            .collect::<Vec<_>>(),
    );
    for (index, value) in measured.iter().enumerate().take(FFT_LEN / 2) {
        let expected = Complex64::from_polar(
            0.5,
            -2.0 * std::f64::consts::PI * index as f64 * 37.0 / FFT_LEN as f64,
        );
        assert!((*value - expected).norm() < 1e-12);
    }
    assert_eq!(db(0.5), 20.0 * 0.5_f64.log10());
}
