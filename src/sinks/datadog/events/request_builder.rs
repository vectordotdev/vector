use crate::sinks::util::{RequestBuilder, Compression};
use crate::event::{LogEvent, EventFinalizers, Finalizable};
use crate::sinks::util::encoding::{EncodingConfigFixed, StandardJsonEncoding, TimestampFormat};
use std::io;
use crate::internal_events::{DatadogEventsProcessed, DatadogEventsFieldInvalid};
use crate::event::PathComponent;

pub struct DatadogEventsRequest {
    body: Vec<u8>,
    finalizers: EventFinalizers
}

pub struct Metadata {
    finalizers: EventFinalizers
}

pub struct DatadogEventsRequestBuilder {
    encoder: EncodingConfigFixed<StandardJsonEncoding>
}

impl DatadogEventsRequestBuilder {
    pub fn new() -> DatadogEventsRequestBuilder {
        DatadogEventsRequestBuilder {
            encoder: encoder()
        }
    }
}

impl RequestBuilder<LogEvent> for DatadogEventsRequestBuilder {
    type Metadata = Metadata;
    type Events = LogEvent;
    type Encoder = EncodingConfigFixed<StandardJsonEncoding>;
    type Payload = Vec<u8>;
    type Request = DatadogEventsRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(&self, mut log: LogEvent) -> (Self::Metadata, Self::Events) {
        // let (fields, mut log_metadata) = log.into_parts();
        let x = log.metadata();
        let metadata = Metadata {
            finalizers: log_metadata.take_finalizers(),
        };
        (metadata, log)
    }

    fn build_request(&self, metadata: Self::Metadata, body: Self::Payload) -> Self::Request {
        // deprecated - kept for backwards compatibility
        emit!(&DatadogEventsProcessed {
            byte_size: body.len(),
        });

        DatadogEventsRequest {
            body,
            finalizers: metadata.finalizers
        }
    }
}

fn encoder() -> EncodingConfigFixed<StandardJsonEncoding> {
    EncodingConfigFixed {
        // DataDog Event API allows only some fields, and refuses
        // to accept event if it contains any other field.
        only_fields: Some(
            [
                "aggregation_key",
                "alert_type",
                "date_happened",
                "device_name",
                "host",
                "priority",
                "related_event_id",
                "source_type_name",
                "tags",
                "text",
                "title",
            ]
                .iter()
                .map(|field| vec![PathComponent::Key((*field).into())])
                .collect(),
        ),
        // DataDog Event API requires unix timestamp.
        timestamp_format: Some(TimestampFormat::Unix),
        codec: StandardJsonEncoding,
        ..EncodingConfigFixed::default()
    }
}
