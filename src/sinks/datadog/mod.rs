use http::{Request, StatusCode, Uri};
use hyper::body::Body;
use snafu::Snafu;

use crate::{
    common::datadog::{get_api_base_endpoint, DD_US_SITE},
    http::{HttpClient, HttpError},
    sinks::HealthcheckError,
};

#[cfg(feature = "sinks-datadog_events")]
pub mod events;
#[cfg(feature = "sinks-datadog_logs")]
pub mod logs;
#[cfg(feature = "sinks-datadog_metrics")]
pub mod metrics;
#[cfg(feature = "sinks-datadog_traces")]
pub mod traces;

pub fn default_site() -> String {
    DD_US_SITE.to_owned()
}

/// Gets the API endpoint for validating credentials.
///
/// If `endpoint` is not specified, we fallback to `site`.
fn get_api_validate_endpoint(endpoint: Option<&String>, site: &str) -> crate::Result<Uri> {
    let base = get_api_base_endpoint(endpoint, site);
    let validate = format!("{}{}", base, "/api/v1/validate");
    validate.parse::<Uri>().map_err(Into::into)
}

async fn healthcheck(
    client: HttpClient,
    validate_endpoint: Uri,
    api_key: String,
) -> crate::Result<()> {
    let request = Request::get(validate_endpoint)
        .header("DD-API-KEY", api_key)
        .body(hyper::Body::empty())
        .unwrap();

    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => Ok(()),
        other => Err(HealthcheckError::UnexpectedStatus { status: other }.into()),
    }
}

#[derive(Debug, Snafu)]
pub enum DatadogApiError {
    #[snafu(display("Server responded with an error."))]
    ServerError,
    #[snafu(display("Failed to make HTTP(S) request: {}", error))]
    HttpError { error: HttpError },
    #[snafu(display("Client sent a payload that is too large."))]
    PayloadTooLarge,
    #[snafu(display("Client request was not valid for unknown reasons."))]
    BadRequest,
    #[snafu(display("Client request was forbidden."))]
    Forbidden,
}

impl DatadogApiError {
    /// Common DatadogApiError handling for HTTP Responses.
    /// Returns Ok(response) if the response was Ok/Accepted.
    pub fn from_result(
        result: Result<http::Response<Body>, HttpError>,
    ) -> Result<http::Response<Body>, DatadogApiError> {
        match result {
            Ok(response) => {
                match response.status() {
                    // From https://docs.datadoghq.com/api/latest/logs/:
                    //
                    // The status codes answered by the HTTP API are:
                    // 200: OK (v1)
                    // 202: Accepted (v2)
                    // 400: Bad request (likely an issue in the payload
                    //      formatting)
                    // 403: Permission issue (likely using an invalid API Key)
                    // 413: Payload too large (batch is above 5MB uncompressed)
                    // 5xx: Internal error, request should be retried after some
                    //      time
                    StatusCode::BAD_REQUEST => Err(DatadogApiError::BadRequest),
                    StatusCode::FORBIDDEN => Err(DatadogApiError::Forbidden),
                    StatusCode::OK | StatusCode::ACCEPTED => Ok(response),
                    StatusCode::PAYLOAD_TOO_LARGE => Err(DatadogApiError::PayloadTooLarge),
                    _ => Err(DatadogApiError::ServerError),
                }
            }
            Err(error) => Err(DatadogApiError::HttpError { error }),
        }
    }

    pub const fn is_retriable(&self) -> bool {
        match self {
            // This retry logic will be expanded further, but specifically retrying unauthorized
            // requests and lower level HttpErrorsfor now.
            // I verified using `curl` that `403` is the respose code for this.
            //
            // https://github.com/vectordotdev/vector/issues/10870
            // https://github.com/vectordotdev/vector/issues/12220
            DatadogApiError::HttpError { error } => error.is_retriable(),
            DatadogApiError::BadRequest | DatadogApiError::PayloadTooLarge => false,
            DatadogApiError::ServerError | DatadogApiError::Forbidden => true,
        }
    }
}
