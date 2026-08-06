use serde::Deserialize;
use tracing::{debug, error};

use crate::{
    http::HttpError,
    sinks::{
        doris::{service::DorisResponse, sink::DorisPartitionKey},
        prelude::*,
        util::{http::HttpRequest, retries::RetryAction},
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
pub struct DorisRetryLogic;

impl RetryLogic for DorisRetryLogic {
    type Error = HttpError;
    type Request = HttpRequest<DorisPartitionKey>;
    type Response = DorisResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction<Self::Request> {
        let status = response.http_response.status();
        let body = response.http_response.body();
        let body_str = String::from_utf8_lossy(body);

        // Only return success when HTTP status is successful and Doris response status is "Success"
        if status.is_success() {
            // Try to parse Doris response
            if let Ok(doris_resp) = serde_json::from_str::<DorisStreamLoadResponse>(&body_str) {
                if doris_resp.status == "Success" {
                    debug!(message = "Doris stream load completed successfully.");
                    return RetryAction::Successful;
                }

                // Retry for non-Success status
                let message = doris_resp.message.unwrap_or_default();
                error!(
                    message = "Doris stream load failed, will retry.",
                    doris_status = %doris_resp.status,
                    error_message = %message
                );
                return RetryAction::Retry(
                    format!("Doris error: {} - {}", doris_resp.status, message).into(),
                );
            } else {
                // HTTP success but failed to parse response
                // Don't retry to avoid data duplication, but log the response for debugging
                error!(
                    message = "Failed to parse Doris response, not retrying to avoid data duplication.",
                    status_code = %status,
                    body = %body_str
                );
                return RetryAction::DontRetry("Failed to parse Doris response".into());
            }
        }

        // Retry only for server errors (5xx)
        if status.is_server_error() {
            error!(
                message = "Server error encountered, will retry.",
                status_code = %status
            );
            return RetryAction::Retry(format!("Server error from Doris: {}", status).into());
        }

        // Don't retry for client errors (4xx) and other cases
        error!(
            message = "Client error encountered, not retrying.",
            status_code = %status
        );
        RetryAction::DontRetry(format!("Client error from Doris: {}", status).into())
    }
}
