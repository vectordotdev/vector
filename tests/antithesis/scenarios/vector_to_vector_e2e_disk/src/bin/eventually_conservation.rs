//! Asserts two properties:
//!
//! * **conservation** every id the oracle acked has come back. Peer to the
//!   integrity check performed online by the oracle.
//!
//! * **liveness** the vector cluster still accepts and forwards a fresh event.

#![allow(clippy::disallowed_types)] // antithesis assert macros expand to once_cell::Lazy

extern crate antithesis_instrumentation;

use antithesis_sdk::{
    antithesis_init, assert_always, assert_always_less_than_or_equal_to,
    assert_sometimes_greater_than, assert_unreachable,
};
use clap::Parser;
use harness::payload_field;
use serde_json::{json, Value};
use tokio::time;

#[derive(Parser)]
struct Args {
    #[arg(long, env = "VECTOR_SOURCE_URL", default_value = "http://head:8080/")]
    source_url: String,
    #[arg(long, env = "ORACLE_URL", default_value = "http://127.0.0.1:8686")]
    oracle_url: String,
    #[arg(
        long,
        env = "VECTOR_METRICS_URLS",
        value_delimiter = ',',
        default_value = "http://head:9598/metrics,http://tail:9598/metrics"
    )]
    metrics_urls: Vec<String>,
}

/// The oracle's verdict.
struct Report {
    acked: u64,
    delivered: u64,
    delivered_total: u64,
    missing_count: u64,
    missing_sample: Vec<u64>,
    spurious_count: u64,
    corrupted_count: u64,
}

async fn fetch_report(client: &reqwest::Client, oracle_url: &str) -> Option<Report> {
    let body = client
        .get(format!("{oracle_url}/report"))
        .timeout(time::Duration::from_secs(5))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    let v: Value = serde_json::from_str(&body).ok()?;
    Some(Report {
        acked: v["acked"].as_u64()?,
        delivered: v["delivered"].as_u64()?,
        delivered_total: v["delivered_total"].as_u64()?,
        missing_count: v["missing_count"].as_u64()?,
        missing_sample: v["missing_sample"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default(),
        spurious_count: v["spurious_count"].as_u64()?,
        corrupted_count: v["corrupted_count"].as_u64()?,
    })
}

async fn delivered_contains(client: &reqwest::Client, oracle_url: &str, id: u64) -> bool {
    let Ok(resp) = client
        .get(format!("{oracle_url}/delivered?id={id}"))
        .timeout(time::Duration::from_secs(5))
        .send()
        .await
    else {
        return false;
    };
    resp.text().await.map(|s| s.trim() == "1").unwrap_or(false)
}

async fn node_healthy(client: &reqwest::Client, metrics_url: &str) -> bool {
    matches!(
        client.get(metrics_url).timeout(time::Duration::from_secs(3)).send().await,
        Ok(resp) if resp.status().is_success()
    )
}

async fn all_healthy(client: &reqwest::Client, metrics_urls: &[String]) -> bool {
    for u in metrics_urls {
        if !node_healthy(client, u).await {
            return false;
        }
    }
    true
}

async fn claim(client: &reqwest::Client, oracle_url: &str) -> Option<u64> {
    let resp = client
        .post(format!("{oracle_url}/claim"))
        .timeout(time::Duration::from_secs(10))
        .send()
        .await
        .ok()?;
    resp.text().await.ok()?.trim().parse().ok()
}

async fn post_probe(client: &reqwest::Client, source_url: &str, id: u64) -> bool {
    // Same deterministic payload as the producer, so the probe's delivery passes
    // the oracle's content check instead of counting as corruption.
    let event = json!([{ "id": id, "data": payload_field(id) }]);
    matches!(
        client.post(source_url).timeout(time::Duration::from_secs(10)).json(&event).send().await,
        Ok(resp) if resp.status().is_success()
    )
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    antithesis_init();
    let args = Args::parse();
    let client = reqwest::Client::new();

    let source_url = args.source_url;
    let oracle_url = args.oracle_url;
    let metrics_urls = args.metrics_urls;

    // Antithesis kills the producers and stops all fault injection the moment this
    // `eventually_` command starts, so the cluster is now load-free and fault-free.
    // That lets us judge directly instead of guessing when the system is quiet:
    // recover, drain, then assert unconditionally. Nothing is in flight once delivery
    // has plateaued, so any shortfall is real loss, never lag.

    // Faults just stopped; give the nodes a moment to start serving again.
    let recovery_deadline = time::Instant::now() + time::Duration::from_secs(120);
    while time::Instant::now() < recovery_deadline && !all_healthy(&client, &metrics_urls).await {
        time::sleep(time::Duration::from_secs(3)).await;
    }

    // Drain: wait until every acked id has come back, or until delivery stops
    // advancing for several polls. With no load and no faults a healthy buffer
    // flushes its backlog quickly; one that is still short here is wedged or lossy.
    let drain_deadline = time::Instant::now() + time::Duration::from_secs(120);
    let mut last_delivered = u64::MAX;
    let mut plateau = 0u32;
    while time::Instant::now() < drain_deadline {
        time::sleep(time::Duration::from_secs(3)).await;
        let Some(r) = fetch_report(&client, &oracle_url).await else {
            continue;
        };
        if r.missing_count == 0 {
            break;
        }
        if r.delivered == last_delivered {
            plateau += 1;
            if plateau >= 5 {
                break;
            }
        } else {
            plateau = 0;
        }
        last_delivered = r.delivered;
    }

    let Some(report) = fetch_report(&client, &oracle_url).await else {
        // On a healthy run the oracle is up. Reaching this arm is itself the failure.
        assert_unreachable!(
            "oracle unreachable while building the conservation report",
            &json!({ "oracle_url": oracle_url })
        );
        return;
    };

    // Load and faults are stopped and the buffer has settled, so every acked id that
    // has not come back is permanently lost. No quiescence gate: the check always runs.
    assert_always_less_than_or_equal_to!(
        report.missing_count,
        0,
        "every end-to-end-acked event survives faults and reaches the oracle",
        &json!({ "acked": report.acked, "delivered": report.delivered,
            "delivered_total": report.delivered_total,
            "missing_count": report.missing_count, "missing_sample": report.missing_sample })
    );
    if report.missing_count > 0 {
        assert_unreachable!(
            "an end-to-end-acked event was permanently lost",
            &json!({ "acked": report.acked, "delivered": report.delivered,
                "delivered_total": report.delivered_total,
                "missing_count": report.missing_count, "missing_sample": report.missing_sample })
        );
    }
    assert_always_less_than_or_equal_to!(
        report.spurious_count,
        0,
        "every delivered id was actually issued (no invented or corrupted ids)",
        &json!({ "spurious_count": report.spurious_count,
            "acked": report.acked, "delivered": report.delivered })
    );
    assert_always_less_than_or_equal_to!(
        report.corrupted_count,
        0,
        "every delivered record's payload matches what was issued for its id",
        &json!({ "corrupted_count": report.corrupted_count,
            "acked": report.acked, "delivered": report.delivered })
    );

    assert_sometimes_greater_than!(
        report.acked,
        100,
        "a large set of events was acked and conserved",
        &json!({ "acked": report.acked, "delivered": report.delivered })
    );
    assert_sometimes_greater_than!(
        report.delivered_total,
        report.delivered,
        "a duplicate delivery was observed (the at-least-once replay path ran)",
        &json!({ "delivered": report.delivered, "delivered_total": report.delivered_total })
    );

    // New id each attempt, pass if any round-trips. A permanent wedge fails them all.
    if all_healthy(&client, &metrics_urls).await {
        let deadline = time::Instant::now() + time::Duration::from_secs(45);
        let mut progressed = false;
        while !progressed && time::Instant::now() < deadline {
            if let Some(probe) = claim(&client, &oracle_url).await {
                if post_probe(&client, &source_url, probe).await {
                    time::sleep(time::Duration::from_secs(1)).await;
                    progressed = delivered_contains(&client, &oracle_url, probe).await;
                }
            }
            if !progressed {
                time::sleep(time::Duration::from_secs(2)).await;
            }
        }
        assert_always!(
            progressed,
            "post-recovery write makes progress",
            &json!({ "acked": report.acked, "delivered": report.delivered })
        );
    }
}
