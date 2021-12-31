use http::StatusCode;
use serde::Deserialize;

use crate::{
    http::HttpError,
    sinks::{
        elasticsearch::service::ElasticSearchResponse,
        util::retries::{RetryAction, RetryLogic},
    },
};

#[derive(Deserialize, Debug)]
struct EsResultResponse {
    items: Vec<EsResultItem>,
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
    fn result(self) -> EsIndexResult {
        match self {
            EsResultItem::Index(r) => r,
            EsResultItem::Create(r) => r,
        }
    }
}

#[derive(Deserialize, Debug)]
struct EsIndexResult {
    error: Option<EsErrorDetails>,
}

#[derive(Deserialize, Debug)]
struct EsErrorDetails {
    reason: String,
    #[serde(rename = "type")]
    err_type: String,
}

#[derive(Clone)]
pub struct ElasticSearchRetryLogic;

impl RetryLogic for ElasticSearchRetryLogic {
    type Error = HttpError;
    type Response = ElasticSearchResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }

    fn should_retry_response(&self, response: &ElasticSearchResponse) -> RetryAction {
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
                    RetryAction::DontRetry(get_error_reason(&body).into())
                } else {
                    RetryAction::Successful
                }
            }
            _ => RetryAction::DontRetry(format!("response status: {}", status).into()),
        }
    }
}

fn get_error_reason(body: &str) -> String {
    match serde_json::from_str::<EsResultResponse>(body) {
        Err(json_error) => format!(
            "some messages failed, could not parse response, error: {}",
            json_error
        ),
        Ok(resp) => match resp.items.into_iter().find_map(|item| item.result().error) {
            Some(error) => format!("error type: {}, reason: {}", error.err_type, error.reason),
            None => format!("error response: {}", body),
        },
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use http::Response;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::event::EventStatus;

    #[test]
    fn handles_error_response() {
        let json = "{\"took\":185,\"errors\":true,\"items\":[{\"index\":{\"_index\":\"test-hgw28jv10u\",\"_type\":\"log_lines\",\"_id\":\"3GhQLXEBE62DvOOUKdFH\",\"status\":400,\"error\":{\"type\":\"illegal_argument_exception\",\"reason\":\"mapper [message] of different type, current_type [long], merged_type [text]\"}}}]}";
        let response = Response::builder()
            .status(StatusCode::OK)
            .body(Bytes::from(json))
            .unwrap();
        let logic = ElasticSearchRetryLogic;
        assert!(matches!(
            logic.should_retry_response(&ElasticSearchResponse {
                http_response: response,
                event_status: EventStatus::Rejected,
                batch_size: 1,
                events_byte_size: 1,
            }),
            RetryAction::DontRetry(_)
        ));
    }

    #[test]
    fn get_index_error_reason() {
        let json = "{\"took\":185,\"errors\":true,\"items\":[{\"index\":{\"_index\":\"test-hgw28jv10u\",\"_type\":\"log_lines\",\"_id\":\"3GhQLXEBE62DvOOUKdFH\",\"status\":400,\"error\":{\"type\":\"illegal_argument_exception\",\"reason\":\"mapper [message] of different type, current_type [long], merged_type [text]\"}}}]}";
        let reason = get_error_reason(json);
        assert_eq!(reason, "error type: illegal_argument_exception, reason: mapper [message] of different type, current_type [long], merged_type [text]");
    }

    #[test]
    fn get_create_error_reason() {
        let json = "{\"took\":3,\"errors\":true,\"items\":[{\"create\":{\"_index\":\"test-hgw28jv10u\",\"_type\":\"_doc\",\"_id\":\"aBLq1HcBWD7eBWkW2nj4\",\"status\":400,\"error\":{\"type\":\"mapper_parsing_exception\",\"reason\":\"object mapping for [host] tried to parse field [host] as object, but found a concrete value\"}}}]}";
        let reason = get_error_reason(json);
        assert_eq!(reason, "error type: mapper_parsing_exception, reason: object mapping for [host] tried to parse field [host] as object, but found a concrete value");
    }
}
