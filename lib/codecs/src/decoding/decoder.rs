use bytes::{Bytes, BytesMut};
use smallvec::SmallVec;
use vector_common::internal_event::emit;
use vector_core::{config::LogNamespace, event::{Event, Secrets}};

use crate::{
    decoding::format::Deserializer as _,
    decoding::{
        BoxedFramingError, BytesDeserializer, Deserializer, Error, Framer, NewlineDelimitedDecoder,
    },
    internal_events::{DecoderDeserializeError, DecoderFramingError},
};

type DecodedFrame = (SmallVec<[Event; 1]>, usize);

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
    /// Per-decode-call secrets template. When set, the secrets are forwarded to
    /// [`Deserializer::parse_with_secrets`] so user-authored programs (e.g. VRL)
    /// that run inside the deserializer can read them via `%vector.secrets.*`.
    secrets_template: Option<Secrets>,
}

impl Default for Decoder {
    fn default() -> Self {
        Self {
            framer: Framer::NewlineDelimited(NewlineDelimitedDecoder::new()),
            deserializer: Deserializer::Bytes(BytesDeserializer),
            log_namespace: LogNamespace::Legacy,
            secrets_template: None,
        }
    }
}

impl Decoder {
    /// Creates a new `Decoder` with the specified `Framer` to produce byte
    /// frames from the byte stream / byte messages and `Deserializer` to parse
    /// structured events from a byte frame.
    pub fn new(framer: Framer, deserializer: Deserializer) -> Self {
        Self {
            framer,
            deserializer,
            log_namespace: LogNamespace::Legacy,
            secrets_template: None,
        }
    }

    /// Sets the log namespace that will be used when decoding.
    pub fn with_log_namespace(mut self, log_namespace: LogNamespace) -> Self {
        self.log_namespace = log_namespace;
        self
    }

    /// Attaches a per-decode-call secrets template. When set, the secrets are
    /// forwarded to [`Deserializer::parse_with_secrets`] so that deserializers
    /// like the VRL codec can expose them to user-authored programs via
    /// `%vector.secrets.*` during decoding.
    ///
    /// For the VRL deserializer, secrets are injected into the synthetic event
    /// *before* the VRL program executes. For all other deserializers they are
    /// merged into each emitted event's secret store after parsing, filling gaps
    /// without overwriting anything the codec set itself.
    pub fn with_secrets_template(mut self, secrets: Secrets) -> Self {
        self.secrets_template = Some(secrets);
        self
    }

    /// Handles the framing result and parses it into a structured event, if
    /// possible.
    ///
    /// Emits logs if either framing or parsing failed.
    fn handle_framing_result(
        &mut self,
        frame: Result<Option<Bytes>, BoxedFramingError>,
    ) -> Result<Option<DecodedFrame>, Error> {
        let frame = frame.map_err(|error| {
            emit(DecoderFramingError { error: &error });
            Error::FramingError(error)
        })?;

        frame
            .map(|frame| self.deserializer_parse(frame))
            .transpose()
    }

    /// Parses a frame using the included deserializer, and handles any errors by logging.
    pub fn deserializer_parse(&self, frame: Bytes) -> Result<DecodedFrame, Error> {
        let byte_size = frame.len();

        // Parse structured events from the byte frame, forwarding any secrets
        // template so that deserializers that run user programs (e.g. VRL) can
        // make them available during execution.
        let result = match &self.secrets_template {
            Some(secrets) => self
                .deserializer
                .parse_with_secrets(frame, self.log_namespace, secrets),
            None => self.deserializer.parse(frame, self.log_namespace),
        };

        result
            .map(|events| (events, byte_size))
            .map_err(|error| {
                emit(DecoderDeserializeError { error: &error });
                Error::ParsingError(error)
            })
    }
}

impl tokio_util::codec::Decoder for Decoder {
    type Item = DecodedFrame;
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
    use bytes::Bytes;
    use futures::{StreamExt, stream};
    use tokio_util::io::StreamReader;
    use vrl::value::Value;

    use super::Decoder;
    use crate::{
        DecoderFramedRead, JsonDeserializer, NewlineDelimitedDecoder, StreamDecodingError,
        decoding::{Deserializer, Framer},
    };

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
        let mut stream = DecoderFramedRead::new(reader, decoder);

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
