use std::collections::HashMap;
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use chrono::Utc;
use futures_util::FutureExt;
use http::Uri;
use http::response::Parts as ResponseParts;
use http::uri::Parts as UriParts;
use percent_encoding::utf8_percent_encode;
use serde_with::serde_as;
use tokio_util::codec::Decoder as _;
use vector_lib::{
    codecs::{
        StreamDecodingError,
        decoding::{DeserializerConfig, FramingConfig},
    },
    config::LogNamespace,
    configurable::configurable_component,
    event::Event,
};

use crate::{
    Result,
    codecs::{Decoder, DecodingConfig},
    config::{SourceConfig, SourceContext, SourceOutput},
    serde::{default_decoding, default_framing_message_based},
    sources,
    sources::util::{
        http::HttpMethod,
        http_client::{
            GenericHttpClientInputs, HttpClientBuilder, HttpClientContext, call, default_interval,
            default_timeout, warn_if_interval_too_low,
        },
    },
    tls::{TlsConfig, TlsSettings},
};

/// Configuration for the `okta` source.
#[serde_as]
#[configurable_component(source("okta", "Pull Okta system logs via the Okta API",))]
#[derive(Clone, Debug)]
pub struct OktaConfig {
    /// Decoder to use on each received message.
    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    pub decoding: DeserializerConfig,

    /// Framing to use in the decoding.
    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    pub framing: FramingConfig,

    /// The Okta subdomain to scrape
    #[configurable(metadata(docs::examples = "foo.okta.com"))]
    pub domain: String,

    /// API token for authentication
    #[configurable(metadata(docs::examples = "00xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"))]
    pub token: String,

    /// The interval between scrapes. Requests are run concurrently so if a scrape takes longer
    /// than the interval, a new scrape will be started. This can take extra resources, set the timeout
    /// to a value lower than the scrape interval to prevent this from happening.
    #[serde(default = "default_interval")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[serde(rename = "scrape_interval_secs")]
    #[configurable(metadata(docs::human_name = "Scrape Interval"))]
    pub interval: Duration,

    /// The time to look back for logs. This is used to determine the start time of the first
    /// request (that is, the earliest log to fetch)
    #[configurable(metadata(docs::human_name = "Since (seconds before now)"))]
    pub since: Option<u64>,

    /// The timeout for each scrape request.
    #[serde(default = "default_timeout")]
    #[serde_as(as = "serde_with::DurationSecondsWithFrac<f64>")]
    #[serde(rename = "scrape_timeout_secs")]
    #[configurable(metadata(docs::human_name = "Scrape Timeout"))]
    pub timeout: Duration,

    /// TLS configuration.
    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,
}

impl Default for OktaConfig {
    fn default() -> Self {
        Self {
            decoding: default_decoding(),
            framing: default_framing_message_based(),
            domain: "".to_string(),
            token: "".to_string(),
            interval: default_interval(),
            timeout: default_timeout(),
            since: None,
            tls: None,
            log_namespace: None,
        }
    }
}

impl_generate_config_from_default!(OktaConfig);

/// Request-specific context for Okta API scraping.
#[derive(Clone)]
struct OktaContext {
    decoder: Decoder,
    interval: Duration,
    since: Option<u64>,
}

impl OktaContext {
    /// Decode the events from the byte buffer
    fn decode_events(&mut self, buf: &mut BytesMut) -> Vec<Event> {
        let mut events = Vec::new();
        loop {
            match self.decoder.decode_eof(buf) {
                Ok(Some((next, _))) => {
                    events.extend(next);
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
}

impl HttpClientBuilder for OktaContext {
    type Context = OktaContext;

    fn build(&self, _uri: &Uri) -> Self::Context {
        self.clone()
    }
}

impl HttpClientContext for OktaContext {
    fn on_response(
        &mut self,
        _url: &Uri,
        _header: &ResponseParts,
        body: &Bytes,
    ) -> Option<Vec<Event>> {
        let mut buf = BytesMut::new();
        buf.extend_from_slice(body);

        let events = self.decode_events(&mut buf);

        Some(events)
    }

    /// Retrieve the next batch of events for the interval window.
    fn process_url(&self, url: &Uri) -> Option<Uri> {
        let mut url_parts = UriParts::from(url.clone());
        let since = match self.since {
            Some(since) => Utc::now() - Duration::from_secs(since),
            _ => Utc::now() - self.interval,
        };
        let path_and_query = format!(
            "/api/v1/logs?since={}",
            utf8_percent_encode(&since.to_rfc3339(), percent_encoding::NON_ALPHANUMERIC)
        );
        url_parts.path_and_query = Some(path_and_query.parse().ok()?);

        Uri::from_parts(url_parts).ok()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "okta")]
impl SourceConfig for OktaConfig {
    async fn build(&self, cx: SourceContext) -> Result<sources::Source> {
        let mut url_parts = Uri::try_from(&self.domain)
            .map_err(|_| {
                format!(
                    "Invalid domain: {}. Must be a valid Okta subdomain.",
                    self.domain
                )
            })?
            .into_parts();

        if url_parts.scheme.is_none() {
            url_parts.scheme = Some(http::uri::Scheme::HTTPS);
        }
        url_parts.path_and_query = Some("/".parse()?);

        let urls = vec![Uri::from_parts(url_parts)?];

        let tls = TlsSettings::from_options(self.tls.as_ref())?;

        let decoding = self.decoding.clone();
        let framing = self.framing.clone();
        let log_namespace = cx.log_namespace(self.log_namespace);

        let decoder = DecodingConfig::new(framing, decoding, log_namespace).build()?;

        let context = OktaContext {
            decoder,
            interval: self.interval,
            since: self.since,
        };

        warn_if_interval_too_low(self.timeout, self.interval);

        let mut headers = HashMap::new();
        headers.insert(
            http::header::AUTHORIZATION.to_string(),
            vec![format!("SSWS {0}", self.token).to_string()],
        );
        headers.insert(
            http::header::ACCEPT.to_string(),
            vec!["application/json".to_string()],
        );

        let inputs = GenericHttpClientInputs {
            urls,
            interval: self.interval,
            timeout: self.timeout,
            headers,
            content_type: "application/json".to_string(),
            auth: None,
            tls,
            proxy: cx.proxy.clone(),
            shutdown: cx.shutdown,
        };

        Ok(call(inputs, context, cx.out, HttpMethod::Get).boxed())
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        // There is a global and per-source `log_namespace` config. The source config overrides the global setting,
        // and is merged here.
        let log_namespace = global_log_namespace.merge(self.log_namespace);

        let schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata();

        vec![SourceOutput::new_maybe_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}
