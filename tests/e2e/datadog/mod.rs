pub mod logs;
pub mod metrics;

use std::future::Future;
use std::time::Duration;

use reqwest::{Client, Method};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use tracing::{trace, warn};

// Fakeintake may not be ready to accept connections yet, particularly right
// after the compose services start, so transient request failures are retried.
const MAX_FETCH_ATTEMPTS: usize = 10;
const FETCH_RETRY_INTERVAL: Duration = Duration::from_secs(1);

// Shared wait between polling attempts for tests that need to retry until
// expected data has arrived at fakeintake (data itself may take a moment to
// flow through the pipeline, on top of any transient connection issues
// already handled by `get_fakeintake_payloads`). How many attempts are needed
// varies per test, so `max_retries` is left to the caller.
const WAIT_INTERVAL: Duration = Duration::from_secs(1);

// Wait used by `poll_until_stable` once `is_complete` first reports true,
// before re-fetching to confirm the data has actually stopped changing.
// The Agent's default aggregator flush interval is ~15s (see
// tests/e2e/datadog/metrics/mod.rs), so a gap as short as `WAIT_INTERVAL`
// can find two consecutive fetches equal simply because no new flush has
// landed yet, not because all data has arrived. This needs to span at
// least one flush interval, with margin, to actually detect that.
pub(super) const STABLE_WAIT_INTERVAL: Duration = Duration::from_secs(20);

// Calls `fetch` up to `max_retries` times, sleeping `wait` between attempts,
// until `is_complete` reports the fetched value is ready to use.
pub(super) async fn poll_until<T, F, Fut, P>(
    max_retries: usize,
    wait: Duration,
    mut fetch: F,
    mut is_complete: P,
) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = T>,
    P: FnMut(&T) -> bool,
{
    assert!(max_retries > 0, "max_retries must be greater than zero");

    let mut value = fetch().await;
    let mut attempt = 1;
    while attempt < max_retries && !is_complete(&value) {
        tokio::time::sleep(wait).await;
        value = fetch().await;
        attempt += 1;
    }

    value
}

// Like `poll_until`, but for callers where a single `is_complete` pass isn't
// enough to know the data is done arriving: the dogstatsd emitter
// (tests/e2e/datadog-metrics/dogstatsd_client/client.py) sends each metric type
// across many loop iterations, so the first fetch to satisfy `is_complete` may
// still be a partial flush that a later fetch would extend. Once `is_complete`
// is satisfied, this keeps polling until two consecutive fetches are equal
// (i.e. the data has stopped changing) or `max_retries` is hit.
pub(super) async fn poll_until_stable<T, F, Fut, P>(
    max_retries: usize,
    wait: Duration,
    stable_wait: Duration,
    mut fetch: F,
    mut is_complete: P,
) -> T
where
    T: PartialEq,
    F: FnMut() -> Fut,
    Fut: Future<Output = T>,
    P: FnMut(&T) -> bool,
{
    assert!(max_retries > 0, "max_retries must be greater than zero");

    let mut value = fetch().await;
    let mut attempt = 1;
    while attempt < max_retries {
        if is_complete(&value) {
            tokio::time::sleep(stable_wait).await;
            attempt += 1;
            let next = fetch().await;
            if next == value {
                return next;
            }
            value = next;
            continue;
        }

        tokio::time::sleep(wait).await;
        value = fetch().await;
        attempt += 1;
    }

    value
}

fn fake_intake_vector_address() -> String {
    std::env::var("FAKE_INTAKE_VECTOR_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:8082".to_string())
}

fn fake_intake_agent_address() -> String {
    std::env::var("FAKE_INTAKE_AGENT_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:8083".to_string())
}

#[derive(Deserialize, Debug)]
struct FakeIntakePayload<D> {
    // When string, base64 encoded
    data: D,
    #[serde(rename = "encoding")]
    _encoding: String,
    #[serde(rename = "timestamp")]
    _timestamp: String,
}

type FakeIntakePayloadJson = FakeIntakePayload<Value>;

type FakeIntakePayloadRaw = FakeIntakePayload<String>;

trait FakeIntakeResponseT {
    fn build_url(base: &str, endpoint: &str) -> String;
}

#[derive(Deserialize, Debug)]
struct FakeIntakeResponse<P> {
    payloads: Vec<P>,
}

impl<P> Default for FakeIntakeResponse<P> {
    fn default() -> Self {
        Self {
            payloads: Vec::new(),
        }
    }
}

type FakeIntakeResponseJson = FakeIntakeResponse<FakeIntakePayloadJson>;

impl FakeIntakeResponseT for FakeIntakeResponseJson {
    fn build_url(base: &str, endpoint: &str) -> String {
        format!("{base}/fakeintake/payloads?endpoint={endpoint}&format=json",)
    }
}

type FakeIntakeResponseRaw = FakeIntakeResponse<FakeIntakePayloadRaw>;

impl FakeIntakeResponseT for FakeIntakeResponseRaw {
    fn build_url(base: &str, endpoint: &str) -> String {
        format!("{base}/fakeintake/payloads?endpoint={endpoint}",)
    }
}

async fn get_fakeintake_payloads<R>(base: &str, endpoint: &str) -> R
where
    R: FakeIntakeResponseT + DeserializeOwned + Default,
{
    let url = &R::build_url(base, endpoint);

    let mut last_error = String::new();
    for attempt in 1..=MAX_FETCH_ATTEMPTS {
        match Client::new().request(Method::GET, url).send().await {
            Ok(response) => {
                trace!(
                    "Fakeintake response headers for {endpoint}: {:?}",
                    response.headers()
                );

                match response.json::<R>().await {
                    Ok(parsed) => return parsed,
                    Err(e) => last_error = format!("Parsing fakeintake payloads failed: {e}"),
                }
            }
            Err(e) => last_error = format!("Sending GET request to {url} failed: {e}"),
        }

        if attempt < MAX_FETCH_ATTEMPTS {
            warn!("{last_error}, retrying...");
            tokio::time::sleep(FETCH_RETRY_INTERVAL).await;
        }
    }

    // Don't panic here: fakeintake may just be slow to start (no healthcheck
    // gates the compose services), and the outer data-arrival polling loop
    // (`poll_until`/`poll_until_stable`) has its own, longer retry budget.
    // Returning empty payloads lets that loop keep retrying instead of the
    // whole test aborting after this function's fixed attempt count.
    warn!(
        "{last_error} after {MAX_FETCH_ATTEMPTS} attempts, yielding empty payloads to outer retry loop"
    );
    R::default()
}
