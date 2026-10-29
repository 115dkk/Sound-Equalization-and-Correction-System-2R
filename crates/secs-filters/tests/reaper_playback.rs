#![forbid(unsafe_code)]
//! Real REAPER playback. Only the ignored test starts an external engine.

use secs_filters::{FilterRun, export_convolution_named};
use secs_oracle::Fixture;
use secs_types::chain::{MimoPaths, SignalChain};
use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

const RATE: u32 = 48_000;
const GUARD: usize = 4096;
// Compare f32 samples directly, with no fitted gain or time alignment. These
// bounds allow float convolution roundoff, not an extra delay or gain stage.
const REL: f64 = 2e-6;
const ABS: f64 = 1e-7;
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("secs_reaper_playback_{}_{id}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Deserialize)]
struct Routing {
    sample_rate: u32,
    length: usize,
    inputs: Vec<String>,
    outputs: Vec<String>,
    paths: Vec<Route>,
}
#[derive(Deserialize)]
struct Route {
    input: String,
    output: String,
    wav: String,
}
#[derive(Debug)]
struct Wav {
    channels: usize,
    rate: u32,
    samples: Vec<f32>, // interleaved
}

fn wav_bytes(channels: usize, samples: &[f32]) -> Vec<u8> {
    assert!(channels > 0 && channels <= 128 && samples.len().is_multiple_of(channels));
    let data_size = u32::try_from(samples.len() * 4).unwrap();
    let mut bytes = Vec::with_capacity(44 + data_size as usize);
    bytes.extend(b"RIFF");
    bytes.extend((36 + data_size).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16_u32.to_le_bytes());
    bytes.extend(3_u16.to_le_bytes()); // WAVE_FORMAT_IEEE_FLOAT
    bytes.extend((channels as u16).to_le_bytes());
    bytes.extend(RATE.to_le_bytes());
    bytes.extend((RATE * channels as u32 * 4).to_le_bytes());
    bytes.extend((channels as u16 * 4).to_le_bytes());
    bytes.extend(32_u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(data_size.to_le_bytes());
    for sample in samples {
        assert!(sample.is_finite());
        bytes.extend(sample.to_le_bytes());
    }
    bytes
}

fn read_wav(bytes: &[u8]) -> Result<Wav, String> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE file".into());
    }
    let end = 8 + u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    if end > bytes.len() || end < 12 {
        return Err("truncated RIFF".into());
    }
    let mut offset = 12;
    let mut format = None;
    let mut data = None;
    while offset + 8 <= end {
        let tag = &bytes[offset..offset + 4];
        let length = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        offset += 8;
        let next = offset.checked_add(length).ok_or("chunk length overflow")?;
        let chunk = bytes
            .get(offset..next)
            .filter(|_| next <= end)
            .ok_or("truncated WAV chunk")?;
        match tag {
            b"fmt " => format = Some(chunk),
            b"data" => data = Some(chunk),
            _ => {}
        }
        offset = next + length % 2;
    }
    let fmt = format
        .filter(|fmt| fmt.len() >= 16)
        .ok_or("missing WAV format")?;
    let tag = u16::from_le_bytes(fmt[..2].try_into().unwrap());
    let channels = u16::from_le_bytes(fmt[2..4].try_into().unwrap()) as usize;
    let rate = u32::from_le_bytes(fmt[4..8].try_into().unwrap());
    let align = u16::from_le_bytes(fmt[12..14].try_into().unwrap()) as usize;
    let bits = u16::from_le_bytes(fmt[14..16].try_into().unwrap());
    let extensible_float = tag == 0xfffe
        && fmt.len() >= 40
        && fmt[24..40] == [3, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113];
    if (tag != 3 && !extensible_float) || bits != 32 || channels == 0 || align != channels * 4 {
        return Err(format!(
            "expected 32-bit float WAV, got tag={tag} bits={bits} channels={channels} align={align}"
        ));
    }
    let data = data.ok_or("missing WAV data")?;
    if !data.len().is_multiple_of(align) {
        return Err("incomplete sample frame".into());
    }
    let samples: Vec<_> = data
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect();
    if samples.iter().any(|sample| !sample.is_finite()) {
        return Err("non-finite WAV sample".into());
    }
    Ok(Wav {
        channels,
        rate,
        samples,
    })
}

fn oracle(name: &str) -> (SignalChain, Option<MimoPaths>) {
    let fixture = Fixture::load("filters", name);
    let mut chain = fixture.inputs()["chain"].clone();
    for main in chain["mains"].as_array_mut().unwrap() {
        main["fir"] = serde_json::to_value(fixture.f64s(&main["fir"])).unwrap();
    }
    let chain = serde_json::from_value(chain).unwrap();
    let mut mimo = fixture.inputs().get("mimo").cloned().unwrap_or(Value::Null);
    if let Some(paths) = mimo.get_mut("paths").and_then(Value::as_array_mut) {
        for path in paths {
            if let Some(fir) = path["filter"].get_mut("Fir") {
                fir["taps_48k"] = serde_json::to_value(fixture.f64s(&fir["taps_48k"])).unwrap();
            }
        }
    }
    (chain, serde_json::from_value(mimo).unwrap())
}

fn source_samples(routing: &Routing) -> (usize, Vec<f32>) {
    let stride = routing.length + GUARD;
    let frames = GUARD + routing.inputs.len() * stride + GUARD;
    let mut samples = vec![0.0; frames * routing.inputs.len()];
    for input in 0..routing.inputs.len() {
        samples[(GUARD + input * stride) * routing.inputs.len() + input] = 1.0;
    }
    (frames, samples)
}

fn rpp_path(path: &Path) -> String {
    let value = path.to_str().unwrap().replace('\\', "/");
    assert!(
        !value.contains(['"', '\n', '\r']),
        "unsupported scratch path"
    );
    value
}

fn prepare(root: &Path, name: &str) -> Routing {
    fs::create_dir_all(root).unwrap();
    let (chain, mimo) = oracle(name);
    assert!(!chain.subs.is_empty());
    if name == "mimo_iir" {
        assert!(mimo.as_ref().is_some_and(|mimo| !mimo.paths.is_empty()));
    }
    let run = FilterRun {
        chain: &chain,
        mimo: mimo.as_ref(),
        mode: "mixed",
        note: "",
        label: "REAPER playback",
        latency_ms: chain.latency_ms,
        rc_pieces: &[],
        stamp: "fixed",
    };
    let bundle = root.join("bundle");
    export_convolution_named(&run, &bundle, Some("CI 보정")).unwrap();
    let routing: Routing =
        serde_json::from_slice(&fs::read(bundle.join("routing.json")).unwrap()).unwrap();
    assert_eq!(routing.sample_rate, RATE);
    let (frames, samples) = source_samples(&routing);
    let source = root.join("source.wav");
    fs::write(&source, wav_bytes(routing.inputs.len(), &samples)).unwrap();
    let config = root.join("config");
    fs::create_dir(&config).unwrap();
    fs::write(
        config.join("SECS_REAPER_TEST_ONLY"),
        "disposable CI instance\n",
    )
    .unwrap();
    // No license, evaluation-state or user preferences are copied or fabricated.
    fs::write(config.join("reaper.ini"), "[REAPER]\nverchk=0\n").unwrap();
    fs::write(config.join("empty.rpp"), "<REAPER_PROJECT 0.1 7.84 1\n>\n").unwrap();
    fs::write(config.join("source.rpp"), format!(
        "<REAPER_PROJECT 0.1 7.84 1\n SAMPLERATE 48000\n <TRACK\n  NAME \"Source\"\n  NCHAN {}\n  <ITEM\n   POSITION 0\n   LENGTH {:.15}\n   SOFFS 0\n   VOLPAN 1 0 1 -1\n   FADEIN 1 0 0\n   FADEOUT 1 0 0\n   <SOURCE WAVE\n    FILE \"{}\"\n   >\n  >\n >\n>\n",
        routing.inputs.len().max(2).next_multiple_of(2), frames as f64 / f64::from(RATE), rpp_path(&source)
    )).unwrap();
    fs::write(
        root.join("playback.lua"),
        include_str!("reaper/playback.lua").replace("\r\n", "\n"),
    )
    .unwrap();
    routing
}

fn expected(routing: &Routing, bundle: &Path) -> Wav {
    let (frames, _) = source_samples(routing);
    let channels = routing.outputs.len().max(2).next_multiple_of(2);
    let mut samples = vec![0.0; frames * channels];
    for path in &routing.paths {
        let input = routing
            .inputs
            .iter()
            .position(|name| name == &path.input)
            .unwrap();
        let output = routing
            .outputs
            .iter()
            .position(|name| name == &path.output)
            .unwrap();
        let wav = read_wav(&fs::read(bundle.join(&path.wav)).unwrap()).unwrap();
        assert_eq!(
            (wav.channels, wav.rate, wav.samples.len()),
            (1, RATE, routing.length)
        );
        let start = GUARD + input * (routing.length + GUARD);
        for (index, value) in wav.samples.iter().enumerate() {
            samples[(start + index) * channels + output] += value;
        }
    }
    Wav {
        channels,
        rate: RATE,
        samples,
    }
}

fn compare(actual: &Wav, expected: &Wav) -> Result<f64, String> {
    if actual.channels != expected.channels
        || actual.rate != expected.rate
        || actual.samples.len() != expected.samples.len()
    {
        return Err(format!(
            "WAV shape mismatch: actual {}ch/{}Hz/{}samples, expected {}ch/{}Hz/{}samples",
            actual.channels,
            actual.rate,
            actual.samples.len(),
            expected.channels,
            expected.rate,
            expected.samples.len()
        ));
    }
    let mut max_error = 0.0_f64;
    for (index, (&got, &want)) in actual.samples.iter().zip(&expected.samples).enumerate() {
        let error = (f64::from(got) - f64::from(want)).abs();
        let bound = ABS + REL * f64::from(want).abs();
        if !got.is_finite() || error > bound {
            return Err(format!(
                "frame {} channel {}: actual={got:.9e} expected={want:.9e}, error={error:.9e} bound={bound:.9e}; no time or gain fitting",
                index / actual.channels,
                index % actual.channels + 1
            ));
        }
        max_error = max_error.max(error);
    }
    Ok(max_error)
}

fn collect_artifacts(root: &Path, name: &str) {
    let Some(destination) = std::env::var_os("SECS_REAPER_ARTIFACTS") else {
        return;
    };
    let destination = PathBuf::from(destination).join(name);
    fs::create_dir_all(&destination).unwrap();
    // Deliberate allowlist: never copy the installation, config/resource tree,
    // downloaded archive, source project or bundle into uploaded artifacts.
    for file in [
        "dump.txt",
        "result.txt",
        "render.wav",
        "stdout.log",
        "stderr.log",
        "splash.log",
        "comparison.log",
    ] {
        let source = root.join(file);
        if source.is_file() {
            fs::copy(source, destination.join(file)).unwrap();
        }
    }
}

fn run_engine(root: &Path, routing: &Routing, executable: &Path) -> Result<(), String> {
    let config = root.join("config");
    let home = root.join("home");
    fs::create_dir(&home).unwrap();
    let (frames, _) = source_samples(routing);
    // GNU timeout supervises xvfb-run's whole process group, including REAPER.
    // This is a Linux-only ignored test; no platform API or unsafe code is used.
    let status = Command::new("timeout")
        .args([
            "--kill-after=10s",
            "180s",
            "xvfb-run",
            "-a",
            "--server-args=-screen 0 1280x1024x24",
        ])
        .arg(executable)
        .args(["-newinst", "-nosplash", "-ignoreerrors", "-splashlog"])
        .arg(root.join("splash.log"))
        .arg("-cfgfile")
        .arg(config.join("reaper.ini"))
        .arg(config.join("source.rpp"))
        .arg(root.join("playback.lua"))
        .current_dir(root)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("SECS_REAPER_TEST_ROOT", root)
        .env("SECS_REAPER_INPUTS", routing.inputs.len().to_string())
        .env(
            "SECS_REAPER_OUTPUTS",
            routing.outputs.len().max(2).next_multiple_of(2).to_string(),
        )
        .env("SECS_REAPER_PATHS", routing.paths.len().to_string())
        .env("SECS_REAPER_FRAMES", frames.to_string())
        .stdout(Stdio::from(
            fs::File::create(root.join("stdout.log")).unwrap(),
        ))
        .stderr(Stdio::from(
            fs::File::create(root.join("stderr.log")).unwrap(),
        ))
        .status()
        .map_err(|error| format!("starting REAPER: {error}"))?;
    let result = fs::read_to_string(root.join("result.txt")).unwrap_or_default();
    if !status.success() || result.trim() != "PASS" {
        let mut detail =
            format!("REAPER status={status} (124/137 means timeout); result={result}\n");
        for name in ["stdout.log", "stderr.log", "splash.log", "dump.txt"] {
            let text = fs::read_to_string(root.join(name)).unwrap_or_default();
            detail.push_str(&format!("\n{name}:\n{text}"));
        }
        return Err(detail);
    }
    Ok(())
}

#[test]
#[ignore = "REAPER is required; CI runs this test with --ignored"]
fn reaper_plays_convolution_bundles_as_written() {
    assert_eq!(
        std::env::consts::OS,
        "linux",
        "real-host harness requires Linux xvfb-run and timeout"
    );
    let executable = PathBuf::from(
        std::env::var_os("SECS_REAPER").expect("set SECS_REAPER to the pinned executable"),
    );
    assert!(executable.is_absolute() && executable.is_file());
    let temporary = TempDir::new();
    for name in ["computed_dolby", "mimo_iir"] {
        let root = temporary.0.join(name);
        let routing = prepare(&root, name);
        let result = run_engine(&root, &routing, &executable).and_then(|()| {
            let actual = read_wav(&fs::read(root.join("render.wav")).map_err(|error| error.to_string())?)?;
            let error = compare(&actual, &expected(&routing, &root.join("bundle")))?;
            Ok(format!("{name}: {} inputs, {} outputs, {} paths; max sample error={error:.9e}; REL={REL} ABS={ABS}; no alignment/gain fitting\n",
                routing.inputs.len(), routing.outputs.len(), routing.paths.len()))
        });
        fs::write(
            root.join("comparison.log"),
            match &result {
                Ok(text) | Err(text) => text,
            },
        )
        .unwrap();
        collect_artifacts(&root, name);
        match result {
            Ok(text) => eprint!("{text}"),
            Err(error) => panic!("{name}: {error}"),
        }
    }
}

#[test]
fn reaper_playback_wav_roundtrip_and_rejects_wrong_format() {
    let samples = [0.0, 1.0, -0.25, 0.5, 0.0, -1.0];
    let bytes = wav_bytes(3, &samples);
    let wav = read_wav(&bytes).unwrap();
    assert_eq!((wav.channels, wav.rate), (3, RATE));
    assert_eq!(wav.samples, samples);
    for end in [0, 11, 40, bytes.len() - 1] {
        assert!(read_wav(&bytes[..end]).is_err());
    }
    let mut pcm = bytes.clone();
    pcm[20] = 1;
    assert!(read_wav(&pcm).unwrap_err().contains("32-bit float"));
    let mut nan = bytes;
    nan[44..48].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(read_wav(&nan).unwrap_err().contains("non-finite"));
}

#[test]
fn reaper_playback_comparison_rejects_gain_delay_leaks_and_nonfinite() {
    let expected = Wav {
        channels: 2,
        rate: RATE,
        samples: vec![0.0, 0.0, 1.0, 0.0, 0.25, 0.0, 0.0, 0.0],
    };
    assert_eq!(compare(&expected, &expected).unwrap(), 0.0);
    for samples in [
        vec![0.0, 0.0, 0.99, 0.0, 0.2475, 0.0, 0.0, 0.0],
        vec![0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.25, 0.0],
        vec![0.0, 0.0, 1.0, 0.01, 0.25, 0.0, 0.0, 0.0],
        vec![0.0, 0.0, f32::NAN, 0.0, 0.25, 0.0, 0.0, 0.0],
    ] {
        assert!(
            compare(
                &Wav {
                    channels: 2,
                    rate: RATE,
                    samples
                },
                &expected
            )
            .is_err()
        );
    }
}

#[test]
fn reaper_playback_prepares_oracle_impulses_and_silent_routes_without_engine() {
    let temporary = TempDir::new();
    for name in ["computed_dolby", "mimo_iir"] {
        let root = temporary.0.join(name);
        let routing = prepare(&root, name);
        let source = read_wav(&fs::read(root.join("source.wav")).unwrap()).unwrap();
        assert_eq!(
            source
                .samples
                .iter()
                .filter(|&&sample| sample == 1.0)
                .count(),
            routing.inputs.len()
        );
        assert_eq!(source.channels, routing.inputs.len());
        let expected = expected(&routing, &root.join("bundle"));
        assert!(expected.samples.iter().any(|sample| sample.abs() > 1e-3));
        for (input, role) in routing.inputs.iter().enumerate() {
            let start = GUARD + input * (routing.length + GUARD);
            for output in 0..expected.channels {
                if output >= routing.outputs.len()
                    || !routing
                        .paths
                        .iter()
                        .any(|path| path.input == *role && path.output == routing.outputs[output])
                {
                    assert!(
                        (start..start + routing.length).all(|frame| expected.samples
                            [frame * expected.channels + output]
                            == 0.0)
                    );
                }
            }
        }
        assert!(
            fs::read_to_string(root.join("config/source.rpp"))
                .unwrap()
                .contains("NCHAN ")
        );
    }
}
