use std::env;

use http::Request;
use hyper::{Body, StatusCode};
use serde::{Deserialize, Serialize};
use vector_core::config::proxy::ProxyConfig;

use super::{
    load_source_from_paths, process_paths, ComponentKey, Config, ConfigPath, OutputId, SinkOuter,
    SourceOuter,
};
use crate::{
    common::datadog::get_api_base_endpoint,
    http::{HttpClient, HttpError},
    sinks::datadog::metrics::DatadogMetricsConfig,
    sources::{host_metrics::HostMetricsConfig, internal_metrics::InternalMetricsConfig},
};

static HOST_METRICS_KEY: &str = "#datadog_host_metrics";
static INTERNAL_METRICS_KEY: &str = "#datadog_internal_metrics";
static DATADOG_METRICS_KEY: &str = "#datadog_metrics";

static DATADOG_REPORTING_PATH_STUB: &str = "/api/unstable/observability_pipelines/configuration";

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct Options {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(default = "default_exit_on_fatal_error")]
    pub exit_on_fatal_error: bool,

    #[serde(default)]
    site: Option<String>,

    #[serde(default)]
    pub api_key: Option<String>,

    pub application_key: String,
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

impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            exit_on_fatal_error: default_exit_on_fatal_error(),
            site: None,
            api_key: None,
            application_key: "".to_owned(),
            configuration_key: "".to_owned(),
            reporting_interval_secs: default_reporting_interval_secs(),
            max_retries: default_max_retries(),
            retry_interval_secs: default_retry_interval_secs(),
            proxy: ProxyConfig::default(),
        }
    }
}

pub enum PipelinesError {
    Disabled,
    MissingApiKey,
    FatalCouldNotReportConfig,
}

/// Holds data required to authorize a request to the Datadog Pipelines reporting endpoint.
struct PipelinesAuth<'a> {
    api_key: &'a str,
    application_key: &'a str,
}

/// Holds the relevant fields for reporting a configuration to Datadog Observability Pipelines.
struct PipelinesStrFields<'a> {
    config_version: &'a str,
    vector_version: &'a str,
}

/// Top-level struct representing the field structure for reporting a config to Datadog Pipelines.
#[derive(Debug, Serialize)]
struct PipelinesVersionPayload<'a> {
    data: PipelinesData<'a>,
}

#[derive(Debug, Serialize)]
struct PipelinesData<'a> {
    attributes: PipelinesAttributes<'a>,
    r#type: &'a str,
}

#[derive(Debug, Serialize)]
struct PipelinesAttributes<'a> {
    config_hash: &'a str,
    vector_version: &'a str,
    config: &'a toml::value::Table,
}

enum ReportingError {
    Http(HttpError),
    StatusCode(StatusCode),
}

impl ReportingError {
    fn to_error_string(&self) -> String {
        match self {
            Self::Http(err) => err.to_string(),
            Self::StatusCode(status) => {
                format!("Request was unsuccessful: {}", status)
            }
        }
    }
}

impl<'a> PipelinesVersionPayload<'a> {
    /// Create a new Pipelines reporting payload from a config and string fields.
    const fn new(config: &'a toml::value::Table, fields: &PipelinesStrFields<'a>) -> Self {
        Self {
            data: PipelinesData {
                attributes: PipelinesAttributes {
                    config_hash: fields.config_version,
                    vector_version: fields.vector_version,
                    config,
                },
                r#type: "pipelines_configuration_version",
            },
        }
    }

    /// Helper method to serialize payload as a JSON string.
    fn json_string(&self) -> String {
        serde_json::to_string(self)
            .expect("couldn't serialize Pipelines fields to JSON. Please report")
    }
}

/// Augment configuration with observability via Datadog if the feature is enabled and
/// an API key is provided.
pub async fn try_attach(
    config: &mut Config,
    config_paths: &[ConfigPath],
) -> Result<(), PipelinesError> {
    // Only valid if a [datadog] section is present in config.
    let datadog = match config.datadog.as_ref() {
        Some(datadog) => datadog,
        _ => return Err(PipelinesError::Disabled),
    };

    // Return early if an API key is missing, or the feature isn't enabled.
    let api_key = match (&datadog.api_key, datadog.enabled) {
        // API key provided explicitly.
        (Some(api_key), true) => api_key.clone(),
        // No API key; attempt to get it from the environment.
        (None, true) => match env::var("DATADOG_API_KEY").or_else(|_| env::var("DD_API_KEY")) {
            Ok(api_key) => api_key,
            _ => return Err(PipelinesError::MissingApiKey),
        },
        _ => return Err(PipelinesError::MissingApiKey),
    };

    info!("Datadog API key provided. Integration with Datadog Observability Pipelines is enabled.");

    // Get the configuration version. In DD Pipelines, this is referred to as the 'config hash'.
    let config_version = config.version.as_ref().expect("Config should be versioned");

    // Get the Vector version. This is reported to Pipelines along with a config hash.
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
    let fields = PipelinesStrFields {
        config_version,
        vector_version: &vector_version,
    };

    // Set the Datadog authorization fields. There's an API and app key, to allow read/write
    // access in tandem with RBAC on the Datadog side.
    let auth = PipelinesAuth {
        api_key: &api_key,
        application_key: &datadog.application_key,
    };

    // Create a HTTP client for posting a Vector version to Datadog Pipelines. This will
    // respect any proxy settings provided in top-level config.
    let client = HttpClient::new(None, &datadog.proxy)
        .expect("couldn't instrument Datadog HTTP client. Please report");

    // Endpoint to report a config to Datadog Pipelines.
    let endpoint = get_reporting_endpoint(datadog.site.as_ref(), &datadog.configuration_key);

    // Datadog uses a JSON:API, so we'll serialize the config to a JSON
    let payload = PipelinesVersionPayload::new(&table, &fields);

    // Attempt to report a config to Datadog. This should happen in a loop, up to a maximum
    // of `max_retries`.
    for _ in 0..datadog.max_retries {
        match report_serialized_config_to_datadog(
            &client,
            build_request(&endpoint, &auth, &payload),
        )
        .await
        {
            Ok(()) => {
                info!(
                    "Vector config {} successfully reported to Datadog Observability Pipelines",
                    &config_version
                );
                break;
            }
            Err(err) => {
                error!(
                    message = "Could not report Vector config to Datadog Observability Pipelines",
                    err = ?err.to_error_string()
                );

                if datadog.exit_on_fatal_error {
                    return Err(PipelinesError::FatalCouldNotReportConfig);
                }
            }
        }
    }

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

    Ok(())
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

/// Returns the full URL endpoint of where to POST a Datadog Vector configuration.
fn get_reporting_endpoint(site: Option<&String>, configuration_key: &str) -> String {
    format!(
        "{}{}/{}/versions",
        get_api_base_endpoint(None, site, None),
        DATADOG_REPORTING_PATH_STUB,
        configuration_key
    )
}

/// Build a POST request for reporting a Vector config to Datadog Pipelines.
fn build_request<'a>(
    endpoint: &'a str,
    auth: &'a PipelinesAuth,
    payload: &'a PipelinesVersionPayload,
) -> Request<Body> {
    Request::post(endpoint)
        .header("DD-API-KEY", auth.api_key)
        .header("DD-APPLICATION-KEY", auth.application_key)
        .body(Body::from(payload.json_string()))
        .expect("couldn't create Datadog Pipelines HTTP request. Please report")
}

/// Reports a JSON serialized Vector config to Datadog, for use with Observability Pipelines.
async fn report_serialized_config_to_datadog(
    client: &HttpClient,
    request: Request<Body>,
) -> Result<(), ReportingError> {
    info!("Attempting to report configuration to Datadog Pipelines");
    let response = client.send(request).await.map_err(ReportingError::Http)?;

    let status = response.status();

    if status.is_success() {
        Ok(())
    } else {
        Err(ReportingError::StatusCode(status))
    }
}
