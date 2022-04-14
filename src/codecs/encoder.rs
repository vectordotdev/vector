use crate::{
    event::Event,
    internal_events::{EncoderFramingFailed, EncoderSerializeFailed},
};
use bytes::BytesMut;
use codecs::{
    encoding::{Error, Framer, Serializer},
    NewlineDelimitedEncoder, RawMessageSerializer,
};
use tokio_util::codec::Encoder as _;

#[derive(Debug, Clone)]
/// An encoder that can encode structured events into byte frames.
pub struct Encoder<Framer>
where
    Framer: Clone,
{
    framer: Framer,
    serializer: Serializer,
}

impl Default for Encoder<Framer> {
    fn default() -> Self {
        Self {
            framer: Framer::NewlineDelimited(NewlineDelimitedEncoder::new()),
            serializer: Serializer::RawMessage(RawMessageSerializer::new()),
        }
    }
}

impl Default for Encoder<()> {
    fn default() -> Self {
        Self {
            framer: (),
            serializer: Serializer::RawMessage(RawMessageSerializer::new()),
        }
    }
}

impl<Framer> Encoder<Framer>
where
    Framer: Clone,
{
    // Serialize the event.
    fn serialize(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Error> {
        self.serializer.encode(event, buffer).map_err(|error| {
            emit!(EncoderSerializeFailed { error: &error });
            Error::SerializingError(error)
        })
    }
}

impl Encoder<Framer> {
    /// Creates a new `Encoder` with the specified `Serializer` to produce bytes
    /// from a structured event, and the `Framer` to wrap these into a byte
    /// frame.
    pub const fn new(framer: Framer, serializer: Serializer) -> Self {
        Self { framer, serializer }
    }

    /// Get the framer.
    pub const fn framer(&self) -> &Framer {
        &self.framer
    }

    /// Get the serializer.
    pub const fn serializer(&self) -> &Serializer {
        &self.serializer
    }
}

impl Encoder<()> {
    /// Creates a new `Encoder` with the specified `Serializer` to produce bytes
    /// from a structured event.
    pub const fn new(serializer: Serializer) -> Self {
        Self {
            framer: (),
            serializer,
        }
    }

    /// Get the serializer.
    pub const fn serializer(&self) -> &Serializer {
        &self.serializer
    }
}

impl tokio_util::codec::Encoder<Event> for Encoder<Framer> {
    type Error = Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let len = buffer.len();
        let mut payload = buffer.split_off(len);

        self.serialize(event, &mut payload)?;

        // Frame the serialized event.
        self.framer.encode((), &mut payload).map_err(|error| {
            emit!(EncoderFramingFailed { error: &error });
            Error::FramingError(error)
        })?;

        buffer.unsplit(payload);

        Ok(())
    }
}

impl tokio_util::codec::Encoder<Event> for Encoder<()> {
    type Error = Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let len = buffer.len();
        let mut payload = buffer.split_off(len);

        self.serialize(event, &mut payload)?;

        buffer.unsplit(payload);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BufMut;
    use codecs::{encoding::BoxedFramingError, RawMessageSerializer};
    use futures_util::{SinkExt, StreamExt};
    use tokio_util::codec::FramedWrite;

    #[derive(Debug, Clone)]
    struct ParenEncoder;

    impl ParenEncoder {
        pub const fn new() -> Self {
            Self
        }
    }

    impl tokio_util::codec::Encoder<()> for ParenEncoder {
        type Error = BoxedFramingError;

        fn encode(&mut self, _: (), dst: &mut BytesMut) -> Result<(), Self::Error> {
            dst.reserve(2);
            let inner = dst.split();
            dst.put_u8(b'(');
            dst.unsplit(inner);
            dst.put_u8(b')');
            Ok(())
        }
    }

    #[derive(Debug, Clone)]
    struct ErrorNthEncoder<T>(T, usize, usize)
    where
        T: tokio_util::codec::Encoder<(), Error = BoxedFramingError>;

    impl<T> ErrorNthEncoder<T>
    where
        T: tokio_util::codec::Encoder<(), Error = BoxedFramingError>,
    {
        pub fn new(encoder: T, n: usize) -> Self {
            Self(encoder, 0, n)
        }
    }

    impl<T> tokio_util::codec::Encoder<()> for ErrorNthEncoder<T>
    where
        T: tokio_util::codec::Encoder<(), Error = BoxedFramingError>,
    {
        type Error = BoxedFramingError;

        fn encode(&mut self, _: (), dst: &mut BytesMut) -> Result<(), Self::Error> {
            self.0.encode((), dst)?;
            let result = if self.1 == self.2 {
                Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "error")) as _)
            } else {
                Ok(())
            };
            self.1 += 1;
            result
        }
    }

    #[tokio::test]
    async fn test_encode_events_sink_empty() {
        let encoder = Encoder::<Framer>::new(
            Framer::Boxed(Box::new(ParenEncoder::new())),
            Serializer::RawMessage(RawMessageSerializer::new()),
        );
        let source = futures::stream::iter(vec![
            Event::from("foo"),
            Event::from("bar"),
            Event::from("baz"),
        ])
        .map(Ok);
        let sink = Vec::new();
        let mut framed = FramedWrite::new(sink, encoder);
        source.forward(&mut framed).await.unwrap();
        let sink = framed.into_inner();
        assert_eq!(sink, b"(foo)(bar)(baz)");
    }

    #[tokio::test]
    async fn test_encode_events_sink_non_empty() {
        let encoder = Encoder::<Framer>::new(
            Framer::Boxed(Box::new(ParenEncoder::new())),
            Serializer::RawMessage(RawMessageSerializer::new()),
        );
        let source = futures::stream::iter(vec![
            Event::from("bar"),
            Event::from("baz"),
            Event::from("bat"),
        ])
        .map(Ok);
        let sink = Vec::from("(foo)");
        let mut framed = FramedWrite::new(sink, encoder);
        source.forward(&mut framed).await.unwrap();
        let sink = framed.into_inner();
        assert_eq!(sink, b"(foo)(bar)(baz)(bat)");
    }

    #[tokio::test]
    async fn test_encode_events_sink_empty_handle_framing_error() {
        let encoder = Encoder::<Framer>::new(
            Framer::Boxed(Box::new(ErrorNthEncoder::new(ParenEncoder::new(), 1))),
            Serializer::RawMessage(RawMessageSerializer::new()),
        );
        let source = futures::stream::iter(vec![
            Event::from("foo"),
            Event::from("bar"),
            Event::from("baz"),
        ])
        .map(Ok);
        let sink = Vec::new();
        let mut framed = FramedWrite::new(sink, encoder);
        assert!(source.forward(&mut framed).await.is_err());
        framed.flush().await.unwrap();
        let sink = framed.into_inner();
        assert_eq!(sink, b"(foo)");
    }

    #[tokio::test]
    async fn test_encode_events_sink_non_empty_handle_framing_error() {
        let encoder = Encoder::<Framer>::new(
            Framer::Boxed(Box::new(ErrorNthEncoder::new(ParenEncoder::new(), 1))),
            Serializer::RawMessage(RawMessageSerializer::new()),
        );
        let source = futures::stream::iter(vec![
            Event::from("bar"),
            Event::from("baz"),
            Event::from("bat"),
        ])
        .map(Ok);
        let sink = Vec::from("(foo)");
        let mut framed = FramedWrite::new(sink, encoder);
        assert!(source.forward(&mut framed).await.is_err());
        framed.flush().await.unwrap();
        let sink = framed.into_inner();
        assert_eq!(sink, b"(foo)(bar)");
    }
}
