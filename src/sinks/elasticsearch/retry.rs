use crate::sinks::{
    elasticsearch::encoder::ProcessedEvent,
    util::{metadata::RequestMetadataBuilder, request_builder::RequestBuilder},
};
use crate::{
    event::Finalizable,
    http::HttpError,
    sinks::{
        elasticsearch::service::{ElasticsearchRequest, ElasticsearchResponse},
        util::retries::{RetryAction, RetryLogic},
    },
};
use http::StatusCode;
use serde::Deserialize;
use vector_lib::json_size::JsonSize;
use vector_lib::EstimatedJsonEncodedSizeOf;

#[derive(Deserialize, Debug)]
struct EsResultResponse {
    items: Vec<EsResultItem>,
}

impl EsResultResponse {
    fn parse(body: &str) -> Result<Self, String> {
        serde_json::from_str::<EsResultResponse>(body).map_err(|json_error| {
            format!("some messages failed, could not parse response, error: {json_error}")
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
            None => format!("error response: {body}"),
        }
    }
}

#[derive(Deserialize, Debug)]
enum EsResultItem {
    #[serde(rename = "index")]
    Index(EsIndexResult),
    #[serde(rename = "create")]
    Create(EsIndexResult),
    #[serde(rename = "update")]
    Update(EsIndexResult),
}

impl EsResultItem {
    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    fn result(&self) -> &EsIndexResult {
        match self {
            EsResultItem::Index(r) => r,
            EsResultItem::Create(r) => r,
            EsResultItem::Update(r) => r,
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
    type Request = ElasticsearchRequest;
    type Response = ElasticsearchResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }

    fn should_retry_response(
        &self,
        response: &ElasticsearchResponse,
    ) -> RetryAction<ElasticsearchRequest> {
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
                RetryAction::DontRetry(format!("client-side error, {status}: {body}").into())
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
                                let status_codes: Vec<bool> = resp
                                    .iter_status()
                                    .map(|(status, _)| {
                                        status == StatusCode::TOO_MANY_REQUESTS
                                            || status.is_server_error()
                                    })
                                    .collect();
                                if let Some((_status, _error)) =
                                    resp.iter_status().find(|(status, _)| {
                                        *status == StatusCode::TOO_MANY_REQUESTS
                                            || status.is_server_error()
                                    })
                                {
                                    return RetryAction::RetryPartial(Box::new(
                                        move |req: ElasticsearchRequest| {
                                            let mut failed_events: Vec<ProcessedEvent> = req
                                                .original_events
                                                .clone()
                                                .into_iter()
                                                .zip(status_codes.iter())
                                                .filter(|(_, &flag)| flag)
                                                .map(|(item, _)| item)
                                                .collect();
                                            let finalizers = failed_events.take_finalizers();
                                            let batch_size = failed_events.len();
                                            let events_byte_size = failed_events
                                                .iter()
                                                .map(|x| x.log.estimated_json_encoded_size_of())
                                                .fold(JsonSize::zero(), |a, b| a + b);
                                            let encode_result = match req
                                                .elasticsearch_request_builder
                                                .encode_events(failed_events.clone())
                                            {
                                                Ok(s) => s,
                                                Err(_) => return req,
                                            };
                                            let metadata_builder =
                                                RequestMetadataBuilder::from_events(&failed_events);
                                            let metadata = metadata_builder.build(&encode_result);
                                            ElasticsearchRequest {
                                                payload: encode_result.into_payload(),
                                                finalizers,
                                                batch_size,
                                                events_byte_size,
                                                metadata,
                                                original_events: failed_events,
                                                elasticsearch_request_builder: req
                                                    .elasticsearch_request_builder,
                                            }
                                        },
                                    ));
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
            _ => RetryAction::DontRetry(format!("response status: {status}").into()),
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
            RetryAction::RetryPartial(_)
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
