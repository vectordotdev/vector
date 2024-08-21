use std::collections::{HashMap, HashSet};

use aws_sdk_secretsmanager::{config, Client};
use vector_lib::configurable::{component::GenerateConfig, configurable_component};

use crate::aws::{create_client, AwsAuthentication, ClientBuilder, RegionOrEndpoint};
use crate::config::ProxyConfig;
use crate::tls::TlsConfig;
use crate::{config::SecretBackend, signal};

pub(crate) struct SecretsManagerClientBuilder;

impl ClientBuilder for SecretsManagerClientBuilder {
    type Client = Client;

    fn build(config: &aws_types::SdkConfig) -> Self::Client {
        let config = config::Builder::from(config).build();
        Client::from_conf(config)
    }
}

/// Configuration for the `aws_secrets_manager` secrets backend.
#[configurable_component(secrets("aws_secrets_manager"))]
#[derive(Clone, Debug)]
pub struct AwsSecretsManagerBackend {
    /// ID of the secret to resolve.
    pub secret_id: String,

    #[serde(flatten)]
    #[configurable(derived)]
    pub region: RegionOrEndpoint,

    #[configurable(derived)]
    #[serde(default)]
    pub auth: AwsAuthentication,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,
}

impl GenerateConfig for AwsSecretsManagerBackend {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(AwsSecretsManagerBackend {
            secret_id: String::from("secret-id"),
            region: Default::default(),
            auth: Default::default(),
            tls: None,
        })
        .unwrap()
    }
}

impl SecretBackend for AwsSecretsManagerBackend {
    async fn retrieve(
        &mut self,
        secret_keys: HashSet<String>,
        _: &mut signal::SignalRx,
    ) -> crate::Result<HashMap<String, String>> {
        let client = create_client::<SecretsManagerClientBuilder>(
            &self.auth,
            self.region.region(),
            self.region.endpoint(),
            &ProxyConfig::default(),
            &self.tls,
            &None,
        )
        .await?;

        let get_secret_value_response = client
            .get_secret_value()
            .secret_id(&self.secret_id)
            .send()
            .await?;

        let secret_string = get_secret_value_response
            .secret_string
            .ok_or::<String>(format!(
                "secret for secret-id '{}' could not be retrieved",
                &self.secret_id
            ))?;

        let output = serde_json::from_str::<HashMap<String, String>>(secret_string.as_str())?;

        let mut secrets = HashMap::new();
        for k in secret_keys.into_iter() {
            if let Some(secret) = output.get(&k) {
                if secret.is_empty() {
                    return Err(format!(
                        "value for key '{}' in secret with id '{}' was empty",
                        k, &self.secret_id
                    )
                    .into());
                }
                secrets.insert(k.to_string(), secret.to_string());
            } else {
                return Err(format!(
                    "key '{}' in secret with id '{}' does not exist",
                    k, &self.secret_id
                )
                .into());
            }
        }
        Ok(secrets)
    }
}
