//! Service implementation for the `honeycomb` sink.

use bytes::Bytes;
use http::{Request, Uri};
use vector_lib::sensitive_string::SensitiveString;

use crate::sinks::{
    util::buffer::compression::Compression,
    util::http::{HttpRequest, HttpServiceRequestBuilder},
    HTTPRequestBuilderSnafu,
};
use snafu::ResultExt;

use super::config::HTTP_HEADER_HONEYCOMB;

#[derive(Debug, Clone)]
pub(super) struct HoneycombSvcRequestBuilder {
    pub(super) uri: Uri,
    pub(super) api_key: SensitiveString,
    pub(super) compression: Compression,
}

impl HttpServiceRequestBuilder<()> for HoneycombSvcRequestBuilder {
    fn build(&self, mut request: HttpRequest<()>) -> Result<Request<Bytes>, crate::Error> {
        let mut builder =
            Request::post(&self.uri).header(HTTP_HEADER_HONEYCOMB, self.api_key.inner());

        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header("Content-Encoding".to_string(), ce.to_string());
        }

        builder
            .body(request.take_payload())
            .context(HTTPRequestBuilderSnafu)
            .map_err(Into::into)
    }
}
