use async_trait::async_trait;
use bytes::BytesMut;
use futures::{StreamExt, stream::BoxStream};
use tokio::{io, io::AsyncWriteExt};
use tokio_util::codec::Encoder as _;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::encoding::Framer,
    internal_event::{
        ByteSize, BytesSent, CountByteSize, EventsSent, InternalEventHandle as _, Output, Protocol,
    },
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
            if self.encoder.encode(event, &mut bytes).is_err() {
                // Error is already logged + counted + emits `ComponentEventsDropped`
                // by the `Encoder` framework. Mark this event's finalizers Errored so
                // source-side acks reflect the drop, then continue with the next
                // event. Surfacing the error here as fatal would terminate the sink
                // on the first data-dependent encoder failure (e.g., a single event
                // exceeding the native codec's nesting budget) and silently drop
                // every subsequent valid event.
                finalizers.update_status(EventStatus::Errored);
                continue;
            }

            match self.output.write_all(&bytes).await {
                Err(error) => {
                    // Error when writing to stdout/stderr is likely irrecoverable,
                    // so stop the sink.
                    error!(message = "Error writing to output. Stopping sink.", %error, internal_log_rate_limit = false);
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
    use futures_util::stream::{self, StreamExt};
    use vector_lib::{
        codecs::{JsonSerializerConfig, NativeSerializerConfig, NewlineDelimitedEncoder},
        event::{BatchNotifier, BatchStatus, ObjectMap, Value},
        sink::VectorSink,
    };

    use super::*;
    use crate::{
        event::{Event, LogEvent},
        test_util::components::{SINK_TAGS, run_and_assert_sink_compliance},
    };

    #[tokio::test]
    async fn component_spec_compliance() {
        let event = Event::Log(LogEvent::from("foo"));

        let encoder = Encoder::<Framer>::new(
            NewlineDelimitedEncoder::default().into(),
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

    /// A per-event encoder failure (e.g., a single event exceeding the native codec's
    /// nesting budget) must not terminate the sink. The offending event is finalized
    /// as `Errored`, the next event still goes through and is `Delivered`.
    #[tokio::test]
    async fn per_event_encoder_failure_does_not_terminate_sink() {
        // Build a log whose value is an Object chain too deep for the native codec
        // (object cost 34 * 3 = 102 frames, over the 99-frame value budget).
        let mut deep_value = Value::from("x");
        for _ in 0..34 {
            let mut m = ObjectMap::new();
            m.insert("nested".into(), deep_value);
            deep_value = Value::Object(m);
        }

        let (bad_batch, mut bad_rx) = BatchNotifier::new_with_receiver();
        let mut bad_event = LogEvent::default().with_batch_notifier(&bad_batch);
        bad_event.insert("data", deep_value);
        drop(bad_batch);

        let (good_batch, mut good_rx) = BatchNotifier::new_with_receiver();
        let good_event = LogEvent::from("ok").with_batch_notifier(&good_batch);
        drop(good_batch);

        let encoder = Encoder::<Framer>::new(
            NewlineDelimitedEncoder::default().into(),
            NativeSerializerConfig.build().into(),
        );

        let sink = Box::new(WriterSink {
            output: Vec::new(),
            transformer: Default::default(),
            encoder,
        });

        let events = stream::iter(vec![Event::Log(bad_event), Event::Log(good_event)]).boxed();
        sink.run(events)
            .await
            .expect("sink should not return Err for a per-event encode failure");

        assert_eq!(
            bad_rx.try_recv(),
            Ok(BatchStatus::Errored),
            "the over-budget event must be finalized as Errored",
        );
        assert_eq!(
            good_rx.try_recv(),
            Ok(BatchStatus::Delivered),
            "the subsequent valid event must still be Delivered \
             (the sink must keep processing past a per-event encoder failure)",
        );
    }
}
