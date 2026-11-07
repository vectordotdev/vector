use std::{io, sync::Arc};

use bytes::Bytes;
use vector_lib::{
    codecs::JsonSerializerConfig, lookup::lookup_v2::ConfigValuePath,
    request_metadata::RequestMetadata,
};

use crate::{
    codecs::{Encoder, TimestampFormat, Transformer},
    event::{Event, EventFinalizers, Finalizable},
    sinks::util::{
        Compression, RequestBuilder, http::HttpRequest, metadata::RequestMetadataBuilder,
        request_builder::EncodeResult,
    },
};

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
    #[must_use]
    pub fn new() -> DatadogEventsRequestBuilder {
        DatadogEventsRequestBuilder { encoder: encoder() }
    }
}

impl RequestBuilder<Event> for DatadogEventsRequestBuilder {
    type Metadata = Metadata;
    type Events = Event;
    type Encoder = (Transformer, Encoder<()>);
    type Payload = Bytes;
    type Request = HttpRequest<Option<Arc<str>>>;
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
        HttpRequest::new(
            payload.into_payload(),
            metadata.finalizers,
            request_metadata,
            metadata.api_key,
        )
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
