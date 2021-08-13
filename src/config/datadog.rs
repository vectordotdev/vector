use super::{Config, SinkOuter, SourceOuter};
use crate::{
    sinks::datadog::metrics::DatadogConfig, sources::internal_metrics::InternalMetricsConfig,
};
use std::env::var;

// The '#' character here is being used to denote an internal name. It's 'unspeakable'
// in default TOML configuration, but could clash in JSON config so this isn't fool-proof.
// TODO: Refactor for component scope once https://github.com/timberio/vector/pull/8654 lands.
static INTERNAL_METRICS_NAME: &'static str = "#datadog_internal_metrics";
static DATADOG_METRICS_NAME: &'static str = "#datadog_sink";

/// Attempts to retrieve a Datadog API key from the environment.
pub fn get_api_key() -> Option<String> {
    var("DATADOG_API_KEY").or_else(|_| var("DD_API_KEY")).ok()
}

/// Augment configuration with observability via Datadog.
pub fn init<K: Into<String>>(config: &mut Config, api_key: K) {
    // Create an internal metrics source. We may eventually re-use an existing source if
    // defined, but this introduces tighter coupling with user-land config, so this is a
    // distinct source for now.
    let internal_metrics = InternalMetricsConfig::default();

    config.sources.insert(
        INTERNAL_METRICS_NAME.to_string(),
        SourceOuter::new(internal_metrics),
    );

    // Create a Datadog metrics sink to consume and emit internal + host metrics.
    let datadog_metrics = DatadogConfig {
        api_key: api_key.into(),
        default_namespace: Some("pipelines".to_string()),
        ..DatadogConfig::default()
    };

    config.sinks.insert(
        DATADOG_METRICS_NAME.to_string(),
        SinkOuter::new(
            vec![INTERNAL_METRICS_NAME.to_string()],
            Box::new(datadog_metrics),
        ),
    );
}
