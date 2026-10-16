use std::io;

use bytes::Bytes;

use super::{PartitionKey, encoder::VmEncoder, sink::VmMetric};
use crate::sinks::prelude::*;

#[derive(Clone)]
pub(super) struct VmRequest {
    pub(super) body: Bytes,
    pub(super) key: PartitionKey,
    finalizers: EventFinalizers,
    metadata: RequestMetadata,
}

impl Finalizable for VmRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for VmRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

/// Builds requests from batches. The encoder compresses the body itself, so the request builder
/// does not compress.
pub(super) struct VmRequestBuilder {
    pub(super) encoder: VmEncoder,
}

impl RequestBuilder<(PartitionKey, Vec<VmMetric>)> for VmRequestBuilder {
    type Metadata = (PartitionKey, EventFinalizers);
    type Events = Vec<VmMetric>;
    type Encoder = VmEncoder;
    type Payload = Bytes;
    type Request = VmRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: (PartitionKey, Vec<VmMetric>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (key, mut events) = input;
        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);
        ((key, finalizers), builder, events)
    }

    fn build_request(
        &self,
        (key, finalizers): Self::Metadata,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        VmRequest {
            body: payload.into_payload(),
            key,
            finalizers,
            metadata,
        }
    }
}
