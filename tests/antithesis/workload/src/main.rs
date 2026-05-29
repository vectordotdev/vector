//! vdbuf-workload: Antithesis workload for Vector disk buffer v2.
//!
//! Modes (selected by argv[1]):
//!   - `serve`   (container entrypoint, long-lived): HTTP collector for the
//!               Vector `http` sink. Records every delivered event id to a
//!               shared log, waits for Vector, emits `setup_complete`, then idles
//!               so Antithesis can run test commands. Emits bootstrap reachables.
//!   - `produce` (parallel_driver): continuously POST uniquely-IDed events into
//!               Vector's `http_server` source (e2e acks). Payload sizes are
//!               drawn from a disk-buffer boundary menu (256KB write-buffer
//!               threshold, large-record bypass, file-rotation filler) to drive
//!               the rotation / partial-write paths where the #21683 underflow
//!               triggers. Records attempted + acked ids. Fault-tolerant.
//!   - `check`   (eventually_): runs with faults paused. Waits for Vector to
//!               recover, then asserts (a) every acked event reached the
//!               collector [durability / at-least-once], and (b) a fresh
//!               post-recovery write is delivered within a bound [no permanent
//!               writer deadlock — the #21683 demonstration].
//!
//! With e2e acknowledgements ON, a 200 from the source means the event was
//! delivered all the way to the collector. So "acked" ⟹ "should be in the
//! collector's delivered set", and the durability oracle needs no fsync-window
//! timestamp logic (flush_interval is not user-configurable in Vector).

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::json;

// Keep the Antithesis coverage-instrumentation runtime shim linked in.
use antithesis_instrumentation as _;

const SHARED_DIR: &str = "/tmp/vdbuf";
const ATTEMPTED_LOG: &str = "/tmp/vdbuf/attempted.log";
const ACKED_LOG: &str = "/tmp/vdbuf/acked.log";
const DELIVERED_LOG: &str = "/tmp/vdbuf/delivered.log";

// Disk-buffer boundary menu for event payload sizes (bytes of the "pad" field).
// DEFAULT_WRITE_BUFFER_SIZE = 256KB (internal-buffer flush threshold + the
// large-record-bypass boundary); 1MB fills 128MB data files quickly to force
// rotation. These hit the rotation / partial-write code paths the deadlock lives in.
const W: usize = 256 * 1024;
const SIZE_MENU: [usize; 6] = [0, 1, W - 1, W, W + 1, 1024 * 1024];

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()
}

fn append_line(path: &str, line: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

fn read_ids(path: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    if let Ok(s) = fs::read_to_string(path) {
        for line in s.lines() {
            let t = line.trim();
            if !t.is_empty() {
                set.insert(t.to_string());
            }
        }
    }
    set
}

/// Record every event id found in a decoded JSON value (array / object).
fn record_value(v: &serde_json::Value, delivered: &AtomicU64) {
    match v {
        serde_json::Value::Array(a) => {
            for e in a {
                record_value(e, delivered);
            }
        }
        serde_json::Value::Object(_) => {
            if let Some(id) = v.get("id").and_then(|x| x.as_str()) {
                append_line(DELIVERED_LOG, id);
                delivered.fetch_add(1, Ordering::Relaxed);
            }
        }
        _ => {}
    }
}

/// Record delivered ids across whatever framing the http sink uses. Returns true
/// iff the body was understood as JSON (array / object / NDJSON). False for an
/// unparseable body, so the caller can refuse to ack it (no false delivery).
fn record_delivered(body: &str, delivered: &AtomicU64) -> bool {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        record_value(&v, delivered);
        return true;
    }
    let mut any = false;
    for line in body.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(t) {
            any = true;
            record_value(&v, delivered);
        }
    }
    any
}

/// Deterministic-ish per-run unique prefix so ids never collide across driver
/// instances / restarts within a timeline.
fn run_prefix() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("p{:x}{:x}", std::process::id(), nanos & 0xffffff)
}

fn wait_for_vector(metrics_url: &str, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(resp) = ureq::get(metrics_url).timeout(Duration::from_secs(2)).call() {
            if resp.status() == 200 {
                return true;
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
    false
}

/// POST one event with id `id` and a pad of `pad` bytes. Returns true on 2xx
/// (which, with e2e acks, means delivered end-to-end).
fn post_event(source_url: &str, id: &str, pad: usize, timeout: Duration) -> bool {
    let event = json!([{ "id": id, "ts_ms": now_ms() as u64, "pad": "x".repeat(pad) }]);
    let body = serde_json::to_string(&event).unwrap();
    matches!(
        ureq::post(source_url)
            .set("content-type", "application/json")
            .timeout(timeout)
            .send_string(&body),
        Ok(r) if (200..300).contains(&r.status())
    )
}

fn main() {
    antithesis_sdk::antithesis_init();
    let _ = fs::create_dir_all(SHARED_DIR);

    let mode = std::env::args().nth(1).unwrap_or_else(|| "serve".into());
    let source_url = env_or("VECTOR_SOURCE_URL", "http://vdbuf-vector:8080/");
    let metrics_url = env_or("VECTOR_METRICS_URL", "http://vdbuf-vector:9598/metrics");
    let collector_addr = env_or("COLLECTOR_ADDR", "0.0.0.0:8686");

    match mode.as_str() {
        "serve" => serve(&metrics_url, &collector_addr),
        "produce" => produce(&source_url),
        "check" => check(&source_url, &metrics_url),
        "metrics_check" => metrics_check(&metrics_url),
        "drop_check" => drop_check(&metrics_url),
        "fill" => fill(&source_url),
        other => {
            eprintln!("unknown mode: {other} (expected serve|produce|check|metrics_check|drop_check|fill)");
            std::process::exit(2);
        }
    }
}

/// Collector + lifecycle. Long-lived (container entrypoint).
fn serve(metrics_url: &str, collector_addr: &str) {
    let delivered = Arc::new(AtomicU64::new(0));
    // Optional artificial collector latency. A throttled collector makes the
    // http sink drain slowly so the 256MB disk buffer actually fills — the
    // precondition for exercising when_full=drop_newest (#24606).
    let delay_ms: u64 = env_or("COLLECTOR_DELAY_MS", "0").parse().unwrap_or(0);
    {
        let delivered = Arc::clone(&delivered);
        let addr = collector_addr.to_string();
        thread::spawn(move || {
            let server =
                tiny_http::Server::http(addr.as_str()).expect("failed to bind collector");
            for mut req in server.incoming_requests() {
                let mut body = String::new();
                let _ = std::io::Read::read_to_string(req.as_reader(), &mut body);
                if delay_ms > 0 {
                    thread::sleep(Duration::from_millis(delay_ms));
                }
                // Robustly record delivered ids across whatever framing the http
                // sink uses (JSON array, single object, or NDJSON). Return 200
                // ONLY if we actually understood the body — otherwise the sink
                // must NOT treat it as delivered (no false acks). This makes
                // "acked ⟹ recorded" airtight, so any residual acked-but-not-
                // delivered gap is a genuine Vector ack-without-delivery.
                let understood = record_delivered(&body, &delivered);
                let status = if understood { 200 } else { 500 };
                let _ = req.respond(tiny_http::Response::empty(status));
            }
        });
    }

    eprintln!("[workload] serve: waiting for vector at {metrics_url}");
    if !wait_for_vector(metrics_url, Duration::from_secs(180)) {
        eprintln!("[workload] WARNING: vector not ready within timeout");
    }
    antithesis_sdk::lifecycle::setup_complete(&json!({ "component": "vdbuf-workload" }));
    antithesis_sdk::assert_reachable!("workload serve started");

    // Idle forever so Antithesis can run test commands; periodically note that
    // delivery is happening end-to-end through the disk buffer.
    let mut seen = false;
    loop {
        if !seen && delivered.load(Ordering::Relaxed) > 0 {
            seen = true;
            antithesis_sdk::assert_reachable!("event delivered end-to-end through disk buffer");
        }
        thread::sleep(Duration::from_secs(2));
    }
}

/// parallel_driver: produce uniquely-IDed events under fault injection.
fn produce(source_url: &str) {
    // Per-timeline shape axis: bias payload sizes and produce duration. Drawn
    // from the SDK random module so it replays deterministically and swarms
    // across timelines.
    let size_bias = (antithesis_sdk::random::get_random() % 3) as usize; // 0=tiny,1=mixed,2=large
    let iters = 200 + (antithesis_sdk::random::get_random() % 400) as u64; // 200..600 events

    let prefix = run_prefix();
    let mut acked_any = false;
    for i in 0..iters {
        // menu axis: draw a payload size from the disk-buffer boundary menu,
        // weighted by the per-timeline size bias.
        let pad = match size_bias {
            0 => *antithesis_sdk::random::random_choice(&SIZE_MENU[0..3]).unwrap_or(&0),
            2 => *antithesis_sdk::random::random_choice(&SIZE_MENU[3..6]).unwrap_or(&W),
            _ => *antithesis_sdk::random::random_choice(&SIZE_MENU).unwrap_or(&0),
        };
        let id = format!("{prefix}-{i}");
        append_line(ATTEMPTED_LOG, &id);
        // Generous timeout: under the #21683 deadlock the source blocks forever
        // on backpressure, so we time out and keep going (fault-tolerant).
        if post_event(source_url, &id, pad, Duration::from_secs(10)) {
            append_line(ACKED_LOG, &id);
            if !acked_any {
                acked_any = true;
                antithesis_sdk::assert_reachable!("produce driver got an end-to-end ack");
            }
        }
        // Small pacing jitter so timelines interleave with faults differently.
        if antithesis_sdk::random::get_random() % 8 == 0 {
            thread::sleep(Duration::from_millis(50));
        }
    }
}

/// Parse the numeric value out of a prometheus exposition line:
/// `name{labels} VALUE TIMESTAMP` -> VALUE as f64.
fn parse_metric_value(line: &str) -> Option<f64> {
    line.split_whitespace().nth(1).and_then(|v| v.parse::<f64>().ok())
}

/// anytime_ invariant: the disk buffer's size gauges must stay within a sane
/// bound. A 256MB buffer cannot hold ~1e18 events/bytes — a gauge that large is
/// the `total_buffer_size` / `get_total_records` u64 underflow surfacing
/// (empty-buffer `0 - 1` => ~1.8e19, or a decrement underflow). Runs alongside
/// the produce driver under fault injection, so it observes gauges right after
/// node-kill restarts (where the drained-buffer underflow fires).
fn metrics_check(metrics_url: &str) {
    // ~60s of continuous sampling, then exit (Antithesis reruns anytime_ cmds).
    // SANE_MAX is cleanly below 2^64 (~1.8e19) yet far above any real value a
    // 256MB buffer could hold.
    const SANE_MAX: f64 = 1e15;
    for _ in 0..30 {
        if let Ok(resp) = ureq::get(metrics_url).timeout(Duration::from_secs(3)).call() {
            if resp.status() == 200 {
                if let Ok(body) = resp.into_string() {
                    for line in body.lines() {
                        let is_disk_gauge = line.contains("buffer_type=\"disk\"")
                            && (line.starts_with("vector_buffer_events{")
                                || line.starts_with("vector_buffer_byte_size{"));
                        if is_disk_gauge {
                            if let Some(v) = parse_metric_value(line) {
                                antithesis_sdk::assert_always!(
                                    v >= 0.0 && v < SANE_MAX,
                                    "disk buffer size gauge stays within a sane bound (no u64 underflow)",
                                    &json!({ "value": v, "line": line })
                                );
                            }
                        }
                    }
                }
            }
        }
        thread::sleep(Duration::from_secs(2));
    }
}

/// Sum the values of all prometheus lines whose metric name is `metric`.
fn sum_metric(body: &str, metric: &str) -> f64 {
    body.lines()
        .filter(|l| l.starts_with(metric))
        .filter_map(parse_metric_value)
        .sum()
}

/// anytime_ invariant for #24606: when `when_full=drop_newest` drops events, the
/// buffer-level `buffer_discarded_events_total` increments, but the
/// component-level `component_discarded_events_total` (what operators monitor
/// for data loss) must reflect it too. The bug: the component counter stays 0,
/// so silent data loss goes undetected. Needs `VDBUF_WHEN_FULL=drop_newest` and
/// a full buffer (network faults stall the sink -> the 256MB buffer fills).
fn drop_check(metrics_url: &str) {
    let mut buf_seen = false;
    for _ in 0..30 {
        if let Ok(resp) = ureq::get(metrics_url).timeout(Duration::from_secs(3)).call() {
            if resp.status() == 200 {
                if let Ok(body) = resp.into_string() {
                    let buf_drop = sum_metric(&body, "vector_buffer_discarded_events_total");
                    let comp_drop = sum_metric(&body, "vector_component_discarded_events_total");
                    if buf_drop > 0.0 {
                        buf_seen = true;
                    }
                    antithesis_sdk::assert_always!(
                        buf_drop == 0.0 || comp_drop >= buf_drop,
                        "buffer drops are reflected in component_discarded_events_total (#24606)",
                        &json!({ "buffer_discarded": buf_drop, "component_discarded": comp_drop })
                    );
                }
            }
        }
        thread::sleep(Duration::from_secs(2));
    }
    antithesis_sdk::assert_sometimes!(
        buf_seen,
        "drop_newest actually dropped events from the disk buffer this timeline",
        &json!({})
    );
}

/// parallel_driver: fire-and-forget high-volume writer to rapidly fill the disk
/// buffer. Unlike `produce` (which waits up to 10s for each e2e ack), `fill`
/// uses a short timeout and ignores the response — the source still buffers each
/// event before the (abandoned) ack wait. With a blocked sink + drop_newest, the
/// 256MB buffer fills and drop_newest drops, exercising #24606.
fn fill(source_url: &str) {
    antithesis_sdk::assert_reachable!("fill driver started");
    let prefix = run_prefix();
    // 64KB events: small enough to send within the short timeout, big enough to
    // fill 256MB in a few thousand requests across parallel fillers.
    for i in 0..50_000u64 {
        let id = format!("{prefix}-f{i}");
        let _ = post_event(source_url, &id, 64 * 1024, Duration::from_millis(800));
    }
    antithesis_sdk::assert_reachable!("fill driver finished a burst");
}

/// eventually_: faults are paused. Verify durability + writer progress.
fn check(source_url: &str, metrics_url: &str) {
    // 1. Let the system recover from whatever faults happened.
    eprintln!("[workload] check: waiting for vector recovery");
    let recovered = wait_for_vector(metrics_url, Duration::from_secs(120));

    // 2. Drain rigorously: wait until Vector reports the disk buffer EMPTY
    //    (buffer_events == 0) AND delivered.log stops growing. Without the
    //    buffer-empty gate, in-flight events (accepted/acked but not yet drained
    //    to the collector) are falsely counted as "lost" — an oracle artifact.
    let mut last = read_ids(DELIVERED_LOG).len();
    let drain_deadline = Instant::now() + Duration::from_secs(150);
    loop {
        thread::sleep(Duration::from_secs(3));
        let buf_events = match ureq::get(metrics_url).timeout(Duration::from_secs(3)).call() {
            Ok(resp) if resp.status() == 200 => resp
                .into_string()
                .ok()
                .map(|b| sum_metric(&b, "vector_buffer_events"))
                .unwrap_or(-1.0),
            _ => -1.0,
        };
        let cur = read_ids(DELIVERED_LOG).len();
        let drained = buf_events == 0.0 && cur == last;
        if drained || Instant::now() > drain_deadline {
            break;
        }
        last = cur;
    }

    let acked = read_ids(ACKED_LOG);
    let delivered = read_ids(DELIVERED_LOG);
    let missing: Vec<&String> = acked.difference(&delivered).take(20).collect();

    // Confirm the buffer was actually exercised this timeline.
    antithesis_sdk::assert_sometimes!(
        !delivered.is_empty(),
        "disk buffer delivered events end-to-end this timeline",
        &json!({ "delivered": delivered.len(), "acked": acked.len() })
    );

    // (a) Durability / at-least-once: with e2e acks, every event the source
    // acked (200) was reported delivered downstream, so the collector must hold
    // it after recovery. A miss = acknowledged-then-lost.
    antithesis_sdk::assert_always!(
        missing.is_empty(),
        "every end-to-end-acked event survives faults and reaches the collector",
        &json!({ "acked": acked.len(), "delivered": delivered.len(),
                 "missing_count": acked.difference(&delivered).count(),
                 "missing_sample": missing })
    );

    // (b) Writer progress (the #21683 deadlock demonstration): after faults
    // stop and Vector recovers, a brand-new write must be deliverable within a
    // generous bound. If the ledger total_buffer_size underflowed, is_buffer_full
    // stays true forever and this never succeeds.
    let probe_id = format!("probe-{}", now_ms());
    let mut probe_delivered = false;
    if recovered {
        let deadline = Instant::now() + Duration::from_secs(45);
        while Instant::now() < deadline {
            if post_event(source_url, &probe_id, 1, Duration::from_secs(10)) {
                // e2e ack ⟹ delivered; double-check it reached the collector.
                thread::sleep(Duration::from_secs(1));
                if read_ids(DELIVERED_LOG).contains(&probe_id) {
                    probe_delivered = true;
                    break;
                }
            }
            thread::sleep(Duration::from_secs(2));
        }
    }
    antithesis_sdk::assert_always!(
        probe_delivered,
        "post-recovery write makes progress (no permanent writer deadlock)",
        &json!({ "recovered": recovered, "probe_id": probe_id })
    );
}
