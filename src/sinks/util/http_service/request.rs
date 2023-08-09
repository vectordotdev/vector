use bytes::Bytes;

use vector_common::{
    finalization::{EventFinalizers, Finalizable},
    request_metadata::{MetaDescriptive, RequestMetadata},
};

/// Request type for use in `RequestBuilder` implementations of HTTP stream sinks.
#[derive(Clone)]
pub struct HttpRequest {
    pub payload: Bytes,
    pub finalizers: EventFinalizers,
    pub request_metadata: RequestMetadata,
}

impl HttpRequest {
    pub fn new(
        payload: Bytes,
        finalizers: EventFinalizers,
        request_metadata: RequestMetadata,
    ) -> Self {
        Self {
            payload,
            finalizers,
            request_metadata,
        }
    }
}

impl Finalizable for HttpRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

impl MetaDescriptive for HttpRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.request_metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.request_metadata
    }
}
