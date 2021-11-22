use crate::bytes::{DecodeBytes, EncodeBytes};
use bytes::{Buf, BufMut};
use core_common::byte_size_of::ByteSizeOf;
use quickcheck::{Arbitrary, Gen};
use std::{error, fmt, mem};

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariableMessage {
    id: u64,
    payload: Vec<u8>,
}

impl VariableMessage {
    pub(crate) fn new(id: u64, payload: Vec<u8>) -> Self {
        VariableMessage { id, payload }
    }
}

impl ByteSizeOf for VariableMessage {
    fn allocated_bytes(&self) -> usize {
        self.payload.len()
    }
}

impl Arbitrary for VariableMessage {
    fn arbitrary(g: &mut Gen) -> Self {
        VariableMessage {
            id: u64::arbitrary(g),
            payload: Vec::<u8>::arbitrary(g),
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let parts = (self.id, self.payload.clone());
        Box::new(
            parts
                .shrink()
                .map(|(id, payload)| VariableMessage { id, payload }),
        )
    }
}

impl EncodeBytes<VariableMessage> for VariableMessage {
    type Error = EncodeError;

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::Error>
    where
        B: BufMut,
        Self: Sized,
    {
        buffer.put_u64(self.id);
        buffer.put_u64(self.payload.len() as u64);
        buffer.put_slice(&self.payload);
        Ok(())
    }

    fn encoded_size(&self) -> Option<usize> {
        Some(mem::size_of::<u64>())
    }
}

impl DecodeBytes<VariableMessage> for VariableMessage {
    type Error = DecodeError;

    fn decode<B>(mut buffer: B) -> Result<Self, Self::Error>
    where
        B: Buf,
        Self: Sized,
    {
        let id = buffer.get_u64();
        let payload_len = buffer.get_u64() as usize;
        let payload = buffer.copy_to_bytes(payload_len).to_vec();
        Ok(VariableMessage::new(id, payload))
    }
}
