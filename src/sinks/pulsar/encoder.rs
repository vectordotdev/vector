//! Encoding for the `Pulsar` sink.
use crate::{
    event::Event,
    sinks::util::encoding::{write_all, Encoder},
};
use bytes::BytesMut;
use std::io;
use tokio_util::codec::Encoder as _;

#[derive(Clone, Debug)]
pub(super) struct PulsarEncoder {
    pub(super) encoder: crate::codecs::Encoder<()>,
    pub(super) transformer: crate::codecs::Transformer,
}

impl Encoder<Event> for PulsarEncoder {
    fn encode_input(&self, mut input: Event, writer: &mut dyn io::Write) -> io::Result<usize> {
        let mut body = BytesMut::new();
        self.transformer.transform(&mut input);
        let mut encoder = self.encoder.clone();
        encoder
            .encode(input, &mut body)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "unable to encode"))?;

        let body = body.freeze();
        write_all(writer, 1, body.as_ref())?;

        Ok(body.len())
    }
}
