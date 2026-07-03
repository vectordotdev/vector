//! oracle: the conservation judge. Mints unique ids, records which ids the
//! pipeline acked, and checks they all come back — an acked id that never
//! returns is loss.
//!
//! Endpoints:
//!   POST /claim          -> one fresh id (body is the id)
//!   POST /acked          -> newline-separated ids the pipeline acked (must come back)
//!   POST /ingest         -> the pipeline's egress sink delivers the round trip here
//!   GET  /report         -> JSON: issued/acked/delivered/delivered_total/missing/spurious/corrupted
//!   GET  /delivered?id=X -> "1" if returned, else "0"
//!
//! /ingest fails on arrival if a delivered id was never issued or its payload
//! does not match what was issued for that id.

#![allow(clippy::disallowed_types)] // antithesis assert macros expand to once_cell::Lazy

#[cfg(target_os = "linux")]
extern crate antithesis_instrumentation;

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use antithesis_harness::{decode_payload_field, payload_for};
use antithesis_sdk::{antithesis_init, assert_always, assert_reachable, lifecycle};
use axum::extract::{DefaultBodyLimit, RawQuery, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Router;
use clap::Parser;
use serde_json::{json, Value};
use tokio::time;

#[derive(Parser)]
struct Args {
    #[arg(
        long,
        env = "VECTOR_METRICS_URL",
        default_value = "http://head:9598/metrics"
    )]
    metrics_url: String,
    #[arg(long, env = "ORACLE_ADDR", default_value = "0.0.0.0:8686")]
    addr: SocketAddr,
    /// Names the scenario in the `setup_complete` lifecycle event so a run records
    /// which topology it exercised.
    #[arg(long, env = "SCENARIO_NAME", default_value = "vector_e2e")]
    scenario: String,
}

/// The oracle's three id sets, the raw delivery count (distinct + duplicates),
/// and the count of delivered records whose payload did not match what was issued
/// for their id.
#[derive(Default)]
struct Sets {
    issued: HashSet<u64>,
    acked: HashSet<u64>,
    delivered: HashSet<u64>,
    delivered_total: u64,
    corrupted: u64,
}

struct AppState {
    next_id: AtomicU64,
    first_delivery: AtomicBool,
    sets: Mutex<Sets>,
}

/// One delivered record: its id and the raw `data` field if present. The data
/// field lets the oracle recompute and compare the full payload, not just the id.
struct Delivered {
    id: u64,
    data: Option<String>,
}

/// Pull every record carrying an integer "id" field out of a decoded JSON value,
/// capturing its "data" field alongside.
fn collect_records(v: &Value, out: &mut Vec<Delivered>) {
    match v {
        Value::Array(a) => a.iter().for_each(|e| collect_records(e, out)),
        Value::Object(_) => {
            if let Some(id) = v.get("id").and_then(Value::as_u64) {
                let data = v.get("data").and_then(Value::as_str).map(str::to_owned);
                out.push(Delivered { id, data });
            }
        }
        _ => {}
    }
}

/// Parse delivered records from a JSON array, single object, or NDJSON body.
/// Returns the records and whether the body parsed at all.
fn parse_delivered(body: &str) -> (Vec<Delivered>, bool) {
    let mut records = Vec::new();
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        collect_records(&v, &mut records);
        return (records, true);
    }
    let mut any = false;
    for line in body.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            any = true;
            collect_records(&v, &mut records);
        }
    }
    (records, any)
}

/// One id per claim: the producer owns exactly one logical event per invocation,
/// which bounds the un-recorded-ack seam to a single id.
async fn claim(State(st): State<Arc<AppState>>) -> String {
    let id = st.next_id.fetch_add(1, Ordering::SeqCst);
    st.sets.lock().unwrap().issued.insert(id);
    id.to_string()
}

async fn acked(State(st): State<Arc<AppState>>, body: String) -> StatusCode {
    let mut sets = st.sets.lock().unwrap();
    for id in body.lines().filter_map(|l| l.trim().parse::<u64>().ok()) {
        sets.acked.insert(id);
    }
    StatusCode::OK
}

async fn ingest(State(st): State<Arc<AppState>>, body: String) -> StatusCode {
    let (records, understood) = parse_delivered(&body);
    // 200 only if understood, so the sink never counts an unparsable body as
    // delivered — keeps the delivered set honest.
    if !understood {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    {
        let mut sets = st.sets.lock().unwrap();
        for Delivered { id, data } in records {
            let was_issued = sets.issued.contains(&id);
            let issued_total = sets.issued.len();
            assert_always!(
                was_issued,
                "every delivered id was actually issued (no invented or corrupted ids)",
                &json!({ "id": id, "issued_total": issued_total })
            );

            // The payload is a deterministic function of the id, so recompute the
            // expected bytes and compare. A round-trip that preserves the id but
            // mangles the content is a data-integrity failure the id check misses.
            let payload_matches =
                data.as_deref().and_then(decode_payload_field) == Some(payload_for(id));
            assert_always!(
                payload_matches,
                "every delivered record's payload matches what was issued for its id",
                &json!({ "id": id })
            );
            if !payload_matches {
                sets.corrupted += 1;
            }

            sets.delivered.insert(id);
            sets.delivered_total += 1;
        }
    }
    if st.first_delivery.swap(false, Ordering::SeqCst) {
        assert_reachable!("event delivered end-to-end");
    }
    StatusCode::OK
}

async fn report(State(st): State<Arc<AppState>>) -> String {
    let sets = st.sets.lock().unwrap();
    let missing: Vec<u64> = sets
        .acked
        .difference(&sets.delivered)
        .copied()
        .take(20)
        .collect();
    json!({
        "issued": sets.issued.len(),
        "acked": sets.acked.len(),
        "delivered": sets.delivered.len(),
        "delivered_total": sets.delivered_total,
        "missing_count": sets.acked.difference(&sets.delivered).count(),
        "missing_sample": missing,
        "spurious_count": sets.delivered.difference(&sets.issued).count(),
        "corrupted_count": sets.corrupted,
    })
    .to_string()
}

async fn delivered(State(st): State<Arc<AppState>>, RawQuery(q): RawQuery) -> String {
    let id: Option<u64> = q
        .as_deref()
        .and_then(|q| q.rsplit("id=").next())
        .and_then(|s| s.parse().ok());
    let hit = id.is_some_and(|i| st.sets.lock().unwrap().delivered.contains(&i));
    if hit { "1" } else { "0" }.to_string()
}

/// Poll the SUT's metrics endpoint until it answers or the timeout elapses.
async fn wait_for_vector(metrics_url: &str, timeout: time::Duration) {
    let client = reqwest::Client::new();
    let deadline = time::Instant::now() + timeout;
    while time::Instant::now() < deadline {
        if let Ok(resp) = client
            .get(metrics_url)
            .timeout(time::Duration::from_secs(2))
            .send()
            .await
        {
            if resp.status().is_success() {
                return;
            }
        }
        time::sleep(time::Duration::from_millis(500)).await;
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    antithesis_init();
    let args = Args::parse();

    let state = Arc::new(AppState {
        next_id: AtomicU64::new(0),
        first_delivery: AtomicBool::new(true),
        sets: Mutex::new(Sets::default()),
    });

    let app = Router::new()
        .route("/claim", post(claim))
        .route("/acked", post(acked))
        .route("/ingest", post(ingest))
        .route("/report", get(report))
        .route("/delivered", get(delivered))
        .layer(DefaultBodyLimit::disable())
        .with_state(state);

    wait_for_vector(&args.metrics_url, time::Duration::from_secs(180)).await;

    let server = axum::Server::bind(&args.addr).serve(app.into_make_service());
    lifecycle::setup_complete(&json!({ "component": args.scenario }));
    assert_reachable!("oracle started");
    server.await.expect("oracle server failed");
}
