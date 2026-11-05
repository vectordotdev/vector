use http::StatusCode;

use super::request_builder::VmRequest;
use crate::{
    http::HttpError,
    sinks::{
        prelude::*,
        util::http::{HttpResponse, RetryStrategy},
    },
};

/// Maximum number of response body bytes included in a failure reason.
const MAX_REASON_BODY_BYTES: usize = 512;

/// Retry logic matching `vmagent`.
///
/// With the default strategy, requests rejected with 400, 409, or 415 are dropped, and every other
/// failed request is retried. This includes 401 and 403, which `vmauth` returns while credentials
/// are rotated, and 404, which a misrouted `vmauth` returns. Other strategies behave as in every
/// HTTP sink.
#[derive(Clone, Debug)]
pub(super) struct VmRetryLogic {
    pub(super) strategy: RetryStrategy,
}

impl RetryLogic for VmRetryLogic {
    type Error = HttpError;
    type Request = VmRequest;
    type Response = HttpResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        self.strategy != RetryStrategy::None && error.is_retriable()
    }

    fn is_retriable_timeout(&self) -> bool {
        self.strategy != RetryStrategy::None
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction<Self::Request> {
        let status = response.http_response.status();
        if self.strategy != RetryStrategy::Default {
            return self.strategy.retry_action(status);
        }
        if status.is_success() {
            return RetryAction::Successful;
        }

        let body = response.http_response.body();
        let body = String::from_utf8_lossy(&body[..body.len().min(MAX_REASON_BODY_BYTES)]);
        let reason = format!("{status}: {}", body.trim()).into();
        if is_retriable_status(status) {
            RetryAction::Retry(reason)
        } else {
            RetryAction::DontRetry(reason)
        }
    }
}

/// VictoriaMetrics rejects invalid data with 400, and Prometheus-compatible receivers use 409 and
/// 415 for requests that never succeed. `vmagent` drops blocks on exactly these status codes.
fn is_retriable_status(status: StatusCode) -> bool {
    !matches!(
        status,
        StatusCode::BAD_REQUEST | StatusCode::CONFLICT | StatusCode::UNSUPPORTED_MEDIA_TYPE
    )
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;

    fn response(status: u16, body: &'static str) -> HttpResponse {
        HttpResponse {
            http_response: http::Response::builder()
                .status(status)
                .body(Bytes::from_static(body.as_bytes()))
                .unwrap(),
            events_byte_size: telemetry().create_request_count_byte_size(),
            raw_byte_size: 0,
        }
    }

    fn action(strategy: RetryStrategy, status: u16) -> &'static str {
        match (VmRetryLogic { strategy }).should_retry_response(&response(status, "")) {
            RetryAction::Successful => "success",
            RetryAction::Retry(_) => "retry",
            RetryAction::DontRetry(_) => "drop",
            RetryAction::RetryPartial(_) => "partial",
        }
    }

    #[test]
    fn default_strategy_matches_vmagent() {
        for (status, expected) in [
            (200, "success"),
            (204, "success"),
            (400, "drop"),
            (401, "retry"),
            (403, "retry"),
            (404, "retry"),
            (408, "retry"),
            (409, "drop"),
            (413, "retry"),
            (415, "drop"),
            (429, "retry"),
            (500, "retry"),
            (501, "retry"),
            (502, "retry"),
            (503, "retry"),
        ] {
            assert_eq!(action(RetryStrategy::Default, status), expected, "{status}");
        }
    }

    #[test]
    fn explicit_strategies_are_respected() {
        assert_eq!(action(RetryStrategy::None, 503), "drop");
        assert_eq!(action(RetryStrategy::All, 400), "retry");
    }

    #[test]
    fn reason_includes_response_body() {
        let logic = VmRetryLogic {
            strategy: RetryStrategy::Default,
        };
        let action = logic.should_retry_response(&response(400, "cannot parse labels\n"));
        let RetryAction::DontRetry(reason) = action else {
            panic!("400 must not be retried");
        };
        assert_eq!(reason, "400 Bad Request: cannot parse labels");
    }
}
