use super::{ProviderControl, ProviderRx};
use crate::config::{
    load_from_str,
    provider::{ProviderConfig, ProviderDescription},
    Config,
};
use crate::{
    http::HttpClient,
    tls::{TlsOptions, TlsSettings},
};
use hyper::Body;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct HttpConfig {
    url: Option<Url>,
    tls_options: Option<TlsOptions>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            url: None,
            tls_options: None,
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "http")]
impl ProviderConfig for HttpConfig {
    async fn build(&self) -> Result<ProviderRx, &'static str> {
        let url = self
            .url
            .as_ref()
            .ok_or("URL is required for the `http` provider.")?;

        let (provider_tx, provider_rx) = super::provider_control();

        let tls_settings =
            TlsSettings::from_options(&self.tls_options).map_err(|_| "Invalid TLS options")?;
        let http_client =
            HttpClient::<Body>::new(tls_settings).map_err(|_| "Invalid TLS settings")?;

        info!(message = "Attempting to retrieve config from HTTP provider.", url = ?url);

        let request = http::request::Builder::new()
            .uri(url.to_string())
            .body(Body::empty())
            .map_err(|_| "Couldn't create HTTP request")?;

        // Attempt to fetch remote resource
        match http_client.send(request).await {
            Ok(response) => {
                info!("A response was received.");
                let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
                let text = String::from_utf8_lossy(body.as_ref());
                let config = load_from_str(&text, None);

                if let Ok(mut config) = config {
                    info!("Configuration was successfully received.");
                    // Explicitly set provider to `None`.
                    config.provider = None;

                    // Send down the control channel.
                    let _ = provider_tx.send(ProviderControl::Config(config)).await;
                } else {
                    error!("Invalid configuration received.");
                }
            }
            Err(_) => {
                error!("Couldn't retrieve configuration.");
            }
        }

        Ok(provider_rx)
    }

    fn provider_type(&self) -> &'static str {
        "http"
    }
}

inventory::submit! {
    ProviderDescription::new::<HttpConfig>("http")
}

impl_generate_config_from_default!(HttpConfig);
