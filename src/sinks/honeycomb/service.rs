//! Service implementation for the `honeycomb` sink.
#![expect(
    clippy::let_underscore_must_use,
    reason = "derivative's Debug derive with ignored fields expands to a must_use let binding"
)]

use bytes::Bytes;
use derivative::Derivative;
use http::{HeaderValue, Request};
use snafu::ResultExt;

use super::config::HTTP_HEADER_HONEYCOMB;
use crate::sinks::{
    HTTPRequestBuilderSnafu,
    util::{
        HttpEndpoint,
        buffer::compression::Compression,
        http::{HttpRequest, HttpServiceRequestBuilder},
    },
};

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub(super) struct HoneycombSvcRequestBuilder {
    pub(super) uri: HttpEndpoint,
    // Omitted: `api_key` is sent as the `X-Honeycomb-Team` header on every
    // request.
    #[derivative(Debug = "ignore")]
    pub(super) api_key: HeaderValue,
    pub(super) compression: Compression,
}

impl HttpServiceRequestBuilder<()> for HoneycombSvcRequestBuilder {
    fn build(&self, mut request: HttpRequest<()>) -> Result<Request<Bytes>, crate::Error> {
        let mut builder =
            Request::post(self.uri.as_uri()).header(HTTP_HEADER_HONEYCOMB, self.api_key.clone());

        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header("Content-Encoding".to_string(), ce.to_string());
        }

        builder
            .body(request.take_payload())
            .context(HTTPRequestBuilderSnafu)
            .map_err(Into::into)
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
