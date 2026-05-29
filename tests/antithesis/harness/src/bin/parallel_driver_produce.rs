//! parallel_driver_produce: drive ONE logical event into node0 under fault
//! injection. Antithesis owns the parallelism (it runs many copies concurrently)
//! and the crash timing, so each invocation does the smallest unit of work: claim
//! one id, POST it, and — because the network is lossy and the outcome of a
//! timeout is ambiguous — RETRY THE SAME id until node0 acks it or we give up. The
//! id is the idempotency key: a retry is never a new event, so a duplicate landing
//! downstream is the legal at-least-once behaviour, not lost or doubled data.
//!
//! On a 2xx (with e2e acks, node0 has durably accepted the event) we relay an
//! ack-back so the collector expects the id to come back out. If we give up before
//! any 2xx, the id was never acked, so it is no obligation and no false loss.

extern crate antithesis_instrumentation;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use antithesis_sdk::random::random_choice;
use antithesis_sdk::{antithesis_init, assert_reachable};
use serde_json::json;

// Payload "pad" sizes in bytes, biased to the disk-buffer boundaries. W = 256KB is
// the internal-buffer flush threshold and the large-record bypass. With node0's
// data file forced to 1MiB (VECTOR_DISK_V2_MAX_DATA_FILE_SIZE), 768KB nearly fills
// a data file so rotations and near-full single-record files happen constantly,
// while staying under max_record_size (= 1MiB) so the record is never rejected.
const W: usize = 256 * 1024;
const SIZE_MENU: [usize; 6] = [0, 1, W - 1, W, W + 1, 768 * 1024];

const MAX_ATTEMPTS: u32 = 5;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

/// POST one event to node0. ureq returns Err for any non-2xx (and for transport
/// errors), so Ok means node0 returned 2xx, which with e2e acks means it took
/// durable responsibility for the event.
fn post_event(source_url: &str, id: u64, pad: usize, timeout: Duration) -> bool {
    let event = json!([{ "id": id, "ts_ms": now_ms(), "pad": "x".repeat(pad) }]);
    ureq::post(source_url)
        .set("content-type", "application/json")
        .timeout(timeout)
        .send_string(&event.to_string())
        .is_ok()
}

/// Claim one fresh id from the collector. Returns the id.
fn claim(collector_url: &str) -> Option<u64> {
    ureq::post(&format!("{collector_url}/claim"))
        .timeout(Duration::from_secs(10))
        .call()
        .ok()
        .and_then(|r| r.into_string().ok())
        .and_then(|s| s.trim().parse().ok())
}

/// Tell the collector node0 acked this id, so it must come back.
fn report_acked(collector_url: &str, id: u64) {
    let _ = ureq::post(&format!("{collector_url}/acked"))
        .timeout(Duration::from_secs(10))
        .send_string(&id.to_string());
}

fn main() {
    antithesis_init();
    let source_url = env_or("VECTOR_SOURCE_URL", "http://node0:8080/");
    let collector_url = env_or("COLLECTOR_URL", "http://127.0.0.1:8686");

    let Some(id) = claim(&collector_url) else {
        return; // collector unreachable; nothing to do this invocation
    };
    // One stable size for this id's whole retry sequence: a retry must re-send the
    // SAME event, never a re-minted one.
    let pad = *random_choice(&SIZE_MENU).unwrap_or(&0);

    for _ in 0..MAX_ATTEMPTS {
        // Tight timeout: a wedged node0 (the #21683 deadlock) blocks forever, so we
        // stop waiting and retry the same id rather than absorb the failure.
        if post_event(&source_url, id, pad, Duration::from_secs(5)) {
            report_acked(&collector_url, id);
            assert_reachable!("produce driver got an end-to-end ack");
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // Gave up before any 2xx: id is claimed but never acked, so it is no obligation.
}
