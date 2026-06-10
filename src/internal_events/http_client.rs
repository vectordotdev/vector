use std::time::Duration;

use http::{
    HeaderMap, Request, Response, Version,
    header::{self, HeaderName, HeaderValue},
};
use hyper::body::HttpBody;
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

    /// Returns the headers with sensitive values redacted.
    ///
    /// Provided by default; implementors only need to implement [`headers`].
    fn sanitized_headers(&self) -> HeaderMap<HeaderValue> {
        remove_sensitive(self.headers())
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

    /// Returns the headers with sensitive values redacted.
    ///
    /// Provided by default; implementors only need to implement [`headers`].
    fn sanitized_headers(&self) -> HeaderMap<HeaderValue> {
        remove_sensitive(self.headers())
    }
}

// ── Implementations for the hyper HTTP types (full data) ──────────────────────

impl<T: HttpBody> HttpRequestTelemetry for Request<T> {
    fn method(&self) -> &str {
        self.method().as_str()
    }

    fn uri(&self) -> String {
        self.uri().to_string()
    }

    fn headers(&self) -> HeaderMap<HeaderValue> {
        self.headers().clone()
    }

    fn body_size_hint(&self) -> (u64, Option<u64>) {
        let hint = self.body().size_hint();
        (hint.lower(), hint.upper())
    }

    fn version(&self) -> Option<Version> {
        Some(self.version())
    }
}

impl<T: HttpBody> HttpResponseTelemetry for Response<T> {
    fn status_u16(&self) -> u16 {
        self.status().as_u16()
    }

    fn headers(&self) -> HeaderMap<HeaderValue> {
        self.headers().clone()
    }

    fn body_size_hint(&self) -> (u64, Option<u64>) {
        let hint = self.body().size_hint();
        (hint.lower(), hint.upper())
    }

    fn version(&self) -> Option<Version> {
        Some(self.version())
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

fn remove_sensitive(mut headers: HeaderMap<HeaderValue>) -> HeaderMap<HeaderValue> {
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
    for (name, value) in headers.iter_mut() {
        if sensitive.contains(name) {
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
    use http::header::{self, HeaderMap, HeaderName, HeaderValue};

    use super::remove_sensitive;

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
        let result = remove_sensitive(headers);
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

        let result = remove_sensitive(headers);
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
        let result = remove_sensitive(headers);
        // Lookup with the mixed-case form resolves to the same normalized name.
        let mixed_case = HeaderName::from_bytes(b"X-Api-Key").unwrap();
        assert!(is_sensitive(&result, &mixed_case).iter().all(|&s| s));
    }

    #[test]
    fn does_not_mark_non_sensitive_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let result = remove_sensitive(headers);
        assert!(
            is_sensitive(&result, &header::CONTENT_TYPE)
                .iter()
                .all(|&s| !s)
        );
    }
}
