#![forbid(unsafe_code)]
//! Checks on a real loopback pair (a virtual cable) that a shared-mode stream
//! carries device channels unchanged: a tone on channel K of an N-channel
//! playback stream arrives on capture channel K at the played level and on no
//! other channel, whether the capture stream has one channel or all of them.
//!
//! `channel_check <output name part> <input name part>`; exits 1 on a failure.
//! The cable must connect playback channel K to capture channel K. Channels
//! the capture endpoint does not have are played but not judged.

use std::process::ExitCode;
use std::time::Duration;

use secs_audio::session::{PlaybackBuffer, SessionRequest, play_and_record};
use secs_audio::{CancelToken, Endpoint, SharePreference};

const RATE: u32 = 48_000;
const PLAYED_DBFS: f64 = -20.0;
/// The remixes this check exists for move a tone by 4 dB or more.
const LEVEL_TOLERANCE_DB: f64 = 1.0;
/// A remixed tone arrives above −28 dBFS; what a cable still holds from the
/// previous case has been seen up to −37.
const SILENT_BELOW_DBFS: f64 = -32.0;

fn pick(endpoints: &[Endpoint], filter: &str, output: bool) -> Result<Endpoint, String> {
    let filter = filter.to_lowercase();
    endpoints
        .iter()
        .filter(|e| {
            if output {
                e.max_output_channels > 0
            } else {
                e.max_input_channels > 0
            }
        })
        .filter(|e| e.name.to_lowercase().contains(&filter))
        .min_by_key(|e| e.name.len())
        .cloned()
        .ok_or_else(|| format!("no endpoint contains {filter:?}"))
}

/// Level in dBFS of the loudest quarter second of every captured channel.
fn levels(interleaved: &[f32], channels: usize) -> Vec<f64> {
    let window = RATE as usize / 4;
    (0..channels)
        .map(|channel| {
            let samples: Vec<f64> = interleaved
                .chunks_exact(channels)
                .map(|frame| f64::from(frame[channel]))
                .collect();
            let mut loudest = 0.0_f64;
            let mut start = 0;
            while start + window <= samples.len() {
                let power = samples[start..start + window]
                    .iter()
                    .map(|x| x * x)
                    .sum::<f64>()
                    / window as f64;
                loudest = loudest.max(power);
                start += window / 4;
            }
            10.0 * loudest.log10()
        })
        .collect()
}

fn run() -> Result<bool, String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        return Err("channel_check <output name part> <input name part>".into());
    }
    let backend = secs_sys::audio::default_backend();
    let endpoints = backend.enumerate().map_err(|e| e.to_string())?;
    let output = pick(&endpoints, &args[1], true)?;
    let input = pick(&endpoints, &args[2], false)?;
    println!(
        "output {} ({} ch), input {} ({} ch)",
        output.name, output.max_output_channels, input.name, input.max_input_channels
    );
    let frames = RATE as usize / 2;
    let amplitude = 10_f64.powf(PLAYED_DBFS / 20.0) * 2_f64.sqrt();
    let mut capture_counts = vec![1, input.max_input_channels.min(8)];
    capture_counts.dedup();
    let mut passed = true;
    for &capture_channels in &capture_counts {
        for stream_channels in 1..=output.max_output_channels.min(8) {
            for tone in 1..=stream_channels {
                let mut interleaved = vec![0.0_f32; frames * usize::from(stream_channels)];
                for (i, frame) in interleaved
                    .chunks_exact_mut(usize::from(stream_channels))
                    .enumerate()
                {
                    let phase = 2.0 * std::f64::consts::PI * 1000.0 * i as f64 / f64::from(RATE);
                    frame[usize::from(tone - 1)] = (amplitude * phase.sin()) as f32;
                }
                let mut request = SessionRequest::new(
                    output.clone(),
                    input.clone(),
                    PlaybackBuffer {
                        sample_rate: RATE,
                        channels: stream_channels,
                        interleaved,
                    },
                    capture_channels,
                );
                request.share = SharePreference::Shared;
                request.tail_seconds = 0.5;
                let label =
                    format!("tone on {tone} of {stream_channels}, capture {capture_channels} ch");
                let recording =
                    play_and_record(backend.as_ref(), request, &CancelToken::new(), &mut |_| {})
                        .map_err(|e| format!("{label}: {e}"))?;
                let found = levels(&recording.interleaved, usize::from(capture_channels));
                // Channels the capture endpoint lacks are mixed by the cable itself.
                let judged = tone <= input.max_input_channels;
                let ok = !judged
                    || found.iter().enumerate().all(|(index, &level)| {
                        if index + 1 == usize::from(tone) {
                            (level - PLAYED_DBFS).abs() <= LEVEL_TOLERANCE_DB
                        } else {
                            level < SILENT_BELOW_DBFS
                        }
                    });
                passed &= ok;
                let text: Vec<String> = found.iter().map(|level| format!("{level:7.2}")).collect();
                println!(
                    "{} {label}: {}",
                    if !judged {
                        "SKIP"
                    } else if ok {
                        "ok  "
                    } else {
                        "FAIL"
                    },
                    text.join(" ")
                );
                // Let the cable empty before the next case.
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
    Ok(passed)
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => {
            eprintln!("a shared-mode stream did not carry device channels unchanged");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
