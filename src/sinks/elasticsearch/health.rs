use vector_http::HttpError;

use crate::sinks::{elasticsearch::service::ElasticsearchResponse, util::service::HealthLogic};

#[derive(Clone)]
pub struct ElasticsearchHealthLogic;

impl HealthLogic for ElasticsearchHealthLogic {
    type Error = crate::Error;
    type Response = ElasticsearchResponse;

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
            // Retriable errors are transport failures of the request.
            Err(error) => match error.downcast_ref::<HttpError>() {
                Some(error) if error.is_retriable() => Some(false),
                _ => None,
            },
        }
    }
}
