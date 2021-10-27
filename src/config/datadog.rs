use super::{ComponentKey, Config, OutputId, SinkOuter, SourceOuter};
use crate::{
    sinks::datadog::metrics::DatadogConfig,
    sources::{host_metrics::HostMetricsConfig, internal_metrics::InternalMetricsConfig},
};
use serde::{Deserialize, Serialize};
use std::env;

static HOST_METRICS_KEY: &str = "#datadog_host_metrics";
static INTERNAL_METRICS_KEY: &str = "#datadog_internal_metrics";
static DATADOG_METRICS_KEY: &str = "#datadog_metrics";

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(default)]
    pub api_key: Option<String>,

    #[serde(default = "default_reporting_interval_secs")]
    pub reporting_interval_secs: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            api_key: None,
            reporting_interval_secs: default_reporting_interval_secs(),
        }
    }
}

/// By default, the Datadog feature is enabled.
const fn default_enabled() -> bool {
    false
}

/// By default, report to Datadog every 5 seconds.
const fn default_reporting_interval_secs() -> u64 {
    5
}

/// Augment configuration with observability via Datadog if the feature is enabled and
/// an API key is provided.
pub fn try_attach(config: &mut Config) -> bool {
    // Return early if an API key is missing, or the feature isn't enabled.
    let api_key = match (&config.datadog.api_key, config.datadog.enabled) {
        // API key provided explicitly.
        (Some(api_key), true) => api_key.clone(),
        // No API key; attempt to get it from the environment.
        (None, true) => match env::var("DATADOG_API_KEY").or_else(|_| env::var("DD_API_KEY")) {
            Ok(api_key) => api_key,
            _ => return false,
        },
        _ => return false,
    };

    info!("Datadog API key provided. Host and internal metrics will be sent to Datadog.");

    let host_metrics_id = OutputId::from(ComponentKey::from(HOST_METRICS_KEY));
    let internal_metrics_id = OutputId::from(ComponentKey::from(INTERNAL_METRICS_KEY));
    let datadog_metrics_id = ComponentKey::from(DATADOG_METRICS_KEY);

    // Create internal sources for host and internal metrics. We're distinct sources here and not
    // attempting to reuse existing ones, to configure it according to enterprise requirements.
    let mut host_metrics =
        HostMetricsConfig::enterprise(config.hash.as_ref().expect("Config should contain hash"));

    // Create an internal metrics source. We're using a distinct source here and not
    // attempting to reuse an existing one, to configure it according to enterprise requirements.
    let mut internal_metrics = InternalMetricsConfig::enterprise(
        config.hash.as_ref().expect("Config should contain hash"),
    );

    // Override default scrape intervals.
    host_metrics.scrape_interval_secs(config.datadog.reporting_interval_secs);
    internal_metrics.scrape_interval_secs(config.datadog.reporting_interval_secs);

    config.sources.insert(
        host_metrics_id.component.clone(),
        SourceOuter::new(host_metrics),
    );
    config.sources.insert(
        internal_metrics_id.component.clone(),
        SourceOuter::new(internal_metrics),
    );

    // Create a Datadog metrics sink to consume and emit internal + host metrics.
    let datadog_metrics = DatadogConfig::from_api_key(api_key);

    config.sinks.insert(
        datadog_metrics_id,
        SinkOuter::new(
            vec![host_metrics_id, internal_metrics_id],
            Box::new(datadog_metrics),
        ),
    );

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_with_hash() -> Config {
        Config {
            hash: Some("".to_owned()),
            ..Config::default()
        }
    }

    #[test]
    fn default() {
        let config = default_with_hash();

        // The Datadog config should be disabled by default.
        assert!(!config.datadog.enabled);

        // There should be no API key.
        assert_eq!(config.datadog.api_key, None);
    }

    #[test]
    fn disabled() {
        let mut config = default_with_hash();

        // Attaching config without an API enabled should avoid wiring up components.
        assert!(!try_attach(&mut config));

        assert!(!config
            .sources
            .contains_key(&ComponentKey::from(HOST_METRICS_KEY)));
        assert!(!config
            .sources
            .contains_key(&ComponentKey::from(INTERNAL_METRICS_KEY)));
        assert!(!config
            .sinks
            .contains_key(&ComponentKey::from(DATADOG_METRICS_KEY)));
    }

    #[test]
    fn enabled() {
        let mut config = default_with_hash();

        // Explicitly set to enabled and provide an API key to activate.
        config.datadog.enabled = true;
        config.datadog.api_key = Some("xxx".to_string());

        assert!(try_attach(&mut config));

        assert!(config
            .sources
            .contains_key(&ComponentKey::from(HOST_METRICS_KEY)));
        assert!(config
            .sources
            .contains_key(&ComponentKey::from(INTERNAL_METRICS_KEY)));
        assert!(config
            .sinks
            .contains_key(&ComponentKey::from(DATADOG_METRICS_KEY)));
    }

    #[test]
    fn default_reporting_interval_secs() {
        let config = default_with_hash();

        // Reporting interval should default to 5 seconds.
        assert_eq!(config.datadog.reporting_interval_secs, 5);
    }
}
