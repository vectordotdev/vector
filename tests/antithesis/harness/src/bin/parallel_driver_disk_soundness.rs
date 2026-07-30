//! Fault-phase load for the disk-buffer soundness scenario.
//!
//! A source 2xx is deliberately not recorded as a delivery obligation. The
//! current acknowledgement boundary is not tied to the disk buffer's fsync, so
//! crashes are allowed to lose these records. Retries and duplicates are also
//! allowed; the oracle checks the integrity of anything that does arrive.

#![allow(clippy::disallowed_types)] // antithesis assert macros expand to once_cell::Lazy

#[cfg(target_os = "linux")]
extern crate antithesis_instrumentation;

use antithesis_harness::payload_field;
use antithesis_sdk::{antithesis_init, assert_always, assert_reachable};
use clap::Parser;
use serde_json::json;
use tokio::time;

const MAX_BUFFER_SIZE_BYTES: f64 = 8_388_608.0;
const MAX_ATTEMPTS: usize = 3;

#[derive(Parser)]
struct Args {
    #[arg(long, env = "VECTOR_SOURCE_URL", default_value = "http://vector:8080/")]
    source_url: String,
    #[arg(long, env = "ORACLE_URL", default_value = "http://127.0.0.1:8686")]
    oracle_url: String,
    #[arg(
        long,
        env = "VECTOR_METRICS_URL",
        default_value = "http://vector:9598/metrics"
    )]
    metrics_url: String,
}

fn disk_metric_sum(body: &str, metric_name: &str) -> Option<f64> {
    let mut matches = 0usize;
    let sum = body
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let sample = fields.next()?;
            let value = fields.next()?;
            if sample.starts_with(metric_name)
                && sample.contains("buffer_type=\"disk\"")
                && sample.contains("buffer_id=\"out\"")
            {
                matches += 1;
                value.parse::<f64>().ok()
            } else {
                None
            }
        })
        .sum();
    (matches > 0).then_some(sum)
}

async fn check_observed_occupancy(client: &reqwest::Client, metrics_url: &str) {
    let Ok(response) = client
        .get(metrics_url)
        .timeout(time::Duration::from_secs(2))
        .send()
        .await
    else {
        return;
    };
    let Ok(body) = response.text().await else {
        return;
    };
    let Some(occupancy_bytes) = disk_metric_sum(&body, "vector_buffer_size_bytes") else {
        return;
    };
    assert_always!(
        occupancy_bytes <= MAX_BUFFER_SIZE_BYTES,
        "observed disk-buffer occupancy never exceeds the configured limit",
        &json!({
            "occupancy_bytes": occupancy_bytes,
            "configured_limit": MAX_BUFFER_SIZE_BYTES,
        })
    );
}

async fn claim(client: &reqwest::Client, oracle_url: &str) -> Option<u64> {
    let response = client
        .post(format!("{oracle_url}/claim"))
        .timeout(time::Duration::from_secs(5))
        .send()
        .await
        .ok()?;
    response.text().await.ok()?.trim().parse().ok()
}

async fn post_event(client: &reqwest::Client, source_url: &str, id: u64) -> bool {
    let event = json!([{ "id": id, "data": payload_field(id) }]);
    matches!(
        client
            .post(source_url)
            .timeout(time::Duration::from_secs(3))
            .json(&event)
            .send()
            .await,
        Ok(response) if response.status().is_success()
    )
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    antithesis_init();
    let args = Args::parse();
    let client = reqwest::Client::new();

    check_observed_occupancy(&client, &args.metrics_url).await;

    let Some(id) = claim(&client, &args.oracle_url).await else {
        return;
    };
    for _ in 0..MAX_ATTEMPTS {
        if post_event(&client, &args.source_url, id).await {
            assert_reachable!(
                "the disk-buffer source accepts traffic while faults are active",
                &json!({ "id": id })
            );
            return;
        }
        time::sleep(time::Duration::from_millis(100)).await;
    }
}
