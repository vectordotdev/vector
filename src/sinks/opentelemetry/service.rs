//! Service implementation for the `opentelemetry` sink.

use bytes::Bytes;
use http::{header::CONTENT_TYPE, Request};

use super::sink::PartitionKey;

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
    pub(super) auth: Option<Auth>,
}

impl HttpServiceRequestBuilder<PartitionKey> for OpentelemetryServiceRequestBuilder {
    fn build(
        &self,
        mut request: HttpRequest<PartitionKey>,
    ) -> Result<Request<Bytes>, crate::Error> {
        let metadata = request.get_additional_metadata();

        let builder =
            Request::post(metadata.endpoint.clone()).header(CONTENT_TYPE, "application/x-protobuf");

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
