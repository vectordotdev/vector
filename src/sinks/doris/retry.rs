use http::StatusCode;
use serde::Deserialize;
use tracing::{debug, error};

use crate::{
    http::HttpError,
    sinks::{
        prelude::*,
        util::{
            http::{HttpRetryLogic},
            retries::RetryAction,
        },
    },
};
use crate::sinks::doris::service::DorisResponse;

/// 用于解析 Doris Stream Load API 响应的内部结构体
#[derive(Debug, Deserialize)]
struct DorisStreamLoadResponse {
    
    #[serde(rename = "Status")]
    status: String,
    
    #[serde(rename = "Message")]
    message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DorisRetryLogic {
    inner: HttpRetryLogic,
}

impl DorisRetryLogic {
    /// Create a new DorisRetryLogic
    pub fn new() -> Self {
        Self {
            inner: HttpRetryLogic,
        }
    }
    
    /// 检查错误消息，判断是否为临时错误
    fn is_transient_error(&self, message: &str) -> bool {
        message.contains("timeout") || 
        message.contains("overload") || 
        message.contains("too many requests") ||
        message.contains("try again later") ||
        message.contains("temporarily unavailable")
    }
    
    /// 检查错误消息，判断是否为永久错误
    fn is_permanent_error(&self, message: &str) -> bool {
        message.contains("DATA_QUALITY_ERROR") || 
        message.contains("too many filtered rows") ||
        message.contains("schema change") ||
        message.contains("column not exist") ||
        message.contains("format error") ||
        message.contains("parse error") ||
        message.contains("duplicate key")
    }
}

impl RetryLogic for DorisRetryLogic {
    type Error = HttpError;
    type Response = DorisResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        // 所有 HTTP 错误都可以重试
        debug!(
            target: "doris_sink",
            "HTTP error encountered (will retry): {:?}", 
            error
        );
        self.inner.is_retriable_error(error)
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        let status = response.http_response.status();
        let body = response.http_response.body();
        let body_str = String::from_utf8_lossy(body);
        
        // 基于HTTP状态码和响应内容决定重试策略
        match status {
            // 特定的 HTTP 状态码处理
            StatusCode::TOO_MANY_REQUESTS => RetryAction::Retry("too many requests".into()),
            StatusCode::REQUEST_TIMEOUT => RetryAction::Retry("request timeout".into()),
            
            // 服务器错误都会重试
            _ if status.is_server_error() => {
                error!(
                    target: "doris_sink",
                    "Server error (will retry): {}", 
                    status
                );
                RetryAction::Retry(format!("Server error: {}", status).into())
            },
            
            // 客户端错误通常不重试（除了429，已在上面处理）
            _ if status.is_client_error() => {
                error!(
                    target: "doris_sink",
                    "Client error (won't retry): {}", 
                    status
                );
                RetryAction::DontRetry(format!("Client error: {}", status).into())
            },
            
            // HTTP 状态码成功(2xx)，需要解析响应内容
            _ if status.is_success() => {
                // 尝试解析 Doris 响应
                match serde_json::from_str::<DorisStreamLoadResponse>(&body_str) {
                    Ok(doris_resp) => {
                        // 根据 Doris 响应状态决定重试
                        match doris_resp.status.as_str() {
                            "Success" => {
                                debug!(
                                    target: "doris_sink",
                                    "Doris stream load successful"
                                );
                                RetryAction::Successful
                            },
                            _ => {
                                // 获取错误消息
                                let message = doris_resp.message.clone().unwrap_or_default();
                                
                                // 检查是否为临时性错误
                                if self.is_transient_error(&message) {
                                    error!(
                                        target: "doris_sink",
                                        "Doris stream load failed (transient error, will retry): Status={}, Message={}", 
                                        doris_resp.status,
                                        message
                                    );
                                    RetryAction::Retry(format!("Transient Doris error: {}", message).into())
                                }
                                // 检查是否为永久性错误
                                else if self.is_permanent_error(&message) {
                                    error!(
                                        target: "doris_sink",
                                        "Doris stream load failed (permanent error, won't retry): Status={}, Message={}", 
                                        doris_resp.status,
                                        message
                                    );
                                    RetryAction::DontRetry(format!("Permanent Doris error: {}", message).into())
                                }
                                // 默认行为
                                else {
                                    error!(
                                        target: "doris_sink",
                                        "Doris stream load failed (unknown error type, will retry): Status={}, Message={}", 
                                        doris_resp.status,
                                        message
                                    );
                                    RetryAction::Retry(format!("Unknown Doris error: {}", message).into())
                                }
                            }
                        }
                    },
                    Err(err) => {
                        // 解析失败，但 HTTP 状态码成功
                        error!(
                            target: "doris_sink",
                            "Could not parse Doris response body: {}, Error: {}", 
                            body_str,
                            err
                        );
                        // 解析错误视为临时错误，可以重试
                        RetryAction::Retry(format!("Failed to parse response: {}", err).into())
                    }
                }
            },
            
            // 处理其他所有HTTP状态码
            _ => {
                error!(
                    target: "doris_sink",
                    "Unexpected HTTP status (won't retry): {}", 
                    status
                );
                RetryAction::DontRetry(format!("Unexpected status: {}", status).into())
            }
        }
    }
}
