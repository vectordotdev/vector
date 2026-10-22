use std::sync::Arc;

use http_1::Request;

use super::NewRelicCredentials;
use crate::{
    http::client_v1::{HttpClient, empty_body},
    sinks::HealthcheckError,
};

pub(crate) async fn healthcheck(
    client: HttpClient,
    credentials: Arc<NewRelicCredentials>,
) -> crate::Result<()> {
    let request = Request::post(credentials.get_uri())
        .header("Api-Key", credentials.license_key.clone())
        .body(empty_body())
        .expect("building healthcheck HTTP request failed unexpectedly");

    let response = client.send(request).await?;

    match response.status() {
        status if status.is_success() => Ok(()),
        other => Err(HealthcheckError::unexpected_status(other).into()),
    }
}
