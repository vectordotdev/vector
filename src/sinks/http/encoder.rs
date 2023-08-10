//! Encoding for the `http` sink.

use crate::{
    event::Event,
    sinks::util::encoding::{write_all, Encoder as SinkEncoder},
};
use bytes::BytesMut;
use codecs::encoding::Framer;
use std::io;
use tokio_util::codec::Encoder as _;

use crate::sinks::prelude::*;

#[derive(Clone, Debug)]
pub(super) struct HttpEncoder {
    pub(super) encoder: Encoder<Framer>,
    pub(super) transformer: Transformer,
}

impl HttpEncoder {
    /// Creates a new `HttpEncoder`.
    pub(super) const fn new(encoder: Encoder<Framer>, transformer: Transformer) -> Self {
        Self {
            encoder,
            transformer,
        }
    }
}

impl SinkEncoder<Vec<Event>> for HttpEncoder {
    fn encode_input(
        &self,
        events: Vec<Event>,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let mut encoder = self.encoder.clone();
        let mut byte_size = telemetry().create_request_count_byte_size();
        let mut body = BytesMut::new();

        for mut event in events {
            self.transformer.transform(&mut event);

            byte_size.add_event(&event, event.estimated_json_encoded_size_of());

            encoder
                .encode(event, &mut body)
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "unable to encode event"))?;
        }

        let body = body.freeze();

        write_all(writer, 1, body.as_ref()).map(|()| (body.len(), byte_size))
    }
}
