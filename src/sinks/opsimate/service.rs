//! Service implementation for the `opsimate` sink.

use bytes::Bytes;
use http::{Request, header::CONTENT_TYPE};
use snafu::ResultExt;
use vector_lib::sensitive_string::SensitiveString;

use super::config::HTTP_HEADER_OPSIMATE_API_TOKEN;
use crate::sinks::{
    HTTPRequestBuilderSnafu,
    util::{
        HttpEndpoint,
        http::{HttpRequest, HttpServiceRequestBuilder},
    },
};

#[derive(Debug, Clone)]
pub(super) struct OpsiMateSvcRequestBuilder {
    /// The fully resolved custom alerts endpoint (`{endpoint}/api/v1/alerts/custom`).
    pub(super) alerts_endpoint: HttpEndpoint,
    pub(super) api_token: SensitiveString,
}

impl HttpServiceRequestBuilder<()> for OpsiMateSvcRequestBuilder {
    fn build(&self, mut request: HttpRequest<()>) -> Result<Request<Bytes>, crate::Error> {
        Request::post(self.alerts_endpoint.as_uri().clone())
            .header(HTTP_HEADER_OPSIMATE_API_TOKEN, self.api_token.inner())
            .header(CONTENT_TYPE, "application/json")
            .body(request.take_payload())
            .context(HTTPRequestBuilderSnafu)
            .map_err(crate::Error::from)
    }
}

#[cfg(test)]
mod tests {
    use http::Method;
    use vector_lib::request_metadata::RequestMetadata;

    use super::*;
    use crate::sinks::prelude::*;

    #[test]
    fn posts_to_the_custom_alerts_endpoint_with_token_and_json_headers() {
        let builder = OpsiMateSvcRequestBuilder {
            alerts_endpoint: HttpEndpoint::parse(
                "http://opsimate.example.com:3001/api/v1/alerts/custom",
            )
            .unwrap(),
            api_token: SensitiveString::from("secret-token".to_owned()),
        };
        let payload = Bytes::from_static(br#"{"id":"alert-1"}"#);
        let request = HttpRequest::new(
            payload.clone(),
            EventFinalizers::default(),
            RequestMetadata::default(),
            (),
        );

        let request = builder.build(request).expect("request must build");

        assert_eq!(request.method(), Method::POST);
        assert_eq!(
            request.uri().to_string(),
            "http://opsimate.example.com:3001/api/v1/alerts/custom"
        );
        assert_eq!(
            request
                .headers()
                .get(HTTP_HEADER_OPSIMATE_API_TOKEN)
                .unwrap(),
            "secret-token"
        );
        assert_eq!(
            request.headers().get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
        assert_eq!(request.body(), &payload);
    }
}
