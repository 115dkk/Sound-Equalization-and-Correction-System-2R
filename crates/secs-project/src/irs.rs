use crate::{ProjectError, record::atomic_write};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const MAX_IR_BYTES: u64 = 256 * 1024 * 1024;

pub fn ir_directory(project: &Path) -> PathBuf {
    project.with_extension("irs")
}
pub fn ir_file(project: &Path, role: &str, pos: i64) -> PathBuf {
    let safe = role.replace([':', '/', '\\'], "_");
    ir_directory(project).join(format!("{safe}_P{pos}.wav"))
}
/// Writes the upstream mono float32 RIFF layout.
///
/// For values that Python's `struct.pack('<I', ...)` cannot represent, the
/// legacy infallible API returns an empty vector instead of panicking. Call
/// [`try_write_wav_float32`] when the error must be reported.
pub fn write_wav_float32(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    try_write_wav_float32(samples, sample_rate).unwrap_or_default()
}

pub fn try_write_wav_float32(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>, ProjectError> {
    let bytes = samples
        .len()
        .checked_mul(4)
        .ok_or_else(|| ProjectError::Wav("WAV payload size overflow".into()))?;
    let payload_size = u32::try_from(bytes)
        .map_err(|_| ProjectError::Wav("WAV payload exceeds RIFF limit".into()))?;
    let riff_size = payload_size
        .checked_add(36)
        .ok_or_else(|| ProjectError::Wav("WAV RIFF size overflow".into()))?;
    let byte_rate = sample_rate
        .checked_mul(4)
        .ok_or_else(|| ProjectError::Wav("WAV sample rate overflow".into()))?;
    let capacity = 44usize
        .checked_add(bytes)
        .ok_or_else(|| ProjectError::Wav("WAV allocation size overflow".into()))?;
    let mut out = Vec::with_capacity(capacity);
    out.extend(b"RIFF");
    out.extend(&riff_size.to_le_bytes());
    out.extend(b"WAVEfmt \x10\0\0\0\x03\0\x01\0");
    out.extend(&sample_rate.to_le_bytes());
    out.extend(&byte_rate.to_le_bytes());
    out.extend(&4u16.to_le_bytes());
    out.extend(&32u16.to_le_bytes());
    out.extend(b"data");
    out.extend(&payload_size.to_le_bytes());
    for x in samples {
        out.extend(&x.to_le_bytes());
    }
    Ok(out)
}
const PCM_GUID: [u8; 16] = [1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113];
const FLOAT_GUID: [u8; 16] = [3, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113];

#[derive(Debug, Clone, Copy)]
struct WavFormat {
    tag: u16,
    channels: u16,
    sample_rate: u32,
    block_align: u16,
    bits: u16,
    valid_bits: u16,
}
type WavChunks = (WavFormat, Vec<u8>);

fn le_u16(bytes: &[u8], offset: usize) -> Result<u16, ProjectError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| ProjectError::Wav("truncated WAV field".into()))?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}
fn le_u32(bytes: &[u8], offset: usize) -> Result<u32, ProjectError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| ProjectError::Wav("truncated WAV field".into()))?;
    Ok(u32::from_le_bytes(value.try_into().map_err(|_| {
        ProjectError::Wav("truncated WAV field".into())
    })?))
}
fn parse_wav_format(bytes: &[u8]) -> Result<WavFormat, ProjectError> {
    if bytes.len() < 16 {
        return Err(ProjectError::Wav("short fmt chunk".into()));
    }
    let mut tag = le_u16(bytes, 0)?;
    let channels = le_u16(bytes, 2)?;
    let sample_rate = le_u32(bytes, 4)?;
    let block_align = le_u16(bytes, 12)?;
    let bits = le_u16(bytes, 14)?;
    let mut valid_bits = bits;
    if tag == 0xfffe {
        if bytes.len() < 40 || le_u16(bytes, 16)? < 22 {
            return Err(ProjectError::Wav("short extensible fmt chunk".into()));
        }
        valid_bits = le_u16(bytes, 18)?;
        let guid = bytes
            .get(24..40)
            .ok_or_else(|| ProjectError::Wav("missing extensible subformat".into()))?;
        tag = if guid == PCM_GUID {
            1
        } else if guid == FLOAT_GUID {
            3
        } else {
            return Err(ProjectError::Wav(
                "unsupported extensible WAV subformat".into(),
            ));
        };
    }
    Ok(WavFormat {
        tag,
        channels,
        sample_rate,
        block_align,
        bits,
        valid_bits,
    })
}
fn wav_chunks(data: &[u8], default_format: Option<WavFormat>) -> Result<WavChunks, ProjectError> {
    if data.len() < 12 || &data[..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(ProjectError::Wav("not a RIFF/WAVE file".into()));
    }
    let mut pos = 12usize;
    let mut format = None;
    let mut payload = None;
    while let Some(header_end) = pos.checked_add(8) {
        if header_end > data.len() {
            break;
        }
        let size_u32 = le_u32(data, pos + 4)?;
        if &data[pos..pos + 4] == b"data" && u64::from(size_u32) > MAX_IR_BYTES {
            return Err(ProjectError::Wav(format!(
                "WAV data chunk exceeds {MAX_IR_BYTES} bytes"
            )));
        }
        let size = size_u32 as usize;
        let declared_end = header_end
            .checked_add(size)
            .ok_or_else(|| ProjectError::Wav("chunk size overflow".into()))?;
        // F17: a cut `data` chunk is an error, not a shorter IR. Other chunks
        // keep upstream's clamp to the file end.
        if &data[pos..pos + 4] == b"data" && declared_end > data.len() {
            return Err(ProjectError::Wav(format!(
                "WAV data chunk declares {size} bytes but the file holds {}",
                data.len() - header_end
            )));
        }
        let end = declared_end.min(data.len());
        match &data[pos..pos + 4] {
            b"fmt " => format = Some(parse_wav_format(&data[header_end..end])?),
            b"data" => payload = Some(data[header_end..end].to_vec()),
            _ => {}
        }
        if declared_end >= data.len() {
            break;
        }
        pos = declared_end
            .checked_add(size & 1)
            .ok_or_else(|| ProjectError::Wav("chunk padding overflow".into()))?;
        if pos > data.len() {
            break;
        }
    }
    Ok((
        format
            .or(default_format)
            .ok_or_else(|| ProjectError::Wav("missing fmt chunk".into()))?,
        payload.ok_or_else(|| ProjectError::Wav("missing data chunk".into()))?,
    ))
}
pub fn read_wav_float32(data: &[u8]) -> Result<(Vec<f32>, u32), ProjectError> {
    let default = WavFormat {
        tag: 3,
        channels: 1,
        sample_rate: 48000,
        block_align: 4,
        bits: 32,
        valid_bits: 32,
    };
    let (format, payload) = wav_chunks(data, Some(default))?;
    if format.tag != 3 || format.bits != 32 || payload.len() % 4 != 0 {
        return Err(ProjectError::Wav(format!(
            "not 32-bit float WAV (fmt={}, bits={})",
            format.tag, format.bits
        )));
    }
    Ok((
        payload
            .as_chunks::<4>()
            .0
            .iter()
            .map(|x| f32::from_le_bytes(*x))
            .collect(),
        format.sample_rate,
    ))
}
/// Reads mono PCM 16/24/32 and IEEE float 32/64 WAV, including extensible WAV.
/// PCM 8-bit and valid-bit/container combinations other than left-aligned PCM in
/// a 32-bit container are unsupported. It also does not decode compressed codecs,
/// RF64, endian-swapped RIFX, multichannel audio, or containers other than WAV.
pub fn read_mono_float_wav(data: &[u8]) -> Result<(Vec<f32>, u32), ProjectError> {
    let (format, payload) = wav_chunks(data, None)?;
    if format.channels != 1 {
        return Err(ProjectError::Wav("WAV is not mono".into()));
    }
    let bytes_per_sample = usize::from(format.bits.div_ceil(8));
    if bytes_per_sample == 0
        || usize::from(format.block_align) != bytes_per_sample
        || payload.len() % bytes_per_sample != 0
    {
        return Err(ProjectError::Wav("invalid block alignment".into()));
    }
    let out = match (format.tag, format.bits, format.valid_bits) {
        (3, 32, 32) => payload
            .as_chunks::<4>()
            .0
            .iter()
            .map(|x| f32::from_le_bytes(*x))
            .collect(),
        (3, 64, 64) => payload
            .as_chunks::<8>()
            .0
            .iter()
            .map(|x| f64::from_le_bytes(*x) as f32)
            .collect(),
        (1, 16, 16) => payload
            .as_chunks::<2>()
            .0
            .iter()
            .map(|x| i16::from_le_bytes(*x) as f32 / 32768.)
            .collect(),
        (1, 24, 24) => payload
            .as_chunks::<3>()
            .0
            .iter()
            .map(|x| {
                let n = ((x[0] as i32) | ((x[1] as i32) << 8) | ((x[2] as i32) << 16)) << 8 >> 8;
                n as f32 / 8388608.
            })
            .collect(),
        (1, 32, 32) => payload
            .as_chunks::<4>()
            .0
            .iter()
            .map(|x| i32::from_le_bytes(*x) as f32 / 2147483648.)
            .collect(),
        (1, 32, valid @ (1..=31)) => {
            let shift = 32 - u32::from(valid);
            let divisor = (1_u64 << (valid - 1)) as f32;
            payload
                .as_chunks::<4>()
                .0
                .iter()
                .map(|x| (i32::from_le_bytes(*x) >> shift) as f32 / divisor)
                .collect()
        }
        _ => {
            return Err(ProjectError::Wav(format!(
                "unsupported WAV format {}/{}/{}",
                format.tag, format.bits, format.valid_bits
            )));
        }
    };
    Ok((out, format.sample_rate))
}
pub fn save_ir(
    project: &Path,
    role: &str,
    pos: i64,
    samples: &[f32],
    rate: u32,
) -> Result<PathBuf, ProjectError> {
    let path = ir_file(project, role, pos);
    let directory = ir_directory(project);
    reject_symlink(&directory)?;
    if !directory.exists() {
        fs::create_dir_all(&directory)?;
    }
    reject_symlink(&directory)?;
    reject_symlink(&path)?;
    atomic_write(&path, &try_write_wav_float32(samples, rate)?)?;
    Ok(path)
}

fn reject_symlink(path: &Path) -> Result<(), ProjectError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(ProjectError::Invalid(format!(
            "IR write path is a symlink: {}",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
pub fn load_ir(project: &Path, role: &str, pos: i64) -> Result<(Vec<f32>, u32), ProjectError> {
    let path = ir_file(project, role, pos);
    read_mono_float_wav(&read_bounded(&path, MAX_IR_BYTES, "WAV")?)
        .map_err(|error| ProjectError::Wav(format!("{}: {error}", path.display())))
}

pub(crate) fn read_bounded(path: &Path, limit: u64, kind: &str) -> Result<Vec<u8>, ProjectError> {
    let mut file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.by_ref().take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(ProjectError::Invalid(format!(
            "{kind} file {} exceeds {limit} bytes",
            path.display()
        )));
    }
    Ok(bytes)
}
