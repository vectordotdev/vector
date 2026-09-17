//! Service implementation for the `honeycomb` sink.
use bytes::Bytes;
use http_1::{HeaderValue, Request};
use snafu::ResultExt;

use super::config::HTTP_HEADER_HONEYCOMB;
use crate::sinks::{
    HTTPV1RequestBuilderSnafu,
    util::{
        HttpEndpoint, buffer::compression::Compression, http::HttpRequest,
        http_v1::HttpServiceRequestBuilder,
    },
};
#[derive(Clone, derive_more::Debug)]
pub(super) struct HoneycombSvcRequestBuilder {
    pub(super) uri: HttpEndpoint,
    // Omitted: `api_key` is sent as the `X-Honeycomb-Team` header on every
    // request.
    #[debug(skip)]
    pub(super) api_key: HeaderValue,
    pub(super) compression: Compression,
}

impl HttpServiceRequestBuilder<()> for HoneycombSvcRequestBuilder {
    fn build(&self, mut request: HttpRequest<()>) -> Result<Request<Bytes>, crate::Error> {
        let mut builder = Request::post(self.uri.clone().into_v1())
            .header(HTTP_HEADER_HONEYCOMB, self.api_key.clone());

        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header("Content-Encoding".to_string(), ce.to_string());
        }

        builder
            .body(request.take_payload())
            .context(HTTPV1RequestBuilderSnafu)
            .map_err(crate::Error::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_api_key() {
        let api_key = "secret-honeycomb-api-key";
        let builder = HoneycombSvcRequestBuilder {
            uri: HttpEndpoint::parse("https://api.honeycomb.io")
                .expect("static endpoint should be a valid http(s) URL"),
            api_key: HeaderValue::from_str(api_key).expect("api key should be a valid header"),
            compression: Compression::None,
        };

        let debug = format!("{builder:?}");
        assert!(
            !debug.contains(api_key),
            "Debug output must not leak the API key: {debug}"
        );
    }
}
