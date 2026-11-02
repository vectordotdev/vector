//! Service implementation for the `keep` sink.

use bytes::Bytes;
use http_1::Request;
use snafu::ResultExt;
use vector_lib::sensitive_string::SensitiveString;

use super::config::HTTP_HEADER_KEEP_API_KEY;
use crate::sinks::{
    HTTPV1RequestBuilderSnafu,
    util::{HttpEndpoint, http::HttpRequest, http_v1::HttpServiceRequestBuilder},
};

#[derive(Debug, Clone)]
pub(super) struct KeepSvcRequestBuilder {
    pub(super) endpoint: HttpEndpoint,
    pub(super) api_key: SensitiveString,
}

impl HttpServiceRequestBuilder<()> for KeepSvcRequestBuilder {
    fn build(&self, mut request: HttpRequest<()>) -> Result<Request<Bytes>, crate::Error> {
        let builder = Request::post(self.endpoint.clone().into_v1())
            .header(HTTP_HEADER_KEEP_API_KEY, self.api_key.inner());

        let builder = builder.header("Content-Type".to_string(), "application/json".to_string());

        builder
            .body(request.take_payload())
            .context(HTTPV1RequestBuilderSnafu)
            .map_err(crate::Error::from)
    }
}
