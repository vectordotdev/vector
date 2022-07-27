//! Generalized HTTP scrape source.
//! Scrapes an endpoint at an interval, decoding the HTTP responses into events.

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

/// The name of this source
const NAME: &str = "http_scrape";

// TODO:
//   - request headers
//   - framing for the decoding?
//   - cue files

/// Configuration for the `http_scrape` source.
#[configurable_component(source)]
#[derive(Clone, Debug)]
pub struct HttpScrapeConfig {
    /// Endpoint to scrape events from.
    endpoint: String,

    /// Custom parameters for the scrape request query string.
    ///
    /// One or more values for the same parameter key can be provided. The parameters provided in this option are
    /// appended to any parameters manually provided in the `endpoint` option.
    query: Option<HashMap<String, Vec<String>>>,

    /// The interval between scrapes, in seconds.
    #[serde(default = "super::default_scrape_interval_secs")]
    scrape_interval_secs: u64,

    /// Decoder to use on the HTTP responses.
    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    decoding: DeserializerConfig,

    /// Framing to use in the decoding.
    #[configurable(derived)]
    framing: Option<FramingConfig>,

    /// Headers to apply to the HTTP requests.
    #[serde(default)]
    headers: Option<Vec<String>>,

    /// TLS configuration.
    #[configurable(derived)]
    tls: Option<TlsConfig>,

    /// HTTP Authentication.
    #[configurable(derived)]
    auth: Option<Auth>,
}

inventory::submit! {
    SourceDescription::new::<HttpScrapeConfig>(NAME)
}

impl GenerateConfig for HttpScrapeConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            endpoint: "http://localhost:9898/logs".to_string(),
            query: None,
            scrape_interval_secs: super::default_scrape_interval_secs(),
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
        // build the url
        let endpoints = vec![self.endpoint.clone()];
        let urls = endpoints
            .iter()
            .map(|s| s.parse::<Uri>().context(sources::UriParseSnafu))
            .map(|r| r.map(|uri| super::get_url(&uri, &self.query)))
            .collect::<std::result::Result<Vec<Uri>, sources::BuildError>>()?;

        let tls = TlsSettings::from_options(&self.tls)?;

        // build the decoder
        let decoder = DecodingConfig::new(
            self.framing
                .clone()
                .unwrap_or_else(|| self.decoding.default_stream_framing()),
            self.decoding.clone(),
            LogNamespace::Vector,
        )
        .build();

        // the only specific context needed is the ability to decode
        let context = HttpScrapeContext { decoder };

        let inputs = super::GenericHttpScrapeInputs::new(
            urls,
            self.scrape_interval_secs,
            self.auth.clone(),
            tls,
            cx.proxy.clone(),
            cx.shutdown,
        );

        Ok(super::http_scrape(inputs, context, cx.out).boxed())
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
    /// Decodes the HTTP response body into events per the decoder configured.
    fn on_response(
        &mut self,
        _url: &http::Uri,
        _header: &Parts,
        body: &Bytes,
    ) -> Option<Vec<Event>> {
        let mut bytes = BytesMut::new();
        let body = String::from_utf8_lossy(body);
        bytes.extend_from_slice(body.as_bytes());

        let mut events = Vec::new();
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
        Some(events)
    }
}

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
    fn test_http_scrape_generate_config() {
        test_generate_config::<HttpScrapeConfig>();
    }

    #[tokio::test]
    async fn test_http_scrape_bytes_decoding() {
        let in_addr = next_addr();

        let dummy_endpoint = warp::path!("endpoint")
            .and(warp::header::exact("Accept", "text/plain"))
            .map(|| r#"A plain text event"#);

        tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));

        let config = HttpScrapeConfig {
            endpoint: format!("http://{}/endpoint", in_addr),
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

    #[tokio::test]
    async fn test_http_scrape_json_decoding() {
        let in_addr = next_addr();

        let dummy_endpoint = warp::path!("endpoint")
            .and(warp::header::exact("Accept", "text/plain"))
            .map(|| r#"{"data" : "foo"}"#);

        tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));

        let config = HttpScrapeConfig {
            endpoint: format!("http://{}/endpoint", in_addr),
            scrape_interval_secs: 1,
            query: None,
            decoding: DeserializerConfig::Json,
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

    #[tokio::test]
    async fn test_http_scrape_request_query() {
        let in_addr = next_addr();

        let dummy_endpoint = warp::path!("endpoint")
            .and(warp::query::raw())
            .map(|query| format!(r#"{{"data" : "{}"}}"#, query));

        tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));

        let config = HttpScrapeConfig {
            endpoint: format!("http://{}/endpoint?key1=val1", in_addr),
            scrape_interval_secs: 1,
            query: Some(HashMap::from([
                ("key1".to_string(), vec!["val2".to_string()]),
                (
                    "key2".to_string(),
                    vec!["val1".to_string(), "val2".to_string()],
                ),
            ])),
            decoding: DeserializerConfig::Json,
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

        let logs: Vec<_> = events.into_iter().map(|event| event.into_log()).collect();

        let expected = HashMap::from([
            (
                "key1".to_string(),
                vec!["val1".to_string(), "val2".to_string()],
            ),
            (
                "key2".to_string(),
                vec!["val1".to_string(), "val2".to_string()],
            ),
        ]);

        for log in logs {
            let query = log.get("data").expect("data must be available");
            let mut got: HashMap<String, Vec<String>> = HashMap::new();
            for (k, v) in url::form_urlencoded::parse(
                query.as_bytes().expect("byte conversion should succeed"),
            ) {
                got.entry(k.to_string())
                    .or_insert_with(Vec::new)
                    .push(v.to_string());
            }
            for v in got.values_mut() {
                v.sort();
            }
            assert_eq!(got, expected);
        }
    }
}
