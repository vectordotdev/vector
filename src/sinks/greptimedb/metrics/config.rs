use crate::sinks::{
    greptimedb::{
        default_dbname,
        metrics::{
            request::GreptimeDBGrpcRetryLogic,
            request_builder::RequestBuilderOptions,
            service::{healthcheck, GreptimeDBGrpcService},
            sink,
        },
        GreptimeDBDefaultBatchSettings,
    },
    prelude::*,
};
use vector_lib::{configurable::configurable_component, sensitive_string::SensitiveString};

/// Configuration for the `greptimedb` sink.
#[configurable_component(sink("greptimedb", "Ingest metrics data into GreptimeDB."))]
#[configurable(metadata(
    deprecated = "The `greptimedb` sink has been renamed. Please use `greptimedb_metrics` instead."
))]
#[derive(Clone, Debug, Derivative)]
pub struct GreptimeDBConfig(GreptimeDBMetricsConfig);

impl GenerateConfig for GreptimeDBConfig {
    fn generate_config() -> toml::Value {
        <GreptimeDBMetricsConfig as GenerateConfig>::generate_config()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "greptimedb")]
impl SinkConfig for GreptimeDBConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        warn!("DEPRECATED: The `greptimedb` sink has been renamed. Please use `greptimedb_metrics` instead.");
        self.0.build(cx).await
    }

    fn input(&self) -> Input {
        self.0.input()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        self.0.acknowledgements()
    }
}

/// Configuration items for GreptimeDB
#[configurable_component(sink("greptimedb_metrics", "Ingest metrics data into GreptimeDB."))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct GreptimeDBMetricsConfig {
    /// The [GreptimeDB database][database] name to connect.
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

    /// Use Greptime's prefixed naming for time index and value columns.
    ///
    /// This is to keep consistency with GreptimeDB's naming pattern. By
    /// default, this sink will use `val` for value column name, and `ts` for
    /// time index name. When turned on, `greptime_value` and
    /// `greptime_timestamp` will be used for these names.
    ///
    /// If you are using this Vector sink together with other data ingestion
    /// sources of GreptimeDB, like Prometheus Remote Write and Influxdb Line
    /// Protocol, it is highly recommended to turn on this.
    ///
    /// Also if there is a tag name conflict from your data source, for
    /// example, you have a tag named as `val` or `ts`, you need to turn on
    /// this option to avoid the conflict.
    ///
    /// Default to `false` for compatibility.
    #[configurable]
    pub new_naming: Option<bool>,
}

impl_generate_config_from_default!(GreptimeDBMetricsConfig);

#[typetag::serde(name = "greptimedb_metrics")]
#[async_trait::async_trait]
impl SinkConfig for GreptimeDBMetricsConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let request_settings = self.request.into_settings();
        let service = ServiceBuilder::new()
            .settings(request_settings, GreptimeDBGrpcRetryLogic)
            .service(GreptimeDBGrpcService::try_new(self)?);
        let sink = sink::GreptimeDBGrpcSink {
            service,
            batch_settings: self.batch.into_batcher_settings()?,
            request_builder_options: RequestBuilderOptions {
                use_new_naming: self.new_naming.unwrap_or(false),
            },
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
        crate::test_util::test_generate_config::<GreptimeDBMetricsConfig>();
    }

    #[test]
    fn test_config_with_username() {
        let config = indoc! {r#"
            endpoint = "foo-bar.ap-southeast-1.aws.greptime.cloud:4001"
            dbname = "foo-bar"
        "#};

        toml::from_str::<GreptimeDBMetricsConfig>(config).unwrap();
    }
}
