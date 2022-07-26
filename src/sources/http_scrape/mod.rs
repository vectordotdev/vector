#[cfg(all(unix, feature = "sources-http_scrape"))]
pub mod config;
#[cfg(all(unix, feature = "sources-http_scrape"))]
pub mod source;

pub use config::HttpScrapeConfig;

//use crate::config::SinkContext;
use crate::{
    //codecs::{Decoder, DecodingConfig},
    //config::{GenerateConfig, SourceConfig, SourceContext, SourceDescription},
    //config::SourceConfig,
    http::{Auth, HttpClient},
    internal_events::{EndpointBytesReceived, StreamClosedError},
    sources,
    tls::TlsSettings,
    Error,
    SourceSender,
};
use async_trait::async_trait;
//use bytes::BytesMut;
//use codecs::decoding::{DeserializerConfig, FramingConfig};
use futures_util::{stream, FutureExt, StreamExt, TryFutureExt};
use hyper::{Body, Request};
use snafu::ResultExt;
use std::collections::HashMap;
//use std::future::ready;
use std::time::{Duration, Instant};
use tokio_stream::wrappers::IntervalStream;
//use tokio_util::codec::Decoder as _;
use vector_common::shutdown::ShutdownSignal;
//use vector_config::configurable_component;
//use vector_core::config::{proxy::ProxyConfig, LogNamespace, Output};
use vector_core::{config::proxy::ProxyConfig, event::Event};

fn get_urls(
    endpoints: &Vec<String>,
    query: Option<HashMap<String, Vec<String>>>,
) -> Result<Vec<http::Uri>, sources::BuildError> {
    endpoints
        .iter()
        .map(|s| s.parse::<http::Uri>().context(sources::UriParseSnafu))
        .map(|r| {
            r.map(|uri| {
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
                let mut builder = http::Uri::builder();
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
            })
        })
        .collect::<Result<Vec<http::Uri>, sources::BuildError>>()
}

#[async_trait]
pub trait HttpScrape {
    async fn pre_request_context(&self, url: &http::Uri);

    async fn post_request(
        &self,
        response: Result<
            (http::response::Parts, bytes::Bytes),
            Box<dyn snafu::Error + std::marker::Send + std::marker::Sync>,
        >,
    ) -> Option<futures_util::stream::Iter<std::vec::IntoIter<Event>>>;

    async fn http_scrape(
        &self,
        urls: &Vec<http::Uri>,
        interval_secs: u64,
        //decoder: Decoder,
        //url: http::Uri,
        auth: Option<Auth>,
        tls: TlsSettings,
        proxy: ProxyConfig,
        shutdown: ShutdownSignal,
        mut out: SourceSender,
    ) -> Result<(), ()> {
        let mut stream =
            IntervalStream::new(tokio::time::interval(Duration::from_secs(interval_secs)))
                .take_until(shutdown)
                .map(move |_| stream::iter(urls.clone()))
                .flatten()
                .map(move |url| {
                    let client =
                        HttpClient::new(tls.clone(), &proxy).expect("Building HTTP client failed");
                    let endpoint = url.to_string();

                    let mut request = Request::get(&url)
                        .header(http::header::ACCEPT, "text/plain")
                        .body(Body::empty())
                        .expect("error creating request");
                    if let Some(auth) = &auth {
                        auth.apply(&mut request);
                    }

                    self.pre_request_context(&url);

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
                        .filter_map(move |response| self.post_request(response))
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
}
