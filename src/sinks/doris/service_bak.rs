use std::{
    sync::Arc,
    task::{Context, Poll},
};
use std::time::SystemTime;
use bytes::Bytes;
use futures::future::BoxFuture;
use http::{Method, Response, StatusCode, Uri};
use http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use hyper::{service::Service, Body, Request};
use tower::ServiceExt;
use uuid::Uuid;
use vector_lib::stream::DriverResponse;
use vector_lib::ByteSizeOf;
use vector_lib::{
    json_size::JsonSize,
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
};
use std::collections::HashMap;
use tracing::{debug, info};

use super::DorisConfig;
use crate::{
    event::{EventFinalizers, EventStatus, Finalizable},
    http::HttpClient,
    sinks::util::{
        auth::Auth,
        http::{HttpBatchService, RequestConfig},
        Compression, ElementCount,
    },
};
use crate::sinks::doris::common::DorisCommon;
use crate::sinks::doris::sink::DorisPartitionKey;

#[derive(Clone, Debug)]
pub struct DorisRequest {
    pub payload: Bytes,
    pub finalizers: EventFinalizers,
    pub batch_size: usize,
    #[allow(dead_code)]
    pub events_byte_size: JsonSize,
    pub metadata: RequestMetadata,
    pub partition_key: DorisPartitionKey,
    pub redirect_url: Option<String>,
}

impl ByteSizeOf for DorisRequest {
    fn allocated_bytes(&self) -> usize {
        self.payload.allocated_bytes() + self.finalizers.allocated_bytes()
    }
}

impl ElementCount for DorisRequest {
    fn element_count(&self) -> usize {
        self.batch_size
    }
}

impl Finalizable for DorisRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for DorisRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

pub struct HttpRequestBuilder {
    pub auth: Option<Auth>,
    pub compression: Compression,
    pub http_request_config: RequestConfig,
    pub base_url: String,
    pub label_prefix: String,
    // pub http_client: Option<HttpClient<Body>>,
    pub headers: HashMap<String, String>,
}

impl HttpRequestBuilder {
    pub fn new(common: &DorisCommon, config: &DorisConfig) -> HttpRequestBuilder {
        let auth = common.auth.clone().map(|http_auth| Auth::Basic(http_auth));
        
        // 创建并设置 headers
        let mut headers = HashMap::new();
        // 基本头部
        headers.insert("Expect".to_string(), "100-continue".to_string());
        headers.insert("Content-Type".to_string(), "text/plain;charset=utf-8".to_string());
        
        // 添加行分隔符头部（如果非默认）
        if !config.line_delimiter.is_empty() && config.line_delimiter != "\n" {
            headers.insert("line_delimiter".to_string(), config.line_delimiter.clone());
        }
        
        // 添加自定义头部
        for (k, v) in &config.headers {
            headers.insert(k.clone(), v.clone());
        }
        
        HttpRequestBuilder {
            auth,
            compression: config.compression.clone(),
            http_request_config: config.request.clone(),
            base_url: common.base_url.clone(),
            label_prefix: config.label_prefix.clone(),
            // http_client: None,
            headers,
        }
    }

    pub async fn build_request(
        &self,
        doris_req: DorisRequest,
    ) -> Result<Request<Bytes>, crate::Error> {
        let database = &doris_req.partition_key.database;
        let table = &doris_req.partition_key.table;
        
        info!(
            message = "Building Doris Stream Load request",
            database = %database,
            table = %table,
            payload_size = doris_req.payload.len()
        );
        
        // Generate a unique label
        let label = format!(
            "{}_{}_{}_{}_{}",
            self.label_prefix,
            database,
            table,
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            Uuid::new_v4()
        );

        // 检查是否有重定向URL
        let uri = if let Some(redirect_url) = &doris_req.redirect_url {
            // 使用重定向URL
            info!(
                message = "Using redirect URL",
                redirect_url = %redirect_url
            );
            redirect_url.parse::<Uri>()
                .map_err(|e| crate::Error::from(format!("Invalid redirect URI: {}", e)))?
        } else {
            // 构建原始URL
            let stream_load_url = format!(
                "{}/api/{}/{}/_stream_load",
                self.base_url, database, table
            );
            
            stream_load_url.parse::<Uri>()
                .map_err(|e| crate::Error::from(format!("Invalid URI: {}", e)))?
        };
        
        info!(
            message = "Building request",
            uri = %uri,
            label = %label
        );

        let mut builder = Request::builder()
            .method(Method::PUT)
            .uri(uri)
            .header(CONTENT_LENGTH, doris_req.payload.len())
            .header(CONTENT_TYPE, "text/plain;charset=utf-8")
            .header("Expect", "100-continue")
            .header("label", &label);

        // Add compression headers if needed
        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header("Content-Encoding", ce);
        }

        if let Some(ae) = self.compression.accept_encoding() {
            builder = builder.header("Accept-Encoding", ae);
        }

        // 先添加我们在构造函数中创建的 headers
        for (header, value) in &self.headers {
            builder = builder.header(&header[..], &value[..]);
        }
        
        // 添加 http_request_config 中的自定义 headers (保留兼容性)
        for (header, value) in &self.http_request_config.headers {
            builder = builder.header(&header[..], &value[..]);
        }

        let mut request = builder
            .body(doris_req.payload)
            .map_err(|e| crate::Error::from(format!("Failed to build request: {}", e)))?;

        // Apply authentication if configured
        if let Some(auth) = &self.auth {
            if let Auth::Basic(http_auth) = auth {
                http_auth.apply(&mut request);
                debug!(message = "Applied Basic authentication to request");
            }
        }
        
        info!(
            message = "Request built successfully",
            method = %request.method(),
            uri = %request.uri(),
            headers_count = request.headers().len()
        );

        Ok(request)
    }
}

pub struct DorisResponse {
    pub http_response: Response<Bytes>,
    pub event_status: EventStatus,
    pub events_byte_size: GroupedCountByteSize,
}

impl DriverResponse for DorisResponse {
    fn event_status(&self) -> EventStatus {
        self.event_status
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }
}

#[derive(Clone)]
pub struct DorisService {
    batch_service: HttpBatchService<
        BoxFuture<'static, Result<http::Request<Bytes>, crate::Error>>,
        DorisRequest,
    >,
    log_request: bool,
    reporter: Arc<super::progress::ProgressReporter>,
}

impl DorisService {
    pub fn new(
        http_client: HttpClient<Body>,
        http_request_builder: HttpRequestBuilder,
        log_request: bool,
        reporter: Arc<super::progress::ProgressReporter>,
    ) -> DorisService {
        let http_request_builder = Arc::new(http_request_builder);
        let batch_service = HttpBatchService::new(http_client, move |req| {
            let request_builder = Arc::clone(&http_request_builder);
            let future: BoxFuture<'static, Result<http::Request<Bytes>, crate::Error>> =
                Box::pin(async move { request_builder.build_request(req).await });
            future
        });

        DorisService {
            batch_service: batch_service,
            log_request: log_request,
            reporter: reporter,
        }
    }
}

impl Service<DorisRequest> for DorisService {
    type Response = DorisResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, mut req: DorisRequest) -> Self::Future {
        // 克隆必要的数据，以便在 async 闭包中使用
        let mut http_service = self.batch_service.clone();
        let log_request = self.log_request;
        let reporter = Arc::clone(&self.reporter);
        
        // 为了调试，输出DorisRequest的payload内容
        info!(
            target: "doris_sink",
            "DorisRequest payload (batch_size={}): {}",
            req.batch_size,
            String::from_utf8_lossy(&req.payload)
        );
        
        Box::pin(async move {
            // 确保服务已准备好
            http_service.ready().await?;
            
            // 提取元数据中的事件字节大小
            let events_byte_size =
                std::mem::take(req.metadata_mut()).into_events_estimated_json_encoded_byte_size();
            
            // 保存原始请求的副本，用于处理重定向
            let original_payload = req.payload.clone();
            let original_finalizers = req.finalizers.clone();
            let original_batch_size = req.batch_size;
            let original_partition_key = req.partition_key.clone();
            let original_event_byte_size= req.events_byte_size.clone();
            let original_metadata = req.metadata.clone();
            let mut redirect_count = 0;
            const MAX_REDIRECTS: u8 = 3;
            
            // 发送初始请求
            let mut http_response = http_service.call(req).await?;
            
            // 检查是否收到重定向响应
            let mut status = http_response.status();
            
            // 处理重定向
            while (status == StatusCode::TEMPORARY_REDIRECT || 
                   status == StatusCode::PERMANENT_REDIRECT || 
                   status == StatusCode::FOUND) && 
                  redirect_count < MAX_REDIRECTS {
                
                // 尝试获取重定向位置
                if let Some(location) = http_response.headers().get(http::header::LOCATION) {
                    if let Ok(location_str) = location.to_str() {
                        info!(
                            message = "Following redirect",
                            status = %status,
                            to = %location_str,
                            redirect_count = redirect_count + 1
                        );
                        
                        // 创建一个新的 DorisRequest，使用重定向 URL
                        let redirect_req = DorisRequest {
                            payload: original_payload.clone(),
                            finalizers: original_finalizers.clone(),
                            batch_size: original_batch_size,
                            events_byte_size: original_event_byte_size,
                            metadata: original_metadata.clone(),
                            partition_key: DorisPartitionKey {
                                database: original_partition_key.database.clone(),
                                table: original_partition_key.table.clone(),
                            },
                            redirect_url: Some(location_str.to_string()),
                        };
                        
                        // 发送重定向请求
                        http_service.ready().await?;
                        http_response = http_service.call(redirect_req).await?;
                        status = http_response.status();
                        
                        // 增加重定向计数
                        redirect_count += 1;
                        
                        info!(
                            message = "Received response after redirect",
                            new_status = %status,
                            redirect_count = redirect_count
                        );
                    } else {
                        return Err(crate::Error::from("Invalid Location header in redirect response"));
                    }
                } else {
                    return Err(crate::Error::from("Missing Location header in redirect response"));
                }
            }
            
            // 检查是否超过最大重定向次数
            if redirect_count >= MAX_REDIRECTS {
                return Err(crate::Error::from(format!(
                    "Exceeded maximum number of redirects ({})", MAX_REDIRECTS
                )));
            }
            
            // 处理最终响应
            let event_status = get_event_status(&http_response);
            
            // 记录最终响应体 - 无论成功与否都记录
            if log_request {
                let body = http_response.body();
                let body_str = String::from_utf8_lossy(body);
                info!(
                    target: "doris_sink",
                    "Doris stream load response:\nStatus: {}\nBody: {}", 
                    status,
                    body_str
                );

                // 如果响应成功，尝试解析响应体并更新进度统计
                if status.is_success() {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body_str) {
                        if let Some(status_value) = json.get("Status") {
                            if let Some(status_str) = status_value.as_str() {
                                if status_str == "Success" {
                                    // 更新字节数统计
                                    if let Some(load_bytes) = json.get("LoadBytes").and_then(|b| b.as_i64()) {
                                        reporter.incr_total_bytes(load_bytes);
                                        debug!(
                                            target: "doris_sink",
                                            "Updated bytes stats: +{} bytes", 
                                            load_bytes
                                        );
                                    }

                                    // 更新行数统计
                                    if let Some(loaded_rows) = json.get("NumberLoadedRows").and_then(|r| r.as_i64()) {
                                        reporter.incr_total_rows(loaded_rows);
                                        debug!(
                                            target: "doris_sink",
                                            "Updated rows stats: +{} rows", 
                                            loaded_rows
                                        );
                                    }

                                    // 更新过滤行数统计
                                    if let Some(filtered_rows) = json.get("NumberFilteredRows").and_then(|r| r.as_i64()) {
                                        if filtered_rows > 0 {
                                            reporter.incr_failed_rows(filtered_rows);
                                            debug!(
                                                target: "doris_sink",
                                                "Updated filtered rows stats: +{} filtered rows", 
                                                filtered_rows
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            Ok(DorisResponse {
                event_status,
                http_response,
                events_byte_size,
            })
        })
    }
}

fn get_event_status(response: &Response<Bytes>) -> EventStatus {
    let status = response.status();
    
    // 处理HTTP层面的基本状态
    if !status.is_success() {
        if status.is_server_error() {
            // 服务器错误通常是临时性的
            return EventStatus::Errored;
        } else {
            // 客户端错误和其他非成功状态视为永久性失败
            return EventStatus::Rejected;
        }
    }
    
    // HTTP状态码是2xx，需要进一步解析响应体
    let body = String::from_utf8_lossy(response.body());
    
    // 尝试解析响应体为JSON
    match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(json) => {
            // 检查Doris特定的Status字段
            if let Some(status_value) = json.get("Status") {
                if let Some(status_str) = status_value.as_str() {
                    if status_str == "Success" {
                        // 明确成功的情况
                        return EventStatus::Delivered;
                    } else {
                        // 非成功状态视为拒绝
                        return EventStatus::Rejected;
                    }
                }
            }
            
            // 兜底 - 如果找不到Status字段但格式是JSON
            if body.contains("\"errors\":true") || body.contains("\"status\":\"Fail\"") {
                return EventStatus::Rejected;
            }
            
            // 没有明确错误指示，假设成功
            return EventStatus::Delivered;
        },
        Err(_) => {
            // 无法解析JSON，尝试基于文本内容判断
            if body.contains("Success") {
                return EventStatus::Delivered;
            } else {
                return EventStatus::Rejected;
            }
        }
    }
}
