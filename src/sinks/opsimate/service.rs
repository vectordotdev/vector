//! Service implementation for the `opsimate` sink.

use bytes::Bytes;
use http::{HeaderValue, Request, header::CONTENT_TYPE};
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
        // `x-api-token` is not on the shared redaction list of the outgoing-request debug log,
        // so the value is flagged sensitive here and the log prints it as `Sensitive`.
        let mut api_token = HeaderValue::from_str(self.api_token.inner())
            .map_err(http::Error::from)
            .context(HTTPRequestBuilderSnafu)?;
        api_token.set_sensitive(true);

        Request::post(self.alerts_endpoint.as_uri().clone())
            .header(HTTP_HEADER_OPSIMATE_API_TOKEN, api_token)
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
        let token = request
            .headers()
            .get(HTTP_HEADER_OPSIMATE_API_TOKEN)
            .unwrap();
        assert_eq!(token, "secret-token");
        assert!(
            token.is_sensitive(),
            "the API token header must be flagged sensitive so debug logs redact it"
        );
        assert_eq!(
            request.headers().get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
        assert_eq!(request.body(), &payload);
    }

    #[test]
    fn rejects_a_token_that_is_not_a_valid_header_value() {
        let builder = OpsiMateSvcRequestBuilder {
            alerts_endpoint: HttpEndpoint::parse(
                "http://opsimate.example.com:3001/api/v1/alerts/custom",
            )
            .unwrap(),
            api_token: SensitiveString::from("secret\ntoken".to_owned()),
        };
        let request = HttpRequest::new(
            Bytes::from_static(b"{}"),
            EventFinalizers::default(),
            RequestMetadata::default(),
            (),
        );

        assert!(
            builder.build(request).is_err(),
            "a token with a line break cannot be sent as a header"
        );
    }
}
