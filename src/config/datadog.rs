use super::{ComponentId, Config, SinkOuter, SourceOuter};
use crate::{
    sinks::datadog::metrics::DatadogConfig, sources::internal_metrics::InternalMetricsConfig,
};
use serde::{Deserialize, Serialize};

static INTERNAL_METRICS_KEY: &str = "#datadog_internal_metrics";
static DATADOG_METRICS_KEY: &str = "#datadog_metrics";

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(default = "default_api_key")]
    pub api_key: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            api_key: default_api_key(),
        }
    }
}

/// By default, the Datadog feature is enabled.
fn default_enabled() -> bool {
    true
}

/// By default, no API key is provided.
fn default_api_key() -> Option<String> {
    None
}

/// Augment configuration with observability via Datadog if the feature is enabled and
/// an API key is provided.
pub fn attach(config: &mut Config) {
    // Return early if an API key is missing, or the feature isn't enabled.
    let api_key = match (&config.datadog.api_key, config.datadog.enabled) {
        (Some(api_key), true) => api_key.clone(),
        _ => return,
    };

    info!("Datadog API key detected. Internal metrics will be sent to Datadog.");

    let internal_metrics_id = ComponentId::from(INTERNAL_METRICS_KEY);
    let datadog_metrics_id = ComponentId::from(DATADOG_METRICS_KEY);

    // Create an internal metrics source. We're using a distinct source here and not
    // attempting to reuse an existing one, due to the use of a custom namespace to
    // satisfy reporting to Datadog.
    let internal_metrics = InternalMetricsConfig::namespace("pipelines");

    config.sources.insert(
        internal_metrics_id.clone(),
        SourceOuter::new(internal_metrics),
    );

    // Create a Datadog metrics sink to consume and emit internal + host metrics.
    let datadog_metrics = DatadogConfig::from_api_key(api_key);

    config.sinks.insert(
        datadog_metrics_id,
        SinkOuter::new(vec![internal_metrics_id], Box::new(datadog_metrics)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default() {
        let config = Config::default();

        // The Datadog config should be enabled by default.
        assert!(config.datadog.enabled);

        // There should be no API key.
        assert_eq!(config.datadog.api_key, None);
    }

    #[test]
    fn disabled() {
        let mut config = Config::default();

        // Attaching config without an API enabled should avoid wiring up components.
        attach(&mut config);

        assert!(!config
            .sources
            .contains_key(&ComponentId::from(INTERNAL_METRICS_KEY)));
        assert!(!config
            .sinks
            .contains_key(&ComponentId::from(DATADOG_METRICS_KEY)));
    }

    #[test]
    fn enabled() {
        let mut config = Config::default();

        // Adding an API key should be enough to enable the feature.
        config.datadog.api_key = Some("xxx".to_string());
        attach(&mut config);

        assert!(config
            .sources
            .contains_key(&ComponentId::from(INTERNAL_METRICS_KEY)));
        assert!(config
            .sinks
            .contains_key(&ComponentId::from(DATADOG_METRICS_KEY)));
    }
}
