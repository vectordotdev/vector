use crate::sinks::prelude::*;

// sub level implementations
#[cfg(feature = "sinks-greptimedb_logs")]
mod logs;
#[cfg(feature = "sinks-greptimedb_metrics")]
mod metrics;

/// Compression algorithm for gRPC requests to GreptimeDB.
#[cfg(feature = "sinks-greptimedb_metrics")]
#[configurable_component]
#[derive(Clone, Copy, Debug, Default)]
#[serde(rename_all = "lowercase")]
enum GrpcCompression {
    /// No compression.
    #[default]
    None,
    /// Gzip compression.
    Gzip,
    /// Zstandard compression.
    Zstd,
}

#[cfg(any(
    feature = "sinks-greptimedb_logs",
    feature = "sinks-greptimedb_metrics"
))]
fn default_dbname() -> String {
    greptimedb_ingester::DEFAULT_SCHEMA_NAME.to_string()
}

#[cfg(feature = "sinks-greptimedb_logs")]
fn default_dbname_template() -> Template {
    Template::try_from(default_dbname()).unwrap()
}

#[cfg(feature = "sinks-greptimedb_logs")]
fn default_pipeline_template() -> Template {
    Template::try_from("greptime_identity").unwrap()
}

#[derive(Clone, Copy, Debug, Default)]
struct GreptimeDBDefaultBatchSettings;

impl SinkBatchSettings for GreptimeDBDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(20);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}
