//! Generalized HTTP scrape source.
//! Scrapes an endpoint at an interval, decoding the HTTP responses into events.

use bytes::{Bytes, BytesMut};
use chrono::Utc;
use futures_util::FutureExt;
use http::{response::Parts, Uri};
use snafu::ResultExt;
use std::collections::HashMap;
use tokio_util::codec::Decoder as _;

use crate::{
    codecs::{Decoder, DecodingConfig},
    config::{SourceConfig, SourceContext, SourceDescription},
    http::Auth,
    serde::default_decoding,
    serde::default_framing_message_based,
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
    config::{log_schema, LogNamespace, Output},
    event::Event,
};

/// The name of this source
pub(crate) const NAME: &str = "http_scrape";

/// Configuration for the `http_scrape` source.
#[configurable_component(source)]
#[derive(Clone, Debug)]
pub struct HttpScrapeConfig {
    /// Endpoint to scrape events from.
    endpoint: String,

    /// The interval between scrapes, in seconds.
    #[serde(default = "super::default_scrape_interval_secs")]
    scrape_interval_secs: u64,

    /// Custom parameters for the scrape request query string.
    ///
    /// One or more values for the same parameter key can be provided. The parameters provided in this option are
    /// appended to any parameters manually provided in the `endpoint` option.
    query: Option<HashMap<String, Vec<String>>>,

    /// Decoder to use on the HTTP responses.
    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    decoding: DeserializerConfig,

    /// Framing to use in the decoding.
    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    framing: FramingConfig,

    /// Headers to apply to the HTTP requests.
    #[serde(default)]
    headers: Option<HashMap<String, String>>,

    /// TLS configuration.
    #[configurable(derived)]
    tls: Option<TlsConfig>,

    /// HTTP Authentication.
    #[configurable(derived)]
    auth: Option<Auth>,
}

impl Default for HttpScrapeConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:9898/logs".to_string(),
            query: None,
            scrape_interval_secs: super::default_scrape_interval_secs(),
            decoding: default_decoding(),
            framing: default_framing_message_based(),
            headers: None,
            tls: None,
            auth: None,
        }
    }
}

#[allow(clippy::too_many_arguments)]
impl HttpScrapeConfig {
    pub const fn new(
        endpoint: String,
        scrape_interval_secs: u64,
        query: Option<HashMap<String, Vec<String>>>,
        decoding: DeserializerConfig,
        framing: FramingConfig,
        headers: Option<HashMap<String, String>>,
        tls: Option<TlsConfig>,
        auth: Option<Auth>,
    ) -> Self {
        Self {
            endpoint,
            scrape_interval_secs,
            query,
            decoding,
            framing,
            headers,
            tls,
            auth,
        }
    }
}

inventory::submit! {
    SourceDescription::new::<HttpScrapeConfig>(NAME)
}

impl_generate_config_from_default!(HttpScrapeConfig);

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
            self.framing.clone(),
            self.decoding.clone(),
            LogNamespace::Vector,
        )
        .build();

        let content_type = self.decoding.content_type(&self.framing).to_string();

        // the only specific context needed is the codec decoding
        let context = HttpScrapeContext { decoder };

        let inputs = super::GenericHttpScrapeInputs {
            urls,
            interval_secs: self.scrape_interval_secs,
            headers: self.headers.clone(),
            content_type,
            auth: self.auth.clone(),
            tls,
            proxy: cx.proxy.clone(),
            shutdown: cx.shutdown,
        };

        Ok(super::http_scrape(inputs, context, cx.out).boxed())
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<Output> {
        vec![Output::default(self.decoding.output_type())]
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

impl HttpScrapeContext {
    /// Decode the events from the byte buffer
    fn decode_events(&mut self, buf: &mut BytesMut) -> Vec<Event> {
        let mut events = Vec::new();
        loop {
            match self.decoder.decode_eof(buf) {
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
        events
    }

    /// Enriches events with source_type, timestamp
    fn enrich_events(&self, events: &mut Vec<Event>) {
        for event in events {
            match event {
                Event::Log(ref mut log) => {
                    log.try_insert(log_schema().source_type_key(), Bytes::from(NAME));
                    log.try_insert(log_schema().timestamp_key(), Utc::now());
                }
                Event::Metric(ref mut metric) => {
                    metric.insert_tag(log_schema().source_type_key().to_string(), NAME.to_string());
                }
                Event::Trace(ref mut trace) => {
                    trace.insert(log_schema().source_type_key(), Bytes::from(NAME));
                }
            }
        }
    }
}

impl super::HttpScraper for HttpScrapeContext {
    /// Decodes the HTTP response body into events per the decoder configured.
    fn on_response(
        &mut self,
        _url: &http::Uri,
        _header: &Parts,
        body: &Bytes,
    ) -> Option<Vec<Event>> {
        // get the body into a byte array
        let mut buf = BytesMut::new();
        let body = String::from_utf8_lossy(body);
        buf.extend_from_slice(body.as_bytes());

        // decode and enrich
        let mut events = self.decode_events(&mut buf);
        self.enrich_events(&mut events);

        Some(events)
    }
}
