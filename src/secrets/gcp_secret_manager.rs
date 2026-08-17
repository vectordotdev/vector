use std::collections::{HashMap, HashSet};

use base64::prelude::{BASE64_STANDARD, Engine as _};
use http::Request;
use http_body::{Body as _, Collected};
use hyper::Body;
use serde::{Deserialize, Serialize};
use vector_lib::configurable::{component::GenerateConfig, configurable_component};

use crate::{
    config::{ProxyConfig, SecretBackend},
    gcp::{GcpAuthConfig, Scope},
    http::HttpClient,
    signal,
    tls::{TlsConfig, TlsSettings},
};

const SECRET_MANAGER_URL: &str = "https://secretmanager.googleapis.com";

fn default_endpoint() -> String {
    SECRET_MANAGER_URL.to_string()
}

/// Configuration for the `gcp_secret_manager` secrets backend.
#[configurable_component(secrets("gcp_secret_manager"))]
#[derive(Clone, Debug)]
pub struct GcpSecretManagerBackend {
    /// The GCP project ID containing the secret.
    ///
    /// This is the project ID (not the project number) where the secret is stored.
    #[configurable(metadata(docs::examples = "my-project-123"))]
    pub project: String,

    /// The name of the secret to retrieve.
    ///
    /// Only the secret name should be specified, not the full resource name.
    #[configurable(metadata(docs::examples = "my-secret"))]
    pub secret_name: String,

    /// The endpoint to use for the GCP Secret Manager API.
    ///
    /// The scheme (`http` or `https`) must be specified. No path should be included since the paths
    /// defined by the GCP Secret Manager API are used.
    ///
    /// The trailing slash `/` must not be included.
    #[serde(default = "default_endpoint")]
    #[configurable(metadata(docs::examples = "https://secretmanager.googleapis.com"))]
    pub endpoint: String,

    #[serde(default, flatten)]
    #[configurable(derived)]
    pub auth: GcpAuthConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,
}

impl GenerateConfig for GcpSecretManagerBackend {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(GcpSecretManagerBackend {
            project: String::from("my-project"),
            secret_name: String::from("my-secret"),
            endpoint: default_endpoint(),
            auth: Default::default(),
            tls: None,
        })
        .unwrap()
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct SecretPayload {
    data: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct AccessSecretVersionResponse {
    payload: Option<SecretPayload>,
}

impl SecretBackend for GcpSecretManagerBackend {
    async fn retrieve(
        &mut self,
        secret_keys: HashSet<String>,
        _: &mut signal::SignalRx,
    ) -> crate::Result<HashMap<String, String>> {
        let auth = self.auth.build(Scope::CloudPlatform).await?;

        let tls_settings = TlsSettings::from_options(self.tls.as_ref())?;
        let proxy = ProxyConfig::default();
        let client = HttpClient::new(tls_settings, &proxy)?;

        let url = format!(
            "{}/v1/projects/{}/secrets/{}/versions/latest:access",
            self.endpoint, self.project, self.secret_name,
        );

        let mut request = Request::get(&url)
            .header("Content-Type", "application/json")
            .body(Body::empty())
            .map_err(|e| format!("Failed to build request for GCP Secret Manager: {e}"))?;

        auth.apply(&mut request);

        let response = client.send(request).await?;
        let status = response.status();

        let body_bytes = response
            .into_body()
            .collect()
            .await
            .map(Collected::to_bytes)?;

        if !status.is_success() {
            return Err(format!(
                "GCP Secret Manager request failed with status {}: {}",
                status,
                String::from_utf8_lossy(&body_bytes),
            )
            .into());
        }

        let response: AccessSecretVersionResponse = serde_json::from_slice(&body_bytes)?;

        let data_b64 = response
            .payload
            .and_then(|p| p.data)
            .ok_or_else(|| format!("secret '{}' has no payload data", self.secret_name))?;

        let data_bytes = BASE64_STANDARD
            .decode(&data_b64)
            .map_err(|e| format!("Failed to decode base64 secret data: {e}"))?;

        let secret_string = String::from_utf8(data_bytes)
            .map_err(|e| format!("Secret data is not valid UTF-8: {e}"))?;

        let output = serde_json::from_str::<HashMap<String, String>>(secret_string.as_str())?;

        let mut secrets = HashMap::new();
        for k in secret_keys.into_iter() {
            if let Some(secret) = output.get(&k) {
                if secret.is_empty() {
                    return Err(format!(
                        "value for key '{}' in secret '{}' was empty",
                        k, self.secret_name,
                    )
                    .into());
                }
                secrets.insert(k.to_string(), secret.to_string());
            } else {
                return Err(format!(
                    "key '{}' in secret '{}' does not exist",
                    k, self.secret_name,
                )
                .into());
            }
        }
        Ok(secrets)
    }
}
