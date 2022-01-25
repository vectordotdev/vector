use std::{
    error, fmt, mem,
    num::{NonZeroU16, NonZeroU32, NonZeroU8},
    ops::Range,
};

use bytes::{Buf, BufMut};
use proptest::{
    arbitrary::{Arbitrary, StrategyFor},
    strategy::Map,
};
use vector_common::byte_size_of::ByteSizeOf;

use crate::encoding::{DecodeBytes, EncodeBytes};

#[derive(Debug)]
pub struct EncodeError;

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl error::Error for EncodeError {}

#[derive(Debug)]
pub struct DecodeError;

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl error::Error for DecodeError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    id: u32,
    size: u32,
}

impl Record {
    pub(crate) const fn new(id: u32, size: u32) -> Self {
        Record { id, size }
    }

    const fn header_len() -> usize {
        mem::size_of::<u32>() + mem::size_of::<u32>()
    }

    pub const fn len(&self) -> usize {
        Self::header_len() + self.size as usize
    }
}

impl ByteSizeOf for Record {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

impl EncodeBytes for Record {
    type Error = EncodeError;

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::Error>
    where
        B: BufMut,
        Self: Sized,
    {
        if buffer.remaining_mut() < self.len() {
            return Err(EncodeError);
        }

        buffer.put_u32(self.id);
        buffer.put_u32(self.size);
        buffer.put_bytes(0x42, self.size as usize);
        Ok(())
    }

    fn encoded_size(&self) -> Option<usize> {
        Some(self.len())
    }
}

impl DecodeBytes for Record {
    type Error = DecodeError;

    fn decode<B>(mut buffer: B) -> Result<Self, Self::Error>
    where
        B: Buf,
        Self: Sized,
    {
        if buffer.remaining() < Self::header_len() {
            return Err(DecodeError);
        }

        let id = buffer.get_u32();
        let size = buffer.get_u32();

        if buffer.remaining() < size as usize {
            return Err(DecodeError);
        }

        let payload = buffer.copy_to_bytes(size as usize);
        let valid = &payload.iter().all(|b| *b == 0x42);
        if !valid {
            return Err(DecodeError);
        }

        Ok(Record::new(id, size))
    }
}
