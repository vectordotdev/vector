use std::env;

use serde::{Deserialize, Serialize};

use super::{
    load_source_from_paths, process_paths, ComponentKey, Config, ConfigPath, OutputId, SinkOuter,
    SourceOuter,
};
use crate::{
    sinks::datadog::metrics::DatadogMetricsConfig,
    sources::{host_metrics::HostMetricsConfig, internal_metrics::InternalMetricsConfig},
};

static HOST_METRICS_KEY: &str = "#datadog_host_metrics";
static INTERNAL_METRICS_KEY: &str = "#datadog_internal_metrics";
static DATADOG_METRICS_KEY: &str = "#datadog_metrics";

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct Options {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(default = "default_exit_on_fatal_error")]
    pub exit_on_fatal_error: bool,

    #[serde(default)]
    pub api_key: Option<String>,

    pub app_key: String,
    pub configuration_key: String,

    #[serde(default = "default_reporting_interval_secs")]
    pub reporting_interval_secs: f64,

    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    #[serde(default = "default_retry_interval_secs")]
    pub retry_interval_secs: u32,
}

/// Holds the relevant fields for reporting a configuration to Datadog Observability Pipelines.
struct PipelinesFields<'a> {
    config: &'a str,
    api_key: &'a str,
    app_key: &'a str,
    configuration_key: &'a str,
}

/// Error conditions that indicate how reporting a configuration to Datadog Observability Pipelines
/// failed. Callers can then determine whether reattempt(s) are relevant.
enum PipelinesError {
    Unauthorized,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            exit_on_fatal_error: default_exit_on_fatal_error(),
            api_key: None,
            app_key: "".to_owned(),
            configuration_key: "".to_owned(),
            reporting_interval_secs: default_reporting_interval_secs(),
            max_retries: default_max_retries(),
            retry_interval_secs: default_retry_interval_secs(),
        }
    }
}

/// By default, the Datadog feature is enabled.
const fn default_enabled() -> bool {
    true
}

/// By default, Vector should exit when a fatal reporting error is encountered.
const fn default_exit_on_fatal_error() -> bool {
    true
}

/// By default, report to Datadog every 5 seconds.
const fn default_reporting_interval_secs() -> f64 {
    5.0
}

/// By default, keep retrying (recoverable) failed reporting (infinitely, for practical purposes.)
const fn default_max_retries() -> u32 {
    u32::MAX
}

/// By default, retry (recoverable) failed reporting every 5 seconds.
const fn default_retry_interval_secs() -> u32 {
    5
}

/// Augment configuration with observability via Datadog if the feature is enabled and
/// an API key is provided.
pub fn try_attach(config: &mut Config, config_paths: &[ConfigPath]) -> bool {
    // Only valid if a [datadog] section is present in config.
    let datadog = match config.datadog.as_ref() {
        Some(datadog) => datadog,
        _ => return false,
    };

    // Return early if an API key is missing, or the feature isn't enabled.
    let api_key = match (&datadog.api_key, datadog.enabled) {
        // API key provided explicitly.
        (Some(api_key), true) => api_key.clone(),
        // No API key; attempt to get it from the environment.
        (None, true) => match env::var("DATADOG_API_KEY").or_else(|_| env::var("DD_API_KEY")) {
            Ok(api_key) => api_key,
            _ => return false,
        },
        _ => return false,
    };

    info!("Datadog API key provided. Integration with Datadog Observability Pipelines is enabled.");

    let version = config.version.as_ref().expect("Config should be versioned");

    // Report the internal configuration to Datadog Observability Pipelines.
    // First, we need to create a JSON representation of config, based on the original files
    // that Vector was spawned with.
    let (table, _) = process_paths(config_paths)
        .map(|paths| load_source_from_paths(&paths).ok())
        .flatten()
        .expect("Couldn't load source from config paths. Please report.");

    // Serializing a TOML table as JSON should always succeed.
    let config_json =
        serde_json::to_string(&table).expect("Couldn't serialise config as JSON. Please report.");

    // Set the relevant fields needed to report a config to Datadog. This is a struct rather than
    // exploding as func arguments to avoid confusion with multiple &str fields.
    let fields = PipelinesFields {
        config: &config_json,
        api_key: &api_key,
        app_key: &datadog.app_key,
        configuration_key: &datadog.configuration_key,
    };

    report_serialized_config_to_datadog(fields);

    let host_metrics_id = OutputId::from(ComponentKey::from(HOST_METRICS_KEY));
    let internal_metrics_id = OutputId::from(ComponentKey::from(INTERNAL_METRICS_KEY));
    let datadog_metrics_id = ComponentKey::from(DATADOG_METRICS_KEY);

    // Create internal sources for host and internal metrics. We're using distinct sources here and
    // not attempting to reuse existing ones, to configure according to enterprise requirements.
    let mut host_metrics = HostMetricsConfig::enterprise(version, &datadog.configuration_key);
    let mut internal_metrics =
        InternalMetricsConfig::enterprise(version, &datadog.configuration_key);

    // Override default scrape intervals.
    host_metrics.scrape_interval_secs(datadog.reporting_interval_secs);
    internal_metrics.scrape_interval_secs(datadog.reporting_interval_secs);

    config.sources.insert(
        host_metrics_id.component.clone(),
        SourceOuter::new(host_metrics),
    );
    config.sources.insert(
        internal_metrics_id.component.clone(),
        SourceOuter::new(internal_metrics),
    );

    // Create a Datadog metrics sink to consume and emit internal + host metrics.
    let datadog_metrics = DatadogMetricsConfig::from_api_key(api_key);

    config.sinks.insert(
        datadog_metrics_id,
        SinkOuter::new(
            vec![host_metrics_id, internal_metrics_id],
            Box::new(datadog_metrics),
        ),
    );

    true
}

/// Reports a JSON serialized Vector config to Datadog, for use with Observability Pipelines.
fn report_serialized_config_to_datadog(fields: PipelinesFields) -> Result<(), PipelinesError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_with_hash() -> Config {
        Config {
            version: Some("".to_owned()),
            ..Config::default()
        }
    }

    #[test]
    fn default() {
        let config = default_with_hash();

        // The Datadog config shouldn't exist by default.
        assert!(config.datadog.is_none());
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

        config.datadog = Some(Options {
            api_key: Some("xxx".to_owned()),
            configuration_key: "zzz".to_owned(),
            ..Options::default()
        });

        // Explicitly set to enabled and provide an API key to activate.
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
}
