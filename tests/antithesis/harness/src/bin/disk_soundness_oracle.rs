//! Integrity oracle for the disk-buffer soundness scenario.
//!
//! Issuance is not a delivery obligation: the source does not have an
//! fsync-backed acknowledgement boundary, and loss during injected crashes is
//! permitted. The oracle therefore makes universal assertions only about
//! records that arrive: every delivered id was issued and its payload is exact.

#![allow(clippy::disallowed_types)] // antithesis assert macros expand to once_cell::Lazy

#[cfg(target_os = "linux")]
extern crate antithesis_instrumentation;

use std::{
    collections::HashSet,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use antithesis_harness::{decode_payload_field, payload_for};
use antithesis_sdk::{antithesis_init, assert_always, assert_reachable, lifecycle};
use axum::{
    extract::{DefaultBodyLimit, RawQuery, State},
    http::StatusCode,
    routing::{get, post},
    Router,
};
use clap::Parser;
use serde_json::{json, Value};
use tokio::time;

#[derive(Parser)]
struct Args {
    #[arg(
        long,
        env = "VECTOR_METRICS_URL",
        default_value = "http://vector:9598/metrics"
    )]
    metrics_url: String,
    #[arg(long, env = "ORACLE_ADDR", default_value = "0.0.0.0:8686")]
    addr: SocketAddr,
    #[arg(
        long,
        env = "SCENARIO_NAME",
        default_value = "vector_disk_buffer_soundness"
    )]
    scenario: String,
}

#[derive(Default)]
struct Sets {
    issued: HashSet<u64>,
    delivered: HashSet<u64>,
    delivered_total: u64,
    corrupted: u64,
    spurious: u64,
}

struct AppState {
    next_id: AtomicU64,
    first_delivery: AtomicBool,
    sets: Mutex<Sets>,
}

struct Delivered {
    id: u64,
    data: Option<String>,
}

fn collect_records(value: &Value, records: &mut Vec<Delivered>) {
    match value {
        Value::Array(values) => values
            .iter()
            .for_each(|value| collect_records(value, records)),
        Value::Object(_) => {
            if let Some(id) = value.get("id").and_then(Value::as_u64) {
                let data = value.get("data").and_then(Value::as_str).map(str::to_owned);
                records.push(Delivered { id, data });
            }
        }
        _ => {}
    }
}

fn parse_delivered(body: &str) -> (Vec<Delivered>, bool) {
    let mut records = Vec::new();
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        collect_records(&value, &mut records);
        return (records, true);
    }

    let mut understood = false;
    for line in body.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            understood = true;
            collect_records(&value, &mut records);
        }
    }
    (records, understood)
}

async fn claim(State(state): State<Arc<AppState>>) -> String {
    let id = state.next_id.fetch_add(1, Ordering::SeqCst);
    state.sets.lock().unwrap().issued.insert(id);
    id.to_string()
}

async fn ingest(State(state): State<Arc<AppState>>, body: String) -> StatusCode {
    let (records, understood) = parse_delivered(&body);
    if !understood {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    {
        let mut sets = state.sets.lock().unwrap();
        for Delivered { id, data } in records {
            let was_issued = sets.issued.contains(&id);
            assert_always!(
                was_issued,
                "every disk-buffer delivery has an id issued by the oracle",
                &json!({ "id": id, "issued_count": sets.issued.len() })
            );
            if !was_issued {
                sets.spurious += 1;
            }

            let payload_matches =
                data.as_deref().and_then(decode_payload_field) == Some(payload_for(id));
            assert_always!(
                payload_matches,
                "every disk-buffer delivery preserves the issued payload exactly",
                &json!({ "id": id })
            );
            if !payload_matches {
                sets.corrupted += 1;
            }

            sets.delivered.insert(id);
            sets.delivered_total += 1;
        }
    }

    if state.first_delivery.swap(false, Ordering::SeqCst) {
        assert_reachable!("a record traversed the disk buffer and reached the oracle");
    }
    StatusCode::OK
}

async fn report(State(state): State<Arc<AppState>>) -> String {
    let sets = state.sets.lock().unwrap();
    json!({
        "issued": sets.issued.len(),
        "delivered": sets.delivered.len(),
        "delivered_total": sets.delivered_total,
        "duplicate_count": sets.delivered_total.saturating_sub(sets.delivered.len() as u64),
        "corrupted_count": sets.corrupted,
        "spurious_count": sets.spurious,
    })
    .to_string()
}

async fn delivered(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> String {
    let id = query
        .as_deref()
        .and_then(|query| query.rsplit("id=").next())
        .and_then(|id| id.parse::<u64>().ok());
    let was_delivered = id.is_some_and(|id| state.sets.lock().unwrap().delivered.contains(&id));
    if was_delivered { "1" } else { "0" }.to_string()
}

async fn wait_for_vector(metrics_url: &str, timeout: time::Duration) -> bool {
    let client = reqwest::Client::new();
    let deadline = time::Instant::now() + timeout;
    while time::Instant::now() < deadline {
        if matches!(
            client
                .get(metrics_url)
                .timeout(time::Duration::from_secs(2))
                .send()
                .await,
            Ok(response) if response.status().is_success()
        ) {
            return true;
        }
        time::sleep(time::Duration::from_millis(500)).await;
    }
    false
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    antithesis_init();
    let args = Args::parse();
    let vector_ready = wait_for_vector(&args.metrics_url, time::Duration::from_secs(180)).await;

    let state = Arc::new(AppState {
        next_id: AtomicU64::new(0),
        first_delivery: AtomicBool::new(true),
        sets: Mutex::new(Sets::default()),
    });
    let app = Router::new()
        .route("/claim", post(claim))
        .route("/ingest", post(ingest))
        .route("/report", get(report))
        .route("/delivered", get(delivered))
        .layer(DefaultBodyLimit::disable())
        .with_state(state);

    let server = axum::Server::bind(&args.addr).serve(app.into_make_service());
    lifecycle::setup_complete(&json!({ "component": args.scenario }));
    assert_always!(
        vector_ready,
        "Vector becomes healthy before the disk soundness workload starts",
        &json!({ "metrics_url": args.metrics_url })
    );
    assert_reachable!("disk-buffer soundness oracle started");
    server.await.expect("oracle server failed");
}
