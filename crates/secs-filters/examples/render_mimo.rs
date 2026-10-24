#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Offline, streaming reference consumer; see docs/perf/resource-optimizations.md.
//! All PCM files are interleaved little-endian f64 at 48 kHz, without a header.
use secs_filters::playback::{MimoFirPlayback, PlaybackMode};
use secs_types::chain::{MimoPaths, SignalChain};
use std::{
    error::Error,
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug)]
struct PcmReader {
    reader: BufReader<File>,
    frames: usize,
    channels: usize,
    bytes: Vec<u8>,
}
impl PcmReader {
    fn open(path: &Path, channels: usize, block: usize) -> Result<Self> {
        let file = File::open(path)?;
        let frame_bytes = channels.checked_mul(8).ok_or("PCM frame size overflow")?;
        let length = usize::try_from(file.metadata()?.len())?;
        if frame_bytes == 0 || length % frame_bytes != 0 {
            return Err("PCM length is not a whole number of frames".into());
        }
        Ok(Self {
            reader: BufReader::new(file),
            frames: length / frame_bytes,
            channels,
            bytes: vec![
                0;
                block
                    .checked_mul(frame_bytes)
                    .ok_or("PCM block size overflow")?
            ],
        })
    }
    fn block(&mut self, valid: usize, output: &mut [Vec<f64>]) -> Result<()> {
        for channel in output.iter_mut() {
            channel.fill(0.0);
        }
        let count = valid * self.channels * 8;
        self.reader.read_exact(&mut self.bytes[..count])?;
        for (frame, bytes) in self.bytes[..count]
            .chunks_exact(self.channels * 8)
            .enumerate()
        {
            for (channel, sample) in bytes.as_chunks::<8>().0.iter().enumerate() {
                let value = f64::from_le_bytes(*sample);
                if !value.is_finite() {
                    return Err("PCM contains a nonfinite sample".into());
                }
                output[channel][frame] = value;
            }
        }
        Ok(())
    }
}

fn read_plan(path: &Path) -> Result<(SignalChain, MimoPaths)> {
    let file = File::open(path)?;
    if file.metadata()?.len() > 64 * 1024 * 1024 {
        return Err("plan exceeds 64 MiB".into());
    }
    let mut value: serde_json::Value = serde_json::from_reader(BufReader::new(file))?;
    let (chain, mimo) = if value.get("results").is_some() {
        let version = value["version"].as_u64().ok_or("cache has no version")?;
        if version != 4 && version != 5 {
            return Err("example accepts v4/v5 caches or a {chain,mimo} plan".into());
        }
        if version == 5
            && value["fir_materialization_version"].as_u64()
                != Some(u64::from(secs_dsp::fir::LOWRATE_MATERIALIZATION_VERSION))
        {
            return Err("unsupported FIR materialization version".into());
        }
        (
            value["results"]["calibration_chain"].take(),
            value["results"]["mimo_paths"].take(),
        )
    } else {
        (value["chain"].take(), value["mimo"].take())
    };
    Ok((
        serde_json::from_value(chain)?,
        serde_json::from_value(mimo)?,
    ))
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 3 || args.iter().any(|a| a == "--help") {
        eprintln!(
            "Usage: cargo run --release -p secs-filters --example render_mimo -- \
PLAN.json INPUT.f64 OUTPUT.f64 [--multirate] [--factor N] [--block N] \
[--base BASE.f64] [--tail-frames N]\n\
48 kHz interleaved f64 little-endian PCM. INPUT: raw main channels in chain order.\n\
OUTPUT: main then sub channels. Without --base it contains MIMO contributions only.\n\
--base: already-rendered base outputs in OUTPUT order; longer base tails are retained.\n\
--block: high-rate samples (default 1024), or low-rate samples with --multirate (default 32).\n\
--factor: multirate conversion ratio (default 80, requiring 600 Hz FirLowRate paths).\n\
Default tail: FIR/converter/output-delay tail + 96000 samples of IIR settling.\n\
Existing output files are never overwritten. A failed run can leave a partial output.\n\
Multirate mode has a different response and adds alignment delay; inspect stdout JSON."
        );
        if args.iter().any(|a| a == "--help") {
            return Ok(());
        }
        return Err("three positional paths are required".into());
    }
    let mut multirate = false;
    let mut factor = 80;
    let mut factor_set = false;
    let mut block = None;
    let mut base_path = None;
    let mut tail = None;
    let mut index = 3;
    while index < args.len() {
        match args[index].as_str() {
            "--multirate" => {
                multirate = true;
                index += 1;
            }
            option @ ("--factor" | "--block" | "--base" | "--tail-frames") => {
                let value = args.get(index + 1).ok_or("missing option value")?;
                match option {
                    "--factor" => {
                        factor = value.parse()?;
                        factor_set = true;
                    }
                    "--block" => block = Some(value.parse::<usize>()?),
                    "--base" => base_path = Some(value.as_str()),
                    "--tail-frames" => tail = Some(value.parse::<usize>()?),
                    _ => unreachable!(),
                }
                index += 2;
            }
            _ => return Err(format!("unknown option: {}", args[index]).into()),
        }
    }
    if factor_set && !multirate {
        return Err("--factor requires --multirate".into());
    }
    let (chain, paths) = read_plan(Path::new(&args[0]))?;
    let mode = if multirate {
        PlaybackMode::Multirate {
            low_block_size: block.unwrap_or(32),
            factor,
        }
    } else {
        PlaybackMode::FullRate {
            block_size: block.unwrap_or(1024),
        }
    };
    let mut renderer = MimoFirPlayback::new(&chain, &paths, mode)?;
    let block = renderer.block_size();
    let ni = renderer.input_roles().len();
    let no = renderer.output_roles().len();
    let mut input = PcmReader::open(Path::new(&args[1]), ni, block)?;
    let mut base = base_path
        .map(|p| PcmReader::open(Path::new(p), no, block))
        .transpose()?;
    let tail = tail.unwrap_or(
        renderer
            .fir_tail_samples()
            .checked_add(96_000)
            .ok_or("tail overflow")?,
    );
    let support_end = input
        .frames
        .checked_add(tail)
        .ok_or("output frame count overflow")?;
    let base_end = base
        .as_ref()
        .map(|base| {
            base.frames
                .checked_add(renderer.additional_latency_samples())
                .ok_or("base frame count overflow")
        })
        .transpose()?
        .unwrap_or(0);
    let total = support_end.max(base_end);
    let mut inputs = vec![vec![0.0; block]; ni];
    let mut bases = vec![vec![0.0; block]; no];
    let mut outputs = vec![vec![0.0; block]; no];
    let mut bytes = vec![
        0;
        block
            .checked_mul(no)
            .and_then(|n| n.checked_mul(8))
            .ok_or("output block size overflow")?
    ];
    let output_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?;
    let mut output = BufWriter::new(output_file);
    for offset in (0..total).step_by(block) {
        let valid_input = input.frames.saturating_sub(offset).min(block);
        input.block(valid_input, &mut inputs)?;
        if let Some(base) = &mut base {
            base.block(base.frames.saturating_sub(offset).min(block), &mut bases)?;
        }
        // The example's slice-view vectors are outside the realtime kernel.
        // The public renderer and both DSP kernels reuse their own work buffers.
        let inputs: Vec<_> = inputs.iter().map(Vec::as_slice).collect();
        let mut views: Vec<_> = outputs.iter_mut().map(Vec::as_mut_slice).collect();
        if base.is_some() {
            let bases: Vec<_> = bases.iter().map(Vec::as_slice).collect();
            renderer.process_with_base(&inputs, &bases, &mut views)?;
        } else {
            renderer.process(&inputs, &mut views)?;
        }
        let valid_output = (total - offset).min(block);
        for frame in 0..valid_output {
            for (channel, values) in outputs.iter().enumerate() {
                let start = (frame * no + channel) * 8;
                bytes[start..start + 8].copy_from_slice(&values[frame].to_le_bytes());
            }
        }
        output.write_all(&bytes[..valid_output * no * 8])?;
    }
    output.flush()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "sample_rate": 48_000,
            "format": "interleaved_f64_le",
            "input_roles": renderer.input_roles(),
            "output_roles": renderer.output_roles(),
            "mode": if multirate { "experimental_multirate" } else { "full_rate" },
            "includes_base": base.is_some(),
            "block_frames": block,
            "additional_latency_samples": renderer.additional_latency_samples(),
            "transforms_per_block": renderer.transforms_per_block(),
            "input_frames": input.frames,
            "base_frames": base.as_ref().map(|b| b.frames),
            "tail_frames": tail,
            "output_frames": total
        }))?
    );
    Ok(())
}
