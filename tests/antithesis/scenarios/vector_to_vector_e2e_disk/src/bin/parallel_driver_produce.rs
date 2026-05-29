//! Drive one logical event into head under fault injection.
//!
//! On a 2xx we relay an ack-back so the oracle expects the id to come back
//! out. If we give up before any 2xx, the id was never acked, so it is no
//! obligation and no false loss.

#![allow(clippy::disallowed_types)] // antithesis assert macros expand to once_cell::Lazy

extern crate antithesis_instrumentation;

use antithesis_sdk::{antithesis_init, assert_reachable};
use clap::Parser;
use harness::payload_field;
use serde_json::json;
use tokio::time;

const MAX_ATTEMPTS: u32 = 5;

#[derive(Parser)]
struct Args {
    #[arg(long, env = "VECTOR_SOURCE_URL", default_value = "http://head:8080/")]
    source_url: String,
    #[arg(long, env = "ORACLE_URL", default_value = "http://127.0.0.1:8686")]
    oracle_url: String,
}

/// POST one event to head. Ok(2xx) means head took durable responsibility for the
/// event (with e2e acks enabled).
async fn post_event(
    client: &reqwest::Client,
    source_url: &str,
    id: u64,
    timeout: time::Duration,
) -> bool {
    // The payload is a deterministic function of the id, so every retry re-sends
    // the exact same record and the oracle can recompute the expected bytes.
    let event = json!([{ "id": id, "data": payload_field(id) }]);
    matches!(
        client.post(source_url).timeout(timeout).json(&event).send().await,
        Ok(resp) if resp.status().is_success()
    )
}

/// Claim one fresh id from the oracle.
async fn claim(client: &reqwest::Client, oracle_url: &str) -> Option<u64> {
    let resp = client
        .post(format!("{oracle_url}/claim"))
        .timeout(time::Duration::from_secs(10))
        .send()
        .await
        .ok()?;
    resp.text().await.ok()?.trim().parse().ok()
}

/// Tell the oracle head acked this id, so it must come back.
async fn report_acked(client: &reqwest::Client, oracle_url: &str, id: u64) {
    let _ = client
        .post(format!("{oracle_url}/acked"))
        .timeout(time::Duration::from_secs(10))
        .body(id.to_string())
        .send()
        .await;
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    antithesis_init();
    let args = Args::parse();
    let client = reqwest::Client::new();

    let Some(id) = claim(&client, &args.oracle_url).await else {
        return; // oracle unreachable; nothing to do this invocation
    };
    for _ in 0..MAX_ATTEMPTS {
        // Tight timeout. A head wedged by the underflow blocks forever, so we stop
        // waiting and retry the same id.
        if post_event(&client, &args.source_url, id, time::Duration::from_secs(5)).await {
            report_acked(&client, &args.oracle_url, id).await;
            assert_reachable!("produce driver got an end-to-end ack", &json!({ "id": id }));
            return;
        }
        time::sleep(time::Duration::from_millis(100)).await;
    }
    // Gave up before any 2xx: id is claimed but never acked, so it is no obligation.
}
