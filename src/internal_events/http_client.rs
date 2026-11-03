use std::time::Duration;

use http::{
    HeaderMap, Request, Response, Version,
    header::{self, HeaderName, HeaderValue},
};
use http_body::Body;
use vector_lib::{
    NamedInternalEvent, counter, histogram,
    internal_event::{CounterName, HistogramName, InternalEvent, error_stage, error_type},
};

// ── Telemetry traits ──────────────────────────────────────────────────────────

/// Provides the data required to emit HTTP request telemetry.
///
/// `method`, `uri`, `sanitized_headers`, and `body_debug` are required; every
/// transport must implement them.
///
/// `version` is optional (default: `None`) because some transports — notably
/// the AWS SDK connector layer — do not expose HTTP version in their request
/// type.
pub trait HttpRequestTelemetry {
    fn method(&self) -> &str;
    fn uri(&self) -> String;
    fn headers(&self) -> HeaderMap<HeaderValue>;
    /// Returns the body size bounds as `(lower, upper)`.
    fn body_size_hint(&self) -> (u64, Option<u64>);
    /// Returns the HTTP version when the transport exposes it.
    fn version(&self) -> Option<Version> {
        None
    }

    /// Transport-specific headers that carry credential material.
    ///
    /// The names returned here are marked sensitive in addition to the
    /// generic list (Authorization, cookies, API keys, …) that
    /// [`HttpRequestTelemetry::sanitized_headers`] always applies. Override this — rather than
    /// [`HttpRequestTelemetry::sanitized_headers`] — when a transport adds its own credential headers.
    fn extra_sensitive_headers(&self) -> &'static [HeaderName] {
        &[]
    }

    /// Returns the headers with sensitive values redacted.
    fn sanitized_headers(&self) -> HeaderMap<HeaderValue> {
        remove_sensitive(self.headers(), self.extra_sensitive_headers())
    }
}

/// Provides the data required to emit HTTP response telemetry.
///
/// Same rationale as [`HttpRequestTelemetry`]: `version` is optional.
pub trait HttpResponseTelemetry {
    fn status_u16(&self) -> u16;
    fn headers(&self) -> HeaderMap<HeaderValue>;
    /// Returns the body size bounds as `(lower, upper)`.
    fn body_size_hint(&self) -> (u64, Option<u64>);
    /// Returns the HTTP version when the transport exposes it.
    fn version(&self) -> Option<Version> {
        None
    }

    /// Transport-specific headers that carry credential material.
    ///
    /// See [`HttpRequestTelemetry::extra_sensitive_headers`] for details.
    fn extra_sensitive_headers(&self) -> &'static [HeaderName] {
        &[]
    }

    /// Returns the headers with sensitive values redacted.
    fn sanitized_headers(&self) -> HeaderMap<HeaderValue> {
        remove_sensitive(self.headers(), self.extra_sensitive_headers())
    }
}

// ── Implementations for the legacy http 0.2 / hyper 0.14 types (full data) ────

impl<T: hyper::body::HttpBody> HttpRequestTelemetry for http_0::Request<T> {
    fn method(&self) -> &str {
        self.method().as_str()
    }

    fn uri(&self) -> String {
        self.uri().to_string()
    }

    fn headers(&self) -> HeaderMap<HeaderValue> {
        convert_legacy_headers(self.headers())
    }

    fn body_size_hint(&self) -> (u64, Option<u64>) {
        let hint = self.body().size_hint();
        (hint.lower(), hint.upper())
    }

    fn version(&self) -> Option<Version> {
        convert_legacy_version(self.version())
    }
}

impl<T: hyper::body::HttpBody> HttpResponseTelemetry for http_0::Response<T> {
    fn status_u16(&self) -> u16 {
        self.status().as_u16()
    }

    fn headers(&self) -> HeaderMap<HeaderValue> {
        convert_legacy_headers(self.headers())
    }

    fn body_size_hint(&self) -> (u64, Option<u64>) {
        let hint = self.body().size_hint();
        (hint.lower(), hint.upper())
    }

    fn version(&self) -> Option<Version> {
        convert_legacy_version(self.version())
    }
}

// ── Events ────────────────────────────────────────────────────────────────────

#[derive(Debug, NamedInternalEvent)]
pub struct AboutToSendHttpRequest<'a, T: HttpRequestTelemetry> {
    pub request: &'a T,
}

impl<T: HttpRequestTelemetry> InternalEvent for AboutToSendHttpRequest<'_, T> {
    fn emit(self) {
        debug!(
            message = "Sending HTTP request.",
            uri = %self.request.uri(),
            method = %self.request.method(),
            version = ?self.request.version(),
            headers = ?self.request.sanitized_headers(),
            body = %FormatBodySizeHint::from(self.request.body_size_hint()),
        );
        counter!(CounterName::HttpClientRequestsSentTotal, "method" => self.request.method().to_string())
            .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct GotHttpResponse<'a, T: HttpResponseTelemetry> {
    pub response: &'a T,
    pub roundtrip: Duration,
}

impl<T: HttpResponseTelemetry> InternalEvent for GotHttpResponse<'_, T> {
    fn emit(self) {
        let status = self.response.status_u16();
        let status_str = status.to_string();
        debug!(
            message = "HTTP response.",
            status = %status,
            version = ?self.response.version(),
            headers = ?self.response.sanitized_headers(),
            body = %FormatBodySizeHint::from(self.response.body_size_hint()),

        );
        counter!(CounterName::HttpClientResponsesTotal, "status" => status_str.clone())
            .increment(1);
        histogram!(HistogramName::HttpClientRttSeconds).record(self.roundtrip);
        histogram!(HistogramName::HttpClientResponseRttSeconds, "status" => status_str)
            .record(self.roundtrip);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct GotHttpWarning<'a> {
    pub error: &'a dyn std::error::Error,
    pub roundtrip: Duration,
}

impl InternalEvent for GotHttpWarning<'_> {
    fn emit(self) {
        warn!(
            message = "HTTP error.",
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(CounterName::HttpClientErrorsTotal, "error_kind" => self.error.to_string())
            .increment(1);
        histogram!(HistogramName::HttpClientRttSeconds).record(self.roundtrip);
        histogram!(HistogramName::HttpClientErrorRttSeconds, "error_kind" => self.error.to_string())
            .record(self.roundtrip);
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Adapts Hyper 1 request metadata to the shared HTTP telemetry contract.
pub(crate) struct HttpRequestV1Telemetry<'a, T: Body> {
    request: &'a Request<T>,
}

impl<'a, T: Body> HttpRequestV1Telemetry<'a, T> {
    pub(crate) const fn new(request: &'a Request<T>) -> Self {
        Self { request }
    }
}

impl<T: Body> HttpRequestTelemetry for HttpRequestV1Telemetry<'_, T> {
    fn method(&self) -> &str {
        self.request.method().as_str()
    }

    fn uri(&self) -> String {
        self.request.uri().to_string()
    }

    fn headers(&self) -> HeaderMap<HeaderValue> {
        self.request.headers().clone()
    }

    fn body_size_hint(&self) -> (u64, Option<u64>) {
        let hint = self.request.body().size_hint();
        (hint.lower(), hint.upper())
    }

    fn version(&self) -> Option<Version> {
        Some(self.request.version())
    }
}

/// Adapts Hyper 1 response metadata to the shared HTTP telemetry contract.
pub(crate) struct HttpResponseV1Telemetry<'a, T: Body> {
    response: &'a Response<T>,
}

impl<'a, T: Body> HttpResponseV1Telemetry<'a, T> {
    pub(crate) const fn new(response: &'a Response<T>) -> Self {
        Self { response }
    }
}

impl<T: Body> HttpResponseTelemetry for HttpResponseV1Telemetry<'_, T> {
    fn status_u16(&self) -> u16 {
        self.response.status().as_u16()
    }

    fn headers(&self) -> HeaderMap<HeaderValue> {
        self.response.headers().clone()
    }

    fn body_size_hint(&self) -> (u64, Option<u64>) {
        let hint = self.response.body().size_hint();
        (hint.lower(), hint.upper())
    }

    fn version(&self) -> Option<Version> {
        Some(self.response.version())
    }
}

/// Converts legacy http 0.2 headers to http 1 headers, keeping repeated headers.
fn convert_legacy_headers(headers: &http_0::HeaderMap) -> HeaderMap<HeaderValue> {
    headers.iter().fold(
        HeaderMap::with_capacity(headers.len()),
        |mut converted, (name, value)| {
            let name = HeaderName::from_bytes(name.as_str().as_bytes())
                .expect("http 0.2 header names are valid http 1 header names");
            let mut converted_value = HeaderValue::from_bytes(value.as_bytes())
                .expect("http 0.2 header values are valid http 1 header values");
            // Callers mark their own credential headers sensitive; keep that for redaction.
            converted_value.set_sensitive(value.is_sensitive());
            converted.append(name, converted_value);
            converted
        },
    )
}

const fn convert_legacy_version(version: http_0::Version) -> Option<Version> {
    match version {
        http_0::Version::HTTP_09 => Some(Version::HTTP_09),
        http_0::Version::HTTP_10 => Some(Version::HTTP_10),
        http_0::Version::HTTP_11 => Some(Version::HTTP_11),
        http_0::Version::HTTP_2 => Some(Version::HTTP_2),
        http_0::Version::HTTP_3 => Some(Version::HTTP_3),
        _ => None,
    }
}

fn remove_sensitive(
    mut headers: HeaderMap<HeaderValue>,
    extra: &'static [HeaderName],
) -> HeaderMap<HeaderValue> {
    let sensitive: &[HeaderName] = &[
        header::AUTHORIZATION,
        header::PROXY_AUTHORIZATION,
        header::PROXY_AUTHENTICATE,
        header::WWW_AUTHENTICATE,
        header::COOKIE,
        header::SET_COOKIE,
        HeaderName::from_static("cookie2"),
        HeaderName::from_static("dd-api-key"),
        HeaderName::from_static("x-honeycomb-team"),
        HeaderName::from_static("x-api-key"),
        HeaderName::from_static("api-key"),
    ];
    for (name, value) in &mut headers {
        if sensitive.contains(name) || extra.contains(name) {
            value.set_sensitive(true);
        }
    }
    headers
}

/// Formats a body size hint `(lower, upper)` for debug logging.
struct FormatBodySizeHint(u64, Option<u64>);

impl From<(u64, Option<u64>)> for FormatBodySizeHint {
    fn from((lower, upper): (u64, Option<u64>)) -> Self {
        FormatBodySizeHint(lower, upper)
    }
}

impl std::fmt::Display for FormatBodySizeHint {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match (self.0, self.1) {
            (0, None) => write!(fmt, "[unknown]"),
            (lower, None) => write!(fmt, "[>={lower} bytes]"),

            (0, Some(0)) => write!(fmt, "[empty]"),
            (0, Some(upper)) => write!(fmt, "[<={upper} bytes]"),

            (lower, Some(upper)) if lower == upper => write!(fmt, "[{lower} bytes]"),
            (lower, Some(upper)) => write!(fmt, "[{lower}..={upper} bytes]"),
        }
    }
}

#[cfg(test)]
mod tests {
    use http::{
        Version,
        header::{self, HeaderMap, HeaderName, HeaderValue},
    };

    use super::{HttpRequestTelemetry, HttpResponseTelemetry, remove_sensitive};

    fn is_sensitive(map: &HeaderMap, name: &HeaderName) -> Vec<bool> {
        map.get_all(name)
            .iter()
            .map(HeaderValue::is_sensitive)
            .collect()
    }

    #[test]
    fn marks_single_sensitive_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer token"),
        );
        let result = remove_sensitive(headers, &[]);
        assert!(
            is_sensitive(&result, &header::AUTHORIZATION)
                .iter()
                .all(|&s| s)
        );
    }

    #[test]
    fn marks_all_duplicate_sensitive_headers() {
        let x_api_key: HeaderName = HeaderName::from_static("x-api-key");
        let mut headers = HeaderMap::new();
        headers.insert(x_api_key.clone(), HeaderValue::from_static("key-one"));
        headers.append(x_api_key.clone(), HeaderValue::from_static("key-two"));
        headers.append(x_api_key.clone(), HeaderValue::from_static("key-three"));

        let result = remove_sensitive(headers, &[]);
        let sensitive_flags = is_sensitive(&result, &x_api_key);
        assert_eq!(sensitive_flags.len(), 3);
        assert!(
            sensitive_flags.iter().all(|&s| s),
            "not all duplicate x-api-key values were marked sensitive: {sensitive_flags:?}"
        );
    }

    #[test]
    fn header_name_matching_is_case_insensitive() {
        // HeaderName normalizes to lowercase, so mixed-case variants are identical.
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_static("secret"),
        );
        let result = remove_sensitive(headers, &[]);
        // Lookup with the mixed-case form resolves to the same normalized name.
        let mixed_case = HeaderName::from_bytes(b"X-Api-Key").unwrap();
        assert!(is_sensitive(&result, &mixed_case).iter().all(|&s| s));
    }

    #[test]
    fn extra_headers_are_marked_sensitive() {
        static EXTRA: [HeaderName; 1] = [HeaderName::from_static("x-custom-secret")];
        let custom = HeaderName::from_static("x-custom-secret");
        let mut headers = HeaderMap::new();
        headers.insert(custom.clone(), HeaderValue::from_static("s3cr3t"));
        let result = remove_sensitive(headers, &EXTRA);
        assert!(is_sensitive(&result, &custom).iter().all(|&s| s));
    }

    #[test]
    fn does_not_mark_non_sensitive_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let result = remove_sensitive(headers, &[]);
        assert!(
            is_sensitive(&result, &header::CONTENT_TYPE)
                .iter()
                .all(|&s| !s)
        );
    }

    #[test]
    fn legacy_request_converts_headers_and_version() {
        let request = http_0::Request::builder()
            .version(http_0::Version::HTTP_11)
            .header("x-api-key", "key-one")
            .header("x-api-key", "key-two")
            .header(http_0::header::CONTENT_TYPE, "application/json")
            .body(hyper::Body::empty())
            .unwrap();

        let headers = HttpRequestTelemetry::sanitized_headers(&request);
        let x_api_key = HeaderName::from_static("x-api-key");
        let values: Vec<_> = headers.get_all(&x_api_key).iter().collect();
        assert_eq!(values, ["key-one", "key-two"]);
        assert_eq!(is_sensitive(&headers, &x_api_key), [true, true]);
        assert_eq!(headers[header::CONTENT_TYPE], "application/json");
        assert_eq!(is_sensitive(&headers, &header::CONTENT_TYPE), [false]);
        assert_eq!(
            HttpRequestTelemetry::version(&request),
            Some(Version::HTTP_11)
        );
    }

    #[test]
    fn legacy_request_keeps_caller_marked_sensitive_headers() {
        let mut token = http_0::HeaderValue::from_static("secret");
        token.set_sensitive(true);
        let request = http_0::Request::builder()
            .header("x-custom-token", token)
            .body(hyper::Body::empty())
            .unwrap();

        let headers = HttpRequestTelemetry::sanitized_headers(&request);
        let x_custom_token = HeaderName::from_static("x-custom-token");
        assert_eq!(is_sensitive(&headers, &x_custom_token), [true]);
    }

    #[test]
    fn legacy_response_converts_headers_and_version() {
        let response = http_0::Response::builder()
            .version(http_0::Version::HTTP_2)
            .header(http_0::header::SET_COOKIE, "a=1")
            .header(http_0::header::SET_COOKIE, "b=2")
            .body(hyper::Body::empty())
            .unwrap();

        let headers = HttpResponseTelemetry::sanitized_headers(&response);
        let values: Vec<_> = headers.get_all(header::SET_COOKIE).iter().collect();
        assert_eq!(values, ["a=1", "b=2"]);
        assert_eq!(is_sensitive(&headers, &header::SET_COOKIE), [true, true]);
        assert_eq!(
            HttpResponseTelemetry::version(&response),
            Some(Version::HTTP_2)
        );
    }
}
