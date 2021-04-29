use super::{shutdown_trigger, Result};
use crate::{
    config::provider::{ProviderConfig, ProviderDescription},
    http::HttpClient,
    signal,
    tls::{TlsOptions, TlsSettings},
};
use hyper::Body;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use tokio::{sync::mpsc, time};
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

#[async_trait::async_trait]
#[typetag::serde(name = "http")]
impl ProviderConfig for HttpConfig {
    async fn build(&mut self, signal_handler: &mut signal::SignalHandler) -> Result {
        let url = self
            .url
            .take()
            .ok_or("URL is required for the `http` provider.")?;

        let tls_settings =
            TlsSettings::from_options(&self.tls_options).map_err(|_| "Invalid TLS options")?;
        let http_client =
            HttpClient::<Body>::new(tls_settings).map_err(|_| "Invalid TLS settings")?;

        let poll_interval_secs = self.poll_interval_secs;
        let request = self.request.clone();

        // Create a shutdown trigger.
        let (shutdown_tx, mut shutdown_rx) = shutdown_trigger();
        signal_handler.register_shutdown(shutdown_tx);

        // Configuration channel
        let (config_tx, mut config_rx) = mpsc::channel(2);
        let signal_tx = signal_handler.clone_tx();

        // Spawn an event that will poll the endpoint continuously, surfacing new
        // configuration as it's found.
        tokio::spawn(async move {
            let mut interval = time::interval(time::Duration::from_secs(poll_interval_secs));

            loop {
                tokio::select! {
                    biased;

                    _ = &mut shutdown_rx => break,
                    _ = interval.tick() => {
                        if config_tx.is_closed() {
                            info!("Provider control channel has gone away.");
                            break;
                        }

                        // Build HTTP request.
                        let mut builder = http::request::Builder::new().uri(url.to_string());

                        // Augment with headers. These may be required e.g. for authentication to
                        // private endpoints.
                        for (header, value) in request.headers.iter() {
                            builder = builder.header(header.as_str(), value.as_str());
                        }

                        let request = builder
                            .body(Body::empty())
                            .map_err(|_| "Couldn't create HTTP request")
                            .unwrap();

                        info!(
                            message = "Attempting to retrieve configuration.",
                            url = ?url.as_str()
                        );

                        // Send the request and attempt to parse the remote configurtion.
                        match http_client.send(request).await {
                            Ok(response) => {
                                info!(
                                    message = "Response received.",
                                    url = ?url.as_str());

                                // Attempt the parse the body into bytes.
                                let body = match hyper::body::to_bytes(response.into_body()).await {
                                    Ok(body) => body,
                                    Err(err) => {
                                        let cause = err.into_cause();
                                        error!(
                                            message = "Error interpreting response.",
                                            error = ?cause);

                                        continue;
                                    }
                                };
                                let text = String::from_utf8_lossy(body.as_ref());

                                // Send down the control channel.
                                if config_tx
                                    .send(text.to_string())
                                    .await
                                    .is_err()
                                {
                                    info!(
                                        message = "Couldn't apply config.",
                                        error = "config channel has gone away"
                                    );

                                    break;
                                }
                            }
                            Err(err) => {
                                error!(
                                    message = "HTTP error",
                                    error = ?err,
                                    url = ?url.as_str());
                            }
                        }

                        info!(
                            message = "HTTP provider is waiting.",
                            poll_interval_secs = ?poll_interval_secs,
                            url = ?url.as_str());
                    }
                }
            }
        });

        if let Some(config) = config_rx.recv().await {
            // Map additional config to signals.
            tokio::spawn(async move {
                while let Some(config) = config_rx.recv().await {
                    if signal_tx
                        .send(signal::SignalTo::ReloadFromString(config))
                        .await
                        .is_err()
                    {
                        info!(
                            message = "Couldn't apply config.",
                            error = "signal channel has gone away"
                        );

                        break;
                    }
                }
            });

            Ok(config)
        } else {
            Err("config channel has gone away")
        }
    }

    fn provider_type(&self) -> &'static str {
        "http"
    }
}

inventory::submit! {
    ProviderDescription::new::<HttpConfig>("http")
}

impl_generate_config_from_default!(HttpConfig);
