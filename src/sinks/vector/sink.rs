use std::{fmt, num::NonZeroUsize};

use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use prost::Message;
use tower::Service;
use vector_lib::{
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
    config::telemetry,
    event::event_exceeds_max_nesting_cost,
    internal_event::{ComponentEventsDropped, UNINTENTIONAL},
    request_metadata::GroupedCountByteSize,
    stream::{BatcherSettings, DriverResponse, batcher::data::BatchReduce},
};

use super::service::VectorRequest;
use crate::{
    event::{Event, EventFinalizers, Finalizable, proto::EventWrapper},
    proto::vector as proto_vector,
    sinks::util::{SinkBuilderExt, StreamSink, metadata::RequestMetadataBuilder},
};

/// Data for a single event.
struct EventData {
    byte_size: usize,
    json_byte_size: GroupedCountByteSize,
    finalizers: EventFinalizers,
    wrapper: EventWrapper,
}

/// Temporary struct to collect events during batching.
#[derive(Clone)]
struct EventCollection {
    pub finalizers: EventFinalizers,
    pub events: Vec<EventWrapper>,
    pub events_byte_size: usize,
    pub events_json_byte_size: GroupedCountByteSize,
}

impl Default for EventCollection {
    fn default() -> Self {
        Self {
            finalizers: Default::default(),
            events: Default::default(),
            events_byte_size: Default::default(),
            events_json_byte_size: telemetry().create_request_count_byte_size(),
        }
    }
}

pub struct VectorSink<S> {
    pub batch_settings: BatcherSettings,
    pub service: S,
}

impl<S> VectorSink<S>
where
    S: Service<VectorRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            .filter_map(|event| {
                std::future::ready(
                    if let Some((cost, budget)) = event_exceeds_max_nesting_cost(&event) {
                        let reason = format!(
                            "Event nesting cost {cost} exceeds protobuf budget of {budget}."
                        );
                        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                            count: 1,
                            reason: &reason,
                        });
                        match event {
                            Event::Log(log) => log
                                .metadata()
                                .update_status(vector_lib::event::EventStatus::Rejected),
                            Event::Metric(metric) => metric
                                .metadata()
                                .update_status(vector_lib::event::EventStatus::Rejected),
                            Event::Trace(trace) => trace
                                .metadata()
                                .update_status(vector_lib::event::EventStatus::Rejected),
                        }

                        None
                    } else {
                        Some(event)
                    },
                )
            })
            .map(|mut event| {
                let mut byte_size = telemetry().create_request_count_byte_size();
                byte_size.add_event(&event, event.estimated_json_encoded_size_of());

                EventData {
                    byte_size: event.size_of(),
                    json_byte_size: byte_size,
                    finalizers: event.take_finalizers(),
                    wrapper: EventWrapper::from(event),
                }
            })
            .batched(self.batch_settings.as_reducer_config(
                |data: &EventData| data.wrapper.encoded_len(),
                BatchReduce::new(|event_collection: &mut EventCollection, item: EventData| {
                    event_collection.finalizers.merge(item.finalizers);
                    event_collection.events.push(item.wrapper);
                    event_collection.events_byte_size += item.byte_size;
                    event_collection.events_json_byte_size += item.json_byte_size;
                }),
            ))
            .map(|event_collection| {
                let builder = RequestMetadataBuilder::new(
                    event_collection.events.len(),
                    event_collection.events_byte_size,
                    event_collection.events_json_byte_size,
                );

                let encoded_events = proto_vector::PushEventsRequest {
                    events: event_collection.events,
                };

                let byte_size = encoded_events.encoded_len();
                let bytes_len =
                    NonZeroUsize::new(byte_size).expect("payload should never be zero length");

                VectorRequest {
                    finalizers: event_collection.finalizers,
                    metadata: builder.with_request_size(bytes_len),
                    request: encoded_events,
                }
            })
            .into_driver(self.service)
            .run()
            .await
    }
}

#[async_trait]
impl<S> StreamSink<Event> for VectorSink<S>
where
    S: Service<VectorRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use prost::Message;
    use vector_lib::event::{
        Event, LogEvent, MAX_METADATA_VALUE_NESTING_FRAMES, MAX_VALUE_NESTING_FRAMES, ObjectMap,
        Value, event_exceeds_max_nesting_cost,
    };

    use super::EventWrapper;
    use crate::proto::vector as proto_vector;

    fn build_nested_value(wrapping_levels: usize) -> Value {
        let mut v = Value::from("leaf");
        for _ in 0..wrapping_levels {
            let mut m = ObjectMap::new();
            m.insert("nested".into(), v);
            v = Value::Object(m);
        }
        v
    }

    /// Empirical check: an event sitting *exactly* at the value budget accepted by
    /// `event_exceeds_max_nesting_cost` must roundtrip through the vector sink's
    /// actual wire shape — `PushEventsRequest { events: [EventWrapper] }` — and
    /// not fail decode at the receiver. If this test fails, the value budget is
    /// too high for the gRPC path and needs to be reduced for the outer request
    /// wrapper.
    #[test]
    fn push_events_request_decode_at_value_budget() {
        // 32 nested objects under "data" key → 33 effective object levels in
        // `log.value()` (one outer Object from the inserted key), cost = 99 =
        // MAX_VALUE_NESTING_FRAMES.
        let mut log = LogEvent::default();
        log.insert("data", build_nested_value(32));
        let event = Event::Log(log);
        assert!(
            event_exceeds_max_nesting_cost(&event).is_none(),
            "test setup invariant: event must sit exactly at the value budget \
             (cost {MAX_VALUE_NESTING_FRAMES})",
        );

        let request = proto_vector::PushEventsRequest {
            events: vec![EventWrapper::from(event)],
        };

        let mut buf = BytesMut::with_capacity(65536);
        request.encode(&mut buf).expect("encode should succeed");

        proto_vector::PushEventsRequest::decode(buf.freeze())
            .expect("PushEventsRequest decode should succeed at the accepted value budget");
    }

    /// Boundary check: one step past the value budget must fail decode through
    /// the gRPC wire shape. Together with the at-budget test above this pins
    /// `MAX_VALUE_NESTING_FRAMES` as the tight boundary for the vector-sink
    /// path, identical to the disk-buffer / native-codec `EventArray` path.
    #[test]
    fn push_events_request_decode_one_past_value_budget_fails() {
        // 33 nested objects under "data" → 34 effective object levels, cost 102.
        let mut log = LogEvent::default();
        log.insert("data", build_nested_value(33));
        let event = Event::Log(log);

        let request = proto_vector::PushEventsRequest {
            events: vec![EventWrapper::from(event)],
        };

        let mut buf = BytesMut::with_capacity(65536);
        request.encode(&mut buf).expect("encode should succeed");

        assert!(
            proto_vector::PushEventsRequest::decode(buf.freeze()).is_err(),
            "PushEventsRequest decode must fail one step past the value budget; \
             if this changes, the gate is no longer tight",
        );
    }

    #[test]
    fn push_events_request_decode_at_metadata_budget() {
        let mut log = LogEvent::from("flat");
        *log.metadata_mut().value_mut() = build_nested_value(32);
        let event = Event::Log(log);
        assert!(
            event_exceeds_max_nesting_cost(&event).is_none(),
            "test setup invariant: metadata must sit exactly at the metadata \
             budget (cost {MAX_METADATA_VALUE_NESTING_FRAMES})",
        );

        let request = proto_vector::PushEventsRequest {
            events: vec![EventWrapper::from(event)],
        };

        let mut buf = BytesMut::with_capacity(65536);
        request.encode(&mut buf).expect("encode should succeed");

        proto_vector::PushEventsRequest::decode(buf.freeze())
            .expect("PushEventsRequest decode should succeed at the accepted metadata budget");
    }
}
