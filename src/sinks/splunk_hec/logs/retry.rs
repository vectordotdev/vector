use http::StatusCode;

use crate::{
    http::HttpError,
    sinks::util::retries::{RetryAction, RetryLogic},
};

use super::service::HecLogsResponse;

#[derive(Debug, Default, Clone)]
pub struct HecLogsRetry;

impl RetryLogic for HecLogsRetry {
    type Error = HttpError;
    type Response = HecLogsResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        let status = response.http_response.status();

        match status {
            StatusCode::TOO_MANY_REQUESTS => RetryAction::Retry("too many requests".into()),
            StatusCode::NOT_IMPLEMENTED => {
                RetryAction::DontRetry("endpoint not implemented".into())
            }
            _ if status.is_server_error() => RetryAction::Retry(
                format!(
                    "{}: {}",
                    status,
                    String::from_utf8_lossy(response.http_response.body())
                )
                .into(),
            ),
            _ if status.is_success() => RetryAction::Successful,
            _ => RetryAction::DontRetry(format!("response status: {}", status).into()),
        }
    }
}
