use bytes::Bytes;

use crate::sinks::{
    azure_event_hubs::service::{AzureEventHubsRequest, AzureEventHubsRequestMetadata},
    prelude::*,
};

pub struct AzureEventHubsRequestBuilder {
    pub encoder: (Transformer, Encoder<()>),
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

        let metadata = AzureEventHubsRequestMetadata {
            finalizers: input.take_finalizers(),
        };

        (metadata, builder, input)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        AzureEventHubsRequest {
            body: payload.into_payload(),
            metadata,
            request_metadata,
        }
    }
}
