use http::{Request, StatusCode, Uri};

use crate::{
    common::datadog::{get_api_base_endpoint, Region},
    http::HttpClient,
    sinks::HealthcheckError,
};

#[cfg(feature = "sinks-datadog_events")]
pub mod events;
#[cfg(feature = "sinks-datadog_logs")]
pub mod logs;
#[cfg(feature = "sinks-datadog_metrics")]
pub mod metrics;

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
