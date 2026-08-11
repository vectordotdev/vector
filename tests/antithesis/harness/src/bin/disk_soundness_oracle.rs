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
    ingest_blocked: AtomicBool,
    first_blocked_attempt: AtomicBool,
    blocked_attempts: AtomicU64,
    sets: Mutex<Sets>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            next_id: AtomicU64::new(0),
            first_delivery: AtomicBool::new(true),
            ingest_blocked: AtomicBool::new(false),
            first_blocked_attempt: AtomicBool::new(true),
            blocked_attempts: AtomicU64::new(0),
            sets: Mutex::new(Sets::default()),
        }
    }
}

struct Delivered {
    id: u64,
    data: Option<String>,
}

fn collect_records(value: &Value, records: &mut Vec<Delivered>) -> bool {
    match value {
        Value::Array(values) => {
            !values.is_empty() && values.iter().all(|value| collect_records(value, records))
        }
        Value::Object(_) => {
            let Some(id) = value.get("id").and_then(Value::as_u64) else {
                return false;
            };
            let data = value.get("data").and_then(Value::as_str).map(str::to_owned);
            records.push(Delivered { id, data });
            true
        }
        _ => false,
    }
}

fn parse_delivered(body: &str) -> (Vec<Delivered>, bool) {
    let mut records = Vec::new();
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        return if collect_records(&value, &mut records) {
            (records, true)
        } else {
            (Vec::new(), false)
        };
    }

    let mut understood = false;
    for line in body.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return (Vec::new(), false);
        };
        if !collect_records(&value, &mut records) {
            return (Vec::new(), false);
        }
        understood = true;
    }
    (records, understood)
}

async fn claim(State(state): State<Arc<AppState>>) -> String {
    let id = state.next_id.fetch_add(1, Ordering::SeqCst);
    state.sets.lock().unwrap().issued.insert(id);
    id.to_string()
}

async fn ingest(State(state): State<Arc<AppState>>, body: String) -> StatusCode {
    if state.ingest_blocked.load(Ordering::SeqCst) {
        let blocked_attempts = state.blocked_attempts.fetch_add(1, Ordering::SeqCst) + 1;
        if state.first_blocked_attempt.swap(false, Ordering::SeqCst) {
            assert_reachable!(
                "the HTTP sink retries while the pressure gate is closed",
                &json!({ "blocked_attempts": blocked_attempts })
            );
        }
        return StatusCode::SERVICE_UNAVAILABLE;
    }

    let (records, understood) = parse_delivered(&body);
    assert_always!(
        understood,
        "every disk-buffer delivery is valid JSON with a numeric id",
        &json!({ "body_bytes": body.len() })
    );
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

async fn close_ingest_gate(State(state): State<Arc<AppState>>) -> &'static str {
    if state.ingest_blocked.swap(true, Ordering::SeqCst) {
        "0"
    } else {
        "1"
    }
}

async fn open_ingest_gate(State(state): State<Arc<AppState>>) -> StatusCode {
    state.ingest_blocked.store(false, Ordering::SeqCst);
    StatusCode::NO_CONTENT
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
        "ingest_blocked": state.ingest_blocked.load(Ordering::SeqCst),
        "blocked_attempts": state.blocked_attempts.load(Ordering::SeqCst),
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

    let state = Arc::new(AppState::default());
    let app = Router::new()
        .route("/claim", post(claim))
        .route("/ingest", post(ingest))
        .route("/ingest/close", post(close_ingest_gate))
        .route("/ingest/open", post(open_ingest_gate))
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

#[cfg(test)]
mod tests {
    use super::{close_ingest_gate, ingest, open_ingest_gate, parse_delivered, AppState};
    use axum::{extract::State, http::StatusCode};
    use std::sync::{atomic::Ordering, Arc};

    #[tokio::test]
    async fn ingest_gate_rejects_retriably_until_reopened() {
        let state = Arc::new(AppState::default());
        state.sets.lock().unwrap().issued.insert(0);

        assert_eq!(close_ingest_gate(State(Arc::clone(&state))).await, "1");
        assert_eq!(close_ingest_gate(State(Arc::clone(&state))).await, "0");
        assert_eq!(
            ingest(
                State(Arc::clone(&state)),
                r#"[{"id":0,"data":""}]"#.to_owned()
            )
            .await,
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(state.blocked_attempts.load(Ordering::SeqCst), 1);
        assert!(state.sets.lock().unwrap().delivered.is_empty());

        assert_eq!(
            open_ingest_gate(State(Arc::clone(&state))).await,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            ingest(
                State(Arc::clone(&state)),
                r#"[{"id":0,"data":""}]"#.to_owned()
            )
            .await,
            StatusCode::OK
        );
        assert!(state.sets.lock().unwrap().delivered.contains(&0));
    }

    #[test]
    fn delivered_parser_accepts_supported_record_shapes() {
        for body in [
            r#"{"id":1,"data":"aa"}"#,
            r#"[{"id":1,"data":"aa"},{"id":2,"data":"bb"}]"#,
            "{\"id\":1,\"data\":\"aa\"}\n{\"id\":2,\"data\":\"bb\"}",
        ] {
            let (records, understood) = parse_delivered(body);
            assert!(understood, "body should parse: {body}");
            assert!(!records.is_empty());
        }
    }

    #[test]
    fn delivered_parser_rejects_every_malformed_record() {
        for body in [
            "",
            "[]",
            "{}",
            r#"{"id":"1","data":"aa"}"#,
            r#"[{"id":1,"data":"aa"},{}]"#,
            "{\"id\":1,\"data\":\"aa\"}\nnot-json",
        ] {
            let (records, understood) = parse_delivered(body);
            assert!(!understood, "body should be rejected: {body}");
            assert!(records.is_empty());
        }
    }
}
