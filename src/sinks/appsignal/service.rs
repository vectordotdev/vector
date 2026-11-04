//! Service implementation for the `appsignal` sink.
use bytes::Bytes;
use http::{Request, header::AUTHORIZATION};
use snafu::ResultExt;
use vector_lib::sensitive_string::SensitiveString;

use crate::sinks::{
    HTTPV1RequestBuilderSnafu,
    util::{
        HttpEndpoint, buffer::compression::Compression, http::HttpRequest,
        http_v1::HttpServiceRequestBuilder,
    },
};

#[derive(Clone, derive_more::Debug)]
pub(super) struct AppsignalSvcRequestBuilder {
    pub(super) endpoint: HttpEndpoint,
    // Omitted: `push_api_key` is sent as the `Authorization` bearer header on
    // every request.
    #[debug(skip)]
    pub(super) push_api_key: SensitiveString,
    pub(super) compression: Compression,
}

impl HttpServiceRequestBuilder<()> for AppsignalSvcRequestBuilder {
    fn build(&self, mut request: HttpRequest<()>) -> Result<Request<Bytes>, crate::Error> {
        let mut builder = Request::post(self.endpoint.as_uri().clone())
            .header("Content-Type", "application/json")
            .header(
                AUTHORIZATION,
                format!("Bearer {}", self.push_api_key.inner()),
            );

        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header("Content-Encoding", ce);
        }

        builder
            .body(request.take_payload())
            .context(HTTPV1RequestBuilderSnafu)
            .map_err(crate::Error::from)
    }
}
