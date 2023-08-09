use http::StatusCode;

use crate::{
    http::HttpError,
    sinks::util::retries::{RetryAction, RetryLogic},
};

use super::service::HttpResponse;

/// `RetryLogic` implementation for use in HTTP based stream sinks.
#[derive(Debug, Default, Clone)]
pub struct HttpRetryLogic;

impl RetryLogic for HttpRetryLogic {
    type Error = HttpError;
    type Response = HttpResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        let response = &response.http_response;
        let status = response.status();

        match status {
            StatusCode::TOO_MANY_REQUESTS => RetryAction::Retry("too many requests".into()),
            StatusCode::NOT_IMPLEMENTED => {
                RetryAction::DontRetry("endpoint not implemented".into())
            }
            _ if status.is_server_error() => RetryAction::Retry(
                format!("{}: {}", status, String::from_utf8_lossy(response.body())).into(),
            ),
            _ if status.is_success() => RetryAction::Successful,
            _ => RetryAction::DontRetry(format!("response status: {}", status).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use hyper::Response;
    use vector_common::request_metadata::GroupedCountByteSize;

    use super::{HttpResponse, HttpRetryLogic};
    use crate::sinks::util::retries::RetryLogic;

    #[test]
    fn validate_retry_logic() {
        let logic = HttpRetryLogic;

        fn generate_response(code: u16) -> HttpResponse {
            HttpResponse {
                http_response: Response::builder().status(code).body(Bytes::new()).unwrap(),
                events_byte_size: GroupedCountByteSize::new_untagged(),
                raw_byte_size: 0,
            }
        }

        assert!(logic
            .should_retry_response(&generate_response(429))
            .is_retryable());
        assert!(logic
            .should_retry_response(&generate_response(500))
            .is_retryable());
        assert!(logic
            .should_retry_response(&generate_response(400))
            .is_not_retryable());
        assert!(logic
            .should_retry_response(&generate_response(501))
            .is_not_retryable());
    }
}
