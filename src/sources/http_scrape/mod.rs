//!
//!

#[cfg(all(unix, feature = "sources-http_scrape"))]
pub mod config;
#[cfg(all(unix, feature = "sources-http_scrape"))]
pub mod source;

pub use config::HttpScrapeConfig;

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
        EndpointBytesReceived, PrometheusEventsReceived, PrometheusHttpError,
        PrometheusHttpResponseError, RequestCompleted, StreamClosedError,
    },
    tls::TlsSettings,
    Error, SourceSender,
};
use vector_common::shutdown::ShutdownSignal;
use vector_core::{config::proxy::ProxyConfig, event::Event, ByteSizeOf};

///
pub trait HttpScraper {
    ///
    fn build(&mut self, _url: &Uri) {}

    ///
    fn on_response(&mut self, url: &Uri, header: &Parts, body: &Bytes) -> Option<Vec<Event>>;

    ///
    fn on_http_response_error(&self, _uri: &Uri, _header: &Parts) {}
}

///
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

///
pub(crate) async fn http_scrape<H: HttpScraper + std::marker::Send + Clone>(
    context: H,
    urls: Vec<Uri>,
    interval_secs: u64,
    auth: Option<Auth>,
    tls: TlsSettings,
    proxy: ProxyConfig,
    shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()> {
    let mut stream = IntervalStream::new(tokio::time::interval(Duration::from_secs(interval_secs)))
        .take_until(shutdown)
        .map(move |_| stream::iter(urls.clone()))
        .flatten()
        .map(move |url| {
            let client = HttpClient::new(tls.clone(), &proxy).expect("Building HTTP client failed");
            let endpoint = url.to_string();

            let mut context = context.clone();
            context.build(&url);

            let mut request = Request::get(&url)
                .header(http::header::ACCEPT, "text/plain")
                .body(Body::empty())
                .expect("error creating request");

            if let Some(auth) = &auth {
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
                                    // TODO emit EventsReceived (PrometheusEventsReceived)
                                    emit!(PrometheusEventsReceived {
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
                            emit!(PrometheusHttpResponseError {
                                code: header.status,
                                url: url.clone(),
                            });
                            None
                        }
                        Err(error) => {
                            emit!(PrometheusHttpError {
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
