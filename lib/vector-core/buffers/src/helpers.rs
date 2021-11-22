use crate::bytes::{DecodeBytes, EncodeBytes};
use bytes::{Buf, BufMut};
use core_common::byte_size_of::ByteSizeOf;
use std::{error, fmt, mem};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariableMessage {
    id: u64,
    payload: Vec<u8>,
}

impl VariableMessage {
    pub fn new(id: u64, payload: Vec<u8>) -> Self {
        VariableMessage { id, payload }
    }

    pub fn id(&self) -> u64 {
        self.id
    }
}

impl ByteSizeOf for VariableMessage {
    fn allocated_bytes(&self) -> usize {
        self.payload.len()
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
