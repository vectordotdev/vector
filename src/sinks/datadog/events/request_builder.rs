use std::{io, sync::Arc};

use bytes::Bytes;
use vector_lib::codecs::JsonSerializerConfig;
use vector_lib::lookup::lookup_v2::ConfigValuePath;
use vector_lib::request_metadata::{MetaDescriptive, RequestMetadata};
use vector_lib::ByteSizeOf;

use crate::{
    codecs::{Encoder, TimestampFormat, Transformer},
    event::{Event, EventFinalizers, Finalizable},
    sinks::util::{
        metadata::RequestMetadataBuilder, request_builder::EncodeResult, Compression, ElementCount,
        RequestBuilder,
    },
};

#[derive(Clone)]
pub struct DatadogEventsRequest {
    pub body: Bytes,
    pub metadata: Metadata,
    request_metadata: RequestMetadata,
}

impl Finalizable for DatadogEventsRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.metadata.finalizers)
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

impl MetaDescriptive for DatadogEventsRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.request_metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.request_metadata
    }
}

#[derive(Clone)]
pub struct Metadata {
    pub finalizers: EventFinalizers,
    pub api_key: Option<Arc<str>>,
}

pub struct DatadogEventsRequestBuilder {
    encoder: (Transformer, Encoder<()>),
}

impl Default for DatadogEventsRequestBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DatadogEventsRequestBuilder {
    pub fn new() -> DatadogEventsRequestBuilder {
        DatadogEventsRequestBuilder { encoder: encoder() }
    }
}

impl RequestBuilder<Event> for DatadogEventsRequestBuilder {
    type Metadata = Metadata;
    type Events = Event;
    type Encoder = (Transformer, Encoder<()>);
    type Payload = Bytes;
    type Request = DatadogEventsRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(&self, event: Event) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let builder = RequestMetadataBuilder::from_event(&event);

        let mut log = event.into_log();
        let metadata = Metadata {
            finalizers: log.take_finalizers(),
            api_key: log.metadata_mut().datadog_api_key(),
        };

        (metadata, builder, Event::from(log))
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        DatadogEventsRequest {
            body: payload.into_payload(),
            metadata,
            request_metadata,
        }
    }
}

fn encoder() -> (Transformer, Encoder<()>) {
    // DataDog Event API allows only some fields, and refuses
    // to accept event if it contains any other field.
    let only_fields = Some(
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
        .map(|field| ConfigValuePath::try_from((*field).to_string()).unwrap())
        .collect(),
    );
    // DataDog Event API requires unix timestamp.
    let timestamp_format = Some(TimestampFormat::Unix);

    (
        Transformer::new(only_fields, None, timestamp_format)
            .expect("transformer configuration must be valid"),
        Encoder::<()>::new(JsonSerializerConfig::default().build().into()),
    )
}
