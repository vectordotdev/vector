//! Configuration for the `Doris` sink.

use super::progress::ProgressReporter;
use super::sink::DorisSink;

use crate::{
    http::{Auth, HttpClient},
    sinks::{
        doris::health::DorisHealthLogic,
        prelude::*,
        util::{RealtimeSizeBasedDefaultBatchSettings, service::HealthConfig},
    },
};
use http::{Request, Uri};
use hyper::Body;
use std::collections::HashMap;
use futures;
use futures_util::TryFutureExt;
use serde_json;
use vector_lib::codecs::JsonSerializerConfig;
use crate::sinks::doris::common::DorisCommon;
use crate::sinks::doris::retry::DorisRetryLogic;
use crate::sinks::doris::service_bak::{DorisService, HttpRequestBuilder};
use crate::sinks::util::http::RequestConfig;
use std::sync::Arc;

// 定义用于 Doris 服务的 URI 处理函数
fn get_http_scheme_host(host: &str) -> crate::Result<UriComponents> {
    let uri = host.parse::<Uri>().map_err(|e| format!("Failed to parse URI: {}", e))?;

    // 获取 scheme, 默认为 http
    let scheme = uri.scheme_str().unwrap_or("http").to_string();

    // 获取 host
    let host = uri.host().unwrap_or("localhost").to_string();

    // 获取 port
    let port = uri.port_u16();

    Ok(UriComponents {
        scheme,
        host,
        port,
    })
}

// 构建完整的 URI
fn build_uri(scheme: &str, path: &str, host: &str, port: u16) -> crate::Result<Uri> {
    let uri_str = format!("{}://{}:{}{}", scheme, host, port, path);
    uri_str.parse::<Uri>().map_err(|e| format!("Failed to build URI: {}", e).into())
}

// URI 组件结构体
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

    /// Maximum size of a batch before it is flushed.
    #[serde(default = "default_bulk_max_size")]
    pub bulk_max_size: usize,

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

// fn default_timeout() -> Duration {
//     Duration::from_secs(30)
// }

fn default_bulk_max_size() -> usize {
    100000
}

fn default_max_retries() -> isize {
    -1
}

// /// The format used to parse input/output data.
// #[configurable_component]
// #[derive(Clone, Copy, Debug, Derivative, Eq, PartialEq, Hash)]
// #[serde(rename_all = "snake_case")]
// #[derivative(Default)]
// #[allow(clippy::enum_variant_names)]
// pub enum DorisFormat {
//     #[derivative(Default)]
//     /// JSONEachRow.
//     Json,
//
//     /// json array [{},{},{}].
//     JsonAsArray,
//
//     /// csv.
//     CSV,
// }
//
// impl fmt::Display for DorisFormat {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         match self {
//             DorisFormat::Json => write!(f, "Json"),
//             DorisFormat::JsonAsArray => write!(f, "JsonAsArray"),
//             DorisFormat::CSV => write!(f, "CSV"),
//         }
//     }
// }

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

        // 将reporter包装为Arc以便共享
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
                    Arc::clone(&reporter_arc)
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
            self.clone(), // Pass the entire config
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
    /// Note: This functionality is now directly implemented in HttpRequestBuilder::new in service_bak.rs,
    /// but this function is kept for potential future uses or other implementations.
    #[allow(dead_code)]
    fn create_headers(&self) -> HashMap<String, String> {
        let mut headers = HashMap::new();
        // 确保总是添加这些基本头部
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

        // 为每个节点创建一个健康检查
        let healthchecks = hosts.into_iter().map(move |host| {
            let client = client.clone();
            
            async move {
                let parsed_url = get_http_scheme_host(&host)?;

                // 使用 Doris 的 bootstrap API 端点进行健康检查
                let query_path = "/api/bootstrap";
                let uri = build_uri(
                    &parsed_url.scheme,
                    query_path,
                    &parsed_url.host,
                    parsed_url.port.unwrap_or(8030),
                )?;

                debug!(
                    target: "doris_sink",
                    "Checking health of Doris node: {}", 
                    uri
                );

                let request = Request::get(uri.to_string())
                    .body(Body::empty())
                    .map_err(|_| "Failed to build request".to_string())?;

                let request = request;

                let response = client.send(request).await?;
                let (parts, body) = response.into_parts();
                let body_bytes = hyper::body::to_bytes(body).await
                    .map_err(|e| format!("Failed to read response body: {}", e))?;

                if parts.status.is_success() {
                    // 尝试解析JSON响应
                    match serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                        Ok(json) => {
                            // 检查响应中的code字段是否为0
                            if let Some(code) = json.get("code").and_then(|c| c.as_i64()) {
                                if code == 0 {
                                    info!(
                                        target: "doris_sink",
                                        "Doris node {} is healthy", 
                                        host
                                    );
                                    return Ok(());
                                } else {
                                    let msg = json.get("msg").and_then(|m| m.as_str()).unwrap_or("unknown error");
                                    warn!(
                                        target: "doris_sink",
                                        "Doris node {} is unhealthy: code={}, msg={}", 
                                        host, code, msg
                                    );
                                    return Err(format!("Healthcheck failed for host {}: code={}, msg={}", 
                                                    host, code, msg).into());
                                }
                            }
                        }
                        Err(e) => {
                            warn!(
                                target: "doris_sink",
                                "Failed to parse JSON response from {}: {}", 
                                host, e
                            );
                        }
                    }
                }

                // 如果代码执行到这里，说明响应不成功或者JSON解析失败
                warn!(
                    target: "doris_sink",
                    "Doris node {} is unhealthy: status={}", 
                    host, parts.status
                );
                Err(format!("Healthcheck failed for host {} with status: {}", 
                           host, parts.status).into())
            }.boxed()
        }).collect::<Vec<_>>();

        // 使用 select_ok 来选择第一个成功的健康检查
        let healthcheck = futures::future::select_ok(healthchecks)
            .map_ok(|((), _)| ())
            .boxed();

        Ok(healthcheck)
    }
}
