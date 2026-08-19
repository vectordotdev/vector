//! Service implementation for the `honeycomb` sink.

use bytes::Bytes;
use http_1::Request;
use vector_lib::sensitive_string::SensitiveString;

use super::config::HTTP_HEADER_HONEYCOMB;
use crate::sinks::util::{
    HttpEndpoint, buffer::compression::Compression, http::HttpRequest,
    http_v1::HttpServiceRequestBuilderV1,
};

#[derive(Debug, Clone)]
pub(super) struct HoneycombSvcRequestBuilder {
    pub(super) uri: HttpEndpoint,
    pub(super) api_key: SensitiveString,
    pub(super) compression: Compression,
}

impl HttpServiceRequestBuilderV1<()> for HoneycombSvcRequestBuilder {
    fn build(&self, mut request: HttpRequest<()>) -> Result<Request<Bytes>, crate::Error> {
        let mut builder =
            Request::post(self.uri.to_string()).header(HTTP_HEADER_HONEYCOMB, self.api_key.inner());

        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header("Content-Encoding".to_string(), ce.to_string());
        }

        builder.body(request.take_payload()).map_err(Into::into)
    }
}
