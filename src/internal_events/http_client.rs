use std::time::Duration;

use http::{
    Request, Response,
    header::{self, HeaderMap, HeaderName, HeaderValue},
};
use hyper::{Error, body::HttpBody};
use vector_lib::{
    NamedInternalEvent, counter, histogram,
    internal_event::{CounterName, HistogramName, InternalEvent, error_stage, error_type},
};

#[derive(Debug, NamedInternalEvent)]
pub struct AboutToSendHttpRequest<'a, T> {
    pub request: &'a Request<T>,
}

fn remove_sensitive(headers: &HeaderMap<HeaderValue>) -> HeaderMap<HeaderValue> {
    let mut headers = headers.clone();
    let sensitive: &[HeaderName] = &[
        header::AUTHORIZATION,
        header::PROXY_AUTHORIZATION,
        header::COOKIE,
        header::SET_COOKIE,
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

impl<T: HttpBody> InternalEvent for AboutToSendHttpRequest<'_, T> {
    fn emit(self) {
        debug!(
            message = "Sending HTTP request.",
            uri = %self.request.uri(),
            method = %self.request.method(),
            version = ?self.request.version(),
            headers = ?remove_sensitive(self.request.headers()),
            body = %FormatBody(self.request.body()),
        );
        counter!(CounterName::HttpClientRequestsSentTotal, "method" => self.request.method().to_string())
            .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct GotHttpResponse<'a, T> {
    pub response: &'a Response<T>,
    pub roundtrip: Duration,
}

impl<T: HttpBody> InternalEvent for GotHttpResponse<'_, T> {
    fn emit(self) {
        debug!(
            message = "HTTP response.",
            status = %self.response.status(),
            version = ?self.response.version(),
            headers = ?remove_sensitive(self.response.headers()),
            body = %FormatBody(self.response.body()),
        );
        counter!(
            CounterName::HttpClientResponsesTotal,
            "status" => self.response.status().as_u16().to_string(),
        )
        .increment(1);
        histogram!(HistogramName::HttpClientRttSeconds).record(self.roundtrip);
        histogram!(
            HistogramName::HttpClientResponseRttSeconds,
            "status" => self.response.status().as_u16().to_string(),
        )
        .record(self.roundtrip);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct GotHttpWarning<'a> {
    pub error: &'a Error,
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

/// Newtype placeholder to provide a formatter for the request and response body.
struct FormatBody<'a, B>(&'a B);

impl<B: HttpBody> std::fmt::Display for FormatBody<'_, B> {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let size = self.0.size_hint();
        match (size.lower(), size.upper()) {
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
        let result = remove_sensitive(&headers);
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

        let result = remove_sensitive(&headers);
        let sensitive_flags = is_sensitive(&result, &x_api_key);
        assert_eq!(sensitive_flags.len(), 3);
        assert!(
            sensitive_flags.iter().all(|&s| s),
            "not all duplicate x-api-key values were marked sensitive: {sensitive_flags:?}"
        );
    }

    #[test]
    fn does_not_mark_non_sensitive_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let result = remove_sensitive(&headers);
        assert!(
            is_sensitive(&result, &header::CONTENT_TYPE)
                .iter()
                .all(|&s| !s)
        );
    }
}
