use std::{
    env,
    fmt::{Display, Formatter},
};

use futures_util::{future::BoxFuture, stream::FuturesOrdered, Future, StreamExt};
use http::Request;
use hyper::{header::LOCATION, Body, StatusCode};
use indexmap::IndexMap;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::{
    sync::mpsc::{self},
    time::{sleep, Duration},
};
use url::{ParseError, Url};
use vector_core::config::proxy::ProxyConfig;

use super::{
    load_source_from_paths, process_paths, ComponentKey, Config, ConfigPath, OutputId, SinkOuter,
    SourceOuter, TransformOuter,
};
use crate::{
    built_info,
    common::datadog::{get_api_base_endpoint, Region},
    conditions::AnyCondition,
    http::{HttpClient, HttpError},
    sinks::{
        datadog::{logs::DatadogLogsConfig, metrics::DatadogMetricsConfig},
        util::retries::ExponentialBackoff,
    },
    sources::{
        host_metrics::{Collector, HostMetricsConfig},
        internal_logs::InternalLogsConfig,
        internal_metrics::InternalMetricsConfig,
    },
    transforms::{filter::FilterConfig, remap::RemapConfig},
};

static HOST_METRICS_KEY: &str = "#datadog_host_metrics";
static TAG_METRICS_KEY: &str = "#datadog_tag_metrics";
static TAG_LOGS_KEY: &str = "#datadog_tag_logs";
static FILTER_METRICS_KEY: &str = "#datadog_filter_metrics";
static PIPELINES_NAMESPACE_METRICS_KEY: &str = "#datadog_pipelines_namespace_metrics";
static INTERNAL_METRICS_KEY: &str = "#datadog_internal_metrics";
static INTERNAL_LOGS_KEY: &str = "#datadog_internal_logs";
static DATADOG_METRICS_KEY: &str = "#datadog_metrics";
static DATADOG_LOGS_KEY: &str = "#datadog_logs";

static DATADOG_REPORTING_PRODUCT: &str = "Datadog Observability Pipelines";
static DATADOG_REPORTING_PATH_STUB: &str = "/api/unstable/observability_pipelines/configuration";

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct Options {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(default = "default_enable_logs_reporting")]
    pub enable_logs_reporting: bool,

    #[serde(default)]
    site: Option<String>,
    region: Option<Region>,
    endpoint: Option<String>,

    #[serde(default)]
    pub api_key: Option<String>,

    pub application_key: String,
    pub configuration_key: String,

    #[serde(default = "default_reporting_interval_secs")]
    pub reporting_interval_secs: f64,

    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    #[serde(
        default,
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    proxy: ProxyConfig,

    tags: Option<IndexMap<String, String>>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            enable_logs_reporting: default_enable_logs_reporting(),
            site: None,
            region: None,
            endpoint: None,
            api_key: None,
            application_key: "".to_owned(),
            configuration_key: "".to_owned(),
            reporting_interval_secs: default_reporting_interval_secs(),
            max_retries: default_max_retries(),
            proxy: ProxyConfig::default(),
            tags: None,
        }
    }
}

/// By default, the Datadog feature is enabled.
const fn default_enabled() -> bool {
    true
}

/// By default, internal logs are reported to Datadog.
const fn default_enable_logs_reporting() -> bool {
    true
}

/// By default, report to Datadog every 5 seconds.
const fn default_reporting_interval_secs() -> f64 {
    5.0
}

/// By default, keep retrying (recoverable) failed reporting
const fn default_max_retries() -> u32 {
    u32::MAX
}

/// Enterprise error, relevant to an upstream caller.
pub enum EnterpriseError {
    Disabled,
    MissingApiKey,
}

/// Holds data required to authorize a request to the Datadog OP reporting endpoint.
struct PipelinesAuth<'a> {
    api_key: &'a str,
    application_key: &'a str,
}

/// Holds the relevant fields for reporting a configuration to Datadog Observability Pipelines.
struct PipelinesStrFields<'a> {
    configuration_version_hash: &'a str,
    vector_version: &'a str,
}

/// Top-level struct representing the field structure for reporting a config to Datadog OP.
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

/// Internal reporting error, necessary to determine the severity of an error response.
enum ReportingError {
    Http(HttpError),
    StatusCode(StatusCode),
    EndpointError(ParseError),
    TooManyRedirects,
    InvalidRedirectUrl,
    MaxRetriesReached,
}

impl Display for ReportingError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(err) => write!(f, "{}", err),
            Self::StatusCode(status) => {
                write!(
                    f,
                    "Request was unsuccessful and could not be retried: {}",
                    status
                )
            }
            Self::EndpointError(err) => write!(f, "{}", err),
            Self::TooManyRedirects => {
                write!(f, "Too many redirects from the server")
            }
            Self::InvalidRedirectUrl => write!(f, "Server responded with an invalid redirect URL"),
            Self::MaxRetriesReached => write!(f, "Maximum number of retries reached"),
        }
    }
}

/// Exponential backoff with random jitter for retrying configuration reporting
struct ReportingRetryBackoff {
    backoff: ExponentialBackoff,
}

impl ReportingRetryBackoff {
    /// Retry every 2^n seconds with a maximum delay of 60 seconds (and any
    /// additional jitter)
    const fn new() -> Self {
        let backoff = ExponentialBackoff::from_millis(2)
            .factor(1000)
            .max_delay(Duration::from_secs(60));

        Self { backoff }
    }

    /// Wait before retrying as determined by the backoff and jitter
    async fn wait(&mut self) {
        let retry_backoff = self.next().unwrap();
        info!(
            "Retrying configuration reporting to {} in {} seconds.",
            DATADOG_REPORTING_PRODUCT,
            retry_backoff.as_secs_f32()
        );
        sleep(retry_backoff).await;
    }
}

impl Iterator for ReportingRetryBackoff {
    type Item = Duration;

    fn next(&mut self) -> Option<Self::Item> {
        let jitter_milliseconds = Duration::from_millis(rand::thread_rng().gen_range(0..1000));
        Some(
            self.backoff
                .next()
                .unwrap()
                .saturating_add(jitter_milliseconds),
        )
    }
}

impl<'a> PipelinesVersionPayload<'a> {
    /// Create a new Pipelines reporting payload from a config and string fields.
    const fn new(config: &'a toml::value::Table, fields: &PipelinesStrFields<'a>) -> Self {
        Self {
            data: PipelinesData {
                attributes: PipelinesAttributes {
                    config_hash: fields.configuration_version_hash,
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

#[derive(Clone)]
pub(crate) struct EnterpriseMetadata {
    pub opts: Options,
    pub api_key: String,
    pub configuration_version_hash: String,
}

impl TryFrom<&Config> for EnterpriseMetadata {
    type Error = EnterpriseError;

    fn try_from(value: &Config) -> Result<Self, Self::Error> {
        // Only valid if a [enterprise] section is present in config.
        let opts = match value.enterprise.clone() {
            Some(opts) => opts,
            _ => return Err(EnterpriseError::Disabled),
        };

        // Return early if the feature isn't enabled.
        if !opts.enabled {
            return Err(EnterpriseError::Disabled);
        }

        let api_key = match &opts.api_key {
            // API key provided explicitly.
            Some(api_key) => api_key.clone(),
            // No API key; attempt to get it from the environment.
            None => match env::var("DATADOG_API_KEY").or_else(|_| env::var("DD_API_KEY")) {
                Ok(api_key) => api_key,
                _ => return Err(EnterpriseError::MissingApiKey),
            },
        };

        info!(
            "Datadog API key provided. Integration with {} is enabled.",
            DATADOG_REPORTING_PRODUCT
        );

        // Get the configuration version hash. In DD Pipelines, this is referred to as the 'config hash'.
        let configuration_version_hash = value.version.clone().expect("Config should be versioned");

        Ok(Self {
            opts,
            api_key,
            configuration_version_hash,
        })
    }
}

pub struct EnterpriseReporter<T> {
    reporting_tx: mpsc::UnboundedSender<T>,
}

impl<T> EnterpriseReporter<T>
where
    T: Future<Output = ()> + Send + 'static,
{
    pub fn new() -> Self {
        let (reporting_tx, mut reporting_rx) = mpsc::unbounded_channel();

        // A long running task to report configurations in order
        tokio::spawn(async move {
            let mut pending_reports = FuturesOrdered::new();
            loop {
                tokio::select! {
                    maybe_report = reporting_rx.recv() => {
                        match maybe_report {
                            Some(report) => pending_reports.push(report),
                            None => break,
                        }
                    }
                    _ = pending_reports.next(), if !pending_reports.is_empty() => {
                    }
                }
            }
        });

        Self { reporting_tx }
    }

    pub fn send(&self, reporting_task: T) {
        if let Err(err) = self.reporting_tx.send(reporting_task) {
            error!(
                %err,
                "Unable to report configuration due to internal Vector issue.",
            );
        }
    }
}

impl<T> Default for EnterpriseReporter<T>
where
    T: Future<Output = ()> + Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Report a configuration in a reloading context.
///
/// Returns an [`EnterpriseReporter`] if one was not provided.
pub(crate) fn report_on_reload(
    config: &mut Config,
    metadata: EnterpriseMetadata,
    config_paths: Vec<ConfigPath>,
    enterprise: Option<&EnterpriseReporter<BoxFuture<'static, ()>>>,
) -> Option<EnterpriseReporter<BoxFuture<'static, ()>>> {
    attach_enterprise_components(config, &metadata);

    let enterprise = match enterprise {
        Some(enterprise) => {
            enterprise.send(report_configuration(config_paths, metadata));
            None
        }
        None => {
            let enterprise = EnterpriseReporter::new();
            enterprise.send(report_configuration(config_paths, metadata));
            Some(enterprise)
        }
    };

    enterprise
}

pub(crate) fn attach_enterprise_components(config: &mut Config, metadata: &EnterpriseMetadata) {
    let api_key = metadata.api_key.clone();
    let configuration_version_hash = metadata.configuration_version_hash.clone();

    setup_metrics_reporting(
        config,
        &metadata.opts,
        api_key.clone(),
        configuration_version_hash.clone(),
    );

    if metadata.opts.enable_logs_reporting {
        setup_logs_reporting(config, &metadata.opts, api_key, configuration_version_hash);
    }
}

fn setup_logs_reporting(
    config: &mut Config,
    datadog: &Options,
    api_key: String,
    configuration_version_hash: String,
) {
    let tag_logs_id = OutputId::from(ComponentKey::from(TAG_LOGS_KEY));
    let internal_logs_id = OutputId::from(ComponentKey::from(INTERNAL_LOGS_KEY));
    let datadog_logs_id = ComponentKey::from(DATADOG_LOGS_KEY);

    let internal_logs = InternalLogsConfig {
        ..Default::default()
    };

    let custom_logs_tags_vrl = datadog
        .tags
        .as_ref()
        .map_or("".to_string(), |tags| convert_tags_to_vrl(tags, false));

    let configuration_key = &datadog.configuration_key;
    let vector_version = crate::vector_version();
    let build_arch = built_info::TARGET_ARCH;
    let build_os = built_info::TARGET_OS;
    let build_vendor = built_info::TARGET_VENDOR;
    let tag_logs = RemapConfig {
        source: Some(format!(
            r#"
            .ddsource = "vector"
            .vector.configuration_key = "{configuration_key}"
            .vector.configuration_version_hash = "{configuration_version_hash}"
            .vector.version = "{vector_version}"
            .vector.arch = "{build_arch}"
            .vector.os = "{build_os}"
            .vector.vendor = "{build_vendor}"
            {}
        "#,
            custom_logs_tags_vrl,
        )),
        ..Default::default()
    };

    // Create a Datadog logs sink to consume and emit internal logs.
    let datadog_logs = DatadogLogsConfig {
        default_api_key: api_key,
        endpoint: datadog.endpoint.clone(),
        site: datadog.site.clone(),
        region: datadog.region,
        enterprise: true,
        ..Default::default()
    };

    config.sources.insert(
        internal_logs_id.component.clone(),
        SourceOuter::new(internal_logs),
    );

    config.transforms.insert(
        tag_logs_id.component.clone(),
        TransformOuter::new(vec![internal_logs_id], tag_logs),
    );

    config.sinks.insert(
        datadog_logs_id,
        SinkOuter::new(vec![tag_logs_id], Box::new(datadog_logs)),
    );
}

fn setup_metrics_reporting(
    config: &mut Config,
    datadog: &Options,
    api_key: String,
    configuration_version_hash: String,
) {
    let host_metrics_id = OutputId::from(ComponentKey::from(HOST_METRICS_KEY));
    let tag_metrics_id = OutputId::from(ComponentKey::from(TAG_METRICS_KEY));
    let internal_metrics_id = OutputId::from(ComponentKey::from(INTERNAL_METRICS_KEY));
    let filter_metrics_id = OutputId::from(ComponentKey::from(FILTER_METRICS_KEY));
    let pipelines_namespace_metrics_id =
        OutputId::from(ComponentKey::from(PIPELINES_NAMESPACE_METRICS_KEY));
    let datadog_metrics_id = ComponentKey::from(DATADOG_METRICS_KEY);

    // Create internal sources for host and internal metrics. We're using distinct sources here and
    // not attempting to reuse existing ones, to configure according to enterprise requirements.

    // By default, host_metrics generates many metrics and some with high
    // cardinality which can negatively impact customers' costs and downstream
    // systems' performance. To avoid this, we explicitly set `collectors`.
    let host_metrics = HostMetricsConfig {
        namespace: Some("vector.host".to_owned()),
        scrape_interval_secs: datadog.reporting_interval_secs,
        collectors: Some(vec![
            Collector::Cpu,
            Collector::Disk,
            Collector::Load,
            Collector::Host,
            Collector::Memory,
            Collector::Network,
        ]),
        ..Default::default()
    };

    let internal_metrics = InternalMetricsConfig {
        // While the default namespace for internal metrics is already "vector",
        // setting the namespace here is meant for clarity and resistance
        // against any future or accidental changes.
        namespace: Some("vector".to_owned()),
        scrape_interval_secs: datadog.reporting_interval_secs,
        ..Default::default()
    };

    let custom_metric_tags_vrl = datadog
        .tags
        .as_ref()
        .map_or("".to_string(), |tags| convert_tags_to_vrl(tags, true));

    let configuration_key = &datadog.configuration_key;
    let vector_version = crate::vector_version();
    let tag_metrics = RemapConfig {
        source: Some(format!(
            r#"
            .tags.configuration_version_hash = "{configuration_version_hash}"
            .tags.configuration_key = "{configuration_key}"
            .tags.vector_version = "{vector_version}"
            {}
        "#,
            custom_metric_tags_vrl
        )),
        ..Default::default()
    };

    // Preserve the `pipelines` namespace for specific metrics
    let filter_metrics = FilterConfig::from(AnyCondition::String(
        r#".name == "component_received_bytes_total""#.to_string(),
    ));

    let pipelines_namespace_metrics = RemapConfig {
        source: Some(r#".namespace = "pipelines""#.to_string()),
        ..Default::default()
    };

    // Create a Datadog metrics sink to consume and emit internal + host metrics.
    let datadog_metrics = DatadogMetricsConfig {
        default_api_key: api_key,
        endpoint: datadog.endpoint.clone(),
        site: datadog.site.clone(),
        region: datadog.region,
        ..Default::default()
    };

    config.sources.insert(
        host_metrics_id.component.clone(),
        SourceOuter::new(host_metrics),
    );
    config.sources.insert(
        internal_metrics_id.component.clone(),
        SourceOuter::new(internal_metrics),
    );

    config.transforms.insert(
        tag_metrics_id.component.clone(),
        TransformOuter::new(vec![host_metrics_id, internal_metrics_id], tag_metrics),
    );

    config.transforms.insert(
        filter_metrics_id.component.clone(),
        TransformOuter::new(vec![tag_metrics_id.clone()], filter_metrics),
    );

    config.transforms.insert(
        pipelines_namespace_metrics_id.component.clone(),
        TransformOuter::new(vec![filter_metrics_id], pipelines_namespace_metrics),
    );

    config.sinks.insert(
        datadog_metrics_id,
        SinkOuter::new(
            vec![tag_metrics_id, pipelines_namespace_metrics_id],
            Box::new(datadog_metrics),
        ),
    );
}

/// Converts user configured tags to VRL source code for adding tags/fields to
/// events
fn convert_tags_to_vrl(tags: &IndexMap<String, String>, is_metric: bool) -> String {
    let json_tags = serde_json::to_string(&tags).unwrap();
    if is_metric {
        format!(r#".tags = merge(.tags, {}, deep: true)"#, json_tags)
    } else {
        format!(r#". = merge(., {}, deep: true)"#, json_tags)
    }
}

/// Report the internal configuration to Datadog Observability Pipelines.
pub(crate) fn report_configuration(
    config_paths: Vec<ConfigPath>,
    metadata: EnterpriseMetadata,
) -> BoxFuture<'static, ()> {
    let fut = async move {
        let EnterpriseMetadata {
            api_key,
            configuration_version_hash,
            opts,
        } = metadata;

        // Get the Vector version. This is reported to Pipelines along with a config hash.
        let vector_version = crate::get_version();

        // We need to create a JSON representation of config, based on the original files
        // that Vector was spawned with.
        let (table, _) = process_paths(&config_paths)
            .map(|paths| load_source_from_paths(&paths).ok())
            .flatten()
            .expect("Couldn't load source from config paths. Please report.");

        // Set the relevant fields needed to report a config to Datadog. This is a struct rather than
        // exploding as func arguments to avoid confusion with multiple &str fields.
        let fields = PipelinesStrFields {
            configuration_version_hash: &configuration_version_hash,
            vector_version: &vector_version,
        };

        // Set the Datadog authorization fields. There's an API and app key, to allow read/write
        // access in tandem with RBAC on the Datadog side.
        let auth = PipelinesAuth {
            api_key: &api_key,
            application_key: &opts.application_key,
        };

        // Create a HTTP client for posting a Vector version to Datadog OP. This will
        // respect any proxy settings provided in top-level config.
        let client = HttpClient::new(None, &opts.proxy)
            .expect("couldn't instrument Datadog HTTP client. Please report");

        // Endpoint to report a config to Datadog OP.
        let endpoint = get_reporting_endpoint(
            opts.endpoint.as_ref(),
            opts.site.as_ref(),
            opts.region,
            &opts.configuration_key,
        );
        // Datadog uses a JSON:API, so we'll serialize the config to a JSON
        let payload = PipelinesVersionPayload::new(&table, &fields);

        match report_serialized_config_to_datadog(
            &client,
            &endpoint,
            &auth,
            &payload,
            opts.max_retries,
        )
        .await
        {
            Ok(()) => {
                info!(
                    "Vector config {} successfully reported to {}.",
                    &configuration_version_hash, DATADOG_REPORTING_PRODUCT
                );
            }
            Err(err) => {
                error!(
                    err = ?err.to_string(),
                    "Could not report Vector config to {}.", DATADOG_REPORTING_PRODUCT
                );
            }
        }
    };

    Box::pin(fut)
}

/// Returns the full URL endpoint of where to POST a Datadog Vector configuration.
fn get_reporting_endpoint(
    endpoint: Option<&String>,
    site: Option<&String>,
    region: Option<Region>,
    configuration_key: &str,
) -> String {
    format!(
        "{}{}/{}/versions",
        get_api_base_endpoint(endpoint, site, region),
        DATADOG_REPORTING_PATH_STUB,
        configuration_key
    )
}

/// Build a POST request for reporting a Vector config to Datadog OP.
fn build_request<'a>(
    endpoint: &Url,
    auth: &'a PipelinesAuth,
    payload: &'a PipelinesVersionPayload,
) -> Request<Body> {
    Request::post(endpoint.to_string())
        .header("DD-API-KEY", auth.api_key)
        .header("DD-APPLICATION-KEY", auth.application_key)
        .body(Body::from(payload.json_string()))
        .unwrap_or_else(|_| {
            panic!(
                "couldn't create {} HTTP request. Please report",
                DATADOG_REPORTING_PRODUCT
            )
        })
}

/// Reports a JSON serialized Vector config to Datadog, for use with Observability Pipelines.
async fn report_serialized_config_to_datadog<'a>(
    client: &'a HttpClient,
    endpoint: &'a str,
    auth: &'a PipelinesAuth<'a>,
    payload: &'a PipelinesVersionPayload<'a>,
    max_retries: u32,
) -> Result<(), ReportingError> {
    info!(
        "Attempting to report configuration to {}.",
        DATADOG_REPORTING_PRODUCT
    );

    let mut endpoint = Url::parse(endpoint).map_err(ReportingError::EndpointError)?;
    let mut redirected = false;
    let mut backoff = ReportingRetryBackoff::new();
    let mut retries = 0;

    while retries < max_retries {
        retries += 1;
        let req = build_request(&endpoint, auth, payload);
        let res = client.send(req).await;
        if let Err(HttpError::CallRequest { source: error }) = &res {
            // Retry on request timeouts and network issues
            if error.is_timeout() {
                info!(message = "Configuration reporting request timed out.", error = %error);
                backoff.wait().await;
                continue;
            } else if error.is_connect() {
                warn!(error = %error, "Configuration reporting connection issue.");
                backoff.wait().await;
                continue;
            }
        }
        let res = res.map_err(ReportingError::Http)?;
        let status = res.status();

        // Follow redirection responses a maximum of one time.
        if status.is_redirection() && !redirected {
            redirected = true;
            // A `Location` header could contain a relative path. To guard against that, we'll
            // join the location to the original URL to get a new absolute path.
            endpoint = endpoint
                .join(
                    res.headers()
                        .get(LOCATION)
                        .ok_or(ReportingError::InvalidRedirectUrl)?
                        .to_str()
                        .map_err(|_| ReportingError::InvalidRedirectUrl)?,
                )
                .map_err(ReportingError::EndpointError)?;
            info!(message = "Configuration reporting request redirected.", endpoint = %endpoint);
            continue;
        } else if status.is_redirection() && redirected {
            return Err(ReportingError::TooManyRedirects);
        } else if status.is_client_error() || status.is_server_error() {
            info!(message = "Encountered retriable error.", status = %status);
            backoff.wait().await;
            continue;
        } else if status.is_success() {
            return Ok(());
        } else {
            return Err(ReportingError::StatusCode(status));
        }
    }

    Err(ReportingError::MaxRetriesReached)
}

#[cfg(all(test, feature = "enterprise-tests"))]
mod test {
    use std::{
        collections::BTreeMap, io::Write, net::TcpListener, path::PathBuf, str::FromStr, thread,
        time::Duration,
    };

    use http::StatusCode;
    use indexmap::IndexMap;
    use indoc::formatdoc;
    use tokio::time::sleep;
    use value::Kind;
    use vector_common::btreemap;
    use vector_core::config::proxy::ProxyConfig;
    use wiremock::{matchers, Mock, MockServer, ResponseTemplate};

    use super::{
        report_serialized_config_to_datadog, PipelinesAuth, PipelinesStrFields,
        PipelinesVersionPayload,
    };
    use crate::{
        app::Application,
        cli::{Color, LogFormat, Opts, RootOpts},
        config::enterprise::{convert_tags_to_vrl, default_max_retries},
        http::HttpClient,
        metrics,
        test_util::next_addr,
    };

    const fn get_pipelines_auth() -> PipelinesAuth<'static> {
        PipelinesAuth {
            api_key: "api_key",
            application_key: "application_key",
        }
    }

    const fn get_pipelines_fields() -> PipelinesStrFields<'static> {
        PipelinesStrFields {
            configuration_version_hash: "configuration_version_hash",
            vector_version: "vector_version",
        }
    }

    /// This mocked server will reply with the configured status code 3 times
    /// before falling back to a 200 OK
    async fn build_test_server_error_and_recover(status_code: StatusCode) -> MockServer {
        let mock_server = MockServer::start().await;

        Mock::given(matchers::method("POST"))
            .respond_with(ResponseTemplate::new(status_code))
            .up_to_n_times(3)
            .with_priority(1)
            .mount(&mock_server)
            .await;

        Mock::given(matchers::method("POST"))
            .respond_with(ResponseTemplate::new(StatusCode::OK))
            .with_priority(2)
            .mount(&mock_server)
            .await;

        mock_server
    }

    fn get_vector_config_file(config: impl Into<String>) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let _ = writeln!(file, "{}", config.into());
        file
    }

    fn get_root_opts(config_path: PathBuf) -> RootOpts {
        RootOpts {
            config_paths: vec![config_path],
            config_dirs: vec![],
            config_paths_toml: vec![],
            config_paths_json: vec![],
            config_paths_yaml: vec![],
            require_healthy: None,
            threads: None,
            verbose: 0,
            quiet: 3,
            log_format: LogFormat::from_str("text").unwrap(),
            color: Color::from_str("auto").unwrap(),
            watch_config: false,
        }
    }

    #[tokio::test]
    async fn retry_on_specific_client_error_status_codes() {
        let server = build_test_server_error_and_recover(StatusCode::REQUEST_TIMEOUT).await;

        let endpoint = server.uri();
        let client =
            HttpClient::new(None, &ProxyConfig::default()).expect("Failed to create http client");
        let auth = get_pipelines_auth();
        let fields = get_pipelines_fields();
        let config = toml::map::Map::new();
        let payload = PipelinesVersionPayload::new(&config, &fields);

        assert!(report_serialized_config_to_datadog(
            &client,
            endpoint.as_ref(),
            &auth,
            &payload,
            default_max_retries()
        )
        .await
        .is_ok());
    }

    #[tokio::test]
    async fn retry_on_server_error_status_codes() {
        let server = build_test_server_error_and_recover(StatusCode::INTERNAL_SERVER_ERROR).await;

        let endpoint = server.uri();
        let client =
            HttpClient::new(None, &ProxyConfig::default()).expect("Failed to create http client");
        let auth = get_pipelines_auth();
        let fields = get_pipelines_fields();
        let config = toml::map::Map::new();
        let payload = PipelinesVersionPayload::new(&config, &fields);

        assert!(report_serialized_config_to_datadog(
            &client,
            endpoint.as_ref(),
            &auth,
            &payload,
            default_max_retries()
        )
        .await
        .is_ok());
    }

    #[tokio::test]
    async fn retry_on_loss_of_network_connection() {
        let addr = next_addr();
        let endpoint = format!("http://{}:{}", addr.ip(), addr.port());

        let report = tokio::spawn(async move {
            let client = HttpClient::new(None, &ProxyConfig::default())
                .expect("Failed to create http client");
            let auth = get_pipelines_auth();
            let fields = get_pipelines_fields();
            let config = toml::map::Map::new();
            let payload = PipelinesVersionPayload::new(&config, &fields);

            report_serialized_config_to_datadog(
                &client,
                endpoint.as_ref(),
                &auth,
                &payload,
                default_max_retries(),
            )
            .await
        });
        sleep(Duration::from_secs(2)).await;

        // The server is completely unavailable when initially reporting to
        // simulate a network/connection failure
        let listener = TcpListener::bind(addr).unwrap();
        let server = MockServer::builder().listener(listener).start().await;
        Mock::given(matchers::method("POST"))
            .respond_with(ResponseTemplate::new(StatusCode::OK))
            .mount(&server)
            .await;

        let res = report.await.unwrap();
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn error_exceed_max_retries() {
        let server = build_test_server_error_and_recover(StatusCode::INTERNAL_SERVER_ERROR).await;

        let endpoint = server.uri();
        let client =
            HttpClient::new(None, &ProxyConfig::default()).expect("Failed to create http client");
        let auth = get_pipelines_auth();
        let fields = get_pipelines_fields();
        let config = toml::map::Map::new();
        let payload = PipelinesVersionPayload::new(&config, &fields);

        assert!(report_serialized_config_to_datadog(
            &client,
            endpoint.as_ref(),
            &auth,
            &payload,
            1
        )
        .await
        .is_err());
    }

    /// This test asserts that configuration reporting errors do NOT impact the
    /// rest of Vector starting and running.
    ///
    /// In general, Vector should continue operating even in the event that the
    /// enterprise API is down/having issues. Do not modify this behavior
    /// without prior approval.
    #[tokio::test]
    async fn vector_continues_on_reporting_error() {
        let _ = metrics::init_test();

        let server = build_test_server_error_and_recover(StatusCode::NOT_IMPLEMENTED).await;
        let endpoint = server.uri();

        let vector_config = formatdoc! {r#"
            [enterprise]
            application_key = "application_key"
            api_key = "api_key"
            configuration_key = "configuration_key"
            endpoint = "{endpoint}"
            max_retries = 1

            [sources.in]
            type = "demo_logs"
            format = "syslog"
            count = 3

            [sinks.out]
            type = "blackhole"
            inputs = ["*"]
        "#, endpoint=endpoint};

        let config_file = get_vector_config_file(vector_config);

        let opts = Opts {
            root: get_root_opts(config_file.path().to_path_buf()),
            sub_command: None,
        };

        // Spawn a separate thread to avoid nested async runtime errors
        let vector_continued = thread::spawn(|| {
            // Configuration reporting is guaranteed to fail here due to API
            // server issues. However, the app should still start up and run.
            Application::prepare_from_opts(opts).map_or(false, |app| {
                // Finish running the topology to avoid error logs
                app.run();
                true
            })
        })
        .join()
        .unwrap();

        assert!(!server.received_requests().await.unwrap().is_empty());
        assert!(vector_continued);
    }

    #[tokio::test]
    async fn vector_does_not_start_with_enterprise_misconfigured() {
        let _ = metrics::init_test();

        let server = build_test_server_error_and_recover(StatusCode::NOT_IMPLEMENTED).await;
        let endpoint = server.uri();

        let vector_config = formatdoc! {r#"
            [enterprise]
            application_key = "application_key"
            configuration_key = "configuration_key"
            endpoint = "{endpoint}"
            max_retries = 1

            [sources.in]
            type = "demo_logs"
            format = "syslog"
            count = 1
            interval = 0.0

            [sinks.out]
            type = "blackhole"
            inputs = ["*"]
        "#, endpoint=endpoint};

        let config_file = get_vector_config_file(vector_config);

        let opts = Opts {
            root: get_root_opts(config_file.path().to_path_buf()),
            sub_command: None,
        };

        let vector_failed_to_start = thread::spawn(|| {
            // With [enterprise] configured but no API key, starting the app
            // should fail
            Application::prepare_from_opts(opts).is_err()
        })
        .join()
        .unwrap();

        assert!(server.received_requests().await.unwrap().is_empty());
        assert!(vector_failed_to_start);
    }

    #[test]
    fn dynamic_tags_to_remap_config_for_metrics() {
        let tags = IndexMap::from([
            ("pull_request".to_string(), "1234".to_string()),
            ("replica".to_string(), "abcd".to_string()),
            ("variant".to_string(), "baseline".to_string()),
        ]);

        let vrl = convert_tags_to_vrl(&tags, true);
        assert_eq!(
            vrl,
            r#".tags = merge(.tags, {"pull_request":"1234","replica":"abcd","variant":"baseline"}, deep: true)"#
        );
        // We need to set up some state here to inform the VRL compiler that
        // .tags is an object and merge() is thus a safe operation (mimicking
        // the environment this code will actually run in).
        let mut state = vrl::state::ExternalEnv::new_with_kind(Kind::object(btreemap! {
            "tags" => Kind::object(BTreeMap::new()),
        }));
        assert!(
            vrl::compile_with_state(vrl.as_str(), vrl_stdlib::all().as_ref(), &mut state).is_ok()
        );
    }

    #[test]
    fn dynamic_tags_to_remap_config_for_logs() {
        let tags = IndexMap::from([
            ("pull_request".to_string(), "1234".to_string()),
            ("replica".to_string(), "abcd".to_string()),
            ("variant".to_string(), "baseline".to_string()),
        ]);
        let vrl = convert_tags_to_vrl(&tags, false);

        assert_eq!(
            vrl,
            r#". = merge(., {"pull_request":"1234","replica":"abcd","variant":"baseline"}, deep: true)"#
        );
        assert!(vrl::compile(vrl.as_str(), vrl_stdlib::all().as_ref()).is_ok());
    }
}
