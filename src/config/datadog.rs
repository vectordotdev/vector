use std::env;

use hyper::Body;
use serde::{Deserialize, Serialize};
use vector_core::config::proxy::ProxyConfig;

use super::{
    load_source_from_paths, process_paths, ComponentKey, Config, ConfigPath, OutputId, SinkOuter,
    SourceOuter,
};
use crate::{
    http::HttpClient,
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

    #[serde(
        default,
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    proxy: ProxyConfig,
}

/// Holds the relevant fields for reporting a configuration to Datadog Observability Pipelines.
struct PipelinesFields<'a> {
    config: &'a toml::value::Table,
    api_key: &'a str,
    app_key: &'a str,
    configuration_key: &'a str,
    config_version: &'a str,
    vector_version: &'a str,
}

#[derive(Debug, Serialize)]
struct PipelinesAttributes<'a> {
    config_hash: &'a str,
    vector_version: &'a str,
    config: &'a toml::value::Table,
}

#[derive(Debug, Serialize)]
struct PipelinesData<'a> {
    attributes: PipelinesAttributes<'a>,
    r#type: &'a str,
}

#[derive(Debug, Serialize)]
struct PipelinesVersionPayload<'a> {
    data: PipelinesData<'a>,
}

impl<'a> PipelinesVersionPayload<'a> {
    fn from_fields(fields: &'a PipelinesFields) -> Self {
        Self {
            data: PipelinesData {
                attributes: PipelinesAttributes {
                    config_hash: fields.config_version,
                    vector_version: fields.vector_version,
                    config: fields.config,
                },
                r#type: "pipelines_configuration_version",
            },
        }
    }

    fn json_string(&self) -> String {
        serde_json::to_string(self)
            .expect("couldn't serialize Pipelines fields to JSON. Please report")
    }
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
            proxy: ProxyConfig::default(),
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

    // Get the configuration version. In DD Pipelines, this is referred to as the 'config hash'.
    let config_version = config.version.as_ref().expect("Config should be versioned");

    // Get the Vector version. This is reported to Pipelines along with a config hash version.
    let vector_version = crate::get_version();

    // Report the internal configuration to Datadog Observability Pipelines.
    // First, we need to create a JSON representation of config, based on the original files
    // that Vector was spawned with.
    let (table, _) = process_paths(config_paths)
        .map(|paths| load_source_from_paths(&paths).ok())
        .flatten()
        .expect("Couldn't load source from config paths. Please report.");

    // Set the relevant fields needed to report a config to Datadog. This is a struct rather than
    // exploding as func arguments to avoid confusion with multiple &str fields.
    let fields = PipelinesFields {
        config: &table,
        config_version: &config_version,
        api_key: &api_key,
        app_key: &datadog.app_key,
        configuration_key: &datadog.configuration_key,
        vector_version: &vector_version,
    };

    report_serialized_config_to_datadog(fields, &datadog.proxy);

    let host_metrics_id = OutputId::from(ComponentKey::from(HOST_METRICS_KEY));
    let internal_metrics_id = OutputId::from(ComponentKey::from(INTERNAL_METRICS_KEY));
    let datadog_metrics_id = ComponentKey::from(DATADOG_METRICS_KEY);

    // Create internal sources for host and internal metrics. We're using distinct sources here and
    // not attempting to reuse existing ones, to configure according to enterprise requirements.
    let mut host_metrics =
        HostMetricsConfig::enterprise(config_version, &datadog.configuration_key);
    let mut internal_metrics =
        InternalMetricsConfig::enterprise(config_version, &datadog.configuration_key);

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
fn report_serialized_config_to_datadog(
    fields: PipelinesFields,
    proxy: &ProxyConfig,
) -> Result<(), PipelinesError> {
    let client = HttpClient::<Body>::new(None, proxy)
        .expect("couldn't instrument Datadog HTTP client. Please report");

    Ok(())
}
