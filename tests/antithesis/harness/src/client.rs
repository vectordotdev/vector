use std::time::Duration;

use crate::{Event, OracleReport};

const ORACLE_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const ORACLE_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Client for the oracle's obligation and verdict API.
#[derive(Clone)]
pub struct OracleClient {
    http: reqwest::Client,
    base_url: String,
}

impl OracleClient {
    pub fn new(http: reqwest::Client, base_url: impl Into<String>) -> Self {
        Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_owned(),
        }
    }

    /// Claim one fresh id from the oracle.
    pub async fn claim(&self) -> Option<u64> {
        self.http
            .post(format!("{}/claim", self.base_url))
            .timeout(ORACLE_REQUEST_TIMEOUT)
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?
            .text()
            .await
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    /// Record that the pipeline acknowledged `id` and now owes its delivery.
    pub async fn report_acked(&self, id: u64) -> bool {
        matches!(
            self.http
                .post(format!("{}/acked", self.base_url))
                .timeout(ORACLE_REQUEST_TIMEOUT)
                .body(id.to_string())
                .send()
                .await,
            Ok(resp) if resp.status().is_success()
        )
    }

    pub async fn report(&self) -> Option<OracleReport> {
        self.http
            .get(format!("{}/report", self.base_url))
            .timeout(ORACLE_READ_TIMEOUT)
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?
            .json()
            .await
            .ok()
    }

    pub async fn delivered(&self, id: u64) -> bool {
        let Ok(response) = self
            .http
            .get(format!("{}/delivered?id={id}", self.base_url))
            .timeout(ORACLE_READ_TIMEOUT)
            .send()
            .await
        else {
            return false;
        };
        if !response.status().is_success() {
            return false;
        }
        response
            .text()
            .await
            .map(|body| body.trim() == "1")
            .unwrap_or(false)
    }
}

/// Client for submitting canonical events to the scenario's Vector source.
pub struct VectorClient {
    http: reqwest::Client,
    source_url: String,
}

impl VectorClient {
    pub fn new(http: reqwest::Client, source_url: impl Into<String>) -> Self {
        Self {
            http,
            source_url: source_url.into(),
        }
    }

    /// A successful response means Vector took end-to-end responsibility for the
    /// event when acknowledgements are enabled on the scenario's source and sink.
    pub async fn post_event(&self, id: u64, timeout: Duration) -> bool {
        let events = [Event::for_id(id)];
        matches!(
            self.http
                .post(&self.source_url)
                .timeout(timeout)
                .json(&events)
                .send()
                .await,
            Ok(resp) if resp.status().is_success()
        )
    }
}

pub async fn endpoint_healthy(client: &reqwest::Client, endpoint: &str, timeout: Duration) -> bool {
    matches!(
        client.get(endpoint).timeout(timeout).send().await,
        Ok(resp) if resp.status().is_success()
    )
}

pub async fn all_endpoints_healthy(
    client: &reqwest::Client,
    endpoints: &[String],
    timeout: Duration,
) -> bool {
    for endpoint in endpoints {
        if !endpoint_healthy(client, endpoint, timeout).await {
            return false;
        }
    }
    true
}
