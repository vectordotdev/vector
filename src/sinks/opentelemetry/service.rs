//! Service implementation for the `opentelemetry` sink.

use bytes::Bytes;
use http::{header::CONTENT_TYPE, Request, Uri};

use crate::{
    http::Auth,
    sinks::{
        util::http::{HttpRequest, HttpServiceRequestBuilder},
        HTTPRequestBuilderSnafu,
    },
};
use snafu::ResultExt;

#[derive(Debug, Clone)]
pub(super) struct OpentelemetryServiceRequestBuilder {
    pub(super) uri: Uri,
    pub(super) auth: Option<Auth>,
}

impl HttpServiceRequestBuilder<()> for OpentelemetryServiceRequestBuilder {
    fn build(&self, mut request: HttpRequest<()>) -> Result<Request<Bytes>, crate::Error> {
        let builder =
            Request::post(self.uri.clone()).header(CONTENT_TYPE, "application/x-protobuf");

        let mut request = builder
            .body(request.take_payload())
            .context(HTTPRequestBuilderSnafu)
            .map_err(Into::<crate::Error>::into)?;

        if let Some(auth) = &self.auth {
            auth.apply(&mut request);
        }

        Ok(request)
    }
}
