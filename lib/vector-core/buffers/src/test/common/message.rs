use crate::bytes::{DecodeBytes, EncodeBytes};
use bytes::{Buf, BufMut};
use proptest_derive::Arbitrary;
use std::{fmt, mem};

#[derive(Clone, Debug, PartialEq, Eq, Arbitrary)]
pub struct Message {
    id: u64,
}

impl Message {
    pub(crate) fn new(id: u64) -> Self {
        Message { id }
    }
}

//
// Serialization and Deserialization
//

#[derive(Debug)]
pub enum EncodeError {}

#[derive(Debug)]
pub enum DecodeError {}

impl fmt::Display for DecodeError {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unreachable!()
    }
}

impl EncodeBytes<Message> for Message {
    type Error = EncodeError;

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::Error>
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
}

impl DecodeBytes<Message> for Message {
    type Error = DecodeError;

    fn decode<B>(mut buffer: B) -> Result<Self, Self::Error>
    where
        B: Buf,
        Self: Sized,
    {
        let id = buffer.get_u64();
        Ok(Message::new(id))
    }
}
