use http::StatusCode;
use serde::Deserialize;

use crate::{
    http::HttpError,
    sinks::{
        elasticsearch::service::ElasticsearchResponse,
        util::retries::{RetryAction, RetryLogic},
    },
};

#[derive(Deserialize, Debug)]
struct EsResultResponse {
    items: Vec<EsResultItem>,
}

impl EsResultResponse {
    fn parse(body: &str) -> Result<Self, String> {
        serde_json::from_str::<EsResultResponse>(body).map_err(|json_error| {
            format!(
                "some messages failed, could not parse response, error: {}",
                json_error
            )
        })
    }

    /// Returns iterator over status codes for items and optional error details.
    fn iter_status(&self) -> impl Iterator<Item = (StatusCode, Option<&EsErrorDetails>)> {
        self.items.iter().filter_map(|item| {
            item.result()
                .status
                .and_then(|status| StatusCode::from_u16(status).ok())
                .map(|status| (status, item.result().error.as_ref()))
        })
    }

    /// Selects the first error since logging all errors would be quite verbose and many are duplicates.
    /// If partial retry is enabled and we don't retry, this is because there is no retriable error in the
    /// response, thus all errors are equally interesting so logging the first is sufficient.
    /// When partial retry is disabled, we don't retry on any error.
    fn get_error_reason(&self, body: &str) -> String {
        match self
            .items
            .iter()
            .find_map(|item| item.result().error.as_ref())
        {
            Some(error) => format!("error type: {}, reason: {}", error.err_type, error.reason),
            None => format!("error response: {}", body),
        }
    }
}

#[derive(Deserialize, Debug)]
enum EsResultItem {
    #[serde(rename = "index")]
    Index(EsIndexResult),
    #[serde(rename = "create")]
    Create(EsIndexResult),
}

impl EsResultItem {
    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    fn result(&self) -> &EsIndexResult {
        match self {
            EsResultItem::Index(r) => r,
            EsResultItem::Create(r) => r,
        }
    }
}

#[derive(Deserialize, Debug)]
struct EsIndexResult {
    status: Option<u16>,
    error: Option<EsErrorDetails>,
}

#[derive(Deserialize, Debug)]
struct EsErrorDetails {
    reason: String,
    #[serde(rename = "type")]
    err_type: String,
}

#[derive(Clone)]
pub struct ElasticsearchRetryLogic {
    pub retry_partial: bool,
}

impl RetryLogic for ElasticsearchRetryLogic {
    type Error = HttpError;
    type Response = ElasticsearchResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }

    fn should_retry_response(&self, response: &ElasticsearchResponse) -> RetryAction {
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
            _ if status.is_client_error() => {
                let body = String::from_utf8_lossy(response.http_response.body());
                RetryAction::DontRetry(format!("client-side error, {}: {}", status, body).into())
            }
            _ if status.is_success() => {
                let body = String::from_utf8_lossy(response.http_response.body());

                if body.contains("\"errors\":true") {
                    match EsResultResponse::parse(&body) {
                        Ok(resp) => {
                            if self.retry_partial {
                                // We will retry if there exists at least one item that
                                // failed with a retriable error.
                                // Those are backpressure and server errors.
                                if let Some((status, error)) =
                                    resp.iter_status().find(|(status, _)| {
                                        *status == StatusCode::TOO_MANY_REQUESTS
                                            || status.is_server_error()
                                    })
                                {
                                    let msg = if let Some(error) = error {
                                        format!(
                                            "partial error, status: {}, error type: {}, reason: {}",
                                            status, error.err_type, error.reason
                                        )
                                    } else {
                                        format!("partial error, status: {}", status)
                                    };
                                    return RetryAction::Retry(msg.into());
                                }
                            }

                            RetryAction::DontRetry(resp.get_error_reason(&body).into())
                        }
                        Err(msg) => RetryAction::DontRetry(msg.into()),
                    }
                } else {
                    RetryAction::Successful
                }
            }
            _ => RetryAction::DontRetry(format!("response status: {}", status).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use http::Response;
    use similar_asserts::assert_eq;
    use vector_lib::{internal_event::CountByteSize, json_size::JsonSize};

    use super::*;
    use crate::event::EventStatus;

    #[test]
    fn handles_error_response() {
        let json = "{\"took\":185,\"errors\":true,\"items\":[{\"index\":{\"_index\":\"test-hgw28jv10u\",\"_type\":\"log_lines\",\"_id\":\"3GhQLXEBE62DvOOUKdFH\",\"status\":400,\"error\":{\"type\":\"illegal_argument_exception\",\"reason\":\"mapper [message] of different type, current_type [long], merged_type [text]\"}}}]}";
        let response = Response::builder()
            .status(StatusCode::OK)
            .body(Bytes::from(json))
            .unwrap();
        let logic = ElasticsearchRetryLogic {
            retry_partial: false,
        };
        assert!(matches!(
            logic.should_retry_response(&ElasticsearchResponse {
                http_response: response,
                event_status: EventStatus::Rejected,
                events_byte_size: CountByteSize(1, JsonSize::new(1)).into(),
            }),
            RetryAction::DontRetry(_)
        ));
    }

    #[test]
    fn handles_partial_error_response() {
        let json = "{\"took\":34,\"errors\":true,\"items\":[{\"index\":{\"_index\":\"test-asjkf1234\",\"_type\":\"log_lines\",\"_id\":\"4Z3QLYEBT52RtoOEKz2H\",\"status\":429}}]}";
        let response = Response::builder()
            .status(StatusCode::OK)
            .body(Bytes::from(json))
            .unwrap();
        let logic = ElasticsearchRetryLogic {
            retry_partial: true,
        };
        assert!(matches!(
            logic.should_retry_response(&ElasticsearchResponse {
                http_response: response,
                event_status: EventStatus::Errored,
                events_byte_size: CountByteSize(1, JsonSize::new(1)).into(),
            }),
            RetryAction::Retry(_)
        ));
    }

    #[test]
    fn get_index_error_reason() {
        let json = "{\"took\":185,\"errors\":true,\"items\":[{\"index\":{\"_index\":\"test-hgw28jv10u\",\"_type\":\"log_lines\",\"_id\":\"3GhQLXEBE62DvOOUKdFH\",\"status\":400,\"error\":{\"type\":\"illegal_argument_exception\",\"reason\":\"mapper [message] of different type, current_type [long], merged_type [text]\"}}}]}";
        let reason = match EsResultResponse::parse(json) {
            Ok(resp) => resp.get_error_reason(json),
            Err(msg) => msg,
        };
        assert_eq!(reason, "error type: illegal_argument_exception, reason: mapper [message] of different type, current_type [long], merged_type [text]");
    }

    #[test]
    fn get_create_error_reason() {
        let json = "{\"took\":3,\"errors\":true,\"items\":[{\"create\":{\"_index\":\"test-hgw28jv10u\",\"_type\":\"_doc\",\"_id\":\"aBLq1HcBWD7eBWkW2nj4\",\"status\":400,\"error\":{\"type\":\"mapper_parsing_exception\",\"reason\":\"object mapping for [host] tried to parse field [host] as object, but found a concrete value\"}}}]}";
        let reason = match EsResultResponse::parse(json) {
            Ok(resp) => resp.get_error_reason(json),
            Err(msg) => msg,
        };
        assert_eq!(reason, "error type: mapper_parsing_exception, reason: object mapping for [host] tried to parse field [host] as object, but found a concrete value");
    }
}
