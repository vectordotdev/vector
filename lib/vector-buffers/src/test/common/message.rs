use std::{error, fmt, mem};

use bytes::{Buf, BufMut};
use quickcheck::{Arbitrary, Gen};
use vector_common::byte_size_of::ByteSizeOf;

use crate::encoding::FixedEncodable;

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
pub struct Message {
    id: u64,
}

impl Message {
    pub(crate) fn new(id: u64) -> Self {
        Message { id }
    }
}

impl ByteSizeOf for Message {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

impl Arbitrary for Message {
    fn arbitrary(g: &mut Gen) -> Self {
        Message {
            id: u64::arbitrary(g),
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(self.id.shrink().map(|id| Message { id }))
    }
}

impl FixedEncodable for Message {
    type EncodeError = EncodeError;
    type DecodeError = DecodeError;

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::EncodeError>
    where
        B: BufMut,
        Self: Sized,
    {
        buffer.put_u64(self.id);
        Ok(())
    }

    fn encoded_size(&self) -> Option<usize> {
        Some(mem::size_of::<u64>())
    }

    fn decode<B>(mut buffer: B) -> Result<Self, Self::DecodeError>
    where
        B: Buf,
        Self: Sized,
    {
        let id = buffer.get_u64();
        Ok(Message::new(id))
    }
}
