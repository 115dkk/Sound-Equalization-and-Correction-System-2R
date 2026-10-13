// Port of src/secs/rew_mdat/serialization/reader.py (GPL-3.0-only).
use crate::{JosError, Limits, modified_utf8::decode_modified_utf8};
pub(crate) struct Reader<'a> {
    pub data: &'a [u8],
    pub offset: usize,
    pub limits: &'a Limits,
    pub allocated: usize,
}
impl<'a> Reader<'a> {
    // Cumulative, conservative payload accounting also bounds reference-driven
    // amplification. Vec/map overhead is charged by the parser at each insertion.
    pub fn charge(&mut self, bytes: usize) -> Result<(), JosError> {
        self.allocated = self
            .allocated
            .checked_add(bytes)
            .filter(|n| *n <= self.limits.max_file_bytes)
            .ok_or_else(|| JosError::limit("decoded allocation limit exceeded", self.offset))?;
        Ok(())
    }
    pub fn read(&mut self, count: usize) -> Result<&'a [u8], JosError> {
        let remaining = self.data.len() - self.offset;
        if count > remaining {
            return Err(JosError::format(format!(
                "truncated stream: needed {count} bytes, found {remaining}"
            ))
            .at(self.offset));
        }
        let start = self.offset;
        self.offset += count;
        Ok(&self.data[start..self.offset])
    }
    pub fn fixed<const N: usize>(&mut self) -> Result<[u8; N], JosError> {
        let mut bytes = [0; N];
        bytes.copy_from_slice(self.read(N)?);
        Ok(bytes)
    }
    pub fn u1(&mut self) -> Result<u8, JosError> {
        Ok(self.read(1)?[0])
    }
    pub fn u2(&mut self) -> Result<u16, JosError> {
        Ok(u16::from_be_bytes(self.fixed()?))
    }
    pub fn u4(&mut self) -> Result<u32, JosError> {
        Ok(u32::from_be_bytes(self.fixed()?))
    }
    pub fn i4(&mut self) -> Result<i32, JosError> {
        Ok(i32::from_be_bytes(self.fixed()?))
    }
    pub fn i8(&mut self) -> Result<i64, JosError> {
        Ok(i64::from_be_bytes(self.fixed()?))
    }
    pub fn utf(&mut self, long: bool) -> Result<(String, Vec<u8>), JosError> {
        let length = if long {
            self.i8()?
        } else {
            i64::from(self.u2()?)
        };
        if length < 0 {
            return Err(
                JosError::format(format!("negative string length {length}")).at(self.offset)
            );
        }
        if length as u64 > self.limits.max_string_bytes as u64 {
            return Err(JosError::limit(
                format!(
                    "string length {length} exceeds limit {}",
                    self.limits.max_string_bytes
                ),
                self.offset,
            ));
        }
        let raw = self.read(length as usize)?;
        self.charge(raw.len().saturating_mul(12))?;
        Ok((decode_modified_utf8(raw)?, raw.to_vec()))
    }
}
