//! Encoding for the `http` sink.

use std::io;

use bytes::{BufMut, BytesMut};
use tokio_util::codec::Encoder as _;
use vector_lib::codecs::{
    CharacterDelimitedEncoder,
    encoding::{
        Framer,
        Framer::{CharacterDelimited, NewlineDelimited},
        Serializer::Json,
    },
};

use crate::{
    event::Event,
    sinks::{
        prelude::*,
        util::encoding::{Encoder as SinkEncoder, write_all},
    },
};

#[derive(Clone, Debug)]
pub(super) struct HttpEncoder {
    pub(super) encoder: Encoder<Framer>,
    transformer: Transformer,
    payload_prefix: String,
    payload_suffix: String,
}

impl HttpEncoder {
    /// Whether this encoder serializes with the `otlp` codec.
    #[cfg_attr(not(feature = "codecs-opentelemetry"), expect(clippy::unused_self))]
    pub(super) const fn is_otlp(&self) -> bool {
        #[cfg(feature = "codecs-opentelemetry")]
        {
            matches!(
                self.encoder.serializer(),
                vector_lib::codecs::encoding::Serializer::Otlp(_)
            )
        }
        #[cfg(not(feature = "codecs-opentelemetry"))]
        {
            false
        }
    }

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
        let n_events = events.len();

        if let (Json(_), CharacterDelimited(CharacterDelimitedEncoder { delimiter: b',' })) =
            (self.encoder.serializer(), self.encoder.framer())
        {
            body.put(self.payload_prefix.as_bytes());
            body.put_u8(b'[');
        }

        for mut event in events {
            self.transformer.transform(&mut event);

            #[cfg(not(feature = "typed-trace"))]
            {
                byte_size.add_event(&event, event.estimated_json_encoded_size_of());
                encoder.encode(event, &mut body).map_err(io::Error::other)?;
            }
            // An OTLP trace whose spans were all rejected encodes to nothing, and the
            // codec already counted it as discarded. Other codecs can also write zero
            // bytes without reporting a discard, so they stay in the sent count.
            #[cfg(feature = "typed-trace")]
            {
                let json_size = event.estimated_json_encoded_size_of();
                let tags = byte_size.event_tags(&event);
                let body_len = body.len();
                encoder.encode(event, &mut body).map_err(io::Error::other)?;
                if !self.is_otlp() || body.len() > body_len {
                    byte_size.add_event_tags(tags, json_size);
                }
            }
        }

        match (self.encoder.serializer(), self.encoder.framer()) {
            (Json(_), NewlineDelimited(_)) if !body.is_empty() => {
                // Remove trailing newline for backwards-compatibility
                // with Vector `0.20.x`.
                body.truncate(body.len() - 1);
            }
            (Json(_), CharacterDelimited(CharacterDelimitedEncoder { delimiter: b',' })) => {
                if !body.is_empty() {
                    // remove trailing comma from last record
                    body.truncate(body.len() - 1);
                }
                body.put_u8(b']');
                body.put(self.payload_suffix.as_bytes());
            }
            _ => {}
        }

        let body = body.freeze();

        write_all(writer, n_events, body.as_ref()).map(|()| (body.len(), byte_size))
    }
}
