use std::{
    env,
    fmt::{Display, Formatter},
};

use http::Request;
use hyper::{header::LOCATION, Body, StatusCode};
use indexmap::IndexMap;
use rand::{prelude::ThreadRng, Rng};
use serde::{Deserialize, Serialize};
use tokio::{
    select,
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
    http::{HttpClient, HttpError},
    signal::{SignalRx, SignalTo},
    sinks::{
        datadog::{logs::DatadogLogsConfig, metrics::DatadogMetricsConfig},
        util::retries::ExponentialBackoff,
    },
    sources::{
        host_metrics::{self, HostMetricsConfig},
        internal_logs::InternalLogsConfig,
        internal_metrics::InternalMetricsConfig,
    },
    transforms::remap::RemapConfig,
};

static HOST_METRICS_KEY: &str = "#datadog_host_metrics";
static TAG_METRICS_KEY: &str = "#datadog_tag_metrics";
static TAG_LOGS_KEY: &str = "#datadog_tag_logs";
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

    #[serde(default = "default_exit_on_fatal_error")]
    pub exit_on_fatal_error: bool,

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
            exit_on_fatal_error: default_exit_on_fatal_error(),
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

/// Pipelines error, relevant to an upstream caller. This abstracts away HTTP-specific error
/// codes that are implementation details of whether we consider a request successful or not.
pub enum PipelinesError {
    Disabled,
    MissingApiKey,
    FatalCouldNotReportConfig,
    CouldNotReportConfig,
    Interrupt,
}

/// Holds data required to authorize a request to the Datadog OP reporting endpoint.
struct PipelinesAuth<'a> {
    api_key: &'a str,
    application_key: &'a str,
}

/// Holds the relevant fields for reporting a configuration to Datadog Observability Pipelines.
struct PipelinesStrFields<'a> {
    config_version: &'a str,
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
    jitter_rng: ThreadRng,
}

impl ReportingRetryBackoff {
    /// Retry every 2^n seconds with a maximum delay of 60 seconds (and any
    /// additional jitter)
    fn new() -> Self {
        let backoff = ExponentialBackoff::from_millis(2)
            .factor(1000)
            .max_delay(Duration::from_secs(60));
        let jitter_rng = rand::thread_rng();

        Self {
            backoff,
            jitter_rng,
        }
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
        let jitter_milliseconds = Duration::from_millis(self.jitter_rng.gen_range(0..1000));
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
    mut signal_rx: SignalRx,
) -> Result<(), PipelinesError> {
    // Only valid if a [enterprise] section is present in config.
    let datadog = match config.enterprise.clone() {
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

    info!(
        "Datadog API key provided. Integration with {} is enabled.",
        DATADOG_REPORTING_PRODUCT
    );

    // Get the configuration version. In DD Pipelines, this is referred to as the 'config hash'.
    let config_version = config.version.clone().expect("Config should be versioned");

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
        config_version: config_version.as_ref(),
        vector_version: &vector_version,
    };

    // Set the Datadog authorization fields. There's an API and app key, to allow read/write
    // access in tandem with RBAC on the Datadog side.
    let auth = PipelinesAuth {
        api_key: &api_key,
        application_key: &datadog.application_key,
    };

    // Create a HTTP client for posting a Vector version to Datadog OP. This will
    // respect any proxy settings provided in top-level config.
    let client = HttpClient::new(None, &datadog.proxy)
        .expect("couldn't instrument Datadog HTTP client. Please report");

    // Endpoint to report a config to Datadog OP.
    let endpoint = get_reporting_endpoint(
        datadog.endpoint.as_ref(),
        datadog.site.as_ref(),
        datadog.region,
        &datadog.configuration_key,
    );

    // Datadog uses a JSON:API, so we'll serialize the config to a JSON
    let payload = PipelinesVersionPayload::new(&table, &fields);

    select! {
        biased;
        Ok(SignalTo::Shutdown | SignalTo::Quit) = signal_rx.recv() => return Err(PipelinesError::Interrupt),
        report = report_serialized_config_to_datadog(&client, &endpoint, &auth, &payload, datadog.max_retries) => {
            match report {
                Ok(()) => {
                    info!(
                        "Vector config {} successfully reported to {}.",
                        &config_version, DATADOG_REPORTING_PRODUCT
                    );
                }
                Err(err) => {
                    error!(
                        err = ?err.to_string(),
                        "Could not report Vector config to {}.", DATADOG_REPORTING_PRODUCT
                    );

                    if datadog.exit_on_fatal_error {
                        return Err(PipelinesError::FatalCouldNotReportConfig);
                    } else {
                        return Err(PipelinesError::CouldNotReportConfig);
                    }
                }
            }
        }
    }

    setup_metrics_reporting(config, &datadog, api_key.clone(), config_version.clone());

    if datadog.enable_logs_reporting {
        setup_logs_reporting(config, &datadog, api_key, config_version);
    }

    Ok(())
}

fn setup_logs_reporting(
    config: &mut Config,
    datadog: &Options,
    api_key: String,
    config_version: String,
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

    let tag_logs = RemapConfig {
        source: Some(format!(
            r#"
            .version = "{}"
            .configuration_key = "{}"
            .ddsource = "vector"
            .vector = {{
                "version": "{}",
                "arch": "{}",
                "os": "{}",
                "vendor": "{}"
            }}
            {}
        "#,
            &config_version,
            &datadog.configuration_key,
            crate::vector_version(),
            built_info::TARGET_ARCH,
            built_info::TARGET_OS,
            built_info::TARGET_VENDOR,
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
    config_version: String,
) {
    let host_metrics_id = OutputId::from(ComponentKey::from(HOST_METRICS_KEY));
    let tag_metrics_id = OutputId::from(ComponentKey::from(TAG_METRICS_KEY));
    let internal_metrics_id = OutputId::from(ComponentKey::from(INTERNAL_METRICS_KEY));
    let datadog_metrics_id = ComponentKey::from(DATADOG_METRICS_KEY);

    // Create internal sources for host and internal metrics. We're using distinct sources here and
    // not attempting to reuse existing ones, to configure according to enterprise requirements.
    let host_metrics = HostMetricsConfig {
        namespace: host_metrics::Namespace::from(Some("pipelines".to_owned())),
        scrape_interval_secs: datadog.reporting_interval_secs,
        ..Default::default()
    };

    let internal_metrics = InternalMetricsConfig {
        namespace: Some("pipelines".to_owned()),
        scrape_interval_secs: datadog.reporting_interval_secs,
        ..Default::default()
    };

    let custom_metric_tags_vrl = datadog
        .tags
        .as_ref()
        .map_or("".to_string(), |tags| convert_tags_to_vrl(tags, true));

    let tag_metrics = RemapConfig {
        source: Some(format!(
            r#"
            .tags.version = "{}"
            .tags.configuration_key = "{}"
            {}
        "#,
            &config_version, &datadog.configuration_key, custom_metric_tags_vrl
        )),
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

    config.sinks.insert(
        datadog_metrics_id,
        SinkOuter::new(vec![tag_metrics_id], Box::new(datadog_metrics)),
    );
}

/// By default, the Datadog feature is enabled.
const fn default_enabled() -> bool {
    true
}

/// By default, internal logs are reported to Datadog.
const fn default_enable_logs_reporting() -> bool {
    true
}

/// By default, Vector should not exit when a fatal reporting error is encountered.
const fn default_exit_on_fatal_error() -> bool {
    false
}

/// By default, report to Datadog every 5 seconds.
const fn default_reporting_interval_secs() -> f64 {
    5.0
}

/// By default, keep retrying (recoverable) failed reporting
///
/// This is set to 8 attempts which, with the exponential backoff strategy and
/// maximum of 60 second delay (see [`ReportingRetryBackoff`]), works out to
/// roughly 3 minutes of retrying before giving up and allowing the rest of
/// Vector to start.
const fn default_max_retries() -> u32 {
    8
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
            if error.is_timeout() {
                info!(message = "Configuration reporting request timed out.", error = %error);
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
    use std::{io::Write, path::PathBuf, str::FromStr, thread};

    use http::StatusCode;
    use indexmap::IndexMap;
    use indoc::formatdoc;
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
    };

    const fn get_pipelines_auth() -> PipelinesAuth<'static> {
        PipelinesAuth {
            api_key: "api_key",
            application_key: "application_key",
        }
    }

    const fn get_pipelines_fields() -> PipelinesStrFields<'static> {
        PipelinesStrFields {
            config_version: "config_version",
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

    /// This test asserts that configuration reporting errors, by default, do
    /// NOT impact the rest of Vector starting and running. To exit on errors,
    /// an explicit option must be set in the [enterprise] configuration (see
    /// [`super::Options`]).
    ///
    /// In general, Vector should continue operating even in the event that the
    /// enterprise API is down/having issues. Do not modify this behavior
    /// without prior approval.
    #[tokio::test]
    async fn vector_continues_on_reporting_error() {
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

        // Spawn a separate thread to avoid nested async runtime errors
        let vector_continued = thread::spawn(|| {
            // Configuration reporting is guaranteed to fail here. However, the
            // app should still start up and run since `exit_on_fatal_error =
            // false` by default
            Application::prepare_from_opts(opts).map_or(false, |app| {
                // Finish running the topology to avoid error logs
                app.run();
                true
            })
        })
        .join()
        .unwrap();

        assert!(vector_continued);
    }

    #[tokio::test]
    async fn vector_exits_on_reporting_error_when_configured() {
        let server = build_test_server_error_and_recover(StatusCode::NOT_IMPLEMENTED).await;
        let endpoint = server.uri();

        let vector_config = formatdoc! {r#"
            [enterprise]
            application_key = "application_key"
            api_key = "api_key"
            configuration_key = "configuration_key"
            endpoint = "{endpoint}"
            exit_on_fatal_error = true
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

        let vector_continued = thread::spawn(|| {
            // With `exit_on_fatal_error = true`, starting the app should fail
            Application::prepare_from_opts(opts).map_or(false, |app| {
                app.run();
                true
            })
        })
        .join()
        .unwrap();

        assert!(!vector_continued);
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
            "tags" => Kind::object(btreemap! {}),
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
