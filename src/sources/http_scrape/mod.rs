//! Common logic for sources that are HTTP scrapers.
//!
//! Specific HTTP scraping sources will:
//!   - Call get_url() to build the URL(s) to scrape.
//!   - Implmement a specific context struct which:
//!       - Contains the data that source needs in order to process the HTTP responses into internal_events
//!       - Implements the HttpScraper trait
//!   - Call http_scrape() supplying the generic inputs for scraping and the source-specific
//!     context.

#[cfg(all(unix, feature = "sources-http_scrape"))]
pub mod scrape;

pub use scrape::HttpScrapeConfig;

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
    urls: Vec<Uri>,
    interval_secs: u64,
    headers: Option<HashMap<String, String>>,
    auth: Option<Auth>,
    tls: TlsSettings,
    proxy: ProxyConfig,
    shutdown: ShutdownSignal,
}

impl GenericHttpScrapeInputs {
    pub fn new(
        urls: Vec<Uri>,
        interval_secs: u64,
        headers: Option<HashMap<String, String>>,
        auth: Option<Auth>,
        tls: TlsSettings,
        proxy: ProxyConfig,
        shutdown: ShutdownSignal,
    ) -> Self {
        Self {
            urls,
            interval_secs,
            headers,
            auth,
            tls,
            proxy,
            shutdown,
        }
    }
}

/// The default interval to scrape the http endpoint if none is configured.
pub(crate) const fn default_scrape_interval_secs() -> u64 {
    15
}

/// Methods that allow context-specific behavior during the scraping procedure.
pub(crate) trait HttpScraper {
    /// (Optional) Called before the HTTP request is made, allows building context.
    fn build(&mut self, _url: &Uri) {}

    /// Called after the HTTP request succeeds and returns the decoded/parsed Event array.
    fn on_response(&mut self, url: &Uri, header: &Parts, body: &Bytes) -> Option<Vec<Event>>;

    /// (Optional) Called if the HTTP response is not 200 ('OK').
    fn on_http_response_error(&self, _uri: &Uri, _header: &Parts) {}
}

/// Builds a url for the HTTP requests.
pub(crate) fn get_url(uri: &Uri, query: &Option<HashMap<String, Vec<String>>>) -> Uri {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    if let Some(query) = uri.query() {
        serializer.extend_pairs(url::form_urlencoded::parse(query.as_bytes()));
    };
    if let Some(query) = &query {
        for (k, l) in query {
            for v in l {
                serializer.append_pair(k, v);
            }
        }
    };
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
    builder.build().expect("error building URI")
}

/// Scrapes one or more urls at an interval.
///   - The HTTP request is built per the options in provided generic inputs.
///   - The HTTP response is decoded/parsed into events by the specific context.
///   - The events are then sent to the output stream.
pub(crate) async fn http_scrape<H: HttpScraper + std::marker::Send + Clone>(
    inputs: GenericHttpScrapeInputs,
    context: H,
    mut out: SourceSender,
) -> Result<(), ()> {
    let mut stream = IntervalStream::new(tokio::time::interval(Duration::from_secs(
        inputs.interval_secs,
    )))
    .take_until(inputs.shutdown)
    .map(move |_| stream::iter(inputs.urls.clone()))
    .flatten()
    .map(move |url| {
        let client = HttpClient::new(inputs.tls.clone(), &inputs.proxy)
            .expect("Building HTTP client failed");
        let endpoint = url.to_string();

        let mut context = context.clone();
        context.build(&url);

        let mut builder = Request::get(&url).header(http::header::ACCEPT, "text/plain");

        // add user supplied headers
        if let Some(headers) = &inputs.headers {
            for header in headers {
                builder = builder.header(header.0, header.1);
            }
        }
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
                        match context.on_response(&url, &header, &body) {
                            Some(events) => {
                                emit!(HttpScrapeEventsReceived {
                                    byte_size: events.size_of(),
                                    count: events.len(),
                                    uri: url.clone()
                                });
                                Some(stream::iter(events))
                            }
                            None => None,
                        }
                    }
                    Ok((header, _)) => {
                        context.on_http_response_error(&url, &header);
                        emit!(HttpScrapeHttpResponseError {
                            code: header.status,
                            url: url.clone(),
                        });
                        None
                    }
                    Err(error) => {
                        emit!(HttpScrapeHttpError {
                            error,
                            url: url.clone(),
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
            info!("Finished sending.");
            Ok(())
        }
        Err(error) => {
            let (count, _) = stream.size_hint();
            emit!(StreamClosedError { error, count });
            Err(())
        }
    }
}
