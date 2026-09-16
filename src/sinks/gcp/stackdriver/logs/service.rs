//! Service implementation for the `gcp_stackdriver_logs` sink.

use bytes::Bytes;
use http_1::{Request, header::CONTENT_TYPE};
use snafu::ResultExt;

use crate::{
    gcp::GcpAuthenticator,
    sinks::{
        HTTPV1RequestBuilderSnafu,
        util::{HttpEndpoint, http::HttpRequest, http_v1::HttpServiceRequestBuilder},
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
            Request::post(self.endpoint.clone().into_v1()).header(CONTENT_TYPE, "application/json");

        let mut request = builder
            .body(request.take_payload())
            .context(HTTPV1RequestBuilderSnafu)
            .map_err(crate::Error::from)?;

        self.auth.apply_v1(&mut request);

        Ok(request)
    }
}
