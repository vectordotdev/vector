//! Encoding for the `AMQP` sink.
use std::io;

use bytes::BytesMut;
use tokio_util::codec::Encoder as _;

use crate::{
    event::Event,
    sinks::util::encoding::{write_all, Encoder},
};

#[derive(Clone, Debug)]
pub(super) struct AmqpEncoder {
    pub(super) encoder: crate::codecs::Encoder<()>,
    pub(super) transformer: crate::codecs::Transformer,
}

impl Encoder<Event> for AmqpEncoder {
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
