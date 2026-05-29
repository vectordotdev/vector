//! collector: the loadgen-collector container entrypoint and the experiment's
//! single oracle authority. It mints unique event ids from one counter, records
//! which ids node0 acked (Vector's durability promise, relayed by the producer),
//! and watches them come back when node1 delivers them. One counter issues every
//! id, so ids are unique BY CONSTRUCTION across every parallel and restarted
//! producer — no clock, no RNG.
//!
//! It is the test apparatus, not the SUT, and must survive the faults it judges:
//! Antithesis is told never to terminate it, and it also journals its three sets
//! to a persistent volume and reloads them on startup, so a restart cannot wipe
//! the oracle's memory (which would be both a false miss and a false red).
//!
//! Endpoints:
//!   POST /claim       -> one fresh id (the body is the id)
//!   POST /acked       -> newline-separated ids node0 acked (must come back)
//!   POST /ingest      -> node1's http sink delivers the round trip here
//!   GET  /report      -> JSON: issued/acked/delivered/delivered_total/missing/spurious/duplicate
//!   GET  /delivered?id=X -> "1" if X came back, else "0"
//!
//! Integrity is checked CONTINUOUSLY here, not at quiescence: any delivered id
//! that was never issued is a fabricated/corrupted id and fails immediately.

extern crate antithesis_instrumentation;

use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use antithesis_sdk::{antithesis_init, assert_always, assert_reachable, lifecycle};
use serde_json::{json, Value};

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// A set of ids backed by an append-only log on the persistent volume. `total`
/// counts every append (so `delivered` can record duplicates), while `set` holds
/// the distinct ids. On open the existing log is replayed into both, so a restart
/// resumes exactly where it left off.
struct Journal {
    set: HashSet<u64>,
    total: u64,
    file: File,
}

impl Journal {
    fn open(path: &Path) -> Journal {
        let mut set = HashSet::new();
        let mut total = 0u64;
        if let Ok(existing) = File::open(path) {
            for line in BufReader::new(existing).lines().map_while(Result::ok) {
                if let Ok(id) = line.trim().parse::<u64>() {
                    set.insert(id);
                    total += 1;
                }
            }
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("failed to open journal");
        Journal { set, total, file }
    }

    /// Record `id`. When `dedup` (issued/acked), append only the first time the id
    /// is seen. When not (delivered), append every time so `total` is the raw
    /// delivery count. Returns whether the id was newly distinct.
    fn record(&mut self, id: u64, dedup: bool) -> bool {
        let is_new = self.set.insert(id);
        if is_new || !dedup {
            let _ = writeln!(self.file, "{id}");
            let _ = self.file.flush();
            self.total += 1;
        }
        is_new
    }
}

struct State {
    next_id: AtomicU64,
    issued: Mutex<Journal>,
    acked: Mutex<Journal>,
    delivered: Mutex<Journal>,
}

impl State {
    fn load(dir: &Path) -> State {
        let _ = std::fs::create_dir_all(dir);
        let issued = Journal::open(&dir.join("issued.log"));
        let acked = Journal::open(&dir.join("acked.log"));
        let delivered = Journal::open(&dir.join("delivered.log"));
        // Resume the counter above every id ever issued so post-restart claims
        // never re-mint a collided id.
        let next = issued.set.iter().copied().max().map_or(0, |m| m + 1);
        State {
            next_id: AtomicU64::new(next),
            issued: Mutex::new(issued),
            acked: Mutex::new(acked),
            delivered: Mutex::new(delivered),
        }
    }
}

/// Pull every integer "id" field out of a decoded JSON value.
fn collect_ids(v: &Value, out: &mut Vec<u64>) {
    match v {
        Value::Array(a) => a.iter().for_each(|e| collect_ids(e, out)),
        Value::Object(_) => {
            if let Some(id) = v.get("id").and_then(Value::as_u64) {
                out.push(id);
            }
        }
        _ => {}
    }
}

/// Parse delivered ids from a JSON array, single object, or NDJSON body. Returns
/// the ids and whether the body parsed at all.
fn parse_delivered(body: &str) -> (Vec<u64>, bool) {
    let mut ids = Vec::new();
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        collect_ids(&v, &mut ids);
        return (ids, true);
    }
    let mut any = false;
    for line in body.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            any = true;
            collect_ids(&v, &mut ids);
        }
    }
    (ids, any)
}

fn read_body(req: &mut tiny_http::Request) -> String {
    let mut s = String::new();
    let _ = req.as_reader().read_to_string(&mut s);
    s
}

fn wait_for_vector(metrics_url: &str, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(resp) = ureq::get(metrics_url).timeout(Duration::from_secs(2)).call() {
            if resp.status() == 200 {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    false
}

fn main() {
    antithesis_init();
    let metrics_url = env_or("VECTOR_METRICS_URL", "http://node0:9598/metrics");
    let addr = env_or("COLLECTOR_ADDR", "0.0.0.0:8686");
    let state_dir = env_or("COLLECTOR_STATE_DIR", "/var/lib/collector");

    let state = State::load(Path::new(&state_dir));
    let server = tiny_http::Server::http(addr.as_str()).expect("failed to bind collector");

    eprintln!("[collector] waiting for vector at {metrics_url}");
    if !wait_for_vector(&metrics_url, Duration::from_secs(180)) {
        eprintln!("[collector] WARNING: vector not ready within timeout");
    }
    lifecycle::setup_complete(&json!({ "component": "vector_to_vector_e2e_disk" }));
    assert_reachable!("collector started");

    let mut first_delivery = true;
    for mut req in server.incoming_requests() {
        let method = req.method().as_str().to_string();
        let url = req.url().to_string();
        let path = url.split('?').next().unwrap_or("");
        match (method.as_str(), path) {
            ("POST", "/claim") => {
                // One id per claim: the producer owns exactly one logical event per
                // invocation, which bounds the un-recorded-ack seam to a single id.
                let id = state.next_id.fetch_add(1, Ordering::SeqCst);
                state.issued.lock().unwrap().record(id, true);
                let _ = req.respond(tiny_http::Response::from_string(id.to_string()));
            }
            ("POST", "/acked") => {
                let body = read_body(&mut req);
                let mut acked = state.acked.lock().unwrap();
                for id in body.lines().filter_map(|l| l.trim().parse::<u64>().ok()) {
                    acked.record(id, true);
                }
                let _ = req.respond(tiny_http::Response::empty(200));
            }
            ("POST", "/ingest") => {
                let body = read_body(&mut req);
                let (ids, understood) = parse_delivered(&body);
                if understood {
                    let issued = state.issued.lock().unwrap();
                    let mut delivered = state.delivered.lock().unwrap();
                    for id in ids {
                        // Continuous integrity: a delivered id that was never issued
                        // is a fabricated or corrupted id. Fail the instant it lands,
                        // not at quiescence.
                        assert_always!(
                            issued.set.contains(&id),
                            "every delivered id was actually issued (no invented or corrupted ids)",
                            &json!({ "id": id })
                        );
                        delivered.record(id, false);
                    }
                    if first_delivery {
                        first_delivery = false;
                        assert_reachable!("event delivered end-to-end through disk buffer");
                    }
                }
                // 200 only if understood, so the sink never counts an unparseable
                // body as delivered — keeps the delivered set honest.
                let _ = req.respond(tiny_http::Response::empty(if understood { 200 } else { 500 }));
            }
            ("GET", "/report") => {
                let issued = state.issued.lock().unwrap();
                let acked = state.acked.lock().unwrap();
                let delivered = state.delivered.lock().unwrap();
                let missing: Vec<u64> =
                    acked.set.difference(&delivered.set).copied().take(20).collect();
                let report = json!({
                    "issued": issued.set.len(),
                    "acked": acked.set.len(),
                    "delivered": delivered.set.len(),
                    "delivered_total": delivered.total,
                    "missing_count": acked.set.difference(&delivered.set).count(),
                    "missing_sample": missing,
                    "spurious_count": delivered.set.difference(&issued.set).count(),
                });
                let _ = req.respond(tiny_http::Response::from_string(report.to_string()));
            }
            ("GET", "/delivered") => {
                let id: Option<u64> = url.rsplit("id=").next().and_then(|s| s.parse().ok());
                let hit = id.is_some_and(|i| state.delivered.lock().unwrap().set.contains(&i));
                let _ = req.respond(tiny_http::Response::from_string(if hit { "1" } else { "0" }));
            }
            _ => {
                let _ = req.respond(tiny_http::Response::empty(404));
            }
        }
    }
}
