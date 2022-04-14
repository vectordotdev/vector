use crate::{
    event::Event,
    internal_events::{DecoderDeserializeFailed, DecoderFramingFailed},
};
use bytes::{Bytes, BytesMut};
use codecs::decoding::{
    format::Deserializer as _, BoxedFramingError, BytesDeserializer, Deserializer,
    DeserializerConfig, Error, Framer, FramingConfig, NewlineDelimitedDecoder,
};
use smallvec::SmallVec;
use vector_core::config::LogNamespace;

/// A decoder that can decode structured events from a byte stream / byte
/// messages.
#[derive(Debug, Clone)]
pub struct Decoder {
    framer: Framer,
    deserializer: Deserializer,
    log_namespace: LogNamespace,
}

impl Default for Decoder {
    fn default() -> Self {
        Self {
            framer: Framer::NewlineDelimited(NewlineDelimitedDecoder::new()),
            deserializer: Deserializer::Bytes(BytesDeserializer::new()),
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

    /// Sets the path prefix where all deserialized data will be placed
    pub fn with_log_namespace(mut self, log_namespace: LogNamespace) -> Self {
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
            .map(|mut events| {
                self.add_body_namespace(&mut events);
                Some((events, byte_size))
            })
            .map_err(|error| {
                emit!(DecoderDeserializeFailed { error: &error });
                Error::ParsingError(error)
            })
    }

    fn add_body_namespace(&self, events: &mut SmallVec<[Event; 1]>) {
        for event in events {
            match event {
                Event::Log(log) => {
                    self.log_namespace.add_body_namespace(log);
                }
                _ => { /* only logs support namespacing */ }
            }
        }
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
#[derive(Debug, Clone)]
pub struct DecodingConfig {
    /// The framing config.
    framing: FramingConfig,
    /// The decoding config.
    decoding: DeserializerConfig,
    /// The path prefix used for all deserialized data
    log_namespace: LogNamespace,
}

impl DecodingConfig {
    /// Creates a new `DecodingConfig` with the provided `FramingConfig` and
    /// `DeserializerConfig`.
    pub const fn new(framing: FramingConfig, decoding: DeserializerConfig) -> Self {
        Self {
            framing,
            decoding,
            // TODO: Make this a parameter once all sources support overriding the log namespace
            log_namespace: LogNamespace::Legacy,
        }
    }

    /// Sets the path prefix where all deserialized data will be placed
    pub fn with_log_namespace(mut self, log_namespace: LogNamespace) -> Self {
        self.log_namespace = log_namespace;
        self
    }

    /// Builds a `Decoder` from the provided configuration.
    pub fn build(self) -> Decoder {
        let framer = self.framing.build();
        let deserializer = self.decoding.build();

        Decoder::new(framer, deserializer).with_log_namespace(self.log_namespace)
    }
}
