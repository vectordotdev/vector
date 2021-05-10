use super::Result;
use crate::{
    config::{
        self,
        provider::{ProviderConfig, ProviderDescription},
    },
    http::HttpClient,
    signal,
    tls::{TlsOptions, TlsSettings},
};
use hyper::Body;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use tokio::{sync::mpsc, time};
use tokio_stream::wrappers::ReceiverStream;
use url::Url;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RequestConfig {
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

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct HttpConfig {
    url: Option<Url>,
    request: RequestConfig,
    poll_interval_secs: u64,
    #[serde(flatten)]
    tls_options: Option<TlsOptions>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            url: None,
            request: RequestConfig::default(),
            poll_interval_secs: 30,
            tls_options: None,
        }
    }
}

async fn http_request(
    url: &Url,
    tls_options: &Option<TlsOptions>,
    headers: &IndexMap<String, String>,
) -> std::result::Result<String, &'static str> {
    let tls_settings = TlsSettings::from_options(tls_options).map_err(|_| "Invalid TLS options")?;
    let http_client = HttpClient::<Body>::new(tls_settings).map_err(|_| "Invalid TLS settings")?;

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

    let body = hyper::body::to_bytes(response.into_body())
        .await
        .map_err(|err| {
            let message = "Error interpreting response.";
            let cause = err.into_cause();
            error!(
                    message = ?message,
                    error = ?cause);

            message
        })?;

    Ok(String::from_utf8_lossy(body.as_ref()).to_string())
}

/// Calls `http_request`, serializing the result to a `ConfigBuilder`.
async fn http_request_to_config_builder(
    url: &Url,
    tls_options: &Option<TlsOptions>,
    headers: &IndexMap<String, String>,
) -> Result {
    let config_str = http_request(url, tls_options, headers)
        .await
        .map_err(|e| vec![e.to_owned()])?;

    let (config_builder, warnings) = config::load(config_str.as_bytes(), None)?;

    for warning in warnings.into_iter() {
        warn!("{}", warning);
    }

    Ok(config_builder)
}

#[async_trait::async_trait]
#[typetag::serde(name = "http")]
impl ProviderConfig for HttpConfig {
    async fn build(&mut self, signal_handler: &mut signal::SignalHandler) -> Result {
        let url = self
            .url
            .take()
            .ok_or(vec!["URL is required for the `http` provider.".to_owned()])?;

        let tls_options = self.tls_options.take();
        let poll_interval_secs = self.poll_interval_secs;
        let request = self.request.clone();

        let (signal_tx, signal_rx) = mpsc::channel(2);

        let mut shutdown_rx = signal_handler
            .with_shutdown(ReceiverStream::new(signal_rx))
            .subscribe();

        let config_builder =
            http_request_to_config_builder(&url, &tls_options, &request.headers).await?;

        tokio::spawn(async move {
            let mut interval = time::interval(time::Duration::from_secs(poll_interval_secs));

            loop {
                tokio::select! {
                    biased;

                    _ = shutdown_rx.recv() => break,
                    _ = interval.tick() => {
                        if signal_tx.is_closed() {
                            info!("Provider control channel has gone away.");
                            break;
                        }

                        match http_request_to_config_builder(&url, &tls_options, &request.headers).await {
                            Ok(config_builder) => {
                                if signal_tx.send(signal::SignalTo::ReloadFromConfigBuilder(config_builder)).await.is_err() {
                                    info!("Signal channel has gone away.");
                                }
                            },
                            Err(_) => continue,
                        }

                        info!(
                            message = "HTTP provider is waiting.",
                            poll_interval_secs = ?poll_interval_secs,
                            url = ?url.as_str());
                    }
                }
            }
        });

        Ok(config_builder)
    }

    fn provider_type(&self) -> &'static str {
        "http"
    }
}

inventory::submit! {
    ProviderDescription::new::<HttpConfig>("http")
}

impl_generate_config_from_default!(HttpConfig);
