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
                return RetryAction::Retry(format!("Doris error: {} - {}", doris_resp.status, message).into());
            }
        }
        
        // Retry for all other cases
        error!(
            message = "Error encountered, will retry.",
            status_code = %status
        );
        
        RetryAction::Retry("Error response from Doris, will retry.".into())
    }
}
