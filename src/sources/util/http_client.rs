//! Common logic for sources that are HTTP clients.
//!
//! Specific HTTP client sources will:
//!   - Call build_url() to build the URL(s) to call.
//!   - Implement a specific context struct which:
//!       - Contains the data that source needs in order to process the HTTP responses into internal_events
//!       - Implements the HttpClient trait
//!   - Call call() supplying the generic inputs for calling and the source-specific
//!     context.

use bytes::Bytes;
use futures_util::{stream, FutureExt, StreamExt, TryFutureExt};
use http::{response::Parts, Uri};
use hyper::{Body, Request};
use std::time::Duration;
use std::{collections::HashMap, future::ready};
use tokio_stream::wrappers::IntervalStream;
use vector_lib::json_size::JsonSize;

use crate::http::QueryParameters;
use crate::{
    http::{Auth, HttpClient},
    internal_events::{
        EndpointBytesReceived, HttpClientEventsReceived, HttpClientHttpError,
        HttpClientHttpResponseError, StreamClosedError,
    },
    sources::util::http::HttpMethod,
    tls::TlsSettings,
    SourceSender,
};
use vector_lib::shutdown::ShutdownSignal;
use vector_lib::{config::proxy::ProxyConfig, event::Event, EstimatedJsonEncodedSizeOf};

/// Contains the inputs generic to any http client.
pub(crate) struct GenericHttpClientInputs {
    /// Array of URLs to call.
    pub urls: Vec<Uri>,
    /// Interval between calls.
    pub interval: Duration,
    /// Timeout for the HTTP request.
    pub timeout: Duration,
    /// Map of Header+Value to apply to HTTP request.
    pub headers: HashMap<String, Vec<String>>,
    /// Content type of the HTTP request, determined by the source.
    pub content_type: String,
    pub auth: Option<Auth>,
    pub tls: TlsSettings,
    pub proxy: ProxyConfig,
    pub shutdown: ShutdownSignal,
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
    };
    for (k, l) in query {
        for v in l {
            serializer.append_pair(k, v);
        }
    }
    let mut builder = Uri::builder();
    if let Some(scheme) = uri.scheme() {
        builder = builder.scheme(scheme.clone());
    };
    if let Some(authority) = uri.authority() {
        builder = builder.authority(authority.clone());
    };
    builder = builder.path_and_query(match serializer.finish() {
        query if !query.is_empty() => format!("{}?{}", uri.path(), query),
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
) -> Result<(), ()> {
    // Building the HttpClient should not fail as it is just setting up the client with the
    // proxy and tls settings.
    let client =
        HttpClient::new(inputs.tls.clone(), &inputs.proxy).expect("Building HTTP client failed");
    let mut stream = IntervalStream::new(tokio::time::interval(inputs.interval))
        .take_until(inputs.shutdown)
        .map(move |_| stream::iter(inputs.urls.clone()))
        .flatten()
        .map(move |url| {
            let client = client.clone();
            let endpoint = url.to_string();

            let context_builder = context_builder.clone();
            let mut context = context_builder.build(&url);

            let mut builder = match http_method {
                HttpMethod::Head => Request::head(&url),
                HttpMethod::Get => Request::get(&url),
                HttpMethod::Post => Request::post(&url),
                HttpMethod::Put => Request::put(&url),
                HttpMethod::Patch => Request::patch(&url),
                HttpMethod::Delete => Request::delete(&url),
                HttpMethod::Options => Request::options(&url),
            };

            // add user specified headers
            for (header, values) in &inputs.headers {
                for value in values {
                    builder = builder.header(header, value);
                }
            }

            // set ACCEPT header if not user specified
            if !inputs.headers.contains_key(http::header::ACCEPT.as_str()) {
                builder = builder.header(http::header::ACCEPT, &inputs.content_type);
            }

            // building an empty request should be infallible
            let mut request = builder.body(Body::empty()).expect("error creating request");

            if let Some(auth) = &inputs.auth {
                auth.apply(&mut request);
            }

            tokio::time::timeout(inputs.timeout, client.send(request))
                .then(move |result| async move {
                    match result {
                        Ok(Ok(response)) => Ok(response),
                        Ok(Err(error)) => Err(error.into()),
                        Err(_) => Err(format!(
                            "Timeout error: request exceeded {}s",
                            inputs.timeout.as_secs_f64()
                        )
                        .into()),
                    }
                })
                .and_then(|response| async move {
                    let (header, body) = response.into_parts();
                    let body = hyper::body::to_bytes(body).await?;
                    emit!(EndpointBytesReceived {
                        byte_size: body.len(),
                        protocol: "http",
                        endpoint: endpoint.as_str(),
                    });
                    Ok((header, body))
                })
                .into_stream()
                .filter_map(move |response| {
                    ready(match response {
                        Ok((header, body)) if header.status == hyper::StatusCode::OK => {
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

                                emit!(HttpClientEventsReceived {
                                    byte_size,
                                    count: events.len(),
                                    url: url.to_string()
                                });

                                // We'll enrich after receiving the events so
                                // that the byte sizes are accurate.
                                context.enrich_events(&mut events);

                                stream::iter(events)
                            })
                        }
                        Ok((header, _)) => {
                            context.on_http_response_error(&url, &header);
                            emit!(HttpClientHttpResponseError {
                                code: header.status,
                                url: url.to_string(),
                            });
                            None
                        }
                        Err(error) => {
                            emit!(HttpClientHttpError {
                                error,
                                url: url.to_string()
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
        Err(_) => {
            let (count, _) = stream.size_hint();
            emit!(StreamClosedError { count });
            Err(())
        }
    }
}
