use crate::sinks::UriParseError;
use crate::{http::HttpClient, sinks::HealthcheckError};

use http::{Request, StatusCode, Uri};
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use std::sync::Arc;

#[cfg(feature = "sinks-datadog_events")]
pub mod events;
#[cfg(feature = "sinks-datadog_logs")]
pub mod logs;
#[cfg(feature = "sinks-datadog_metrics")]
pub mod metrics;

type ApiKey = Arc<str>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    Us,
    Eu,
}

async fn healthcheck(endpoint: String, api_key: String, client: HttpClient) -> crate::Result<()> {
    let uri = format!("{}/api/v1/validate", endpoint)
        .parse::<Uri>()
        .context(UriParseError)?;

    let request = Request::get(uri)
        .header("DD-API-KEY", api_key)
        .body(hyper::Body::empty())
        .unwrap();

    let response = client.send(request).await?;

    match response.status() {
        StatusCode::OK => Ok(()),
        other => Err(HealthcheckError::UnexpectedStatus { status: other }.into()),
    }
}
