//! Service implementation for the `keep` sink.

use bytes::Bytes;
use http::{Request, Uri};
use vector_lib::sensitive_string::SensitiveString;

use crate::sinks::{
    util::http::{HttpRequest, HttpServiceRequestBuilder},
    HTTPRequestBuilderSnafu,
};
use snafu::ResultExt;

use super::config::HTTP_HEADER_KEEP_API_KEY;

#[derive(Debug, Clone)]
pub(super) struct KeepSvcRequestBuilder {
    pub(super) uri: Uri,
    pub(super) api_key: SensitiveString,
}

impl HttpServiceRequestBuilder<()> for KeepSvcRequestBuilder {
    fn build(&self, mut request: HttpRequest<()>) -> Result<Request<Bytes>, crate::Error> {
        let builder =
            Request::post(&self.uri).header(HTTP_HEADER_KEEP_API_KEY, self.api_key.inner());

        let builder = builder.header("Content-Type".to_string(), "application/json".to_string());

        builder
            .body(request.take_payload())
            .context(HTTPRequestBuilderSnafu)
            .map_err(Into::into)
    }
}
