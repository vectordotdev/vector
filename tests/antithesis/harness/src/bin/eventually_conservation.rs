//! Asserts two properties:
//!
//! * **conservation** every id the oracle acked has come back. Peer to the
//!   integrity check performed online by the oracle.
//!
//! * **liveness** the vector cluster still accepts and forwards a fresh event.

#![allow(clippy::disallowed_types)] // antithesis assert macros expand to once_cell::Lazy

#[cfg(target_os = "linux")]
extern crate antithesis_instrumentation;

use antithesis_harness::{all_endpoints_healthy, OracleClient, VectorClient};
use antithesis_sdk::{
    antithesis_init, assert_always, assert_always_less_than_or_equal_to,
    assert_sometimes_greater_than, assert_unreachable,
};
use clap::Parser;
use serde_json::json;
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

#[tokio::main(flavor = "current_thread")]
async fn main() {
    antithesis_init();
    let args = Args::parse();
    let http = reqwest::Client::new();
    let oracle_url = args.oracle_url.clone();
    let oracle = OracleClient::new(http.clone(), args.oracle_url);
    let vector = VectorClient::new(http.clone(), args.source_url);
    let metrics_urls = args.metrics_urls;

    // This is an Antithesis `eventually_` command. When it starts Antithesis stops all
    // fault injection across every container and kills the producers, then nothing new
    // starts. So for the rest of this program the cluster is load-free and fault-free:
    // no partitions, drops, or latency faults to tolerate, only recovery to wait out.
    // That is why the checks below assert unconditionally — a shortfall now is real loss,
    // and a probe that never round-trips is a real wedge, not a transient fault.

    // Faults stop instantly but recovery is not, so wait for every node to serve again.
    let recovery_deadline = time::Instant::now() + time::Duration::from_secs(180);
    while time::Instant::now() < recovery_deadline
        && !all_endpoints_healthy(&http, &metrics_urls, time::Duration::from_secs(3)).await
    {
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
        let Some(r) = oracle.report().await else {
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

    let Some(report) = oracle.report().await else {
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

    // Liveness: a fresh write still round-trips. With faults stopped there is nothing to
    // tolerate, so post one probe and poll it until it lands or the deadline. Claim and
    // post retry until one sticks, since a node can briefly refuse a write while it is
    // still recovering. A wedged node never delivers it and fails here. Runs
    // unconditionally.
    //
    // The recovery gate above only proves the metrics endpoint answers. That is a
    // separate listener from the source's data path, so the source and sink can still
    // be unready while metrics already serve, and a just-restarted node needs time to
    // bring them up. The round-trip is therefore the real readiness signal and gets the
    // same budget as recovery rather than a tight window that expires before the data
    // path is serving.
    let deadline = time::Instant::now() + time::Duration::from_secs(180);
    let mut probe = None;
    let mut progressed = false;
    while !progressed && time::Instant::now() < deadline {
        if probe.is_none() {
            if let Some(id) = oracle.claim().await {
                if vector.post_event(id, time::Duration::from_secs(10)).await {
                    probe = Some(id);
                }
            }
        }
        if let Some(id) = probe {
            progressed = oracle.delivered(id).await;
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
