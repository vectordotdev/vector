//! Service implementation for the `keep` sink.

use bytes::Bytes;
use http::Request;
use snafu::ResultExt;
use vector_lib::sensitive_string::SensitiveString;

use super::config::HTTP_HEADER_KEEP_API_KEY;
use crate::sinks::{
    HTTPRequestBuilderSnafu,
    util::{
        HttpEndpoint,
        http::{HttpRequest, HttpServiceRequestBuilder},
    },
};

#[derive(Debug, Clone)]
pub(super) struct KeepSvcRequestBuilder {
    pub(super) endpoint: HttpEndpoint,
    pub(super) api_key: SensitiveString,
}

impl HttpServiceRequestBuilder<()> for KeepSvcRequestBuilder {
    fn build(&self, mut request: HttpRequest<()>) -> Result<Request<Bytes>, crate::Error> {
        let builder = Request::post(self.endpoint.as_uri().clone())
            .header(HTTP_HEADER_KEEP_API_KEY, self.api_key.inner());

        let builder = builder.header("Content-Type".to_string(), "application/json".to_string());

        builder
            .body(request.take_payload())
            .context(HTTPRequestBuilderSnafu)
            .map_err(crate::Error::from)
    }
}
