#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Bounded Dirac Live `.liveproject` reader; format: `docs/DIRAC_LIVEPROJECT.md`.
//! Only measured IRs, output channels and non-private header strings are kept.
//! Recordings and opaque processor state are not copied or interpreted.

mod proto;
mod qt;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
#[cfg(test)]
mod tests;

use proto::Reader;
use qt::Cursor;

pub const MU: f64 = 255.0;
pub const CODE_FULL_SCALE: f64 = 32767.0;
/// Dirac places a position's first arrival about 250 ms into each stored IR.
pub const NOMINAL_ARRIVAL_S: f64 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_file_bytes: usize,
    pub max_string_bytes: usize,
    pub max_channels: usize,
    pub max_positions: usize,
    pub max_ir_samples: usize,
    pub max_total_samples: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_file_bytes: 256 * 1024 * 1024,
            max_string_bytes: 1024 * 1024,
            max_channels: 64,
            max_positions: 64,
            max_ir_samples: 1 << 20,
            max_total_samples: 1 << 24,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Main,
    Subwoofer,
    Other(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub name: String,
    pub kind: ChannelKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveProject {
    pub format_version: u32,
    pub name: String,
    pub application: String,
    pub microphone: String,
    pub sample_rate: u32,
    pub channels: Vec<Channel>,
    pub positions: usize,
    responses: Vec<Option<Vec<f64>>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measured<'a> {
    pub position: usize,
    pub channel: usize,
    pub samples: &'a [f64],
}

impl LiveProject {
    /// None when out of range or not measured.
    pub fn response(&self, position: usize, channel: usize) -> Option<&[f64]> {
        if position >= self.positions || channel >= self.channels.len() {
            return None;
        }
        let index = position
            .checked_mul(self.channels.len())?
            .checked_add(channel)?;
        self.responses.get(index)?.as_deref()
    }

    /// Measured IRs in file order: position by position, channels in list order.
    pub fn measured(&self) -> impl Iterator<Item = Measured<'_>> {
        self.responses
            .iter()
            .enumerate()
            .filter_map(|(index, response)| {
                Some(Measured {
                    position: index.checked_div(self.channels.len())?,
                    channel: index.checked_rem(self.channels.len())?,
                    samples: response.as_deref()?,
                })
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DiracError {
    #[error("truncated {0}")]
    Truncated(&'static str),
    #[error("{0}")]
    Invalid(String),
    #[error("limit exceeded: {0}")]
    Limit(String),
    #[error("the project holds no measured impulse responses")]
    NoMeasurements,
}

/// One stored code to a linear sample. The most negative code decodes slightly
/// past full scale; taking its unsigned absolute value avoids i16 overflow.
pub fn decode_code(code: i16, scale: f32) -> f64 {
    // Keep the exact endpoints independent of libm's powf rounding.
    let magnitude = match code.unsigned_abs() {
        0 => 0.0,
        32767 => 1.0,
        code => ((1.0 + MU).powf(f64::from(code) / CODE_FULL_SCALE) - 1.0) / MU,
    };
    let value = f64::from(scale) * CODE_FULL_SCALE * magnitude;
    if code < 0 { -value } else { value }
}

pub fn parse(bytes: &[u8], limits: &Limits) -> Result<LiveProject, DiracError> {
    if bytes.len() > limits.max_file_bytes {
        return Err(DiracError::Limit("file bytes".into()));
    }
    let mut cursor = Cursor::new(bytes);
    let format_version = cursor.u32("format version")?;
    let name = cursor.string(limits)?;
    let application = cursor.string(limits)?;
    cursor.skip_string(limits)?;
    cursor.skip_string(limits)?;
    let microphone = cursor.string(limits)?;
    cursor.skip_string(limits)?;
    cursor.u32("header flags")?;
    match cursor.byte("session presence")? {
        0 => return Err(DiracError::NoMeasurements),
        1 => {}
        _ => return Err(DiracError::Invalid("invalid session presence byte".into())),
    }
    let length = cursor.u32("session length")?;
    if length == u32::MAX {
        return Err(DiracError::NoMeasurements);
    }
    let length = usize::try_from(length).map_err(|_| DiracError::Truncated("session"))?;
    let session = cursor.take(length, "session")?;
    let mut reader = Reader::new(session);
    let mut ir_set = None;
    let mut channel_count = 0usize;
    while let Some((field, value)) = reader.next()? {
        match field {
            12 => {
                if ir_set.is_some() {
                    return Err(DiracError::Invalid("duplicate IR set".into()));
                }
                ir_set = Some(value.bytes()?);
            }
            30 => {
                // Validate even a mismatched channel list, without allocating it.
                channel_fields(value.bytes()?, limits)?;
                channel_count = channel_count
                    .checked_add(1)
                    .ok_or_else(|| DiracError::Limit("channel list length".into()))?;
            }
            _ => {}
        }
    }
    let ir_set = ir_set.ok_or(DiracError::NoMeasurements)?;
    let shape = set_shape(ir_set, limits)?;
    let mut responses = Vec::new();
    reserve(&mut responses, shape.slots, "IR slots")?;
    let mut reader = Reader::new(ir_set);
    let mut total_samples = 0usize;
    while let Some((field, value)) = reader.next()? {
        if field == 4 {
            responses.push(response(
                value.bytes()?,
                shape.rate,
                limits,
                &mut total_samples,
            )?);
        }
    }
    if total_samples == 0 {
        return Err(DiracError::NoMeasurements);
    }
    let mut channels = Vec::new();
    reserve(&mut channels, shape.channels, "channels")?;
    if channel_count == shape.channels {
        let mut reader = Reader::new(session);
        while let Some((field, value)) = reader.next()? {
            if field == 30 {
                let (name, kind) = channel_fields(value.bytes()?, limits)?;
                channels.push(Channel {
                    name: if name.is_empty() {
                        fallback_name(channels.len())?
                    } else {
                        String::from_utf8_lossy(name).into_owned()
                    },
                    kind,
                });
            }
        }
    } else {
        for index in 0..shape.channels {
            channels.push(Channel {
                name: fallback_name(index)?,
                kind: ChannelKind::Other(0),
            });
        }
    }
    Ok(LiveProject {
        format_version,
        name,
        application,
        microphone,
        sample_rate: shape.rate,
        channels,
        positions: shape.positions,
        responses,
    })
}

fn reserve<T>(values: &mut Vec<T>, count: usize, what: &str) -> Result<(), DiracError> {
    values
        .try_reserve_exact(count)
        .map_err(|_| DiracError::Limit(format!("{what} allocation")))
}

fn fallback_name(index: usize) -> Result<String, DiracError> {
    let number = index
        .checked_add(1)
        .ok_or_else(|| DiracError::Limit("channel number".into()))?;
    Ok(format!("Channel {number}"))
}

fn channel_fields<'a>(
    bytes: &'a [u8],
    limits: &Limits,
) -> Result<(&'a [u8], ChannelKind), DiracError> {
    let mut name = &[][..];
    let mut kind = ChannelKind::Other(0);
    let mut reader = Reader::new(bytes);
    while let Some((field, value)) = reader.next()? {
        match field {
            1 => {
                name = value.bytes()?;
                if name.len() > limits.max_string_bytes {
                    return Err(DiracError::Limit("channel name bytes".into()));
                }
            }
            2 => {
                kind = match value.varint()? {
                    1 => ChannelKind::Main,
                    4 => ChannelKind::Subwoofer,
                    other => ChannelKind::Other(other),
                };
            }
            _ => {}
        }
    }
    Ok((name, kind))
}

struct Shape {
    rate: u32,
    channels: usize,
    positions: usize,
    slots: usize,
}

fn set_shape(bytes: &[u8], limits: &Limits) -> Result<Shape, DiracError> {
    let (mut rate, mut channels, mut positions) = (0, 0, 0);
    let mut entries = 0usize;
    let mut reader = Reader::new(bytes);
    while let Some((field, value)) = reader.next()? {
        match field {
            1 => rate = value.varint()?,
            2 => channels = value.varint()?,
            3 => positions = value.varint()?,
            4 => {
                value.bytes()?;
                entries = entries
                    .checked_add(1)
                    .ok_or_else(|| DiracError::Limit("IR entry count".into()))?;
            }
            _ => {}
        }
    }
    if rate == 0 || rate > 1_000_000 || channels == 0 || positions == 0 {
        return Err(DiracError::Invalid(
            "invalid or missing IR set dimensions".into(),
        ));
    }
    let channels = usize::try_from(channels).map_err(|_| DiracError::Limit("channels".into()))?;
    let positions =
        usize::try_from(positions).map_err(|_| DiracError::Limit("positions".into()))?;
    if channels > limits.max_channels || positions > limits.max_positions {
        return Err(DiracError::Limit("IR set dimensions".into()));
    }
    let slots = channels
        .checked_mul(positions)
        .ok_or_else(|| DiracError::Limit("IR slot count".into()))?;
    if entries != slots {
        return Err(DiracError::Invalid(
            "IR entry count does not match dimensions".into(),
        ));
    }
    Ok(Shape {
        rate: rate as u32,
        channels,
        positions,
        slots,
    })
}

fn response(
    bytes: &[u8],
    rate: u32,
    limits: &Limits,
    total_samples: &mut usize,
) -> Result<Option<Vec<f64>>, DiracError> {
    let mut scale = None;
    let mut codes = None;
    let mut reader = Reader::new(bytes);
    while let Some((field, value)) = reader.next()? {
        match field {
            1 => {
                if value.varint()? != u64::from(rate) {
                    return Err(DiracError::Invalid(
                        "IR rate does not match set rate".into(),
                    ));
                }
            }
            5 => {
                let value = value.float()?;
                if !value.is_finite() || value < 0.0 {
                    return Err(DiracError::Invalid("invalid IR scale".into()));
                }
                scale = Some(value);
            }
            6 => {
                let value = value.bytes()?;
                if value.len() % 2 != 0 {
                    return Err(DiracError::Invalid("odd IR code length".into()));
                }
                if value.len() / 2 > limits.max_ir_samples {
                    return Err(DiracError::Limit("IR samples".into()));
                }
                codes = Some(value);
            }
            _ => {}
        }
    }
    let Some(codes) = codes else {
        return Ok(None);
    };
    let scale = scale.ok_or_else(|| DiracError::Invalid("IR codes without scale".into()))?;
    if codes.is_empty() {
        return Ok(None);
    }
    let count = codes.len() / 2;
    *total_samples = total_samples
        .checked_add(count)
        .filter(|total| *total <= limits.max_total_samples)
        .ok_or_else(|| DiracError::Limit("total IR samples".into()))?;
    let mut samples = Vec::new();
    reserve(&mut samples, count, "IR samples")?;
    for pair in codes.as_chunks::<2>().0 {
        samples.push(decode_code(i16::from_be_bytes(*pair), scale));
    }
    Ok(Some(samples))
}
