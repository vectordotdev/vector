use std::sync::Arc;

use http::Request;
use serde::{Deserialize, Serialize};

use super::NewRelicCredentials;
use crate::{http::HttpClient, sinks::HealthcheckError};

#[derive(Serialize, Deserialize, Debug)]
struct NewRelicStatusModel {
    page: NewRelicStatusPage,
    components: Vec<NewRelicStatusComponent>,
}

#[derive(Serialize, Deserialize, Debug)]
struct NewRelicStatusPage {
    id: String,
    name: String,
    url: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct NewRelicStatusComponent {
    id: String,
    name: String,
    status: String,
}

pub(crate) async fn healthcheck(
    client: HttpClient,
    credentials: Arc<NewRelicCredentials>,
) -> crate::Result<()> {
    let request = Request::post(credentials.get_uri())
        .header("Api-Key", credentials.license_key.clone())
        .body(hyper::Body::empty())
        .unwrap();

    let response = client.send(request).await?;

    match response.status() {
        status if status.is_success() => Ok(()),
        other => Err(HealthcheckError::UnexpectedStatus { status: other }.into()),
    }
}
