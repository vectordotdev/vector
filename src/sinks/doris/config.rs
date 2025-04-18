//! Configuration for the `Doris` sink.

use super::progress::ProgressReporter;
use super::sink::DorisSink;

use crate::sinks::doris::common::DorisCommon;
use crate::sinks::doris::retry::DorisRetryLogic;
use crate::sinks::doris::service::{DorisService, HttpRequestBuilder};
use crate::sinks::util::http::RequestConfig;
use crate::{
    http::{Auth, HttpClient},
    sinks::{
        doris::health::DorisHealthLogic,
        prelude::*,
        util::{service::HealthConfig, RealtimeSizeBasedDefaultBatchSettings},
    },
};
use futures;
use futures_util::TryFutureExt;
use http::{Request, Uri};
use hyper::Body;
use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use vector_lib::codecs::JsonSerializerConfig;

// Define URI handling function for Doris service
fn get_http_scheme_host(host: &str) -> crate::Result<UriComponents> {
    let uri = host
        .parse::<Uri>()
        .map_err(|e| format!("Failed to parse URI: {}", e))?;

    // Get scheme, default to http
    let scheme = uri.scheme_str().unwrap_or("http").to_string();

    // Get host
    let host = uri.host().unwrap_or("localhost").to_string();

    // Get port
    let port = uri.port_u16();

    Ok(UriComponents { scheme, host, port })
}

// Build complete URI
fn build_uri(scheme: &str, path: &str, host: &str, port: u16) -> crate::Result<Uri> {
    let uri_str = format!("{}://{}:{}{}", scheme, host, port, path);
    uri_str
        .parse::<Uri>()
        .map_err(|e| format!("Failed to build URI: {}", e).into())
}

// URI components struct
#[derive(Debug)]
struct UriComponents {
    scheme: String,
    host: String,
    port: Option<u16>,
}

/// Configuration for the `doris` sink.
#[configurable_component(sink("doris", "Deliver log data to an Apache Doris database."))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct DorisConfig {
    /// A list of Doris endpoints to send logs to.
    ///
    /// The endpoint must contain an HTTP scheme, and may specify a
    /// hostname or IP address and port.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "http://127.0.0.1:8030"))]
    pub endpoints: Vec<String>,

    /// The database that contains the table data will be inserted into.
    #[configurable(metadata(docs::examples = "mydatabase"))]
    pub database: Template,

    /// The table data is inserted into.
    #[configurable(metadata(docs::examples = "mytable"))]
    pub table: Template,

    /// The prefix for Stream Load label.
    /// The final label will be in format: `{label_prefix}_{database}_{table}_{timestamp}_{uuid}`.
    #[configurable(metadata(docs::examples = "vector"))]
    #[serde(default = "default_label_prefix")]
    pub label_prefix: String,

    /// The line delimiter for batch data.
    #[configurable(metadata(docs::examples = "\\n"))]
    #[serde(default = "default_line_delimiter")]
    pub line_delimiter: String,

    /// Enable request logging.
    #[serde(default = "default_log_request")]
    pub log_request: bool,

    /// Progress reporting interval in seconds.
    /// Set to 0 to disable progress reporting.
    #[serde(default = "default_log_progress_interval")]
    pub log_progress_interval: u64,

    /// Custom HTTP headers to add to the request.
    #[serde(default)]
    pub headers: HashMap<String, String>,

    /// The codec configuration. This configures how events are encoded before being sent to Doris.
    #[serde(default)]
    pub codec: JsonSerializerConfig,

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub encoding: Transformer,

    /// Compression algorithm to use for HTTP requests.
    #[serde(default)]
    pub compression: Compression,

    /// Number of retries that will be attempted before give up.
    #[serde(default = "default_max_retries")]
    pub max_retries: isize,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    pub auth: Option<Auth>,

    #[serde(default)]
    #[configurable(derived)]
    pub request: RequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    /// Options for determining the health of Doris endpoints.
    #[serde(default)]
    #[configurable(derived)]
    #[serde(rename = "distribution")]
    pub endpoint_health: Option<HealthConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

fn default_label_prefix() -> String {
    "vector".to_string()
}

fn default_line_delimiter() -> String {
    "\n".to_string()
}

fn default_log_request() -> bool {
    true
}

fn default_log_progress_interval() -> u64 {
    10
}

fn default_max_retries() -> isize {
    -1
}

impl_generate_config_from_default!(DorisConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "doris")]
impl SinkConfig for DorisConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let endpoints = self.endpoints.clone();

        if endpoints.is_empty() {
            return Err("No endpoints configured.'.".into());
        }
        let commons = DorisCommon::parse_many(self).await?;
        let common = commons[0].clone();

        let client = HttpClient::new(common.tls_settings.clone(), &cx.proxy)?;

        let batch_settings = self.batch.into_batcher_settings()?;

        // Create and start the progress reporter
        let reporter = ProgressReporter::new(self.log_progress_interval);
        let reporter_clone = reporter.clone();
        // Create a new noop shutdown signal - it will be automatically closed when the Vector process shuts down
        let shutdown = vector_lib::shutdown::ShutdownSignal::noop();
        tokio::spawn(async move {
            reporter_clone.report(Some(shutdown)).await;
        });

        // Setup retry logic using the configured request settings
        let request_settings = self.request.tower.into_settings();

        let health_config = self.endpoint_health.clone().unwrap_or_default();

        // Wrap reporter in Arc for sharing
        let reporter_arc = Arc::new(reporter);

        // Use our new DorisService implementation instead of HttpService
        let services = commons
            .iter()
            .cloned()
            .map(|common| {
                let endpoint = common.base_url.clone();
                let http_request_builder = HttpRequestBuilder::new(&common, self);

                let service = DorisService::new(
                    client.clone(),
                    http_request_builder,
                    self.log_request,
                    Arc::clone(&reporter_arc),
                );
                (endpoint, service)
            })
            .collect::<Vec<_>>();

        let service = request_settings.distributed_service(
            DorisRetryLogic::new(),
            services,
            health_config,
            DorisHealthLogic,
            1,
        );

        // Create DorisSink with the configured service
        let sink = DorisSink::new(
            batch_settings,
            service,
            self.clone(),
            common.request_builder.clone(),
        );

        let sink = VectorSink::from_event_streamsink(sink);
        let healthcheck = self.build_healthcheck(client, endpoints)?;

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl DorisConfig {
    /// Helper function to create HTTP headers for Doris Stream Load.
    ///
    /// Note: This functionality is now directly implemented in HttpRequestBuilder::new in service,
    /// but this function is kept for potential future uses or other implementations.
    #[allow(dead_code)]
    fn create_headers(&self) -> HashMap<String, String> {
        let mut headers = HashMap::new();
        // Always add these basic headers
        headers.insert("Expect".to_string(), "100-continue".to_string());
        headers.insert(
            "Content-Type".to_string(),
            "text/plain;charset=utf-8".to_string(),
        );

        // Add line delimiter header if not default
        if !self.line_delimiter.is_empty() && self.line_delimiter != "\n" {
            headers.insert("line_delimiter".to_string(), self.line_delimiter.clone());
        }

        // Add custom headers
        for (k, v) in &self.headers {
            if k == "line_delimiter" {
                // Store line_delimiter for internal use
                // (this would be done in go by setting config.LineDelimiter = v)
            }
            headers.insert(k.clone(), v.clone());
        }

        headers
    }

    fn build_healthcheck(
        &self,
        client: HttpClient,
        hosts: Vec<String>,
    ) -> crate::Result<Healthcheck> {
        // Create a health check for each node
        let healthchecks = hosts
            .into_iter()
            .map(move |host| {
                let client = client.clone();

                async move {
                    let parsed_url = get_http_scheme_host(&host)?;

                    // Use Doris bootstrap API endpoint for health check
                    let query_path = "/api/bootstrap";
                    let uri = build_uri(
                        &parsed_url.scheme,
                        query_path,
                        &parsed_url.host,
                        parsed_url.port.unwrap_or(8030),
                    )?;

                    debug!(
                        message = "Checking health of Doris node.",
                        node = %uri
                    );

                    let request = Request::get(uri.to_string())
                        .body(Body::empty())
                        .map_err(|_| "Failed to build request".to_string())?;

                    let request = request;

                    let response = client.send(request).await?;
                    let (parts, body) = response.into_parts();
                    let body_bytes = hyper::body::to_bytes(body)
                        .await
                        .map_err(|e| format!("Failed to read response body: {}", e))?;

                    if parts.status.is_success() {
                        // Try to parse JSON response
                        match serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                            Ok(json) => {
                                // Check if code field in response is 0
                                if let Some(code) = json.get("code").and_then(|c| c.as_i64()) {
                                    if code == 0 {
                                        info!(
                                            message = "Doris node is healthy.",
                                            node = %host
                                        );
                                        return Ok(());
                                    } else {
                                        let msg = json
                                            .get("msg")
                                            .and_then(|m| m.as_str())
                                            .unwrap_or("unknown error");
                                        warn!(
                                            message = "Doris node is unhealthy.",
                                            node = %host,
                                            code = %code,
                                            error_msg = %msg
                                        );
                                        return Err(format!(
                                            "Healthcheck failed for host {}: code={}, msg={}",
                                            host, code, msg
                                        )
                                        .into());
                                    }
                                }
                            }
                            Err(e) => {
                                warn!(
                                    message = "Failed to parse JSON response from node.",
                                    node = %host,
                                    error = %e
                                );
                            }
                        }
                    }

                    // If we reach here, response was not successful or JSON parsing failed
                    warn!(
                        message = "Doris node is unhealthy.",
                        node = %host,
                        status = %parts.status
                    );
                    Err(format!(
                        "Healthcheck failed for host {} with status: {}",
                        host, parts.status
                    )
                    .into())
                }
                .boxed()
            })
            .collect::<Vec<_>>();

        // Use select_ok to select the first successful health check
        let healthcheck = futures::future::select_ok(healthchecks)
            .map_ok(|((), _)| ())
            .boxed();

        Ok(healthcheck)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DorisConfig>();
    }

    #[test]
    fn test_build_uri() {
        assert_eq!(
            build_uri("http", "/api/bootstrap", "localhost", 8030).unwrap().to_string(),
            "http://localhost:8030/api/bootstrap"
        );
        assert_eq!(
            build_uri("https", "/api/test", "example.com", 443).unwrap().to_string(),
            "https://example.com:443/api/test"
        );
    }

    #[test]
    fn test_get_http_scheme_host() {
        let result = get_http_scheme_host("http://localhost:8030").unwrap();
        assert_eq!(result.scheme, "http");
        assert_eq!(result.host, "localhost");
        assert_eq!(result.port, Some(8030));

        let result = get_http_scheme_host("https://example.com").unwrap();
        assert_eq!(result.scheme, "https");
        assert_eq!(result.host, "example.com");
        assert_eq!(result.port, None);
    }

    #[test]
    fn test_default_values() {
        assert_eq!(default_label_prefix(), "vector");
        assert_eq!(default_line_delimiter(), "\n");
        assert_eq!(default_log_request(), true);
        assert_eq!(default_log_progress_interval(), 10);
        assert_eq!(default_max_retries(), -1);
    }

    #[test]
    fn parse_config_with_defaults() {
        let config: DorisConfig = toml::from_str(
            r#"
            endpoints = ["http://localhost:8030"]
            database = "test_db"
            table = "test_table"
            "#,
        )
        .unwrap();

        assert_eq!(config.endpoints, vec!["http://localhost:8030"]);
        assert_eq!(config.database.to_string(), "test_db");
        assert_eq!(config.table.to_string(), "test_table");
        assert_eq!(config.label_prefix, "vector");
        assert_eq!(config.line_delimiter, "\n");
        assert!(config.log_request);
        assert_eq!(config.log_progress_interval, 10);
        assert_eq!(config.max_retries, -1);
    }

    #[test]
    fn parse_config_with_custom_values() {
        let config: DorisConfig = toml::from_str(
            r#"
            endpoints = ["http://doris1:8030", "http://doris2:8030"]
            database = "custom_db"
            table = "custom_table"
            label_prefix = "custom_prefix"
            line_delimiter = "\r\n"
            log_request = false
            log_progress_interval = 30
            max_retries = 5
            "#,
        )
        .unwrap();

        assert_eq!(config.endpoints, vec!["http://doris1:8030", "http://doris2:8030"]);
        assert_eq!(config.database.to_string(), "custom_db");
        assert_eq!(config.table.to_string(), "custom_table");
        assert_eq!(config.label_prefix, "custom_prefix");
        assert_eq!(config.line_delimiter, "\r\n");
        assert!(!config.log_request);
        assert_eq!(config.log_progress_interval, 30);
        assert_eq!(config.max_retries, 5);
    }

    #[test]
    fn parse_config_with_auth() {
        let config: DorisConfig = toml::from_str(
            r#"
            endpoints = ["http://localhost:8030"]
            database = "test_db"
            table = "test_table"
            auth.strategy = "basic"
            auth.user = "admin"
            auth.password = "password"
            "#,
        )
        .unwrap();

        assert!(config.auth.is_some());
        if let Some(Auth::Basic { user, password }) = &config.auth {
            assert_eq!(user, "admin");
            assert_eq!(password.inner(), "password");
        } else {
            panic!("Expected Basic auth");
        }
    }

    #[test]
    fn parse_config_with_custom_headers() {
        let config: DorisConfig = toml::from_str(
            r#"
            endpoints = ["http://localhost:8030"]
            database = "test_db"
            table = "test_table"
            [headers]
            "X-Custom-Header" = "custom_value"
            "Content-Type" = "application/json"
            "#,
        )
        .unwrap();

        assert_eq!(config.headers.len(), 2);
        assert_eq!(config.headers.get("X-Custom-Header").unwrap(), "custom_value");
        assert_eq!(config.headers.get("Content-Type").unwrap(), "application/json");
    }

    #[test]
    fn parse_distribution() {
        toml::from_str::<DorisConfig>(
            r#"
            endpoints = ["", ""]
            database = "test_db"
            table = "test_table"
            distribution.retry_initial_backoff_secs = 10
        "#,
        ) .unwrap();
    }

}
