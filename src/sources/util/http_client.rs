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

use std::{collections::HashMap, future::ready, io, sync::Arc, time::Duration};

use bytes::Bytes;
use futures_util::{FutureExt, StreamExt, TryFutureExt, stream};
use http::{
    HeaderMap, StatusCode, Uri,
    header::{HeaderName, HeaderValue, LOCATION},
    response::Parts,
};
use hyper::{Body, Request};
use snafu::{ResultExt, Snafu};
use tokio::time::Instant;
use tokio_stream::wrappers::IntervalStream;
use vector_lib::{
    EstimatedJsonEncodedSizeOf, config::proxy::ProxyConfig, configurable::configurable_component,
    event::Event, json_size::JsonSize, shutdown::ShutdownSignal,
};

use crate::{
    Error, SourceSender,
    http::{Auth, HttpClient, QueryParameterValue, QueryParameters},
    internal_events::{
        EndpointBytesReceived, HttpClientEventsReceived, HttpClientHttpError,
        HttpClientHttpResponseError, StreamClosedError,
    },
    sources::util::http::HttpMethod,
    tls::TlsSettings,
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
    /// Redirect behavior, with the defaults of the source applied.
    pub redirects: ResolvedRedirects,
    /// Content type of the HTTP request, determined by the source.
    pub content_type: String,
    pub auth: Option<Auth>,
    pub tls: TlsSettings,
    pub proxy: ProxyConfig,
    pub shutdown: ShutdownSignal,
}

/// HTTP redirect configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct Redirects {
    /// Whether to follow HTTP redirects.
    ///
    /// Defaults to `false` for the `http_client` source, to preserve the behavior of existing
    /// configurations, and to `true` for the `prometheus_scrape` source, which matches the
    /// behavior of Prometheus itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow: Option<bool>,

    /// The maximum number of redirects to follow.
    ///
    /// This only applies when redirects are followed.
    #[serde(default = "default_max_redirects")]
    pub max: usize,

    /// The behavior to apply to redirects that point to a different origin.
    #[serde(default)]
    pub cross_origin: CrossOriginRedirects,
}

impl Default for Redirects {
    fn default() -> Self {
        Self {
            follow: None,
            max: default_max_redirects(),
            cross_origin: CrossOriginRedirects::default(),
        }
    }
}

/// The behavior to apply to redirects that point to a different origin.
///
/// An origin is the scheme, host, and port of a URL. Headers that carry credentials are not
/// forwarded when a redirect leaves the origin of the original request, because the new origin is
/// not necessarily trusted by whoever configured the source.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct CrossOriginRedirects {
    /// The names of the headers that are not forwarded to a different origin.
    ///
    /// Defaults to `["authorization", "www-authenticate", "cookie", "cookie2",
    /// "proxy-authorization", "proxy-authenticate"]` for the `http_client` source. The
    /// `prometheus_scrape` source forwards `authorization` on redirects to match Prometheus, so
    /// its default omits that header.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_forward_headers: Option<Vec<String>>,
}

/// Redirect behavior with the defaults of a source applied.
#[derive(Clone, Debug)]
pub(crate) struct ResolvedRedirects {
    /// Whether redirects are followed.
    pub follow: bool,
    /// The maximum number of redirects to follow.
    pub max: usize,
    /// The headers that are not forwarded to a different origin.
    pub not_forward_headers: Vec<HeaderName>,
}

/// The headers that are not forwarded to a different origin by default.
///
/// This matches the headers that Go's `net/http` package and `curl` drop when a redirect leaves the
/// origin of the original request.
pub(crate) const DEFAULT_NOT_FORWARD_HEADERS: &[&str] = &[
    "authorization",
    "www-authenticate",
    "cookie",
    "cookie2",
    "proxy-authorization",
    "proxy-authenticate",
];

impl Redirects {
    /// Applies the defaults of the source that uses the redirects.
    pub(crate) fn resolve(
        &self,
        follow_by_default: bool,
        not_forward_headers_by_default: &[&str],
    ) -> Result<ResolvedRedirects, HeaderError> {
        let not_forward_headers: Vec<String> = match &self.cross_origin.not_forward_headers {
            Some(not_forward_headers) => not_forward_headers.clone(),
            None => not_forward_headers_by_default
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
        };

        let not_forward_headers = not_forward_headers
            .iter()
            .map(|name| parse_header_name(name))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ResolvedRedirects {
            follow: self.follow.unwrap_or(follow_by_default),
            max: self.max,
            not_forward_headers,
        })
    }
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
        let header_name = parse_header_name(name)?;

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

fn parse_header_name(name: &str) -> Result<HeaderName, HeaderError> {
    HeaderName::from_bytes(name.as_bytes()).with_context(|_| InvalidNameSnafu {
        name: name.to_owned(),
    })
}

/// The default interval to call the HTTP endpoint if none is configured.
pub(crate) const fn default_interval() -> Duration {
    Duration::from_secs(15)
}

/// The default timeout for the HTTP request if none is configured.
pub(crate) const fn default_timeout() -> Duration {
    Duration::from_secs(5)
}

/// The default maximum number of redirects to follow.
pub(crate) const fn default_max_redirects() -> usize {
    5
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

/// Builds the HTTP request for the given URL.
fn build_request_for_url(
    url: &Uri,
    http_method: HttpMethod,
    inputs: &GenericHttpClientInputs,
    body: Option<&Bytes>,
    cross_origin: bool,
    body_dropped: bool,
) -> Result<Request<Body>, http::Error> {
    let mut builder = match http_method {
        HttpMethod::Head => Request::head(url),
        HttpMethod::Get => Request::get(url),
        HttpMethod::Post => Request::post(url),
        HttpMethod::Put => Request::put(url),
        HttpMethod::Patch => Request::patch(url),
        HttpMethod::Delete => Request::delete(url),
        HttpMethod::Options => Request::options(url),
    };

    // add user specified headers
    for (header, value) in &inputs.headers {
        // An explicit Host override belongs to the configured origin, not to the redirect target.
        if cross_origin && header == http::header::HOST {
            continue;
        }
        // A 301, 302 or 303 can discard the original request body. Its framing headers must
        // not be carried over to the empty GET, even if they were explicitly configured.
        if body_dropped
            && (header == http::header::CONTENT_LENGTH || header == http::header::TRANSFER_ENCODING)
        {
            continue;
        }
        builder = builder.header(header, value);
    }

    // set ACCEPT header if not user specified
    if !inputs.headers.contains_key(http::header::ACCEPT) {
        builder = builder.header(http::header::ACCEPT, &inputs.content_type);
    }

    let body = match body {
        Some(body) => {
            // Set Content-Type header if not already set
            if !inputs.headers.contains_key(http::header::CONTENT_TYPE) {
                builder = builder.header(http::header::CONTENT_TYPE, "application/json");
            }
            Body::from(body.to_vec())
        }
        None => Body::empty(),
    };

    let mut request = builder.body(body)?;

    if let Some(auth) = &inputs.auth {
        auth.apply(&mut request);
    }

    if cross_origin {
        // Credentials are not forwarded to origins that the source was not configured for.
        for header in &inputs.redirects.not_forward_headers {
            request.headers_mut().remove(header);
        }
    }

    Ok(request)
}

fn parse_absolute_url(uri: &Uri) -> Result<url::Url, Error> {
    url::Url::parse(&uri.to_string()).map_err(Error::from)
}

fn resolve_redirect(base: &url::Url, location: &str) -> Result<url::Url, Error> {
    let url = base.join(location).map_err(Error::from)?;

    if !matches!(url.scheme(), "http" | "https") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Redirect location `{location}` uses the unsupported scheme `{}`",
                url.scheme()
            ),
        )
        .into());
    }

    Ok(url)
}

fn timeout_error(timeout: Duration) -> Error {
    let timeout_secs = timeout.as_secs_f64();
    io::Error::new(
        io::ErrorKind::TimedOut,
        format!("Timeout error: request exceeded {timeout_secs}s"),
    )
    .into()
}

fn is_redirect(status: StatusCode) -> bool {
    matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308)
}

/// Sends a single HTTP request.
async fn send_http_request(
    client: HttpClient,
    inputs: &GenericHttpClientInputs,
    http_method: HttpMethod,
    body: Option<&Bytes>,
    url: &Uri,
    cross_origin: bool,
    body_dropped: bool,
) -> Result<http::Response<Body>, Error> {
    let request = build_request_for_url(url, http_method, inputs, body, cross_origin, body_dropped)?;
    client.send(request).await.map_err(Error::from)
}

/// Sends the request, following redirects when they are enabled.
///
/// When redirects are not followed, the request is sent exactly as it is configured. When they are
/// followed, the timeout applies to the entire redirect chain rather than to each request, and the
/// response returned is the last one of the chain.
async fn send_request_with_redirects(
    client: HttpClient,
    inputs: &GenericHttpClientInputs,
    http_method: HttpMethod,
    body: Option<Bytes>,
    initial_url: Uri,
) -> Result<http::Response<Body>, Error> {
    if !inputs.redirects.follow {
        let response = tokio::time::timeout(
            inputs.timeout,
            send_http_request(
                client,
                inputs,
                http_method,
                body.as_ref(),
                &initial_url,
                false,
                false,
            ),
        )
        .await
        .map_err(|_| timeout_error(inputs.timeout))??;

        return Ok(response);
    }

    let initial_url = parse_absolute_url(&initial_url)?;
    let initial_origin = initial_url.origin();
    let deadline = Instant::now() + inputs.timeout;

    let mut url = initial_url;
    let mut http_method = http_method;
    let mut body = body;
    let mut redirects = 0;
    let mut body_dropped = false;

    loop {
        let remaining_timeout = deadline.saturating_duration_since(Instant::now());
        if remaining_timeout.is_zero() {
            return Err(timeout_error(inputs.timeout));
        }

        let request_uri = url.as_str().parse::<Uri>()?;
        let cross_origin = url.origin() != initial_origin;

        let response = tokio::time::timeout(
            remaining_timeout,
            send_http_request(
                client.clone(),
                inputs,
                http_method,
                body.as_ref(),
                &request_uri,
                cross_origin,
                body_dropped,
            ),
        )
        .await
        .map_err(|_| timeout_error(inputs.timeout))??;

        let status = response.status();
        if !is_redirect(status) {
            return Ok(response);
        }

        if redirects >= inputs.redirects.max {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Exceeded the maximum of {} redirects", inputs.redirects.max),
            )
            .into());
        }

        let location = response
            .headers()
            .get(LOCATION)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Response {status} is missing the `Location` header"),
                )
            })?
            .to_str()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
            .to_owned();

        // The HTTP specification requires 301, 302 and 303 to change the method to GET for
        // everything but HEAD. 307 and 308 preserve the method and the body.
        if matches!(status.as_u16(), 301 | 302 | 303) && http_method != HttpMethod::Head {
            http_method = HttpMethod::Get;
            body = None;
            body_dropped = true;
        }

        debug!(
            message = "Following HTTP redirect.",
            from = %url,
            to = %location,
            status = status.as_u16(),
        );

        url = resolve_redirect(&url, &location)?;
        redirects += 1;
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "Splitting this source routine is deferred to keep the lint cleanup mechanical. https://github.com/vectordotdev/vector/issues/23659"
)]
/// Calls one or more urls at an interval.
///   - The HTTP request is built per the options in provided generic inputs.
///   - The HTTP response is decoded/parsed into events by the specific context.
///   - The events are then sent to the output stream.
pub(crate) async fn call<
    B: HttpClientBuilder<Context = C> + Send + Clone,
    C: HttpClientContext + Send,
>(
    inputs: GenericHttpClientInputs,
    context_builder: B,
    mut out: SourceSender,
    http_method: HttpMethod,
) -> vector_lib::source_sender::SendResult {
    // Building the HttpClient should not fail as it is just setting up the client with the
    // proxy and tls settings.
    let client =
        HttpClient::new(inputs.tls.clone(), &inputs.proxy).expect("Building HTTP client failed");
    let urls = inputs.urls.clone();
    let interval = inputs.interval;
    let shutdown = inputs.shutdown.clone();
    let inputs = Arc::new(inputs);

    let mut stream = IntervalStream::new(tokio::time::interval(interval))
        .take_until(shutdown)
        .map(move |_| stream::iter(urls.clone()))
        .flatten()
        .map(move |base_url| {
            let client = client.clone();
            let endpoint = base_url.to_string();
            let inputs = inputs.clone();

            let context_builder = context_builder.clone();
            let mut context = context_builder.build(&base_url);

            // Check if we need to process the URL dynamically (for updating VRL expressions)
            let request_url = context.process_url(&base_url).unwrap_or(base_url);

            // Get the request body from the context (if any)
            let body = context.get_request_body().map(Bytes::from);
            let redirect_url = request_url.clone();

            async move {
                send_request_with_redirects(client, &inputs, http_method, body, redirect_url).await
            }
            .and_then(move |response| {
                let endpoint = endpoint.clone();
                async move {
                    let (header, body) = response.into_parts();
                    let body = http_body::Body::collect(body).await?.to_bytes();
                    emit!(EndpointBytesReceived {
                        byte_size: body.len(),
                        protocol: "http",
                        endpoint: endpoint.as_str(),
                    });
                    Ok((header, body))
                }
            })
            .into_stream()
            .filter_map(move |response| {
                ready(match response {
                    Ok((header, body)) if header.status == hyper::StatusCode::OK => {
                        context
                            .on_response(&request_url, &header, &body)
                            .map(|mut events| {
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

                                emit!(HttpClientEventsReceived {
                                    byte_size,
                                    count: events.len(),
                                    url: request_url.to_string()
                                });

                                // We'll enrich after receiving the events so
                                // that the byte sizes are accurate.
                                context.enrich_events(&mut events);

                                stream::iter(events)
                            })
                    }
                    Ok((header, _)) => {
                        context.on_http_response_error(&request_url, &header);
                        emit!(HttpClientHttpResponseError {
                            code: header.status,
                            url: request_url.to_string(),
                        });
                        None
                    }
                    Err(error) => {
                        emit!(HttpClientHttpError {
                            error,
                            url: request_url.to_string()
                        });
                        None
                    }
                })
            })
            .flatten()
            .boxed()
        })
        .flatten_unordered(None)
        .boxed();

    match out.send_event_stream(&mut stream).await {
        Ok(()) => {
            debug!("Finished sending.");
            Ok(())
        }
        Err(error) => {
            let (count, _) = stream.size_hint();
            emit!(StreamClosedError { count });
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use http::header::COOKIE;
    use warp::{Filter, Reply};

    use super::*;
    use crate::test_util::{next_addr, wait_for_tcp};

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

    fn test_inputs(
        redirects: Redirects,
        headers: HeaderMap<HeaderValue>,
        auth: Option<Auth>,
    ) -> GenericHttpClientInputs {
        GenericHttpClientInputs {
            urls: Vec::new(),
            interval: default_interval(),
            timeout: default_timeout(),
            headers,
            redirects: redirects
                .resolve(false, DEFAULT_NOT_FORWARD_HEADERS)
                .expect("valid redirect configuration"),
            content_type: "text/plain".to_owned(),
            auth,
            tls: TlsSettings::from_options(None).expect("valid TLS settings"),
            proxy: ProxyConfig::default(),
            shutdown: ShutdownSignal::noop(),
        }
    }

    /// Replies with `method|body|authorization|cookie` so that tests can assert on what the server
    /// received.
    fn echo_request() -> impl Filter<Extract = (String,), Error = warp::Rejection> + Clone {
        warp::any()
            .and(warp::method())
            .and(warp::body::bytes())
            .and(warp::header::optional::<String>("authorization"))
            .and(warp::header::optional::<String>("cookie"))
            .map(
                |method: http::Method,
                 body: Bytes,
                 authorization: Option<String>,
                 cookie: Option<String>| {
                    format!(
                        "{}|{}|{}|{}",
                        method.as_str(),
                        String::from_utf8_lossy(&body),
                        authorization.unwrap_or_default(),
                        cookie.unwrap_or_default()
                    )
                },
            )
    }

    fn echo_host() -> impl Filter<Extract = (String,), Error = warp::Rejection> + Clone {
        warp::header::<String>("host")
    }

    async fn send(
        url: &str,
        method: HttpMethod,
        body: Option<&str>,
        inputs: &GenericHttpClientInputs,
    ) -> Result<(StatusCode, String), Error> {
        let client = HttpClient::new(inputs.tls.clone(), &inputs.proxy).expect("valid HTTP client");

        let response = send_request_with_redirects(
            client,
            inputs,
            method,
            body.map(|body| Bytes::copy_from_slice(body.as_bytes())),
            url.parse::<Uri>().expect("valid URL"),
        )
        .await?;

        let status = response.status();
        let body = http_body::Body::collect(response.into_body())
            .await?
            .to_bytes();

        Ok((status, String::from_utf8_lossy(&body).into_owned()))
    }

    /// Replies with the given status and a `Location` header pointing to `location`.
    fn redirect_to(
        status: StatusCode,
        location: &str,
    ) -> impl Filter<Extract = (warp::reply::Response,), Error = warp::Rejection> + Clone {
        let location = location.to_owned();
        warp::any().map(move || with_location(status, &location))
    }

    fn with_location(status: StatusCode, location: &str) -> warp::reply::Response {
        warp::reply::with_status(
            warp::reply::with_header("", "location", location.to_owned()),
            status,
        )
        .into_response()
    }

    #[test]
    fn body_framing_headers_are_only_dropped_after_method_rewrite() {
        let mut headers = HeaderMap::new();
        headers.insert(http::header::CONTENT_LENGTH, HeaderValue::from_static("7"));
        headers.insert(http::header::TRANSFER_ENCODING, HeaderValue::from_static("chunked"));
        headers.insert(http::header::CONTENT_TYPE, HeaderValue::from_static("text/plain"));
        let inputs = test_inputs(Redirects::default(), headers, None);
        let url = "http://example.com/target".parse::<Uri>().unwrap();
        let body = Bytes::from_static(b"payload");

        let initial = build_request_for_url(
            &url,
            HttpMethod::Post,
            &inputs,
            Some(&body),
            false,
            false,
        )
        .unwrap();
        assert_eq!(initial.headers()[http::header::CONTENT_LENGTH], "7");
        assert_eq!(initial.headers()[http::header::TRANSFER_ENCODING], "chunked");

        let redirected = build_request_for_url(&url, HttpMethod::Get, &inputs, None, false, true)
            .unwrap();
        assert!(!redirected.headers().contains_key(http::header::CONTENT_LENGTH));
        assert!(!redirected.headers().contains_key(http::header::TRANSFER_ENCODING));
        assert_eq!(redirected.headers()[http::header::CONTENT_TYPE], "text/plain");
    }

    #[tokio::test]
    async fn redirected_get_does_not_send_the_original_content_length() {
        let target = warp::path("target")
            .and(warp::method())
            .and(warp::header::optional::<String>("content-length"))
            .map(|method: http::Method, content_length: Option<String>| {
                format!("{}|{}", method.as_str(), content_length.unwrap_or_default())
            });
        let routes = warp::path("start")
            .and(redirect_to(StatusCode::FOUND, "/target"))
            .or(target);
        let (_guard, addr) = next_addr();
        tokio::spawn(warp::serve(routes).run(addr));
        wait_for_tcp(addr).await;

        let mut headers = HeaderMap::new();
        headers.insert(http::header::CONTENT_LENGTH, HeaderValue::from_static("7"));
        let inputs = test_inputs(
            Redirects {
                follow: Some(true),
                ..Default::default()
            },
            headers,
            None,
        );

        let (status, body) = send(
            &format!("http://{addr}/start"),
            HttpMethod::Post,
            Some("payload"),
            &inputs,
        )
        .await
        .expect("redirected request should succeed");

        assert_eq!(status, StatusCode::OK);
        assert!(body == "GET|" || body == "GET|0", "unexpected request: {body}");
    }

    #[tokio::test]
    async fn follows_temporary_redirect_and_switches_post_to_get() {
        let routes = warp::path("start")
            .and(redirect_to(StatusCode::FOUND, "/target"))
            .or(warp::path("target").and(echo_request()));
        let (_guard, addr) = next_addr();
        tokio::spawn(warp::serve(routes).run(addr));
        wait_for_tcp(addr).await;

        let inputs = test_inputs(
            Redirects {
                follow: Some(true),
                ..Default::default()
            },
            HeaderMap::new(),
            None,
        );

        let (status, body) = send(
            &format!("http://{addr}/start"),
            HttpMethod::Post,
            Some("payload"),
            &inputs,
        )
        .await
        .expect("request should succeed");

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "GET|||");
    }

    #[tokio::test]
    async fn preserves_method_and_body_on_permanent_redirect() {
        let routes = warp::path("start")
            .and(redirect_to(StatusCode::PERMANENT_REDIRECT, "/target"))
            .or(warp::path("target").and(echo_request()));
        let (_guard, addr) = next_addr();
        tokio::spawn(warp::serve(routes).run(addr));
        wait_for_tcp(addr).await;

        let inputs = test_inputs(
            Redirects {
                follow: Some(true),
                ..Default::default()
            },
            HeaderMap::new(),
            None,
        );

        let (status, body) = send(
            &format!("http://{addr}/start"),
            HttpMethod::Post,
            Some("payload"),
            &inputs,
        )
        .await
        .expect("request should succeed");

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "POST|payload||");
    }

    #[tokio::test]
    async fn does_not_follow_redirects_by_default() {
        let routes = warp::path("start").and(redirect_to(StatusCode::FOUND, "/target"));
        let (_guard, addr) = next_addr();
        tokio::spawn(warp::serve(routes).run(addr));
        wait_for_tcp(addr).await;

        let inputs = test_inputs(Redirects::default(), HeaderMap::new(), None);

        let (status, _) = send(
            &format!("http://{addr}/start"),
            HttpMethod::Get,
            None,
            &inputs,
        )
        .await
        .expect("request should succeed");

        assert_eq!(status, StatusCode::FOUND);
    }

    #[tokio::test]
    async fn stops_after_the_maximum_number_of_redirects() {
        let routes = warp::path("start").and(redirect_to(StatusCode::FOUND, "/start"));
        let (_guard, addr) = next_addr();
        tokio::spawn(warp::serve(routes).run(addr));
        wait_for_tcp(addr).await;

        let inputs = test_inputs(
            Redirects {
                follow: Some(true),
                max: 2,
                ..Default::default()
            },
            HeaderMap::new(),
            None,
        );

        let error = send(
            &format!("http://{addr}/start"),
            HttpMethod::Get,
            None,
            &inputs,
        )
        .await
        .expect_err("redirect loop should fail");

        assert!(
            error
                .to_string()
                .contains("Exceeded the maximum of 2 redirects"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn redirect_without_location_is_an_error() {
        let routes = warp::path("start").and(
            warp::any().map(|| warp::reply::with_status("", StatusCode::FOUND).into_response()),
        );
        let (_guard, addr) = next_addr();
        tokio::spawn(warp::serve(routes).run(addr));
        wait_for_tcp(addr).await;

        let inputs = test_inputs(
            Redirects {
                follow: Some(true),
                ..Default::default()
            },
            HeaderMap::new(),
            None,
        );

        let error = send(
            &format!("http://{addr}/start"),
            HttpMethod::Get,
            None,
            &inputs,
        )
        .await
        .expect_err("missing location should fail");

        assert!(
            error.to_string().contains("missing the `Location` header"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn resolves_relative_redirect_locations() {
        let routes = warp::path("start")
            .and(redirect_to(StatusCode::FOUND, "next"))
            .or(warp::path("next").and(echo_request()));
        let (_guard, addr) = next_addr();
        tokio::spawn(warp::serve(routes).run(addr));
        wait_for_tcp(addr).await;

        let inputs = test_inputs(
            Redirects {
                follow: Some(true),
                ..Default::default()
            },
            HeaderMap::new(),
            None,
        );

        let (status, body) = send(
            &format!("http://{addr}/start"),
            HttpMethod::Get,
            None,
            &inputs,
        )
        .await
        .expect("request should succeed");

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "GET|||");
    }

    #[tokio::test]
    async fn rejects_redirects_to_unsupported_schemes() {
        let routes = warp::path("start")
            .and(warp::any().map(|| with_location(StatusCode::FOUND, "file:///etc/passwd")));
        let (_guard, addr) = next_addr();
        tokio::spawn(warp::serve(routes).run(addr));
        wait_for_tcp(addr).await;

        let inputs = test_inputs(
            Redirects {
                follow: Some(true),
                ..Default::default()
            },
            HeaderMap::new(),
            None,
        );

        let error = send(
            &format!("http://{addr}/start"),
            HttpMethod::Get,
            None,
            &inputs,
        )
        .await
        .expect_err("unsupported scheme should fail");

        assert!(
            error.to_string().contains("unsupported scheme `file`"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn forwards_credentials_within_the_same_origin() {
        let routes = warp::path("start")
            .and(redirect_to(StatusCode::FOUND, "/target"))
            .or(warp::path("target").and(echo_request()));
        let (_guard, addr) = next_addr();
        tokio::spawn(warp::serve(routes).run(addr));
        wait_for_tcp(addr).await;

        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static("session=1"));

        let inputs = test_inputs(
            Redirects {
                follow: Some(true),
                ..Default::default()
            },
            headers,
            None,
        );

        let (status, body) = send(
            &format!("http://{addr}/start"),
            HttpMethod::Get,
            None,
            &inputs,
        )
        .await
        .expect("request should succeed");

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "GET|||session=1");
    }

    #[tokio::test]
    async fn preserves_configured_host_within_the_same_origin() {
        let routes = warp::path("start")
            .and(redirect_to(StatusCode::FOUND, "/target"))
            .or(warp::path("target").and(echo_host()));
        let (_guard, addr) = next_addr();
        tokio::spawn(warp::serve(routes).run(addr));
        wait_for_tcp(addr).await;

        let mut headers = HeaderMap::new();
        headers.insert(http::header::HOST, HeaderValue::from_static("original.example"));
        let inputs = test_inputs(
            Redirects {
                follow: Some(true),
                ..Default::default()
            },
            headers,
            None,
        );

        let (status, host) = send(
            &format!("http://{addr}/start"),
            HttpMethod::Get,
            None,
            &inputs,
        )
        .await
        .expect("redirected request should succeed");

        assert_eq!(status, StatusCode::OK);
        assert_eq!(host, "original.example");
    }

    #[tokio::test]
    async fn uses_destination_host_on_cross_origin_redirect() {
        let (_target_guard, target_addr) = next_addr();
        tokio::spawn(warp::serve(warp::path("target").and(echo_host())).run(target_addr));
        wait_for_tcp(target_addr).await;

        let routes = warp::path("start").and(redirect_to(
            StatusCode::FOUND,
            &format!("http://{target_addr}/target"),
        ));
        let (_guard, addr) = next_addr();
        tokio::spawn(warp::serve(routes).run(addr));
        wait_for_tcp(addr).await;

        let mut headers = HeaderMap::new();
        headers.insert(http::header::HOST, HeaderValue::from_static("original.example"));
        let inputs = test_inputs(
            Redirects {
                follow: Some(true),
                ..Default::default()
            },
            headers,
            None,
        );

        let (status, host) = send(
            &format!("http://{addr}/start"),
            HttpMethod::Get,
            None,
            &inputs,
        )
        .await
        .expect("redirected request should succeed");

        assert_eq!(status, StatusCode::OK);
        assert_eq!(host, target_addr.to_string());
    }

    #[tokio::test]
    async fn does_not_forward_credentials_across_origins() {
        let (_target_guard, target_addr) = next_addr();
        tokio::spawn(warp::serve(warp::path("target").and(echo_request())).run(target_addr));
        wait_for_tcp(target_addr).await;

        let routes = warp::path("start").and(redirect_to(
            StatusCode::FOUND,
            &format!("http://{target_addr}/target"),
        ));
        let (_guard, addr) = next_addr();
        tokio::spawn(warp::serve(routes).run(addr));
        wait_for_tcp(addr).await;

        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static("session=1"));

        let auth = Some(Auth::Basic {
            user: "user".to_owned(),
            password: "pass".to_owned().into(),
        });

        let inputs = test_inputs(
            Redirects {
                follow: Some(true),
                ..Default::default()
            },
            headers,
            auth,
        );

        let (status, body) = send(
            &format!("http://{addr}/start"),
            HttpMethod::Get,
            None,
            &inputs,
        )
        .await
        .expect("request should succeed");

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "GET|||");
    }

    #[tokio::test]
    async fn applies_the_timeout_to_the_whole_redirect_chain() {
        let routes = warp::path("start")
            .and(redirect_to(StatusCode::FOUND, "/slow"))
            .or(warp::path("slow").and_then(|| async {
                tokio::time::sleep(Duration::from_millis(500)).await;
                Ok::<_, warp::Rejection>("too late")
            }));
        let (_guard, addr) = next_addr();
        tokio::spawn(warp::serve(routes).run(addr));
        wait_for_tcp(addr).await;

        let mut inputs = test_inputs(
            Redirects {
                follow: Some(true),
                ..Default::default()
            },
            HeaderMap::new(),
            None,
        );
        inputs.timeout = Duration::from_millis(100);

        let error = send(
            &format!("http://{addr}/start"),
            HttpMethod::Get,
            None,
            &inputs,
        )
        .await
        .expect_err("the redirect chain should time out");

        assert!(
            error.to_string().contains("Timeout error"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn redirects_are_not_followed_by_default() {
        let resolved = Redirects::default()
            .resolve(false, DEFAULT_NOT_FORWARD_HEADERS)
            .expect("valid redirect configuration");

        assert!(!resolved.follow);
        assert_eq!(resolved.max, default_max_redirects());
        assert_eq!(
            resolved.not_forward_headers,
            vec![
                HeaderName::from_static("authorization"),
                HeaderName::from_static("www-authenticate"),
                HeaderName::from_static("cookie"),
                HeaderName::from_static("cookie2"),
                HeaderName::from_static("proxy-authorization"),
                HeaderName::from_static("proxy-authenticate"),
            ]
        );
    }

    #[test]
    fn partial_configuration_keeps_the_source_default() {
        let redirects: Redirects = toml::from_str("max = 2").expect("valid configuration");
        assert_eq!(redirects.max, 2);

        assert!(
            redirects
                .resolve(true, DEFAULT_NOT_FORWARD_HEADERS)
                .expect("valid redirect configuration")
                .follow
        );
        assert!(
            !redirects
                .resolve(false, DEFAULT_NOT_FORWARD_HEADERS)
                .expect("valid redirect configuration")
                .follow
        );
    }

    #[test]
    fn cross_origin_headers_can_be_customized() {
        let redirects: Redirects = toml::from_str(
            r#"
            [cross_origin]
            not_forward_headers = ["x-api-key"]
            "#,
        )
        .expect("valid configuration");

        let resolved = redirects
            .resolve(false, DEFAULT_NOT_FORWARD_HEADERS)
            .expect("valid redirect configuration");

        assert_eq!(
            resolved.not_forward_headers,
            vec![HeaderName::from_static("x-api-key")]
        );
    }

    #[test]
    fn invalid_not_forward_header_is_rejected() {
        let redirects: Redirects = toml::from_str(
            r#"
            [cross_origin]
            not_forward_headers = ["invalid header"]
            "#,
        )
        .expect("valid configuration");

        let error = redirects
            .resolve(false, DEFAULT_NOT_FORWARD_HEADERS)
            .expect_err("invalid header name should be rejected");

        assert!(error.to_string().contains("Invalid HTTP header name"));
    }
}
