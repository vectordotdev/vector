//!
//!

use bytes::{Bytes, BytesMut};
use futures_util::FutureExt;
use http::{response::Parts, Uri};
use snafu::ResultExt;
use std::collections::HashMap;
use tokio_util::codec::Decoder as _;

use crate::{
    codecs::{Decoder, DecodingConfig},
    config::{self, GenerateConfig, SourceConfig, SourceContext, SourceDescription},
    http::Auth,
    serde::default_decoding,
    sources,
    tls::{TlsConfig, TlsSettings},
    Result,
};
use codecs::{
    decoding::{DeserializerConfig, FramingConfig},
    StreamDecodingError,
};
use vector_config::configurable_component;
use vector_core::{
    config::{LogNamespace, Output},
    event::Event,
};

const NAME: &str = "http_scrape";

/// Configuration for the `http_scrape` source.
#[configurable_component(source)]
#[derive(Clone, Debug)]
pub struct HttpScrapeConfig {
    /// Endpoints to scrape metrics from.
    endpoint: String,

    /// Custom parameters for the scrape request query string.
    ///
    /// One or more values for the same parameter key can be provided. The parameters provided in this option are
    /// appended to any parameters manually provided in the `endpoint` option.
    query: Option<HashMap<String, Vec<String>>>,

    /// The interval between scrapes, in seconds.
    #[serde(default = "default_scrape_interval_secs")]
    scrape_interval_secs: u64,

    /// TODO
    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    decoding: DeserializerConfig,

    /// TODO
    #[configurable(derived)]
    framing: Option<FramingConfig>,

    /// TODO
    #[serde(default)]
    headers: Option<Vec<String>>,

    /// TODO
    #[configurable(derived)]
    tls: Option<TlsConfig>,

    /// TODO
    #[configurable(derived)]
    auth: Option<Auth>,
}

pub(crate) const fn default_scrape_interval_secs() -> u64 {
    15
}

inventory::submit! {
    SourceDescription::new::<HttpScrapeConfig>(NAME)
}

impl GenerateConfig for HttpScrapeConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            endpoint: "http://localhost:9090/metrics".to_string(),
            query: None,
            scrape_interval_secs: default_scrape_interval_secs(),
            decoding: default_decoding(),
            framing: None,
            headers: None,
            tls: None,
            auth: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "http_scrape")]
impl SourceConfig for HttpScrapeConfig {
    async fn build(&self, cx: SourceContext) -> Result<sources::Source> {
        let endpoints = vec![self.endpoint.clone()];
        let urls = endpoints
            .iter()
            .map(|s| s.parse::<Uri>().context(sources::UriParseSnafu))
            .map(|r| r.map(|uri| super::get_url(&uri, &self.query)))
            .collect::<std::result::Result<Vec<Uri>, sources::BuildError>>()?;

        let tls = TlsSettings::from_options(&self.tls)?;

        let decoder = DecodingConfig::new(
            self.framing
                .clone()
                .unwrap_or_else(|| self.decoding.default_stream_framing()),
            self.decoding.clone(),
            LogNamespace::Vector,
        )
        .build();

        let context = HttpScrapeContext { decoder };

        Ok(super::http_scrape(
            context,
            urls,
            self.scrape_interval_secs,
            self.auth.clone(),
            tls,
            cx.proxy.clone(),
            cx.shutdown,
            cx.out,
        )
        .boxed())
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<Output> {
        vec![Output::default(config::DataType::Metric)]
    }

    fn source_type(&self) -> &'static str {
        NAME
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

#[derive(Clone)]
struct HttpScrapeContext {
    decoder: Decoder,
}

impl super::HttpScraper for HttpScrapeContext {
    ///
    fn on_response(
        &mut self,
        _url: &http::Uri,
        _header: &Parts,
        body: &Bytes,
    ) -> Option<Vec<Event>> {
        let body = String::from_utf8_lossy(&body);
        dbg!(&body);

        let mut events = Vec::new();
        let mut bytes = BytesMut::new();
        bytes.extend_from_slice(body.as_bytes());

        loop {
            match self.decoder.decode_eof(&mut bytes) {
                Ok(Some((next, _))) => {
                    events.extend(next.into_iter());
                }
                Ok(None) => break,
                Err(error) => {
                    // Error is logged by `crate::codecs::Decoder`, no further
                    // handling is needed here.
                    if !error.can_continue() {
                        break;
                    }
                    break;
                }
            }
        }
        for event in &events {
            dbg!(event);
        }
        Some(events)
    }
}
//async fn http_scrape(
//    config: HttpScrapeConfig,
//    decoder: Decoder,
//    url: http::Uri,
//    tls: TlsSettings,
//    proxy: ProxyConfig,
//    shutdown: ShutdownSignal,
//    mut out: SourceSender,
//) -> Result<(), ()> {
//    let mut stream = IntervalStream::new(tokio::time::interval(Duration::from_secs(
//        config.scrape_interval_secs,
//    )))
//    .take_until(shutdown)
//    .map(move |_| stream::iter(vec![url.clone()]))
//    .flatten()
//    .map(move |url| {
//        let client = HttpClient::new(tls.clone(), &proxy).expect("Building HTTP client failed");
//        let endpoint = url.to_string();
//        let mut decoder = decoder.clone();
//
//        let mut request = Request::get(&url)
//            .header(http::header::ACCEPT, "text/plain")
//            .body(Body::empty())
//            .expect("error creating request");
//        if let Some(auth) = &config.auth {
//            auth.apply(&mut request);
//        }
//
//        let start = Instant::now();
//        client
//            .send(request)
//            .map_err(Error::from)
//            .and_then(|response| async move {
//                let (header, body) = response.into_parts();
//                let body = hyper::body::to_bytes(body).await?;
//                emit!(EndpointBytesReceived {
//                    byte_size: body.len(),
//                    protocol: "http",
//                    endpoint: endpoint.as_str(),
//                });
//                Ok((header, body))
//            })
//            .into_stream()
//            .filter_map(move |response| {
//                ready(match response {
//                    Ok((header, body)) if header.status == hyper::StatusCode::OK => {
//                        emit!(RequestCompleted {
//                            start,
//                            end: Instant::now()
//                        });
//                        let body = String::from_utf8_lossy(&body);
//                        dbg!(&body);
//
//                        let mut events = Vec::new();
//                        let mut bytes = BytesMut::new();
//                        bytes.extend_from_slice(body.as_bytes());
//
//                        loop {
//                            match decoder.decode_eof(&mut bytes) {
//                                Ok(Some((next, _))) => {
//                                    events.extend(next.into_iter());
//                                }
//                                Ok(None) => break,
//                                Err(error) => {
//                                    // Error is logged by `crate::codecs::Decoder`, no further
//                                    // handling is needed here.
//                                    if !error.can_continue() {
//                                        break;
//                                    }
//                                    break;
//                                }
//                            }
//                        }
//                        for event in &events {
//                            dbg!(event);
//                        }
//                        // TODO emit EventsReceived (PrometheusEventsReceived)
//                        Some(stream::iter(events))
//                    }
//                    Ok((_header, _)) => {
//                        // emit!(PrometheusHttpResponseError {
//                        //     code: header.status,
//                        //     url: url.clone(),
//                        // });
//                        println!("error 1");
//                        None
//                    }
//                    Err(_error) => {
//                        // emit!(PrometheusHttpError {
//                        //     error,
//                        //     url: url.clone(),
//                        // });
//                        println!("error 2");
//                        None
//                    }
//                })
//            })
//            .flatten()
//    })
//    .flatten()
//    .boxed();
//
//    match out.send_event_stream(&mut stream).await {
//        Ok(()) => {
//            info!("Finished sending.");
//            Ok(())
//        }
//        Err(error) => {
//            let (count, _) = stream.size_hint();
//            emit!(StreamClosedError { error, count });
//            Err(())
//        }
//    }
//}

#[cfg(test)]
mod test {
    use tokio::time::Duration;
    use warp::Filter;

    use super::*;
    use crate::test_util::{
        components::{run_and_assert_source_compliance, HTTP_PULL_SOURCE_TAGS},
        next_addr, test_generate_config,
    };

    #[test]
    fn generate_config() {
        test_generate_config::<HttpScrapeConfig>();
    }

    #[tokio::test]
    async fn test_() {
        let in_addr = next_addr();

        let dummy_endpoint = warp::path!("metrics")
            .and(warp::header::exact("Accept", "text/plain"))
            .map(|| r#"A plain text event"#);

        tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));

        let config = HttpScrapeConfig {
            endpoint: format!("http://{}/metrics", in_addr),
            scrape_interval_secs: 1,
            query: None,
            decoding: default_decoding(),
            framing: None,
            headers: None,
            auth: None,
            tls: None,
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(1),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert!(!events.is_empty());
    }
}
