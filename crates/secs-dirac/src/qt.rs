//! Bounded Qt QDataStream reads. Ignored header strings are never decoded.

use crate::{DiracError, Limits};

pub(crate) struct Cursor<'a> {
    remaining: &'a [u8],
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.remaining.is_empty()
    }

    pub(crate) fn take(&mut self, len: usize, what: &'static str) -> Result<&'a [u8], DiracError> {
        let (value, rest) = self
            .remaining
            .split_at_checked(len)
            .ok_or(DiracError::Truncated(what))?;
        self.remaining = rest;
        Ok(value)
    }

    pub(crate) fn array<const N: usize>(
        &mut self,
        what: &'static str,
    ) -> Result<[u8; N], DiracError> {
        self.take(N, what)?
            .try_into()
            .map_err(|_| DiracError::Truncated(what))
    }

    pub(crate) fn byte(&mut self, what: &'static str) -> Result<u8, DiracError> {
        let [byte] = self.array(what)?;
        Ok(byte)
    }

    pub(crate) fn u32(&mut self, what: &'static str) -> Result<u32, DiracError> {
        Ok(u32::from_be_bytes(self.array(what)?))
    }

    fn string_bytes(&mut self, limits: &Limits) -> Result<&'a [u8], DiracError> {
        let length = self.u32("QString length")?;
        if length == u32::MAX {
            return Ok(&[]);
        }
        if length % 2 != 0 {
            return Err(DiracError::Invalid("odd QString length".into()));
        }
        let length =
            usize::try_from(length).map_err(|_| DiracError::Limit("QString length".into()))?;
        if length > limits.max_string_bytes {
            return Err(DiracError::Limit("QString bytes".into()));
        }
        self.take(length, "QString")
    }

    pub(crate) fn string(&mut self, limits: &Limits) -> Result<String, DiracError> {
        let units: Vec<u16> = self
            .string_bytes(limits)?
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .collect();
        Ok(String::from_utf16_lossy(&units))
    }

    pub(crate) fn skip_string(&mut self, limits: &Limits) -> Result<(), DiracError> {
        self.string_bytes(limits)?;
        Ok(())
    }
}
