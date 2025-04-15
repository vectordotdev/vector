use http::StatusCode;
use serde::Deserialize;
use tracing::{debug, error};

use crate::sinks::doris::service::DorisResponse;
use crate::{
    http::HttpError,
    sinks::{
        prelude::*,
        util::{http::HttpRetryLogic, retries::RetryAction},
    },
};

/// Internal struct for parsing Doris Stream Load API responses
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

    /// Check if the error message indicates a transient error
    fn is_transient_error(&self, message: &str) -> bool {
        message.contains("timeout")
            || message.contains("overload")
            || message.contains("too many requests")
            || message.contains("try again later")
            || message.contains("temporarily unavailable")
    }

    /// Check if the error message indicates a permanent error
    fn is_permanent_error(&self, message: &str) -> bool {
        message.contains("DATA_QUALITY_ERROR")
            || message.contains("too many filtered rows")
            || message.contains("schema change")
            || message.contains("column not exist")
            || message.contains("format error")
            || message.contains("parse error")
            || message.contains("duplicate key")
    }
}

impl RetryLogic for DorisRetryLogic {
    type Error = HttpError;
    type Response = DorisResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        // All HTTP errors can be retried
        debug!(
            message = "HTTP error encountered, will retry.",
            %error
        );
        self.inner.is_retriable_error(error)
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        let status = response.http_response.status();
        let body = response.http_response.body();
        let body_str = String::from_utf8_lossy(body);

        // Determine retry strategy based on HTTP status code and response content
        match status {
            // Handle specific HTTP status codes
            StatusCode::TOO_MANY_REQUESTS => RetryAction::Retry("too many requests".into()),
            StatusCode::REQUEST_TIMEOUT => RetryAction::Retry("request timeout".into()),

            // Server errors are always retried
            _ if status.is_server_error() => {
                error!(
                    message = "Server error encountered, will retry.",
                    status_code = %status
                );
                RetryAction::Retry(format!("Server error: {}", status).into())
            }

            // Client errors are typically not retried (except 429, which is handled above)
            _ if status.is_client_error() => {
                error!(
                    message = "Client error encountered, won't retry.",
                    status_code = %status
                );
                RetryAction::DontRetry(format!("Client error: {}", status).into())
            }

            // HTTP status code is success (2xx), need to parse response content
            _ if status.is_success() => {
                // Try to parse Doris response
                match serde_json::from_str::<DorisStreamLoadResponse>(&body_str) {
                    Ok(doris_resp) => {
                        // Determine retry based on Doris response status
                        match doris_resp.status.as_str() {
                            "Success" => {
                                debug!(message = "Doris stream load completed successfully.");
                                RetryAction::Successful
                            }
                            _ => {
                                // Get error message
                                let message = doris_resp.message.clone().unwrap_or_default();

                                // Check if it's a transient error
                                if self.is_transient_error(&message) {
                                    error!(
                                        message = "Doris stream load failed with transient error, will retry.",
                                        doris_status = %doris_resp.status,
                                        error_message = %message
                                    );
                                    RetryAction::Retry(
                                        format!("Transient Doris error: {}", message).into(),
                                    )
                                }
                                // Check if it's a permanent error
                                else if self.is_permanent_error(&message) {
                                    error!(
                                        message = "Doris stream load failed with permanent error, won't retry.",
                                        doris_status = %doris_resp.status,
                                        error_message = %message
                                    );
                                    RetryAction::DontRetry(
                                        format!("Permanent Doris error: {}", message).into(),
                                    )
                                }
                                // Default behavior
                                else {
                                    error!(
                                        message = "Doris stream load failed with unknown error type, will retry.",
                                        doris_status = %doris_resp.status,
                                        error_message = %message
                                    );
                                    RetryAction::Retry(
                                        format!("Unknown Doris error: {}", message).into(),
                                    )
                                }
                            }
                        }
                    }
                    Err(error) => {
                        // Parse failed, but HTTP status code is successful
                        error!(
                            message = "Could not parse Doris response body.",
                            response_body = %body_str,
                            %error
                        );
                        // Parse errors are treated as transient errors and can be retried
                        RetryAction::Retry(format!("Failed to parse response: {}", error).into())
                    }
                }
            }

            // Handle all other HTTP status codes
            _ => {
                error!(
                    message = "Unexpected HTTP status encountered, won't retry.",
                    status_code = %status
                );
                RetryAction::DontRetry(format!("Unexpected status: {}", status).into())
            }
        }
    }
}
