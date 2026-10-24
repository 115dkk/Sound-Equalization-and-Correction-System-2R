//! Synthetic, non-private projects for parser tests and import-flow fixtures.

use crate::{CODE_FULL_SCALE, MU};

#[derive(Debug, Clone)]
pub struct SyntheticChannel {
    pub name: String,
    pub kind: u64,
}

#[derive(Debug, Clone)]
pub struct SyntheticProject {
    pub format_version: u32,
    pub name: String,
    pub microphone: String,
    pub sample_rate: u32,
    pub channels: Vec<SyntheticChannel>,
    pub positions: usize,
    /// Position-major; None writes field 6 absent.
    pub responses: Vec<Option<Vec<f64>>>,
}

impl SyntheticProject {
    /// Two positions, Left and Subwoofer, with the second subwoofer unmeasured.
    pub fn minimal() -> Self {
        let impulse = |index, amplitude| {
            Some(
                (0..64)
                    .map(|i| if i == index { amplitude } else { 0.0 })
                    .collect(),
            )
        };
        Self {
            format_version: 10,
            name: "Synthetic".into(),
            microphone: "Test microphone".into(),
            sample_rate: 48000,
            channels: vec![
                SyntheticChannel {
                    name: "Left".into(),
                    kind: 1,
                },
                SyntheticChannel {
                    name: "Subwoofer".into(),
                    kind: 4,
                },
            ],
            positions: 2,
            responses: vec![impulse(16, 1.0), impulse(18, 0.5), impulse(17, -0.75), None],
        }
    }
}

/// Inverse μ-law quantizer for a response with positive peak magnitude `peak`.
pub fn encode_code(x: f64, peak: f64) -> i16 {
    (CODE_FULL_SCALE * x.signum() * (1.0 + MU * x.abs() / peak).ln() / (1.0 + MU).ln())
        .round()
        .clamp(-CODE_FULL_SCALE, CODE_FULL_SCALE) as i16
}

/// Write the Qt envelope and session, including unrelated fields to skip.
pub fn write(project: &SyntheticProject) -> Vec<u8> {
    let mut session = Vec::new();
    bytes(&mut session, 1, b"synthetic-session");
    integer(&mut session, 3, u64::from(project.sample_rate));
    bytes(&mut session, 14, &20.0f32.to_le_bytes());
    bytes(&mut session, 15, &0.0f32.to_le_bytes());
    let mut input = Vec::new();
    integer(&mut input, 1, 0);
    bytes(&mut input, 2, b"Synthetic input");
    bytes(&mut session, 29, &input);
    let mut recording = Vec::new();
    integer(&mut recording, 4, 0);
    bytes(&mut recording, 13, b"OggS\0synthetic-not-a-recording");
    bytes(&mut session, 50, &recording);
    let mut analysis = Vec::new();
    fixed32(&mut analysis, 30, 1.0);
    bytes(&mut session, 51, &analysis);
    let mut sweep = Vec::new();
    fixed32(&mut sweep, 1, project.sample_rate as f32);
    integer(&mut sweep, 4, 240000);
    bytes(&mut session, 60, &sweep);

    let mut set = Vec::new();
    integer(&mut set, 1, u64::from(project.sample_rate));
    integer(&mut set, 2, project.channels.len() as u64);
    integer(&mut set, 3, project.positions as u64);
    for response in &project.responses {
        let mut entry = Vec::new();
        integer(&mut entry, 1, u64::from(project.sample_rate));
        if let Some(response) = response {
            let peak = response.iter().map(|x| x.abs()).fold(0.0, f64::max);
            fixed32(&mut entry, 5, (peak / CODE_FULL_SCALE) as f32);
            let mut codes = Vec::new();
            for &sample in response {
                let code = if peak == 0.0 {
                    0
                } else {
                    encode_code(sample, peak)
                };
                codes.extend_from_slice(&code.to_be_bytes());
            }
            bytes(&mut entry, 6, &codes);
        }
        bytes(&mut set, 4, &entry);
    }
    bytes(&mut session, 12, &set);
    for channel in &project.channels {
        let mut entry = Vec::new();
        bytes(&mut entry, 1, channel.name.as_bytes());
        integer(&mut entry, 2, channel.kind);
        bytes(&mut session, 30, &entry);
    }
    envelope(project, &session)
}

pub(crate) fn varint(output: &mut Vec<u8>, mut value: u64) {
    while value >= 128 {
        output.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

pub(crate) fn integer(output: &mut Vec<u8>, field: u64, value: u64) {
    varint(output, field << 3);
    varint(output, value);
}

pub(crate) fn bytes(output: &mut Vec<u8>, field: u64, value: &[u8]) {
    varint(output, (field << 3) | 2);
    varint(output, value.len() as u64);
    output.extend_from_slice(value);
}

pub(crate) fn fixed32(output: &mut Vec<u8>, field: u64, value: f32) {
    varint(output, (field << 3) | 5);
    output.extend_from_slice(&value.to_le_bytes());
}

fn qstring(output: &mut Vec<u8>, value: &str) {
    let encoded: Vec<u8> = value.encode_utf16().flat_map(u16::to_be_bytes).collect();
    output.extend_from_slice(&(encoded.len() as u32).to_be_bytes());
    output.extend_from_slice(&encoded);
}

pub(crate) fn header(project: &SyntheticProject) -> Vec<u8> {
    let mut output = Vec::new();
    output.extend_from_slice(&project.format_version.to_be_bytes());
    for text in [
        project.name.as_str(),
        "Live Desktop",
        "00000000-0000-0000-0000-000000000000",
        "00000000-0000-0000-0000-000000000000",
        project.microphone.as_str(),
        "",
    ] {
        qstring(&mut output, text);
    }
    output.extend_from_slice(&0u32.to_be_bytes());
    output
}

pub(crate) fn envelope(project: &SyntheticProject, session: &[u8]) -> Vec<u8> {
    let mut output = header(project);
    output.push(1);
    output.extend_from_slice(&(session.len() as u32).to_be_bytes());
    output.extend_from_slice(session);
    output.push(1);
    output.extend_from_slice(&32u32.to_be_bytes());
    output.extend_from_slice(&[0xa5; 32]);
    qstring(&mut output, "3.14.3");
    output
}
