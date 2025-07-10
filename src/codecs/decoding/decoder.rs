use bytes::{Bytes, BytesMut};
use smallvec::SmallVec;
use vector_lib::codecs::decoding::{
    format::Deserializer as _, BoxedFramingError, BytesDeserializer, Deserializer, Error, Framer,
    NewlineDelimitedDecoder,
};
use vector_lib::config::LogNamespace;

use crate::{
    event::Event,
    internal_events::{DecoderDeserializeError, DecoderFramingError},
};

/// A decoder that can decode structured events from a byte stream / byte
/// messages.
#[derive(Clone)]
pub struct Decoder {
    /// The framer being used.
    pub framer: Framer,
    /// The deserializer being used.
    pub deserializer: Deserializer,
    /// The `log_namespace` being used.
    pub log_namespace: LogNamespace,
}

impl Default for Decoder {
    fn default() -> Self {
        Self {
            framer: Framer::NewlineDelimited(NewlineDelimitedDecoder::new()),
            deserializer: Deserializer::Bytes(BytesDeserializer),
            log_namespace: LogNamespace::Legacy,
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
            log_namespace: LogNamespace::Legacy,
        }
    }

    /// Sets the log namespace that will be used when decoding.
    pub const fn with_log_namespace(mut self, log_namespace: LogNamespace) -> Self {
        self.log_namespace = log_namespace;
        self
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
            emit!(DecoderFramingError { error: &error });
            Error::FramingError(error)
        })?;

        frame
            .map(|frame| self.deserializer_parse(frame))
            .transpose()
    }

    /// Parses a frame using the included deserializer, and handles any errors by logging.
    pub fn deserializer_parse(&self, frame: Bytes) -> Result<(SmallVec<[Event; 1]>, usize), Error> {
        let byte_size = frame.len();

        // Parse structured events from the byte frame.
        self.deserializer
            .parse(frame, self.log_namespace)
            .map(|events| (events, byte_size))
            .map_err(|error| {
                emit!(DecoderDeserializeError { error: &error });
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

#[cfg(test)]
mod tests {
    use super::Decoder;
    use bytes::Bytes;
    use futures::{stream, StreamExt};
    use tokio_util::{codec::FramedRead, io::StreamReader};
    use vector_lib::codecs::{
        decoding::{Deserializer, Framer},
        JsonDeserializer, NewlineDelimitedDecoder, StreamDecodingError,
    };
    use vrl::value::Value;

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
            Deserializer::Json(JsonDeserializer::default()),
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
