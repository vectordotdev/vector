use super::models::InsertAllResponse;
use crate::{
    http::HttpError,
    sinks::util::{
        http::HttpRetryLogic,
        retries::{RetryAction, RetryLogic},
        sink::{Response, ServiceLogic},
    },
};

use bytes::Bytes;
use http::StatusCode;

#[derive(Debug, Default, Clone)]
pub(super) struct BigqueryRetryLogic {
    inner: HttpRetryLogic,
}
use vector_core::event::EventStatus;

impl RetryLogic for BigqueryRetryLogic {
    type Error = HttpError;
    type Response = http::Response<Bytes>;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        self.inner.is_retriable_error(error)
    }

    // If ignore_unknown_values is set and the schema of the events inserted is
    // wrong, bigquery will still return a 200. We have to look in the response
    // to determine if the schema was wrong.
    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        match response.status() {
            StatusCode::OK => {
                let body = String::from_utf8_lossy(response.body());
                if body.contains("\"insertErrors\"") {
                    RetryAction::DontRetry(get_error_reason(response.body()))
                } else {
                    RetryAction::Successful
                }
            }
            _ => self.inner.should_retry_response(response),
        }
    }
}

fn get_error_reason(response_body: &[u8]) -> String {
    let response: InsertAllResponse = match serde_json::from_slice(response_body) {
        Err(json_error) => {
            return format!(
                "some messages failed, could not parse response, error: {}",
                json_error
            )
        }
        Ok(insert_all_response) => insert_all_response,
    };

    let error_messages = response.get_error_messages();
    format!("error messages: {}", error_messages)
}

#[derive(Debug, Default, Clone)]
pub(super) struct BigqueryServiceLogic;
impl ServiceLogic for BigqueryServiceLogic {
    type Response = http::Response<Bytes>;
    fn result_status(&self, result: crate::Result<Self::Response>) -> EventStatus {
        match result {
            Ok(response) => {
                if response.is_successful() {
                    let body = String::from_utf8_lossy(response.body());
                    if body.contains("\"insertErrors\"") {
                        error!(message = "", ?response);
                        EventStatus::Failed
                    } else {
                        trace!(message = "Response successful.", ?response);
                        EventStatus::Delivered
                    }
                } else if response.is_transient() {
                    error!(message = "Response wasn't successful.", ?response);
                    EventStatus::Errored
                } else {
                    error!(message = "Response failed.", ?response);
                    EventStatus::Failed
                }
            }
            Err(error) => {
                error!(message = "Request failed.", %error);
                EventStatus::Errored
            }
        }
    }
}
