//! Service implementation for the `gcp_stackdriver_logs` sink.

use bytes::Bytes;
use http::{Request, Uri};

use crate::{gcp::GcpAuthenticator, sinks::util::http::HttpServiceRequestBuilder};

#[derive(Debug, Clone)]
pub(super) struct StackdriverLogsServiceRequestBuilder {
    pub(super) uri: Uri,
    pub(super) auth: GcpAuthenticator,
}

impl HttpServiceRequestBuilder for StackdriverLogsServiceRequestBuilder {
    fn build(&self, body: Bytes) -> Request<Bytes> {
        // TODO: https://github.com/vectordotdev/vector/issues/18682
        #[allow(clippy::unwrap_used)]
        // TODO: https://github.com/vectordotdev/vector/issues/18682
        #[allow(clippy::unwrap_used)]
        let mut request = Request::post(self.uri.clone())
            .header("Content-Type", "application/json")
            .body(body)
            .unwrap();

        self.auth.apply(&mut request);

        request
    }
}
