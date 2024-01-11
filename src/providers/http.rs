use async_stream::stream;
use bytes::Buf;
use futures::Stream;
use hyper::Body;
use indexmap::IndexMap;
use tokio::time;
use url::Url;
use vector_lib::configurable::configurable_component;

use crate::{
    config::{self, provider::ProviderConfig, ProxyConfig},
    http::HttpClient,
    signal,
    tls::{TlsConfig, TlsSettings},
};

use super::BuildResult;

/// Request settings.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct RequestConfig {
    /// HTTP headers to add to the request.
    #[serde(default)]
    pub headers: IndexMap<String, String>,
}

impl Default for RequestConfig {
    fn default() -> Self {
        Self {
            headers: IndexMap::new(),
        }
    }
}

/// Configuration for the `http` provider.
#[configurable_component(provider("http"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct HttpConfig {
    /// URL for the HTTP provider.
    url: Option<Url>,

    #[configurable(derived)]
    request: RequestConfig,

    /// How often to poll the provider, in seconds.
    poll_interval_secs: u64,

    #[serde(flatten)]
    tls_options: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    proxy: ProxyConfig,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            url: None,
            request: RequestConfig::default(),
            poll_interval_secs: 30,
            tls_options: None,
            proxy: Default::default(),
        }
    }
}

/// Makes an HTTP request to the provided endpoint, returning the String body.
async fn http_request(
    url: &Url,
    tls_options: &Option<TlsConfig>,
    headers: &IndexMap<String, String>,
    proxy: &ProxyConfig,
) -> Result<bytes::Bytes, &'static str> {
    let tls_settings = TlsSettings::from_options(tls_options).map_err(|_| "Invalid TLS options")?;
    let http_client =
        HttpClient::<Body>::new(tls_settings, proxy).map_err(|_| "Invalid TLS settings")?;

    // Build HTTP request.
    let mut builder = http::request::Builder::new().uri(url.to_string());

    // Augment with headers. These may be required e.g. for authentication to
    // private endpoints.
    for (header, value) in headers.iter() {
        builder = builder.header(header.as_str(), value.as_str());
    }

    let request = builder
        .body(Body::empty())
        .map_err(|_| "Couldn't create HTTP request")?;

    info!(
        message = "Attempting to retrieve configuration.",
        url = ?url.as_str()
    );

    let response = http_client.send(request).await.map_err(|err| {
        let message = "HTTP error";
        error!(
            message = ?message,
            error = ?err,
            url = ?url.as_str());
        message
    })?;

    info!(message = "Response received.", url = ?url.as_str());

    hyper::body::to_bytes(response.into_body())
        .await
        .map_err(|err| {
            let message = "Error interpreting response.";
            let cause = err.into_cause();
            error!(
                    message = ?message,
                    error = ?cause);

            message
        })
}

/// Calls `http_request`, serializing the result to a `ConfigBuilder`.
async fn http_request_to_config_builder(
    url: &Url,
    tls_options: &Option<TlsConfig>,
    headers: &IndexMap<String, String>,
    proxy: &ProxyConfig,
) -> BuildResult {
    let config_str = http_request(url, tls_options, headers, proxy)
        .await
        .map_err(|e| vec![e.to_owned()])?;

    let (config_builder, warnings) =
        config::load(config_str.chunk(), crate::config::format::Format::Toml)?;

    for warning in warnings.into_iter() {
        warn!("{}", warning);
    }

    Ok(config_builder)
}

/// Polls the HTTP endpoint after/every `poll_interval_secs`, returning a stream of `ConfigBuilder`.
fn poll_http(
    poll_interval_secs: u64,
    url: Url,
    tls_options: Option<TlsConfig>,
    headers: IndexMap<String, String>,
    proxy: ProxyConfig,
) -> impl Stream<Item = signal::SignalTo> {
    let duration = time::Duration::from_secs(poll_interval_secs);
    let mut interval = time::interval_at(time::Instant::now() + duration, duration);

    stream! {
        loop {
            interval.tick().await;

            match http_request_to_config_builder(&url, &tls_options, &headers, &proxy).await {
                Ok(config_builder) => yield signal::SignalTo::ReloadFromConfigBuilder(config_builder),
                Err(_) => {},
            };

            info!(
                message = "HTTP provider is waiting.",
                poll_interval_secs = ?poll_interval_secs,
                url = ?url.as_str());
        }
    }
}

#[async_trait::async_trait]
impl ProviderConfig for HttpConfig {
    async fn build(&mut self, signal_handler: &mut signal::SignalHandler) -> BuildResult {
        let url = self
            .url
            .take()
            .ok_or_else(|| vec!["URL is required for the `http` provider.".to_owned()])?;

        let tls_options = self.tls_options.take();
        let poll_interval_secs = self.poll_interval_secs;
        let request = self.request.clone();

        let proxy = ProxyConfig::from_env().merge(&self.proxy);
        let config_builder =
            http_request_to_config_builder(&url, &tls_options, &request.headers, &proxy).await?;

        // Poll for changes to remote configuration.
        signal_handler.add(poll_http(
            poll_interval_secs,
            url,
            tls_options,
            request.headers.clone(),
            proxy.clone(),
        ));

        Ok(config_builder)
    }
}

impl_generate_config_from_default!(HttpConfig);
