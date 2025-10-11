//! Service implementation for the `sentry` sink.

use bytes::Bytes;
use http::{Method, Request};
use sentry::types::Dsn;

use crate::sinks::util::http::{HttpRequest, HttpServiceRequestBuilder};

use super::constants::{
    AUTH_HEADER_NAME, CONTENT_TYPE_SENTRY_ENVELOPE, SENTRY_CLIENT, SENTRY_VERSION, USER_AGENT,
};

#[derive(Clone)]
pub(super) struct SentryServiceRequestBuilder {
    pub(super) dsn: Dsn,
}

impl SentryServiceRequestBuilder {
    pub(super) const fn new(dsn: Dsn) -> Self {
        Self { dsn }
    }
}

impl HttpServiceRequestBuilder<()> for SentryServiceRequestBuilder {
    fn build(&self, mut request: HttpRequest<()>) -> Result<Request<Bytes>, crate::Error> {
        let payload = request.take_payload();

        let url = self.dsn.envelope_api_url();

        let mut req = Request::builder()
            .method(Method::POST)
            .uri(url.to_string())
            .header("Content-Type", CONTENT_TYPE_SENTRY_ENVELOPE)
            .header("User-Agent", USER_AGENT)
            .body(payload)?;

        // Add authentication header
        let auth_header = format!(
            "Sentry sentry_version={}, sentry_key={}, sentry_client={}",
            SENTRY_VERSION,
            self.dsn.public_key(),
            SENTRY_CLIENT
        );
        req.headers_mut()
            .insert(AUTH_HEADER_NAME, http::HeaderValue::from_str(&auth_header)?);

        Ok(req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::util::http::HttpRequest;
    use bytes::Bytes;
    use http::HeaderValue;
    use sentry::types::Dsn;
    use std::str::FromStr;
    use vector_lib::event::EventFinalizers;
    use vector_lib::request_metadata::RequestMetadata;

    fn create_test_dsn() -> Dsn {
        Dsn::from_str("https://abc123@o123456.ingest.sentry.io/123456")
            .expect("Failed to create test DSN")
    }

    fn create_test_request() -> HttpRequest<()> {
        let payload = Bytes::from("test payload");
        let finalizers = EventFinalizers::default();
        let metadata = RequestMetadata::default();
        HttpRequest::new(payload, finalizers, metadata, ())
    }

    #[test]
    fn test_sentry_svc_request_builder_new() {
        let dsn = create_test_dsn();
        let builder = SentryServiceRequestBuilder::new(dsn.clone());

        assert_eq!(builder.dsn.host(), dsn.host());
        assert_eq!(builder.dsn.public_key(), dsn.public_key());
        assert_eq!(builder.dsn.project_id(), dsn.project_id());
    }

    #[test]
    fn test_build_request_success() {
        let dsn = create_test_dsn();
        let builder = SentryServiceRequestBuilder::new(dsn.clone());
        let request = create_test_request();

        let result = builder.build(request);
        assert!(result.is_ok());

        let http_request = result.unwrap();

        // Check method
        assert_eq!(http_request.method(), &Method::POST);

        // Check URI
        let expected_url = dsn.envelope_api_url();
        assert_eq!(http_request.uri().to_string(), expected_url.to_string());

        // Check Content-Type header
        assert_eq!(
            http_request.headers().get("Content-Type").unwrap(),
            &HeaderValue::from_static(CONTENT_TYPE_SENTRY_ENVELOPE)
        );

        // Check User-Agent header
        assert_eq!(
            http_request.headers().get("User-Agent").unwrap(),
            &HeaderValue::from_static(USER_AGENT)
        );

        // Check body
        assert_eq!(http_request.body(), &Bytes::from("test payload"));
    }

    #[test]
    fn test_authentication_header_format() {
        let dsn = create_test_dsn();
        let builder = SentryServiceRequestBuilder::new(dsn.clone());
        let request = create_test_request();

        let result = builder.build(request);
        assert!(result.is_ok());

        let http_request = result.unwrap();

        // Check authentication header
        let auth_header = http_request.headers().get(AUTH_HEADER_NAME).unwrap();
        let auth_str = auth_header.to_str().unwrap();

        let expected_auth = format!(
            "Sentry sentry_version={}, sentry_key={}, sentry_client={}",
            SENTRY_VERSION,
            dsn.public_key(),
            SENTRY_CLIENT
        );

        assert_eq!(auth_str, expected_auth);
        assert!(auth_str.contains(&format!("sentry_version={}", SENTRY_VERSION)));
        assert!(auth_str.contains(&format!("sentry_key={}", dsn.public_key())));
        assert!(auth_str.contains(&format!("sentry_client={}", SENTRY_CLIENT)));
    }

    #[test]
    fn test_build_with_empty_payload() {
        let dsn = create_test_dsn();
        let builder = SentryServiceRequestBuilder::new(dsn);
        let request = HttpRequest::new(
            Bytes::new(),
            EventFinalizers::default(),
            RequestMetadata::default(),
            (),
        );

        let result = builder.build(request);
        assert!(result.is_ok());

        let http_request = result.unwrap();
        assert_eq!(http_request.body(), &Bytes::new());
    }

    #[test]
    fn test_build_with_large_payload() {
        let dsn = create_test_dsn();
        let builder = SentryServiceRequestBuilder::new(dsn);
        let large_payload = Bytes::from(vec![b'x'; 10000]);
        let request = HttpRequest::new(
            large_payload.clone(),
            EventFinalizers::default(),
            RequestMetadata::default(),
            (),
        );

        let result = builder.build(request);
        assert!(result.is_ok());

        let http_request = result.unwrap();
        assert_eq!(http_request.body(), &large_payload);
    }
}
