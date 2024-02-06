//! Service implementation for the `gcp_stackdriver_logs` sink.

use bytes::Bytes;
use http::{Request, Uri};

use crate::{
    gcp::GcpAuthenticator,
    sinks::util::http::{HttpRequest, HttpServiceRequestBuilder},
};

#[derive(Debug, Clone)]
pub(super) struct StackdriverLogsServiceRequestBuilder {
    pub(super) uri: Uri,
    pub(super) auth: GcpAuthenticator,
}

impl HttpServiceRequestBuilder<()> for StackdriverLogsServiceRequestBuilder {
    fn build(&self, request: HttpRequest<()>) -> Request<Bytes> {
        let mut builder = Request::post(self.uri.clone())
            .header("Content-Type", "application/json")
            .body(request.get_payload().clone())
            .unwrap();

        self.auth.apply(&mut builder);

        builder
    }
}
