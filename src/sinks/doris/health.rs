use crate::{
    http::HttpError,
    sinks::{ util::service::HealthLogic},
};
use crate::sinks::doris::service::DorisResponse;

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
                    Some(true)
                } else if status.is_server_error() {
                    Some(false)
                } else {
                    None
                }
            }
            Err(error) => match error.downcast_ref::<HttpError>() {
                Some(HttpError::CallRequest { .. }) => Some(false),
                _ => None,
            },
        }
    }
}
