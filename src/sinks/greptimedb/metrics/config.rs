use vector_lib::{configurable::configurable_component, sensitive_string::SensitiveString};

use crate::{
    config::ValidatedSink,
    sinks::{
        greptimedb::{
            GreptimeDBDefaultBatchSettings, GrpcCompression, default_dbname,
            metrics::{
                request::GreptimeDBGrpcRetryLogic,
                request_builder::RequestBuilderOptions,
                service::{GreptimeDBGrpcService, healthcheck},
                sink,
            },
        },
        prelude::*,
    },
};

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
    /// Set gRPC compression encoding for the request.
    #[serde(default)]
    pub grpc_compression: GrpcCompression,

    #[serde(default)]
    pub request: TowerRequestConfig,

    #[serde(default)]
    pub(crate) batch: BatchConfig<GreptimeDBDefaultBatchSettings>,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

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
    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedGreptimeDBMetrics {
    batch_settings: BatcherSettings,
    use_new_naming: bool,
}

/// Validate the all-or-none GreptimeDB TLS path requirement without touching the
/// filesystem.
///
/// The greptimedb ingester requires all three TLS paths (`ca_file`, `crt_file`,
/// `key_file`) to be set. Mirrors the check in `new_client_from_config`.
pub(super) fn validate_tls_all_or_none(tls: &TlsConfig) -> crate::Result<()> {
    if tls.ca_file.is_none() || tls.crt_file.is_none() || tls.key_file.is_none() {
        return Err(
            "GreptimeDB TLS requires ca_file, crt_file, and key_file to all be set.".into(),
        );
    }
    Ok(())
}

#[async_trait::async_trait]
impl ValidatedSink for GreptimeDBMetricsConfig {
    type Validated = ValidatedGreptimeDBMetrics;

    fn validate(&self) -> crate::Result<ValidatedGreptimeDBMetrics> {
        let batch_settings = self.batch.into_batcher_settings()?;

        if let Some(tls) = &self.tls {
            validate_tls_all_or_none(tls)?;
        }

        Ok(ValidatedGreptimeDBMetrics {
            batch_settings,
            use_new_naming: self.new_naming.unwrap_or(false),
        })
    }

    async fn build(
        &self,
        validated: &ValidatedGreptimeDBMetrics,
        _cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedGreptimeDBMetrics {
            batch_settings,
            use_new_naming,
        } = validated.clone();

        let request_settings = self.request.into_settings();
        let service = ServiceBuilder::new()
            .settings(request_settings, GreptimeDBGrpcRetryLogic)
            .service(GreptimeDBGrpcService::try_new(self)?);
        let sink = sink::GreptimeDBGrpcSink {
            service,
            batch_settings,
            request_builder_options: RequestBuilderOptions { use_new_naming },
        };

        let healthcheck = healthcheck(self)?;
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;
    use crate::config::ValidatedSink;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<GreptimeDBMetricsConfig>();
    }

    #[test]
    fn test_config_with_username() {
        let config = indoc! {r#"
            endpoint: "foo-bar.ap-southeast-1.aws.greptime.cloud:4001"
            dbname: "foo-bar"
        "#};

        serde_yaml::from_str::<GreptimeDBMetricsConfig>(config).unwrap();
    }

    #[test]
    fn prepares_valid_config() {
        let config = GreptimeDBMetricsConfig {
            endpoint: "example.com:4001".to_string(),
            ..Default::default()
        };

        let validated = config.validate().expect("preparation should succeed");
        assert!(!validated.use_new_naming);
    }

    #[test]
    fn validate_rejects_partial_tls() {
        let config = indoc! {r#"
            endpoint: "example.com:4001"
            tls:
                ca_file: "/path/to/ca.pem"
                crt_file: "/path/to/crt.pem"
        "#};

        let config = serde_yaml::from_str::<GreptimeDBMetricsConfig>(config).unwrap();
        assert!(
            config.validate().is_err(),
            "partial TLS should fail validation"
        );
    }

    #[test]
    fn validate_accepts_full_tls() {
        let config = indoc! {r#"
            endpoint: "example.com:4001"
            tls:
                ca_file: "/path/to/ca.pem"
                crt_file: "/path/to/crt.pem"
                key_file: "/path/to/key.pem"
        "#};

        let config = serde_yaml::from_str::<GreptimeDBMetricsConfig>(config).unwrap();
        assert!(
            config.validate().is_ok(),
            "complete TLS should pass validation"
        );
    }
}
