use std::{io, sync::Arc};

use vector_core::{buffers::Ackable, ByteSizeOf};

use crate::{
    event::{EventFinalizers, Finalizable, LogEvent, PathComponent},
    internal_events::DatadogEventsProcessed,
    sinks::util::{
        encoding::{EncodingConfigFixed, StandardJsonEncoding, TimestampFormat},
        Compression, ElementCount, RequestBuilder,
    },
};

#[derive(Clone)]
pub struct DatadogEventsRequest {
    pub body: Vec<u8>,
    pub metadata: Metadata,
}

impl Finalizable for DatadogEventsRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.metadata.finalizers)
    }
}

impl Ackable for DatadogEventsRequest {
    fn ack_size(&self) -> usize {
        self.element_count()
    }
}

impl ByteSizeOf for DatadogEventsRequest {
    fn allocated_bytes(&self) -> usize {
        self.body.allocated_bytes() + self.metadata.finalizers.allocated_bytes()
    }
}

impl ElementCount for DatadogEventsRequest {
    fn element_count(&self) -> usize {
        // Datadog Events api only accepts a single event per request
        1
    }
}

#[derive(Clone)]
pub struct Metadata {
    pub finalizers: EventFinalizers,
    pub api_key: Option<Arc<str>>,
    pub event_byte_size: usize,
}

#[derive(Default)]
pub struct DatadogEventsRequestBuilder {
    encoder: EncodingConfigFixed<StandardJsonEncoding>,
}

impl DatadogEventsRequestBuilder {
    pub fn new() -> DatadogEventsRequestBuilder {
        DatadogEventsRequestBuilder { encoder: encoder() }
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
        let metadata = Metadata {
            finalizers: log.take_finalizers(),
            api_key: log.metadata_mut().datadog_api_key().clone(),
            event_byte_size: log.size_of(),
        };
        (metadata, log)
    }

    fn build_request(&self, metadata: Self::Metadata, body: Self::Payload) -> Self::Request {
        // deprecated - kept for backwards compatibility
        emit!(&DatadogEventsProcessed {
            byte_size: body.len(),
        });

        DatadogEventsRequest { body, metadata }
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
