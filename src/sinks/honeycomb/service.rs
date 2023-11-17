//! Service implementation for the `honeycomb` sink.

use bytes::Bytes;
use http::{Request, Uri};
use vector_lib::sensitive_string::SensitiveString;

use crate::sinks::util::http::HttpServiceRequestBuilder;

use super::config::HTTP_HEADER_HONEYCOMB;

#[derive(Debug, Clone)]
pub(super) struct HoneycombSvcRequestBuilder {
    pub(super) uri: Uri,
    pub(super) api_key: SensitiveString,
}

impl HttpServiceRequestBuilder for HoneycombSvcRequestBuilder {
    fn build(&self, body: Bytes) -> Request<Bytes> {
        let request = Request::post(&self.uri).header(HTTP_HEADER_HONEYCOMB, self.api_key.inner());

        request
            .body(body)
            .expect("Failed to assign body to request- builder has errors")
    }
}
