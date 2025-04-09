use http::StatusCode;
use serde::Deserialize;
use tracing::{debug, error, info};

use crate::{
    http::HttpError,
    sinks::{
        prelude::*,
        util::{
            http::{ HttpResponse, HttpRetryLogic},
            retries::RetryAction,
        },
    },
};

use super::progress::ProgressReporter;

/// 用于解析 Doris Stream Load API 响应的内部结构体
#[derive(Debug, Deserialize)]
struct DorisStreamLoadResponse {
    
    #[serde(rename = "Status")]
    status: String,
    
    #[serde(rename = "Message")]
    message: Option<String>,

    #[serde(rename = "NumberLoadedRows")]
    number_loaded_rows: Option<i64>,
    
    #[serde(rename = "NumberFilteredRows")]
    number_filtered_rows: Option<i64>,

    #[serde(rename = "LoadBytes")]
    load_bytes: Option<i64>,

}

#[derive(Debug, Clone)]
pub struct DorisRetryLogic {
    inner: HttpRetryLogic,
    reporter: ProgressReporter,
    log_request: bool,
}

impl DorisRetryLogic {
    /// Create a new DorisRetryLogic
    pub fn new(reporter: ProgressReporter, log_request: bool) -> Self {
        Self {
            inner: HttpRetryLogic,
            reporter,
            log_request,
        }
    }
}

impl RetryLogic for DorisRetryLogic {
    type Error = HttpError;
    type Response = HttpResponse;

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
        
        // 记录响应 (Filebeat 中的 logRequest)
        let body_str = String::from_utf8_lossy(body);
        if self.log_request {
            info!(
                target: "doris_sink",
                "Doris stream load response:\nStatus: {}\nBody: {}", 
                status,
                body_str
            );
        }

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
                        // 更新进度报告
                        if let Some(bytes) = doris_resp.load_bytes {
                            self.reporter.incr_total_bytes(bytes);
                        }
                        
                        if let Some(loaded_rows) = doris_resp.number_loaded_rows {
                            self.reporter.incr_total_rows(loaded_rows);
                        }
                        
                        if let Some(filtered_rows) = doris_resp.number_filtered_rows {
                            if filtered_rows > 0 {
                                self.reporter.incr_failed_rows(filtered_rows);
                                debug!(
                                    target: "doris_sink",
                                    "Doris filtered {} rows during load", 
                                    filtered_rows
                                );
                            }
                        }
                        
                        // 根据 Doris 响应状态决定重试
                        match doris_resp.status.as_str() {
                            "Success" => {
                                debug!(
                                    target: "doris_sink",
                                    "Doris stream load successful: loaded {} rows, {} bytes", 
                                    doris_resp.number_loaded_rows.unwrap_or(0),
                                    doris_resp.load_bytes.unwrap_or(0)
                                );
                                RetryAction::Successful
                            },
                            _ => {
                                // 任何非 "Success" 的状态都会触发重试
                                let message = doris_resp.message.clone().unwrap_or_default();
                                error!(
                                    target: "doris_sink",
                                    "Doris stream load failed (will retry): Status={}, Message={}", 
                                    doris_resp.status,
                                    message
                                );
                                RetryAction::Retry(format!("Doris error: {}", message).into())
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
                        // 在 Filebeat 中，解析错误会被视为失败并重试
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
