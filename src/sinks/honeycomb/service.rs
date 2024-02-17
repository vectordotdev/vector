//! Service implementation for the `honeycomb` sink.

use bytes::Bytes;
use http::{Request, Uri};
use vector_lib::sensitive_string::SensitiveString;

use crate::sinks::{
    util::http::{HttpRequest, HttpServiceRequestBuilder},
    HTTPRequestBuilderSnafu,
};
use snafu::ResultExt;

use super::config::HTTP_HEADER_HONEYCOMB;

#[derive(Debug, Clone)]
pub(super) struct HoneycombSvcRequestBuilder {
    pub(super) uri: Uri,
    pub(super) api_key: SensitiveString,
}

impl HttpServiceRequestBuilder<()> for HoneycombSvcRequestBuilder {
    fn build(&self, mut request: HttpRequest<()>) -> Result<Request<Bytes>, crate::Error> {
        let builder = Request::post(&self.uri).header(HTTP_HEADER_HONEYCOMB, self.api_key.inner());

        builder
            .body(request.take_payload())
            .context(HTTPRequestBuilderSnafu)
            .map_err(Into::into)
    }
}
