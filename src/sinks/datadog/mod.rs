use http::{Request, StatusCode, Uri};
use snafu::Snafu;

use crate::{
    common::datadog::{get_api_base_endpoint, Region},
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

/// Gets the API endpoint for validating credentials.
///
/// If `site` is not specified, we fallback to `region`, and if that is not specified, we fallback
/// to the Datadog US domain.
fn get_api_validate_endpoint(
    endpoint: Option<&String>,
    site: Option<&String>,
    region: Option<Region>,
) -> crate::Result<Uri> {
    let base = get_api_base_endpoint(endpoint, site, region);
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

pub const fn is_retriable_error(error: &DatadogApiError) -> bool {
    match *error {
        DatadogApiError::HttpError {
            error: HttpError::BuildRequest { .. },
        }
        | DatadogApiError::HttpError {
            error: HttpError::MakeProxyConnector { .. },
        }
        | DatadogApiError::BadRequest
        | DatadogApiError::PayloadTooLarge => false,
        // This retry logic will be expanded further, but specifically retrying unauthorized
        // requests and lower level HttpErrorsfor now.
        // I verified using `curl` that `403` is the respose code for this.
        //
        // https://github.com/vectordotdev/vector/issues/10870
        // https://github.com/vectordotdev/vector/issues/12220
        DatadogApiError::HttpError {
            error: HttpError::CallRequest { .. },
        }
        | DatadogApiError::HttpError {
            error: HttpError::BuildTlsConnector { .. },
        }
        | DatadogApiError::HttpError {
            error: HttpError::MakeHttpsConnector { .. },
        }
        | DatadogApiError::ServerError
        | DatadogApiError::Forbidden => true,
    }
}
