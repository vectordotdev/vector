//! `GreptimeDB` sink for vector.
//!
//! This sink writes Vector's metric data into
//! [GreptimeDB](https://github.com/greptimeteam/greptimedb), a cloud-native
//! time-series database. It uses GreptimeDB's [gRPC
//! API](https://docs.greptime.com/user-guide/write-data/grpc) and GreptimeDB's
//! [rust client](https://github.com/GreptimeTeam/greptimedb-client-rust).
//!
//! This sink transforms metrics into GreptimeDB table using following rules:
//!
//! - Table name: `{namespace}_{metric_name}`. If the metric doesn't have a
//! namespace, we will use metric_name for table name.
//! - Timestamp: timestamp is stored as a column called `ts`.
//! - Tags: metric tags are stored as string columns with its name as column
//! name
//! - Counter and Gauge: the value of counter and gauge are stored in a column
//! called `val`
//! - Set: the number of set items is stored in a column called `val`.
//! - Distribution, Histogram and Summary, Sketch: Statistical attributes like
//! `sum`, `count`, "max", "min", quantiles and buckets are stored as columns.
//!
use greptimedb_client::Client;
use snafu::Snafu;
use vector_lib::sensitive_string::SensitiveString;

use crate::sinks::prelude::*;

use self::service::GreptimeDBRetryLogic;

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

fn default_dbname() -> String {
    greptimedb_client::DEFAULT_SCHEMA_NAME.to_string()
}

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
            .settings(request_settings, GreptimeDBRetryLogic)
            .service(service::GreptimeDBService::try_new(self)?);
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

#[derive(Debug, Snafu)]
pub enum GreptimeDBConfigError {
    #[snafu(display("greptimedb TLS Config Error: missing key"))]
    TlsMissingKey,
    #[snafu(display("greptimedb TLS Config Error: missing cert"))]
    TlsMissingCert,
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
