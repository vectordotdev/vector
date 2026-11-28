//! Service implementation for the `gcp_stackdriver_logs` sink.

use bytes::Bytes;
use http::{Request, header::CONTENT_TYPE};
use snafu::ResultExt;

use crate::{
    gcp::GcpAuthenticator,
    sinks::{
        HTTPRequestBuilderSnafu,
        util::{
            HttpEndpoint,
            http::{HttpRequest, HttpServiceRequestBuilder},
        },
    },
};

#[derive(Debug, Clone)]
pub(super) struct StackdriverLogsServiceRequestBuilder {
    pub(super) endpoint: HttpEndpoint,
    pub(super) auth: GcpAuthenticator,
}

impl HttpServiceRequestBuilder<()> for StackdriverLogsServiceRequestBuilder {
    fn build(&self, mut request: HttpRequest<()>) -> Result<Request<Bytes>, crate::Error> {
        let builder =
            Request::post(self.endpoint.as_uri().clone()).header(CONTENT_TYPE, "application/json");

        let mut request = builder
            .body(request.take_payload())
            .context(HTTPRequestBuilderSnafu)
            .map_err(crate::Error::from)?;

        self.auth.apply(&mut request);

        Ok(request)
    }
}
