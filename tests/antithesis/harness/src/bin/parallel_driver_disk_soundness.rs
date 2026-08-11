//! Fault-phase load for the disk-buffer soundness scenario.
//!
//! A source 2xx is deliberately not recorded as a delivery obligation. The
//! current acknowledgement boundary is not tied to the disk buffer's fsync, so
//! crashes are allowed to lose these records. Retries and duplicates are also
//! allowed; the oracle checks the integrity of anything that does arrive.

#![allow(clippy::disallowed_types)] // antithesis assert macros expand to once_cell::Lazy

#[cfg(target_os = "linux")]
extern crate antithesis_instrumentation;

use antithesis_harness::{is_pressure_payload, payload_field};
use antithesis_sdk::{antithesis_init, assert_always, assert_reachable};
use clap::Parser;
use serde_json::json;
use tokio::time;

const MAX_BUFFER_SIZE_BYTES: f64 = 8_388_608.0;
const PRESSURE_HIGH_WATERMARK_BYTES: f64 = 5_242_880.0;
const INITIAL_PRESSURE_REQUESTS: usize = 12;
const FOLLOWUP_PRESSURE_REQUESTS: usize = 4;

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

async fn check_observed_occupancy(client: &reqwest::Client, metrics_url: &str) -> Option<f64> {
    let Ok(response) = client
        .get(metrics_url)
        .timeout(time::Duration::from_secs(2))
        .send()
        .await
    else {
        return None;
    };
    let Ok(body) = response.text().await else {
        return None;
    };
    let occupancy_bytes = disk_metric_sum(&body, "vector_buffer_size_bytes")?;
    assert_always!(
        occupancy_bytes <= MAX_BUFFER_SIZE_BYTES,
        "observed disk-buffer occupancy never exceeds the configured limit",
        &json!({
            "occupancy_bytes": occupancy_bytes,
            "configured_limit": MAX_BUFFER_SIZE_BYTES,
        })
    );
    Some(occupancy_bytes)
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

async fn claim_pressure_ids(client: &reqwest::Client, oracle_url: &str, count: usize) -> Vec<u64> {
    let mut ids = Vec::with_capacity(count);
    while ids.len() < count {
        let Some(id) = claim(client, oracle_url).await else {
            break;
        };
        if is_pressure_payload(id) {
            ids.push(id);
        }
    }
    ids
}

async fn close_ingest_gate(client: &reqwest::Client, oracle_url: &str) -> Option<bool> {
    let response = client
        .post(format!("{oracle_url}/ingest/close"))
        .timeout(time::Duration::from_secs(3))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    Some(response.text().await.ok()?.trim() == "1")
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

    let occupancy = check_observed_occupancy(&client, &args.metrics_url).await;
    let Some(newly_closed) = close_ingest_gate(&client, &args.oracle_url).await else {
        return;
    };
    if newly_closed {
        assert_reachable!("the fault-phase driver closes the sink retry gate");
    }

    if !newly_closed && occupancy.is_some_and(|bytes| bytes >= PRESSURE_HIGH_WATERMARK_BYTES) {
        return;
    }

    let request_count = if newly_closed {
        INITIAL_PRESSURE_REQUESTS
    } else {
        FOLLOWUP_PRESSURE_REQUESTS
    };
    let ids = claim_pressure_ids(&client, &args.oracle_url, request_count).await;
    let mut requests = tokio::task::JoinSet::new();
    for id in ids {
        let client = client.clone();
        let source_url = args.source_url.clone();
        requests.spawn(async move { (id, post_event(&client, &source_url, id).await) });
    }

    while let Some(result) = requests.join_next().await {
        if let Ok((id, true)) = result {
            assert_reachable!(
                "the disk-buffer source accepts traffic while faults are active",
                &json!({ "id": id })
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::INITIAL_PRESSURE_REQUESTS;
    use antithesis_harness::{is_pressure_payload, payload_length};

    #[test]
    fn initial_pressure_burst_exceeds_effective_buffer_capacity() {
        let mut ids = (0u64..).filter(|id| is_pressure_payload(*id));
        let encoded_payload_bytes: usize = ids
            .by_ref()
            .take(INITIAL_PRESSURE_REQUESTS)
            .map(|id| payload_length(id) * 2)
            .sum();

        // The 8MiB public limit reserves one 2MiB segment internally, leaving
        // 6MiB of writable capacity. JSON framing and record metadata only add
        // to this lower bound.
        assert!(encoded_payload_bytes > 6 * 1024 * 1024);
    }
}
