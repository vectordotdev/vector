use bytes::Bytes;
use vector_lib::lookup::lookup_v2::OptionalTargetPath;

use crate::sinks::{
    azure_event_hubs::service::{AzureEventHubsRequest, AzureEventHubsRequestMetadata},
    prelude::*,
};

pub struct AzureEventHubsRequestBuilder {
    pub encoder: (Transformer, Encoder<()>),
    pub partition_id_field: Option<OptionalTargetPath>,
}

impl RequestBuilder<Event> for AzureEventHubsRequestBuilder {
    type Metadata = AzureEventHubsRequestMetadata;
    type Events = Event;
    type Encoder = (Transformer, Encoder<()>);
    type Payload = Bytes;
    type Request = AzureEventHubsRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        mut input: Event,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let builder = RequestMetadataBuilder::from_event(&input);

        let partition_id = self
            .partition_id_field
            .as_ref()
            .and_then(|field| field.path.as_ref())
            .and_then(|path| input.as_log().get(path))
            .map(|v| v.to_string_lossy().into_owned());

        let metadata = AzureEventHubsRequestMetadata {
            finalizers: input.take_finalizers(),
            partition_id,
        };

        (metadata, builder, input)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let partition_id = metadata.partition_id.clone();
        AzureEventHubsRequest {
            body: payload.into_payload(),
            partition_id,
            metadata,
            request_metadata,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, LogEvent};

    fn make_builder() -> AzureEventHubsRequestBuilder {
        let transformer = Transformer::default();
        let serializer: vector_lib::codecs::encoding::Serializer =
            vector_lib::codecs::encoding::format::JsonSerializerConfig::default()
                .build()
                .into();
        let encoder = Encoder::<()>::new(serializer);
        AzureEventHubsRequestBuilder {
            encoder: (transformer, encoder),
            partition_id_field: None,
        }
    }

    #[test]
    fn split_input_extracts_finalizers() {
        let builder = make_builder();
        let event = Event::Log(LogEvent::from("hello world"));
        let (metadata, _request_metadata_builder, _events) = builder.split_input(event);
        drop(metadata.finalizers);
    }

    #[test]
    fn build_request_produces_correct_body() {
        use vector_lib::config::telemetry;

        let builder = make_builder();
        let event = Event::Log(LogEvent::from("test message"));
        let (metadata, request_metadata_builder, _events) = builder.split_input(event);

        let payload_bytes = Bytes::from(r#"{"message":"test message"}"#);
        let byte_size = telemetry().create_request_count_byte_size();
        let payload = EncodeResult::uncompressed(payload_bytes.clone(), byte_size);
        let request_metadata = request_metadata_builder.build(&payload);
        let request = builder.build_request(metadata, request_metadata, payload);

        assert_eq!(request.body, payload_bytes);
    }

    #[test]
    fn compression_is_none() {
        let builder = make_builder();
        assert!(matches!(builder.compression(), Compression::None));
    }
}
