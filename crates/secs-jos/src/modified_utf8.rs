// Port of src/secs/rew_mdat/serialization/modified_utf8.py and writer.py (GPL-3.0-only).
use crate::JosError;

/// Decode to UTF-16 units, retaining lone surrogates exactly.
pub fn decode_units(data: &[u8]) -> Result<Vec<u16>, JosError> {
    let mut units = Vec::new();
    let mut index = 0;
    while index < data.len() {
        let first = data[index];
        let (unit, width) = if first <= 127 {
            if first == 0 {
                return Err(JosError::format("literal null is invalid Modified UTF-8"));
            }
            (u16::from(first), 1)
        } else if first & 224 == 192 {
            let second = *data
                .get(index + 1)
                .ok_or_else(|| JosError::format("truncated two-byte Modified UTF-8 sequence"))?;
            if second & 192 != 128 {
                return Err(JosError::format("invalid Modified UTF-8 continuation byte"));
            }
            let value = (u16::from(first & 31) << 6) | u16::from(second & 63);
            if value < 128 && value != 0 {
                return Err(JosError::format("overlong Modified UTF-8 sequence"));
            }
            (value, 2)
        } else if first & 240 == 224 {
            if data.len() - index < 3 {
                return Err(JosError::format(
                    "truncated three-byte Modified UTF-8 sequence",
                ));
            }
            let (second, third) = (data[index + 1], data[index + 2]);
            if second & 192 != 128 || third & 192 != 128 {
                return Err(JosError::format("invalid Modified UTF-8 continuation byte"));
            }
            let value = (u16::from(first & 15) << 12)
                | (u16::from(second & 63) << 6)
                | u16::from(third & 63);
            if value < 2048 {
                return Err(JosError::format("overlong Modified UTF-8 sequence"));
            }
            (value, 3)
        } else {
            return Err(JosError::format(format!(
                "invalid Modified UTF-8 lead byte 0x{first:02x}"
            )));
        };
        units.push(unit);
        index += width;
    }
    Ok(units)
}
/// Decode Java text. Lone UTF-16 surrogates become U+FFFD; use [`decode_units`]
/// for a lossless representation. All byte-level errors match upstream.
pub fn decode_modified_utf8(data: &[u8]) -> Result<String, JosError> {
    Ok(String::from_utf16_lossy(&decode_units(data)?))
}
/// Encode UTF-16 units, including lone surrogates, as Java Modified UTF-8.
pub fn encode_units(units: impl IntoIterator<Item = u16>) -> Vec<u8> {
    let mut out = Vec::new();
    for unit in units {
        if (1..=0x7f).contains(&unit) {
            out.push(unit as u8);
        } else if unit <= 0x7ff {
            out.extend([0xc0 | (unit >> 6) as u8, 0x80 | (unit & 0x3f) as u8]);
        } else {
            out.extend([
                0xe0 | (unit >> 12) as u8,
                0x80 | ((unit >> 6) & 0x3f) as u8,
                0x80 | (unit & 0x3f) as u8,
            ]);
        }
    }
    out
}
pub fn encode_modified_utf8(value: &str) -> Vec<u8> {
    encode_units(value.encode_utf16())
}
