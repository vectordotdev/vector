use vector_lib::configurable::configurable_component;
use vector_lib::sensitive_string::SensitiveString;

use super::request::GreptimeDBGrpcRetryLogic;

use super::service::{healthcheck, GreptimeDBGrpcService};
use super::sink;
use crate::sinks::greptimedb::{default_dbname, GreptimeDBDefaultBatchSettings};
use crate::sinks::prelude::*;

/// Configuration items for GreptimeDB
#[configurable_component(sink("greptimedb", "Ingest metrics data into GreptimeDB."))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct GreptimeDBConfig {
    /// The GreptimeDB [database][database] name to connect.
    ///
    /// Default to `public`, the default database of GreptimeDB.
    ///
    /// Database can be created via `create database` statement on
    /// GreptimeDB. If you are using GreptimeCloud, use `dbname` from the
    /// connection information of your instance.
    ///
    /// [database]: https://docs.greptime.com/user-guide/concepts/key-concepts#database
    #[configurable(metadata(docs::examples = "public"))]
    #[derivative(Default(value = "default_dbname()"))]
    #[serde(default = "default_dbname")]
    pub dbname: String,
    /// The host and port of GreptimeDB gRPC service.
    ///
    /// This sink uses GreptimeDB's gRPC interface for data ingestion. By
    /// default, GreptimeDB listens to port 4001 for gRPC protocol.
    ///
    /// The address _must_ include a port.
    #[configurable(metadata(docs::examples = "example.com:4001"))]
    #[configurable(metadata(
        docs::examples = "1nge17d2r3ns.ap-southeast-1.aws.greptime.cloud:4001"
    ))]
    pub endpoint: String,
    /// The username for your GreptimeDB instance.
    ///
    /// This is required if your instance has authentication enabled.
    #[configurable(metadata(docs::examples = "username"))]
    #[serde(default)]
    pub username: Option<String>,
    /// The password for your GreptimeDB instance.
    ///
    /// This is required if your instance has authentication enabled.
    #[configurable(metadata(docs::examples = "password"))]
    #[serde(default)]
    pub password: Option<SensitiveString>,
    /// Set gRPC compression encoding for the request
    /// Default to none, `gzip` or `zstd` is supported.
    #[configurable(metadata(docs::examples = "grpc_compression"))]
    #[serde(default)]
    pub grpc_compression: Option<String>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub(crate) batch: BatchConfig<GreptimeDBDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,
}

impl_generate_config_from_default!(GreptimeDBConfig);

#[typetag::serde(name = "greptimedb")]
#[async_trait::async_trait]
impl SinkConfig for GreptimeDBConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let request_settings = self.request.into_settings();
        let service = ServiceBuilder::new()
            .settings(request_settings, GreptimeDBGrpcRetryLogic)
            .service(GreptimeDBGrpcService::try_new(self)?);
        let sink = sink::GreptimeDBGrpcSink {
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
