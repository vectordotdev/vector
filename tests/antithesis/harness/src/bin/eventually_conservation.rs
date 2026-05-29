//! eventually_conservation: faults are paused. Wait for the system to go quiet —
//! every node healthy and the collector's acked/delivered counts stable — then
//! assert no acked id was lost and none was invented, and prove the writer still
//! makes progress with a fresh post-recovery write.
//!
//! Quiescence is COUNTER-driven, not wall-clock: we declare quiet only when acked
//! and delivered both hold steady across several polls while all nodes are
//! healthy. `acked` holding steady is the producers-stopped signal. clock_jitter
//! only changes how far apart the polls land, never the verdict.
//!
//! The #21683 deadlock is NOT detected by a buffer gauge: the bug underflows the
//! BYTES counter and wedges the writer, but the reader keeps draining events so
//! the events gauge reads 0. The deadlock is caught instead by the post-recovery
//! probe — a wedged writer blocks a fresh write, which never round-trips — and by
//! the SUT-side underflow assertion compiled into node0.

extern crate antithesis_instrumentation;

use std::time::{Duration, Instant};

use antithesis_sdk::{antithesis_init, assert_always, assert_sometimes, assert_unreachable};
use serde_json::Value;

// Representative probe sizes (bytes of "pad"): a tiny event, a flush-threshold
// event, and a near-data-file-size event. A 1-byte probe could slip under a
// partial underflow; the large one exercises the byte-accounting predicate the
// #21683 wedge poisons.
const W: usize = 256 * 1024;
const PROBE_SIZES: [usize; 3] = [1, W, 768 * 1024];

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// The collector's verdict. Named fields, because this is Rust.
struct Report {
    acked: u64,            // ids node0 acked (must come back)
    delivered: u64,        // distinct ids that came back
    delivered_total: u64,  // raw delivery count (distinct + duplicates)
    missing_count: u64,    // acked but never delivered -> data loss
    missing_sample: Vec<u64>,
    spurious_count: u64,   // delivered but never issued -> invented/corrupted id
}

fn fetch_report(collector_url: &str) -> Option<Report> {
    let body = ureq::get(&format!("{collector_url}/report"))
        .timeout(Duration::from_secs(5)).call().ok()?.into_string().ok()?;
    let v: Value = serde_json::from_str(&body).ok()?;
    Some(Report {
        acked: v["acked"].as_u64()?,
        delivered: v["delivered"].as_u64()?,
        delivered_total: v["delivered_total"].as_u64()?,
        missing_count: v["missing_count"].as_u64()?,
        missing_sample: v["missing_sample"].as_array()
            .map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default(),
        spurious_count: v["spurious_count"].as_u64()?,
    })
}

fn delivered_contains(collector_url: &str, id: u64) -> bool {
    ureq::get(&format!("{collector_url}/delivered?id={id}"))
        .timeout(Duration::from_secs(5)).call().ok()
        .and_then(|r| r.into_string().ok())
        .map(|s| s.trim() == "1").unwrap_or(false)
}

fn claim(collector_url: &str) -> Option<u64> {
    ureq::post(&format!("{collector_url}/claim"))
        .timeout(Duration::from_secs(10)).call().ok()
        .and_then(|r| r.into_string().ok())
        .and_then(|s| s.trim().parse().ok())
}

fn node_healthy(metrics_url: &str) -> bool {
    ureq::get(metrics_url).timeout(Duration::from_secs(3)).call()
        .map(|r| r.status() == 200).unwrap_or(false)
}

fn all_healthy(metrics_urls: &[String]) -> bool {
    metrics_urls.iter().all(|u| node_healthy(u))
}

fn post_probe(source_url: &str, id: u64, pad: usize) -> bool {
    let event = serde_json::json!([{ "id": id, "pad": "x".repeat(pad) }]);
    ureq::post(source_url).set("content-type", "application/json")
        .timeout(Duration::from_secs(10)).send_string(&event.to_string())
        .is_ok()
}

fn main() {
    antithesis_init();
    let source_url = env_or("VECTOR_SOURCE_URL", "http://node0:8080/");
    let metrics_url = env_or("VECTOR_METRICS_URL", "http://node0:9598/metrics");
    let collector_url = env_or("COLLECTOR_URL", "http://127.0.0.1:8686");
    let metrics_urls: Vec<String> = env_or("VECTOR_METRICS_URLS", &metrics_url)
        .split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();

    // Counter-driven quiescence: every node healthy AND acked + delivered both
    // steady for several consecutive polls. A node that is down keeps us waiting
    // (we never assert loss over a system we cannot observe) rather than declaring
    // a false drain or reding a down node as a wedge.
    let mut prev: Option<(u64, u64)> = None;
    let mut stable = 0u32;
    let mut quiescent = false;
    let deadline = Instant::now() + Duration::from_secs(240);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_secs(3));
        if !all_healthy(&metrics_urls) {
            stable = 0;
            prev = None;
            continue;
        }
        let Some(r) = fetch_report(&collector_url) else { stable = 0; continue; };
        let cur = (r.acked, r.delivered);
        if prev == Some(cur) {
            stable += 1;
            if stable >= 5 { quiescent = true; break; }
        } else {
            stable = 0;
        }
        prev = Some(cur);
    }

    let Some(report) = fetch_report(&collector_url) else {
        assert_always!(false, "collector reachable for the conservation report", &serde_json::json!({}));
        return;
    };

    if quiescent {
        // No data loss: every id a node acked came back out. Ungated — never gated
        // on a buffer gauge, which the #21683 underflow leaves reading clean.
        assert_always!(report.missing_count == 0,
            "every end-to-end-acked event survives faults and reaches the collector",
            &serde_json::json!({ "acked": report.acked, "delivered": report.delivered,
                "missing_count": report.missing_count, "missing_sample": report.missing_sample }));
        // The loss magnet. This is an exhibition experiment: we are proving the
        // advertised "e2e acks + disk buffer = durable, chain them = no loss" claim
        // is false. assert_unreachable makes Antithesis actively SEARCH for a fault
        // schedule that reaches confirmed loss (a config reload that drops unflushed
        // acked events, a crash before fsync, a corruption-skip). Reaching it is the
        // demonstration: an event a node acked was permanently lost.
        if report.missing_count > 0 {
            assert_unreachable!("an end-to-end-acked event was permanently lost",
                &serde_json::json!({ "acked": report.acked, "delivered": report.delivered,
                    "missing_count": report.missing_count, "missing_sample": report.missing_sample }));
        }
        // Integrity backstop (the collector also checks this continuously at ingest).
        assert_always!(report.spurious_count == 0,
            "every delivered id was actually issued (no invented or corrupted ids)",
            &serde_json::json!({ "spurious_count": report.spurious_count }));
    }

    // Anti-vacuity: a green run must have moved a meaningful amount of data, and
    // must have actually exercised the at-least-once replay path (a duplicate was
    // observed), or the conservation pass is hollow.
    assert_sometimes!(report.acked > 100, "a large set of events was acked and conserved",
        &serde_json::json!({ "acked": report.acked, "delivered": report.delivered }));
    assert_sometimes!(report.delivered_total > report.delivered,
        "a duplicate delivery was observed (the at-least-once replay path ran)",
        &serde_json::json!({ "delivered": report.delivered, "delivered_total": report.delivered_total }));

    // Writer progress — the #21683 deadlock demonstration. Only meaningful once the
    // system is quiescent and healthy: a fresh write of each representative size
    // must then round-trip within a bound. The deadlock still reaches quiescence
    // (acked and delivered both freeze), so gating the probe on quiescence keeps
    // the catch while avoiding a false red against a merely-busy buffer. If
    // total_buffer_size underflowed, is_buffer_full stays true forever and the
    // wedged writer never accepts the probe.
    if quiescent && all_healthy(&metrics_urls) {
        let mut progressed = true;
        for pad in PROBE_SIZES {
            let Some(probe) = claim(&collector_url) else { progressed = false; break; };
            let deadline = Instant::now() + Duration::from_secs(45);
            let mut landed = false;
            while Instant::now() < deadline {
                if post_probe(&source_url, probe, pad) {
                    std::thread::sleep(Duration::from_secs(1));
                    if delivered_contains(&collector_url, probe) { landed = true; break; }
                }
                std::thread::sleep(Duration::from_secs(2));
            }
            if !landed { progressed = false; break; }
        }
        assert_always!(progressed, "post-recovery write makes progress (no writer deadlock)",
            &serde_json::json!({ "quiescent": quiescent }));
    }
}
