use vector_config::configurable_component;

use crate::sinks::util::SinkBatchSettings;
use crate::sinks::{Healthcheck, VectorSink};

mod client;

#[derive(Clone, Copy, Debug, Default)]
pub struct GreptimeDBDefaultBatchSettings;

impl SinkBatchSettings for GreptimeDBDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(20);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration items for GreptimeDB
#[configurable_component]
#[derive(Clone, Debug)]
pub struct GreptimeDBConfig {
    /// The catalog name to connect
    #[configurable(metadata(docs::examples = "greptime"))]
    pub catalog: String,
    /// The schema name to connect
    #[configurable(metadata(docs::examples = "public"))]
    pub schema: String,
    /// The host and port of greptimedb
    #[configurable(metadata(docs::examples = "localhost:4001"))]
    pub endpoint: String,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    acknowledgements: AcknowledgementsConfig,
    // TODO: tls configuration
}

#[async_trait::async_trait]
impl SinkConfig for GreptimeDBConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = client::GreptimeDBSink::new(&self);
        sink
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}
