//! Common logic for sources that are HTTP scrapers.
//!
//! Specific HTTP scraping sources will:
//!   - Call build_url() to build the URL(s) to scrape.
//!   - Implmement a specific context struct which:
//!       - Contains the data that source needs in order to process the HTTP responses into internal_events
//!       - Implements the HttpScraper trait
//!   - Call http_scrape() supplying the generic inputs for scraping and the source-specific
//!     context.

use bytes::Bytes;
use futures_util::{stream, FutureExt, StreamExt, TryFutureExt};
use http::{response::Parts, Uri};
use hyper::{Body, Request};
use std::time::{Duration, Instant};
use std::{collections::HashMap, future::ready};
use tokio_stream::wrappers::IntervalStream;

use crate::{
    http::{Auth, HttpClient},
    internal_events::{
        EndpointBytesReceived, HttpScrapeEventsReceived, HttpScrapeHttpError,
        HttpScrapeHttpResponseError, RequestCompleted, StreamClosedError,
    },
    tls::TlsSettings,
    Error, SourceSender,
};
use vector_common::shutdown::ShutdownSignal;
use vector_core::{config::proxy::ProxyConfig, event::Event, ByteSizeOf};

/// Contains the inputs generic to any http scrape.
pub(crate) struct GenericHttpScrapeInputs {
    /// Array of URLs to scrape
    pub urls: Vec<Uri>,
    /// Interval to scrape on in seconds
    pub interval_secs: u64,
    /// Map of Header+Value to apply to HTTP request
    pub headers: HashMap<String, Vec<String>>,
    /// Content type of the HTTP request, determined by the source
    pub content_type: String,
    pub auth: Option<Auth>,
    pub tls: TlsSettings,
    pub proxy: ProxyConfig,
    pub shutdown: ShutdownSignal,
}

/// The default interval to scrape the http endpoint if none is configured.
pub(crate) const fn default_scrape_interval_secs() -> u64 {
    15
}

/// Builds the context, allowing the source-specific implementation to leverage data from the
/// config and the current HTTP request.
pub(crate) trait HttpScraperBuilder {
    type Context: HttpScraperContext;

    /// Called before the HTTP request is made to build out the context.
    fn build(&self, url: &Uri) -> Self::Context;
}

/// Methods that allow context-specific behavior during the scraping procedure.
pub(crate) trait HttpScraperContext {
    /// Called after the HTTP request succeeds and returns the decoded/parsed Event array.
    fn on_response(&mut self, url: &Uri, header: &Parts, body: &Bytes) -> Option<Vec<Event>>;

    /// (Optional) Called if the HTTP response is not 200 ('OK').
    fn on_http_response_error(&self, _uri: &Uri, _header: &Parts) {}
}

/// Builds a url for the HTTP requests.
pub(crate) fn build_url(uri: &Uri, query: &HashMap<String, Vec<String>>) -> Uri {
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

/// Scrapes one or more urls at an interval.
///   - The HTTP request is built per the options in provided generic inputs.
///   - The HTTP response is decoded/parsed into events by the specific context.
///   - The events are then sent to the output stream.
pub(crate) async fn http_scrape<
    B: HttpScraperBuilder<Context = C> + std::marker::Send + Clone,
    C: HttpScraperContext + std::marker::Send,
>(
    inputs: GenericHttpScrapeInputs,
    context_builder: B,
    mut out: SourceSender,
) -> Result<(), ()> {
    let mut stream = IntervalStream::new(tokio::time::interval(Duration::from_secs(
        inputs.interval_secs,
    )))
    .take_until(inputs.shutdown)
    .map(move |_| stream::iter(inputs.urls.clone()))
    .flatten()
    .map(move |url| {
        // Building the HttpClient should not fail as it is just setting up the client with the
        // proxy and tls settings.
        let client = HttpClient::new(inputs.tls.clone(), &inputs.proxy)
            .expect("Building HTTP client failed");
        let endpoint = url.to_string();

        let context_builder = context_builder.clone();
        let mut context = context_builder.build(&url);

        let mut builder = Request::get(&url);

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

        let start = Instant::now();
        client
            .send(request)
            .map_err(Error::from)
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
                        emit!(RequestCompleted {
                            start,
                            end: Instant::now()
                        });
                        context.on_response(&url, &header, &body).map(|events| {
                            emit!(HttpScrapeEventsReceived {
                                byte_size: events.size_of(),
                                count: events.len(),
                                url: url.to_string()
                            });
                            stream::iter(events)
                        })
                    }
                    Ok((header, _)) => {
                        context.on_http_response_error(&url, &header);
                        emit!(HttpScrapeHttpResponseError {
                            code: header.status,
                            url: url.to_string(),
                        });
                        None
                    }
                    Err(error) => {
                        emit!(HttpScrapeHttpError {
                            error,
                            url: url.to_string()
                        });
                        None
                    }
                })
            })
            .flatten()
    })
    .flatten()
    .boxed();

    match out.send_event_stream(&mut stream).await {
        Ok(()) => {
            debug!("Finished sending.");
            Ok(())
        }
        Err(error) => {
            let (count, _) = stream.size_hint();
            emit!(StreamClosedError { error, count });
            Err(())
        }
    }
}
