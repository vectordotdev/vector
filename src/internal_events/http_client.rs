use std::time::Duration;

use http::{
    Request, Response,
    header::{self, HeaderMap, HeaderValue},
};
use hyper::{Error, body::HttpBody};
use vector_lib::{
    NamedInternalEvent, counter, histogram,
    internal_event::{CounterName, HistogramName, InternalEvent, error_stage, error_type},
};

pub mod http_1 {
    use std::time::Duration;

    use http_1::{
        Request, Response,
        header::{self, HeaderMap, HeaderValue},
    };
    use hyper_1::body::Body;
    use hyper_util::client::legacy::Error;
    use metrics::{counter, histogram};
    use vector_lib::internal_event::{InternalEvent, error_stage, error_type};

    #[derive(Debug, NamedInternalEvent)]
    pub struct AboutToSendHttpRequest<'a, T> {
        pub request: &'a Request<T>,
    }

    fn remove_sensitive(headers: &HeaderMap<HeaderValue>) -> HeaderMap<HeaderValue> {
        let mut headers = headers.clone();
        for name in &[
            header::AUTHORIZATION,
            header::PROXY_AUTHORIZATION,
            header::COOKIE,
            header::SET_COOKIE,
        ] {
            if let Some(value) = headers.get_mut(name) {
                value.set_sensitive(true);
            }
        }
        headers
    }

    impl<T: Body> InternalEvent for AboutToSendHttpRequest<'_, T> {
        fn emit(self) {
            debug!(
                message = "Sending HTTP request.",
                uri = %self.request.uri(),
                method = %self.request.method(),
                version = ?self.request.version(),
                headers = ?remove_sensitive(self.request.headers()),
                body = %FormatBody(self.request.body()),
            );
            counter!("http_client_requests_sent_total", "method" => self.request.method().to_string())
            .increment(1);
        }
    }

    #[derive(Debug, NamedInternalEvent)]
    pub struct GotHttpResponse<'a, T> {
        pub response: &'a Response<T>,
        pub roundtrip: Duration,
    }

    impl<T: hyper_1::body::Body> InternalEvent for GotHttpResponse<'_, T> {
        fn emit(self) {
            debug!(
                message = "HTTP response.",
                status = %self.response.status(),
                version = ?self.response.version(),
                headers = ?remove_sensitive(self.response.headers()),
                body = %FormatBody(self.response.body()),
            );
            counter!(
                "http_client_responses_total",
                "status" => self.response.status().as_u16().to_string(),
            )
            .increment(1);
            histogram!("http_client_rtt_seconds").record(self.roundtrip);
            histogram!(
                "http_client_response_rtt_seconds",
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
            counter!("http_client_errors_total", "error_kind" => self.error.to_string())
                .increment(1);
            histogram!("http_client_rtt_seconds").record(self.roundtrip);
            histogram!("http_client_error_rtt_seconds", "error_kind" => self.error.to_string())
                .record(self.roundtrip);
        }
    }

    /// Newtype placeholder to provide a formatter for the request and response body.
    struct FormatBody<'a, B>(&'a B);

    impl<B: hyper_1::body::Body> std::fmt::Display for FormatBody<'_, B> {
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
}

#[derive(Debug, NamedInternalEvent)]
pub struct AboutToSendHttpRequest<'a, T> {
    pub request: &'a Request<T>,
}

fn remove_sensitive(headers: &HeaderMap<HeaderValue>) -> HeaderMap<HeaderValue> {
    let mut headers = headers.clone();
    for name in &[
        header::AUTHORIZATION,
        header::PROXY_AUTHORIZATION,
        header::COOKIE,
        header::SET_COOKIE,
    ] {
        if let Some(value) = headers.get_mut(name) {
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
