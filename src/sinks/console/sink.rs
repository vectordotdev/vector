use async_trait::async_trait;
use bytes::BytesMut;
use futures::{stream::BoxStream, StreamExt};
use tokio::{io, io::AsyncWriteExt};
use tokio_util::codec::Encoder as _;
use vector_lib::codecs::encoding::Framer;
use vector_lib::{
    internal_event::{
        ByteSize, BytesSent, CountByteSize, EventsSent, InternalEventHandle as _, Output, Protocol,
    },
    EstimatedJsonEncodedSizeOf,
};

use crate::{
    codecs::{Encoder, Transformer},
    event::{Event, EventStatus, Finalizable},
    sinks::util::StreamSink,
};

pub struct WriterSink<T> {
    pub output: T,
    pub transformer: Transformer,
    pub encoder: Encoder<Framer>,
}

#[async_trait]
impl<T> StreamSink<Event> for WriterSink<T>
where
    T: io::AsyncWrite + Send + Sync + Unpin,
{
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        let bytes_sent = register!(BytesSent::from(Protocol("console".into(),)));
        let events_sent = register!(EventsSent::from(Output(None)));
        while let Some(mut event) = input.next().await {
            let event_byte_size = event.estimated_json_encoded_size_of();
            self.transformer.transform(&mut event);

            let finalizers = event.take_finalizers();
            let mut bytes = BytesMut::new();
            self.encoder.encode(event, &mut bytes).map_err(|_| {
                // Error is handled by `Encoder`.
                finalizers.update_status(EventStatus::Errored);
            })?;

            match self.output.write_all(&bytes).await {
                Err(error) => {
                    // Error when writing to stdout/stderr is likely irrecoverable,
                    // so stop the sink.
                    error!(message = "Error writing to output. Stopping sink.", %error);
                    finalizers.update_status(EventStatus::Errored);
                    return Err(());
                }
                Ok(()) => {
                    finalizers.update_status(EventStatus::Delivered);

                    events_sent.emit(CountByteSize(1, event_byte_size));
                    bytes_sent.emit(ByteSize(bytes.len()));
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use futures::future::ready;
    use futures_util::stream;
    use vector_lib::codecs::{JsonSerializerConfig, NewlineDelimitedEncoder};
    use vector_lib::sink::VectorSink;

    use super::*;
    use crate::{
        event::{Event, LogEvent},
        test_util::components::{run_and_assert_sink_compliance, SINK_TAGS},
    };

    #[tokio::test]
    async fn component_spec_compliance() {
        let event = Event::Log(LogEvent::from("foo"));

        let encoder = Encoder::<Framer>::new(
            NewlineDelimitedEncoder::new().into(),
            JsonSerializerConfig::default().build().into(),
        );

        let sink = WriterSink {
            output: Vec::new(),
            transformer: Default::default(),
            encoder,
        };

        run_and_assert_sink_compliance(
            VectorSink::from_event_streamsink(sink),
            stream::once(ready(event)),
            &SINK_TAGS,
        )
        .await;
    }
}
