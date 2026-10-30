//! Native HTTP request builder for Datadog events.
use std::sync::Arc;

use bytes::Bytes;
use http::{Request, Uri};
use snafu::ResultExt;
use vector_lib::sensitive_string::SensitiveString;

use crate::sinks::{
    HTTPV1RequestBuilderSnafu,
    util::{http::HttpRequest, http_v1::HttpServiceRequestBuilder},
};

#[derive(Clone)]
pub(super) struct DatadogEventsServiceRequestBuilder {
    pub(super) endpoint: Uri,
    pub(super) default_api_key: SensitiveString,
}

impl HttpServiceRequestBuilder<Option<Arc<str>>> for DatadogEventsServiceRequestBuilder {
    fn build(
        &self,
        mut request: HttpRequest<Option<Arc<str>>>,
    ) -> Result<Request<Bytes>, crate::Error> {
        let body = request.take_payload();
        let api_key = request
            .get_additional_metadata()
            .as_deref()
            .unwrap_or(self.default_api_key.inner());

        Request::post(self.endpoint.clone())
            .header("Content-Type", "application/json")
            .header("DD-API-KEY", api_key)
            .header("Content-Length", body.len())
            .body(body)
            .context(HTTPV1RequestBuilderSnafu)
            .map_err(crate::Error::from)
    }
}
