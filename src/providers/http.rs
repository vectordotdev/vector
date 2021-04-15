use super::{ProviderControl, ProviderRx};
use crate::config::{
    load_from_str,
    provider::{ProviderConfig, ProviderDescription},
};
use crate::{
    http::HttpClient,
    tls::{TlsOptions, TlsSettings},
};
use hyper::Body;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RequestConfig {
    #[serde(default)]
    pub headers: IndexMap<String, String>,
    pub timeout_secs: Option<u64>,               // 60
    pub rate_limit_duration_secs: Option<u64>,   // 1
    pub rate_limit_num: Option<u64>,             // 5
    pub retry_attempts: Option<usize>,           // max_value()
    pub retry_max_duration_secs: Option<u64>,    // 3600
    pub retry_initial_backoff_secs: Option<u64>, // 1
}

impl Default for RequestConfig {
    fn default() -> Self {
        Self {
            headers: IndexMap::new(),
            timeout_secs: Some(60),
            rate_limit_duration_secs: Some(1),
            rate_limit_num: Some(5),
            retry_attempts: Some(usize::MAX),
            retry_max_duration_secs: Some(3600),
            retry_initial_backoff_secs: Some(1),
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct HttpConfig {
    url: Option<Url>,
    request: RequestConfig,
    #[serde(flatten)]
    tls_options: Option<TlsOptions>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            url: None,
            request: RequestConfig::default(),
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

        let mut builder = http::request::Builder::new().uri(url.to_string());

        for (header, value) in self.request.headers.iter() {
            builder = builder.header(header.as_str(), value.as_str());
        }

        let request = builder
            .body(Body::empty())
            .map_err(|_| "Couldn't create HTTP request")?;

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
