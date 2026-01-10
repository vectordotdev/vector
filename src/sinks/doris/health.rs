use crate::{
    http::HttpError,
    sinks::{doris::service::DorisResponse, util::service::HealthLogic},
};
use tracing::{debug, error};

#[derive(Clone)]
pub struct DorisHealthLogic;

impl HealthLogic for DorisHealthLogic {
    type Error = crate::Error;
    type Response = DorisResponse;

    fn is_healthy(&self, response: &Result<Self::Response, Self::Error>) -> Option<bool> {
        match response {
            Ok(response) => {
                let status = response.http_response.status();
                if status.is_success() {
                    debug!(
                        message = "Health check succeeded with success status code.",
                        status_code = %status
                    );
                    Some(true)
                } else if status.is_server_error() {
                    error!(
                        message = "Health check failed with server error status code.",
                        status_code = %status
                    );
                    Some(false)
                } else {
                    debug!(
                        message = "Health check returned non-success status code, but not determining health state.",
                        status_code = %status
                    );
                    None
                }
            }
            Err(error) => match error.downcast_ref::<HttpError>() {
                Some(http_error) => {
                    error!(
                        message = "Health check failed with HTTP error.",
                        error_type = "HttpError::CallRequest",
                        %http_error
                    );
                    Some(false)
                }
                _ => {
                    debug!(
                        message = "Health check failed with non-HTTP error, not determining health state.",
                        %error
                    );
                    None
                }
            },
        }
    }
}
