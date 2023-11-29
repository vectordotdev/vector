//! Service implementation for the `better_stack_logs` sink.

use bytes::Bytes;
use http::{Request, Uri};
use vector_lib::sensitive_string::SensitiveString;

use crate::sinks::util::http::HttpServiceRequestBuilder;

#[derive(Debug, Clone)]
pub(super) struct BetterStackLogsSvcRequestBuilder {
    pub(super) uri: Uri,
    pub(super) source_token: SensitiveString,
}

impl HttpServiceRequestBuilder for BetterStackLogsSvcRequestBuilder {
    fn build(&self, body: Bytes) -> Request<Bytes> {
        let request = Request::post(&self.uri)
            .header("Authorization", format!("Bearer {}", self.source_token.inner()))
            .header("Content-Type", "application/json")
            .header("Accept", "*/*")
            .header("Accept-Encoding", "gzip");

        request
            .body(body)
            .expect("Failed to assign body to request- builder has errors")
    }
}
