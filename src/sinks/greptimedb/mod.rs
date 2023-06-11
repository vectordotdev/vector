use futures_util::FutureExt;
use greptimedb_client::Client;
use vector_common::sensitive_string::SensitiveString;
use vector_config::configurable_component;
use vector_core::config::{AcknowledgementsConfig, Input};
use vector_core::tls::TlsConfig;

use crate::config::{SinkConfig, SinkContext};
use crate::sinks::util::{BatchConfig, SinkBatchSettings};
use crate::sinks::{Healthcheck, VectorSink};

use super::util::TowerRequestConfig;

mod batch;
#[cfg(all(test, feature = "greptimedb-integration-tests"))]
mod integration_tests;
mod request_builder;
mod service;
mod sink;

#[derive(Clone, Copy, Debug, Default)]
pub struct GreptimeDBDefaultBatchSettings;

impl SinkBatchSettings for GreptimeDBDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(20);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration items for GreptimeDB
#[configurable_component(sink("greptimedb"))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct GreptimeDBConfig {
    /// The database name to connect
    #[configurable(metadata(docs::examples = "public"))]
    #[serde(default)]
    pub dbname: Option<String>,
    /// The host and port of greptimedb grpc service
    #[configurable(metadata(docs::examples = "example.com:4001"))]
    pub endpoint: String,
    /// The username of greptimedb
    #[configurable(metadata(docs::examples = "username"))]
    #[serde(default)]
    pub username: Option<String>,
    /// The password of greptimedb
    #[configurable(metadata(docs::examples = "password"))]
    #[serde(default)]
    pub password: Option<SensitiveString>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<GreptimeDBDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,
}

impl_generate_config_from_default!(GreptimeDBConfig);

#[async_trait::async_trait]
impl SinkConfig for GreptimeDBConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let service = service::GreptimeDBService::new(self);
        let sink = sink::GreptimeDBSink {
            service,
            batch_settings: self.batch.into_batcher_settings()?,
        };

        let healthcheck = healthcheck(self)?;
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

fn healthcheck(config: &GreptimeDBConfig) -> crate::Result<super::Healthcheck> {
    let client = Client::with_urls(vec![&config.endpoint]);

    Ok(async move { client.health_check().await.map_err(|error| error.into()) }.boxed())
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<GreptimeDBConfig>();
    }

    #[test]
    fn test_config_with_username() {
        let config = indoc! {r#"
            endpoint = "foo-bar.ap-southeast-1.aws.greptime.cloud:4001"
            dbname = "foo-bar"
        "#};

        toml::from_str::<GreptimeDBConfig>(config).unwrap();
    }
}
