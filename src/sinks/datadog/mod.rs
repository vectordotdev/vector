use http::{Request, StatusCode, Uri};
use serde::{Deserialize, Serialize};

use crate::{http::HttpClient, sinks::HealthcheckError};

#[cfg(feature = "sinks-datadog_events")]
pub mod events;
#[cfg(feature = "sinks-datadog_logs")]
pub mod logs;
#[cfg(feature = "sinks-datadog_metrics")]
pub mod metrics;
#[cfg(feature = "sinks-datadog_traces")]
pub mod traces;

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    Us,
    Eu,
}

/// Gets the base domain to use for any calls to Datadog.
///
/// If `site` is not specified, we fallback to `region`, and if that is not specified, we
/// fallback to the Datadog US domain.
fn get_base_domain(site: Option<&String>, region: Option<Region>) -> &str {
    site.map(|s| s.as_str()).unwrap_or_else(|| match region {
        Some(Region::Eu) => "datadoghq.eu",
        None | Some(Region::Us) => "datadoghq.com",
    })
}

/// Gets the base API endpoint to use for any calls to Datadog.
///
/// If `site` is not specified, we fallback to `region`, and if that is not specified, we fallback
/// to the Datadog US domain.
fn get_api_base_endpoint(
    endpoint: Option<&String>,
    site: Option<&String>,
    region: Option<Region>,
) -> String {
    endpoint.cloned().unwrap_or_else(|| {
        let base = get_base_domain(site, region);
        format!("https://api.{}", base)
    })
}

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
