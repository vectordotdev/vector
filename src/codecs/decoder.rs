use bytes::{Bytes, BytesMut};
use codecs::decoding::{
    format::Deserializer as _, BoxedFramingError, BytesDeserializer, Deserializer,
    DeserializerConfig, Error, Framer, FramingConfig, NewlineDelimitedDecoder,
};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::{
    event::Event,
    internal_events::{DecoderDeserializeFailed, DecoderFramingFailed},
};

/// A decoder that can decode structured events from a byte stream / byte
/// messages.
#[derive(Debug, Clone)]
pub struct Decoder {
    framer: Framer,
    deserializer: Deserializer,
}

impl Default for Decoder {
    fn default() -> Self {
        Self {
            framer: Framer::NewlineDelimited(NewlineDelimitedDecoder::new()),
            deserializer: Deserializer::Bytes(BytesDeserializer::new()),
        }
    }
}

impl Decoder {
    /// Creates a new `Decoder` with the specified `Framer` to produce byte
    /// frames from the byte stream / byte messages and `Deserializer` to parse
    /// structured events from a byte frame.
    pub const fn new(framer: Framer, deserializer: Deserializer) -> Self {
        Self {
            framer,
            deserializer,
        }
    }

    /// Handles the framing result and parses it into a structured event, if
    /// possible.
    ///
    /// Emits logs if either framing or parsing failed.
    fn handle_framing_result(
        &mut self,
        frame: Result<Option<Bytes>, BoxedFramingError>,
    ) -> Result<Option<(SmallVec<[Event; 1]>, usize)>, Error> {
        let frame = frame.map_err(|error| {
            emit!(DecoderFramingFailed { error: &error });
            Error::FramingError(error)
        })?;

        let frame = match frame {
            Some(frame) => frame,
            _ => return Ok(None),
        };

        let byte_size = frame.len();

        // Parse structured events from the byte frame.
        self.deserializer
            .parse(frame)
            .map(|event| Some((event, byte_size)))
            .map_err(|error| {
                emit!(DecoderDeserializeFailed { error: &error });
                Error::ParsingError(error)
            })
    }
}

impl tokio_util::codec::Decoder for Decoder {
    type Item = (SmallVec<[Event; 1]>, usize);
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let frame = self.framer.decode(buf);
        self.handle_framing_result(frame)
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let frame = self.framer.decode_eof(buf);
        self.handle_framing_result(frame)
    }
}

/// Config used to build a `Decoder`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DecodingConfig {
    /// The framing config.
    framing: FramingConfig,
    /// The decoding config.
    decoding: DeserializerConfig,
}

impl DecodingConfig {
    /// Creates a new `DecodingConfig` with the provided `FramingConfig` and
    /// `DeserializerConfig`.
    pub const fn new(framing: FramingConfig, decoding: DeserializerConfig) -> Self {
        Self { framing, decoding }
    }

    /// Builds a `Decoder` from the provided configuration.
    pub fn build(self) -> Decoder {
        // Build the framer.
        let framer = self.framing.build();

        // Build the deserializer.
        let deserializer = self.decoding.build();

        Decoder::new(framer, deserializer)
    }
}

#[cfg(test)]
mod tests {
    use super::Decoder;
    use bytes::Bytes;
    use codecs::{
        decoding::{Deserializer, Framer},
        JsonDeserializer, NewlineDelimitedDecoder, StreamDecodingError,
    };
    use futures::{stream, StreamExt};
    use tokio_util::{codec::FramedRead, io::StreamReader};
    use value::Value;

    #[tokio::test]
    async fn framed_read_recover_from_error() {
        let iter = stream::iter(
            ["{ \"foo\": 1 }\n", "invalid\n", "{ \"bar\": 2 }\n"]
                .into_iter()
                .map(Bytes::from),
        );
        let stream = iter.map(Ok::<_, std::io::Error>);
        let reader = StreamReader::new(stream);
        let decoder = Decoder::new(
            Framer::NewlineDelimited(NewlineDelimitedDecoder::new()),
            Deserializer::Json(JsonDeserializer::new()),
        );
        let mut stream = FramedRead::new(reader, decoder);

        let next = stream.next().await.unwrap();
        let event = next.unwrap().0.pop().unwrap().into_log();
        assert_eq!(event.get("foo").unwrap(), &Value::from(1));

        let next = stream.next().await.unwrap();
        let error = next.unwrap_err();
        assert!(error.can_continue());

        let next = stream.next().await.unwrap();
        let event = next.unwrap().0.pop().unwrap().into_log();
        assert_eq!(event.get("bar").unwrap(), &Value::from(2));
    }
}
