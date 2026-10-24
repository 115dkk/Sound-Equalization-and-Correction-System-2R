//! Non-recursive protobuf wire reader. Length-delimited values borrow the input.

use crate::{DiracError, qt::Cursor};

pub(crate) enum Value<'a> {
    Varint(u64),
    Fixed64,
    Bytes(&'a [u8]),
    Fixed32([u8; 4]),
}

impl<'a> Value<'a> {
    pub(crate) fn varint(self) -> Result<u64, DiracError> {
        match self {
            Self::Varint(value) => Ok(value),
            _ => Err(DiracError::Invalid("expected varint field".into())),
        }
    }

    pub(crate) fn bytes(self) -> Result<&'a [u8], DiracError> {
        match self {
            Self::Bytes(value) => Ok(value),
            _ => Err(DiracError::Invalid(
                "expected length-delimited field".into(),
            )),
        }
    }

    pub(crate) fn float(self) -> Result<f32, DiracError> {
        match self {
            Self::Fixed32(value) => Ok(f32::from_le_bytes(value)),
            _ => Err(DiracError::Invalid("expected fixed32 field".into())),
        }
    }
}

pub(crate) struct Reader<'a> {
    cursor: Cursor<'a>,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self {
            cursor: Cursor::new(bytes),
        }
    }

    fn varint(&mut self) -> Result<u64, DiracError> {
        let mut value = 0u64;
        for index in 0..10 {
            let byte = self.cursor.byte("protobuf varint")?;
            if index == 9 && byte > 1 {
                return Err(DiracError::Invalid("protobuf varint overflow".into()));
            }
            value |= u64::from(byte & 0x7f) << (index * 7);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(DiracError::Invalid("protobuf varint overflow".into()))
    }

    pub(crate) fn next(&mut self) -> Result<Option<(u64, Value<'a>)>, DiracError> {
        if self.cursor.is_empty() {
            return Ok(None);
        }
        let tag = self.varint()?;
        let field = tag >> 3;
        if field == 0 {
            return Err(DiracError::Invalid("protobuf field number zero".into()));
        }
        let value = match tag & 7 {
            0 => Value::Varint(self.varint()?),
            1 => {
                self.cursor.take(8, "protobuf fixed64")?;
                Value::Fixed64
            }
            2 => {
                let length = usize::try_from(self.varint()?)
                    .map_err(|_| DiracError::Truncated("protobuf bytes"))?;
                Value::Bytes(self.cursor.take(length, "protobuf bytes")?)
            }
            5 => Value::Fixed32(self.cursor.array("protobuf fixed32")?),
            _ => return Err(DiracError::Invalid("unsupported protobuf wire type".into())),
        };
        Ok(Some((field, value)))
    }
}
