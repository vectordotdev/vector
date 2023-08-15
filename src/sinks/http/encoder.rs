//! Encoding for the `http` sink.

use crate::{
    event::Event,
    sinks::util::encoding::{write_all, Encoder as SinkEncoder},
};
use bytes::{BufMut, BytesMut};
use codecs::{
    encoding::{
        Framer,
        Framer::{CharacterDelimited, NewlineDelimited},
        Serializer::Json,
    },
    CharacterDelimitedEncoder,
};
use std::io;
use tokio_util::codec::Encoder as _;

use crate::sinks::prelude::*;

#[derive(Clone, Debug)]
pub(super) struct HttpEncoder {
    pub(super) encoder: Encoder<Framer>,
    pub(super) transformer: Transformer,
    pub(super) payload_prefix: String,
    pub(super) payload_suffix: String,
}

impl HttpEncoder {
    /// Creates a new `HttpEncoder`.
    pub(super) const fn new(
        encoder: Encoder<Framer>,
        transformer: Transformer,
        payload_prefix: String,
        payload_suffix: String,
    ) -> Self {
        Self {
            encoder,
            transformer,
            payload_prefix,
            payload_suffix,
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

        match (self.encoder.serializer(), self.encoder.framer()) {
            (Json(_), NewlineDelimited(_)) => {
                if !body.is_empty() {
                    // Remove trailing newline for backwards-compatibility
                    // with Vector `0.20.x`.
                    body.truncate(body.len() - 1);
                }
            }
            (Json(_), CharacterDelimited(CharacterDelimitedEncoder { delimiter: b',' })) => {
                // TODO(https://github.com/vectordotdev/vector/issues/11253):
                // Prepend before building a request body to eliminate the
                // additional copy here.
                let message = body.split();
                body.put(self.payload_prefix.as_bytes());
                body.put_u8(b'[');
                if !message.is_empty() {
                    body.unsplit(message);
                    // remove trailing comma from last record
                    body.truncate(body.len() - 1);
                }
                body.put_u8(b']');
                body.put(self.payload_suffix.as_bytes());
            }
            _ => {}
        }

        let body = body.freeze();

        write_all(writer, 1, body.as_ref()).map(|()| (body.len(), byte_size))
    }
}
