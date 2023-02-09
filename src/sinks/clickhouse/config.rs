use vector_config::configurable_component;

use crate::{
    codecs::Transformer,
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    http::Auth,
    sinks::{
        util::{
            BatchConfig, Compression, RealtimeSizeBasedDefaultBatchSettings, TowerRequestConfig,
            UriSerde,
        },
        Healthcheck, VectorSink,
    },
    tls::TlsConfig,
};

use super::http_sink::build_http_sink;

/// Configuration for the `clickhouse` sink.
#[configurable_component(sink("clickhouse"))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ClickhouseConfig {
    /// The endpoint of the ClickHouse server.
    #[serde(alias = "host")]
    #[configurable(metadata(docs::examples = "http://localhost:8123"))]
    pub endpoint: UriSerde,

    /// The table that data will be inserted into.
    #[configurable(metadata(docs::examples = "mytable"))]
    pub table: String,

    /// The database that contains the table that data will be inserted into.
    #[configurable(metadata(docs::examples = "mydatabase"))]
    pub database: Option<String>,

    /// Sets `input_format_skip_unknown_fields`, allowing ClickHouse to discard fields not present in the table schema.
    #[serde(default)]
    pub skip_unknown_fields: bool,

    /// Sets `date_time_input_format` to `best_effort`, allowing ClickHouse to properly parse RFC3339/ISO 8601.
    #[serde(default)]
    pub date_time_best_effort: bool,

    #[configurable(derived)]
    #[serde(default = "Compression::gzip_default")]
    pub compression: Compression,

    #[configurable(derived)]
    #[serde(
        default,
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub encoding: Transformer,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    pub auth: Option<Auth>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl_generate_config_from_default!(ClickhouseConfig);

#[async_trait::async_trait]
impl SinkConfig for ClickhouseConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        // later we can build different sink(http, native) here
        // according to the clickhouseConfig
        build_http_sink(self, cx).await
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}
