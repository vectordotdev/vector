//! Service implementation for the `gcp_stackdriver_logs` sink.

use bytes::Bytes;
use http::{header::CONTENT_TYPE, Request, Uri};

use crate::{
    gcp::GcpAuthenticator,
    sinks::{
        util::http::{HttpRequest, HttpServiceRequestBuilder},
        HTTPRequestBuilderSnafu,
    },
};
use snafu::ResultExt;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub(super) struct StackdriverLogsServiceRequestBuilder {
    pub(super) uri: Uri,
    pub(super) auth: GcpAuthenticator,
}

#[async_trait]
impl HttpServiceRequestBuilder<()> for StackdriverLogsServiceRequestBuilder {
    async fn build(&self, mut request: HttpRequest<()>) -> Result<Request<Bytes>, crate::Error> {
        let builder = Request::post(self.uri.clone()).header(CONTENT_TYPE, "application/json");

        let mut request = builder
            .body(request.take_payload())
            .context(HTTPRequestBuilderSnafu)
            .map_err(Into::<crate::Error>::into)?;

        self.auth.apply(&mut request);

        Ok(request)
    }
}
