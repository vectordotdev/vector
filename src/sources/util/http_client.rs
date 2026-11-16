//! Common logic for sources that are HTTP clients.
//!
//! Specific HTTP client sources will:
//!   - Call `build_url()` to build the URL(s) to call.
//!   - Implement a specific context struct which:
//!       - Contains the data that source needs in order to process the HTTP responses into `internal_events`
//!       - Implements the `HttpClient` trait
//!   - Call `call()` supplying the generic inputs for calling and the source-specific
//!     context.

// Okta source only imports defaults but doesn't use the rest of the client
#![cfg_attr(feature = "sources-okta", allow(dead_code))]

use std::{collections::HashMap, future::ready, time::Duration};

use bytes::Bytes;
use futures_util::{FutureExt, Stream, StreamExt, stream, stream::BoxStream};
use http::{
    HeaderMap, Request, StatusCode, Uri,
    header::{ACCEPT, CONTENT_TYPE, HeaderName, HeaderValue},
    response::Parts,
};
use http_body_util::BodyExt as _;
use snafu::{ResultExt, Snafu};
use tokio_stream::wrappers::IntervalStream;
use vector_http::{empty_body, full_body};
use vector_lib::{
    EstimatedJsonEncodedSizeOf, event::Event, internal_event::emit, json_size::JsonSize,
    shutdown::ShutdownSignal,
};

use crate::{
    SourceSender,
    http::{Auth, HttpClient, QueryParameterValue, QueryParameters},
    internal_events::{
        EndpointBytesReceived, HttpClientEventsReceived, HttpClientHttpError,
        HttpClientHttpResponseError, StreamClosedError,
    },
    sources::util::http::HttpMethod,
};

/// Contains the inputs generic to any http client.
pub(crate) struct GenericHttpClientInputs {
    /// Array of URLs to call.
    pub urls: Vec<Uri>,
    /// Interval between calls.
    pub interval: Duration,
    /// Timeout for the HTTP request.
    pub timeout: Duration,
    /// Map of Header+Value to apply to HTTP request.
    pub headers: HeaderMap<HeaderValue>,
    /// Content type of the HTTP request, determined by the source.
    pub content_type: String,
    pub auth: Option<Auth>,
    /// Client that sends every request of the source.
    pub client: HttpClient,
    pub shutdown: ShutdownSignal,
}

#[derive(Debug, Snafu)]
pub(crate) enum HeaderError {
    #[snafu(display("Invalid HTTP header name {name:?}: {source}"))]
    InvalidName {
        name: String,
        source: http::header::InvalidHeaderName,
    },
    #[snafu(display("Invalid value for HTTP header {name:?}: {source}"))]
    InvalidValue {
        name: String,
        source: http::header::InvalidHeaderValue,
    },
}

pub(crate) fn build_headers(
    headers: &HashMap<String, Vec<String>>,
) -> Result<HeaderMap<HeaderValue>, HeaderError> {
    let mut parsed = HeaderMap::new();

    for (name, values) in headers {
        let header_name =
            HeaderName::from_bytes(name.as_bytes()).with_context(|_| InvalidNameSnafu {
                name: name.to_owned(),
            })?;

        for value in values {
            let mut header_value =
                HeaderValue::from_bytes(value.as_bytes()).with_context(|_| InvalidValueSnafu {
                    name: name.to_owned(),
                })?;
            header_value.set_sensitive(true);
            parsed.append(header_name.clone(), header_value);
        }
    }

    Ok(parsed)
}

/// The default interval to call the HTTP endpoint if none is configured.
pub(crate) const fn default_interval() -> Duration {
    Duration::from_secs(15)
}

/// The default timeout for the HTTP request if none is configured.
pub(crate) const fn default_timeout() -> Duration {
    Duration::from_secs(5)
}

/// Builds the context, allowing the source-specific implementation to leverage data from the
/// config and the current HTTP request.
pub(crate) trait HttpClientBuilder {
    type Context: HttpClientContext;

    /// Called before the HTTP request is made to build out the context.
    fn build(&self, url: &Uri) -> Self::Context;
}

/// Methods that allow context-specific behavior during the scraping procedure.
pub(crate) trait HttpClientContext {
    /// Called after the HTTP request succeeds and returns the decoded/parsed Event array.
    fn on_response(&mut self, url: &Uri, header: &Parts, body: &Bytes) -> Option<Vec<Event>>;

    /// (Optional) Called if the HTTP response is not 200 ('OK').
    fn on_http_response_error(&self, _uri: &Uri, _header: &Parts) {}

    /// (Optional) Process the base URL before each request.
    /// Allows for dynamic query parameters that update at runtime.
    /// Returns a new URL if parameters need to be updated, or None to use the original URL.
    fn process_url(&self, _url: &Uri) -> Option<Uri> {
        None
    }

    /// (Optional) Get the request body to send with the HTTP request.
    /// Returns the body as a String if one should be sent, or None for an empty body.
    fn get_request_body(&self) -> Option<String> {
        None
    }

    // This function can be defined to enrich events with additional HTTP
    // metadata. This function should be used rather than internal enrichment so
    // that accurate byte count metrics can be emitted.
    fn enrich_events(&mut self, _events: &mut Vec<Event>) {}
}

/// Builds a url for the HTTP requests.
pub(crate) fn build_url(uri: &Uri, query: &QueryParameters) -> Uri {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    if let Some(query) = uri.query() {
        serializer.extend_pairs(url::form_urlencoded::parse(query.as_bytes()));
    }
    for (k, query_value) in query {
        match query_value {
            QueryParameterValue::SingleParam(param) => {
                serializer.append_pair(k, param.value());
            }
            QueryParameterValue::MultiParams(params) => {
                for v in params {
                    serializer.append_pair(k, v.value());
                }
            }
        }
    }
    let mut builder = Uri::builder();
    if let Some(scheme) = uri.scheme() {
        builder = builder.scheme(scheme.clone());
    }
    if let Some(authority) = uri.authority() {
        builder = builder.authority(authority.clone());
    }
    builder = builder.path_and_query(match serializer.finish() {
        query if !query.is_empty() => format!("{}?{query}", uri.path()),
        _ => uri.path().to_string(),
    });
    builder
        .build()
        .expect("Failed to build URI from parsed arguments")
}

/// Warns if the scrape timeout is greater than the scrape interval.
pub(crate) fn warn_if_interval_too_low(timeout: Duration, interval: Duration) {
    if timeout > interval {
        warn!(
            interval_secs = %interval.as_secs_f64(),
            timeout_secs = %timeout.as_secs_f64(),
            message = "Having a scrape timeout that exceeds the scrape interval can lead to excessive resource consumption.",
        );
    }
}

/// Options that apply to every request a source sends.
pub(crate) struct RequestOptions {
    /// Client that sends every request of the source.
    pub client: HttpClient,
    pub method: HttpMethod,
    /// Timeout for each request, including reading the response body.
    pub timeout: Duration,
    /// Map of Header+Value to apply to each request.
    pub headers: HeaderMap<HeaderValue>,
    /// Content type of the request, determined by the source.
    pub content_type: String,
    pub auth: Option<Auth>,
}

/// Calls one or more urls at an interval.
///   - The HTTP request is built per the options in provided generic inputs.
///   - The HTTP response is decoded/parsed into events by the specific context.
///   - The events are then sent to the output stream.
pub(crate) async fn call<
    B: HttpClientBuilder<Context = C> + Send + Clone,
    C: HttpClientContext + Send + 'static,
>(
    inputs: GenericHttpClientInputs,
    context_builder: B,
    out: SourceSender,
    http_method: HttpMethod,
) -> vector_lib::source_sender::SendResult {
    let GenericHttpClientInputs {
        urls,
        interval,
        timeout,
        headers,
        content_type,
        auth,
        client,
        shutdown,
    } = inputs;
    let requests = IntervalStream::new(tokio::time::interval(interval))
        .take_until(shutdown)
        .flat_map(move |_| stream::iter(urls.clone()))
        .map(move |url| {
            let context = context_builder.build(&url);
            (url, context)
        });
    let options = RequestOptions {
        client,
        method: http_method,
        timeout,
        headers,
        content_type,
        auth,
    };
    run(requests, options, out).await
}

/// Sends every request in `requests` and forwards the events that each request's context
/// produces.
///
/// Requests run concurrently, and each request's events are sent as soon as it finishes. The
/// caller decides when to send which requests: `call` sends a fixed list of URLs at an interval.
pub(crate) async fn run<S, C>(
    requests: S,
    options: RequestOptions,
    mut out: SourceSender,
) -> vector_lib::source_sender::SendResult
where
    S: Stream<Item = (Uri, C)> + Send,
    C: HttpClientContext + Send + 'static,
{
    let mut stream = requests
        .map(move |(url, context)| send_request(&options, url, context))
        .flatten_unordered(None)
        .boxed();

    match out.send_event_stream(&mut stream).await {
        Ok(()) => {
            debug!("Finished sending.");
            Ok(())
        }
        Err(error) => {
            let (count, _) = stream.size_hint();
            emit(StreamClosedError { count });
            Err(error)
        }
    }
}

/// Sends one request and returns the events that `context` produces from the response.
fn send_request<C: HttpClientContext + Send + 'static>(
    options: &RequestOptions,
    base_url: Uri,
    mut context: C,
) -> BoxStream<'static, Event> {
    let client = options.client.clone();
    let timeout = options.timeout;
    let endpoint = base_url.to_string();

    // Check if we need to process the URL dynamically (for updating VRL expressions)
    let url = context.process_url(&base_url).unwrap_or(base_url);

    let mut builder = match options.method {
        HttpMethod::Head => Request::head(&url),
        HttpMethod::Get => Request::get(&url),
        HttpMethod::Post => Request::post(&url),
        HttpMethod::Put => Request::put(&url),
        HttpMethod::Patch => Request::patch(&url),
        HttpMethod::Delete => Request::delete(&url),
        HttpMethod::Options => Request::options(&url),
    };

    // add user specified headers
    for (header, value) in &options.headers {
        builder = builder.header(header, value);
    }

    // set ACCEPT header if not user specified
    if !options.headers.contains_key(ACCEPT) {
        builder = builder.header(ACCEPT, &options.content_type);
    }

    // Get the request body from the context (if any)
    let body = match context.get_request_body() {
        Some(body_str) => {
            // Set Content-Type header if not already set
            if !options.headers.contains_key(CONTENT_TYPE) {
                builder = builder.header(CONTENT_TYPE, "application/json");
            }
            full_body(body_str.into())
        }
        None => empty_body(),
    };

    // building the request should be infallible
    let mut request = builder.body(body).expect("error creating request");

    if let Some(auth) = &options.auth {
        auth.apply(&mut request);
    }

    // The timeout covers reading the body too, so a stalled body can't hang the request.
    let fetch = async move {
        let response = client.send(request).await?;
        let (header, body) = response.into_parts();
        let body = body.collect().await?.to_bytes();
        Ok::<_, crate::Error>((header, body))
    };
    tokio::time::timeout(timeout, fetch)
        .map(move |result| match result {
            Ok(Ok((header, body))) => {
                emit(EndpointBytesReceived {
                    byte_size: body.len(),
                    protocol: "http",
                    endpoint: endpoint.as_str(),
                });
                Ok((header, body))
            }
            Ok(Err(error)) => Err(error),
            Err(_) => {
                Err(format!("Timeout error: request exceeded {}s", timeout.as_secs_f64()).into())
            }
        })
        .into_stream()
        .filter_map(move |response| {
            ready(match response {
                Ok((header, body)) if header.status == StatusCode::OK => {
                    context.on_response(&url, &header, &body).map(|mut events| {
                        let byte_size = if events.is_empty() {
                            // We need to explicitly set the byte size
                            // to 0 since
                            // `estimated_json_encoded_size_of` returns
                            // at least 1 for an empty collection. For
                            // the purposes of the
                            // HttpClientEventsReceived event, we should
                            // emit 0 when there aren't any usable
                            // metrics.
                            JsonSize::zero()
                        } else {
                            events.estimated_json_encoded_size_of()
                        };

                        emit(HttpClientEventsReceived {
                            byte_size,
                            count: events.len(),
                            url: url.to_string(),
                        });

                        // We'll enrich after receiving the events so
                        // that the byte sizes are accurate.
                        context.enrich_events(&mut events);

                        stream::iter(events)
                    })
                }
                Ok((header, _)) => {
                    context.on_http_response_error(&url, &header);
                    emit(HttpClientHttpResponseError {
                        code: header.status,
                        url: url.to_string(),
                    });
                    None
                }
                Err(error) => {
                    emit(HttpClientHttpError {
                        error,
                        url: url.to_string(),
                    });
                    None
                }
            })
        })
        .flatten()
        .boxed()
}

#[cfg(test)]
mod tests {
    use vector_lib::event::LogEvent;
    use vrl::event_path;

    use super::*;
    use crate::{
        config::ProxyConfig,
        test_util::http::{always_200_response, spawn_blackhole_http_server},
        tls::MaybeTlsSettings,
    };

    struct LabelContext {
        label: &'static str,
    }

    impl HttpClientContext for LabelContext {
        fn on_response(&mut self, url: &Uri, _: &Parts, _: &Bytes) -> Option<Vec<Event>> {
            let mut log = LogEvent::default();
            log.insert(event_path!("label"), self.label);
            log.insert(event_path!("path"), url.path().to_owned());
            Some(vec![log.into()])
        }
    }

    #[tokio::test]
    async fn run_uses_each_request_context() {
        let base = spawn_blackhole_http_server(always_200_response).await;
        let request = |path: &str, label| {
            let url: Uri = format!("{base}{path}").parse().unwrap();
            (url, LabelContext { label })
        };
        let requests = stream::iter([request("a", "first"), request("b", "second")]);
        let options = RequestOptions {
            client: HttpClient::new(MaybeTlsSettings::Raw(()), &ProxyConfig::default()).unwrap(),
            method: HttpMethod::Get,
            timeout: Duration::from_secs(1),
            headers: HeaderMap::new(),
            content_type: "text/plain".to_owned(),
            auth: None,
        };

        let (out, rx) = SourceSender::new_test();
        tokio::time::timeout(Duration::from_secs(5), run(requests, options, out))
            .await
            .expect("run should finish once the request stream ends")
            .unwrap();

        let mut seen: Vec<(String, String)> = rx
            .map(|event| {
                let log = event.into_log();
                let label = log.get(event_path!("label")).unwrap();
                let path = log.get(event_path!("path")).unwrap();
                (
                    label.to_string_lossy().into_owned(),
                    path.to_string_lossy().into_owned(),
                )
            })
            .collect()
            .await;
        seen.sort();

        // Each event's label must come from the context sent with its URL.
        assert_eq!(
            seen,
            [
                ("first".to_owned(), "/a".to_owned()),
                ("second".to_owned(), "/b".to_owned()),
            ]
        );
    }

    #[test]
    fn headers_are_normalized_and_sensitive() {
        let headers = build_headers(&HashMap::from([(
            "X-Private-Token".to_string(),
            vec!["secret".to_string(), "another-secret".to_string()],
        )]))
        .unwrap();

        let values = headers
            .get_all("x-private-token")
            .iter()
            .collect::<Vec<_>>();
        assert_eq!(values.len(), 2);
        assert!(values.iter().all(|value| value.is_sensitive()));
    }

    #[test]
    fn invalid_header_name_is_rejected() {
        let error = build_headers(&HashMap::from([(
            "invalid header".to_string(),
            vec!["value".to_string()],
        )]))
        .unwrap_err();

        assert!(error.to_string().contains("Invalid HTTP header name"));
    }

    #[test]
    fn invalid_header_value_is_rejected() {
        let error = build_headers(&HashMap::from([(
            "x-header".to_string(),
            vec!["invalid\nvalue".to_string()],
        )]))
        .unwrap_err();

        assert!(error.to_string().contains("Invalid value for HTTP header"));
    }
}
