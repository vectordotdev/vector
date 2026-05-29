---
slug: throughput-progresses-under-contention
type: Liveness / Sometimes(throughput_above_floor)
sut_path: lib/vector-buffers/src/variants/disk_v2/
commit: 049eec79b737450c4669b7f8aa1dd814551ec466
updated: 2026-06-02
---

# Property: throughput-progresses-under-contention

## Catalog Entry

**Type:** Liveness / `Sometimes(throughput_above_floor)`

**Property:** With N≥4 parallel source components all writing through a
single disk-buffer writer (sharing the `Arc<Mutex<BufferWriter>>` lock), and
with Antithesis CPU throttle active, write throughput during a quiet
(fault-free) observation window stays above a configurable floor (e.g.,
1,000 events/second or 1 MiB/s). This distinguishes three states that exist
on a continuum but require different operator responses:

1. **Healthy** — throughput at or near the documented ~90 MiB/s ceiling.
2. **Degenerate-but-alive** — throughput severely degraded (e.g., 100×
   below normal) but still making progress; lock starvation, not deadlock.
3. **Permanently deadlocked** — throughput zero; caught by
   `writer-eventually-makes-progress`.

The `writer-eventually-makes-progress` property only catches case 3.
This property catches case 2: a degenerate-but-alive system that would be
indistinguishable from healthy on a coarse dashboard but is actually a
regression to near-zero progress.

**Invariant:** `Sometimes(throughput_above_floor)`: during any 10-second
quiet-period window in a run with N≥4 parallel senders and CPU throttle
active, the count of successfully written events exceeds a floor threshold.
`Sometimes` is correct (not `Always`) because CPU throttle may prevent
even a single write during a brief window; the assertion fires to prove
that progress is not permanently below the floor over the full observation
window.

**Antithesis Angle:** Configure N≥4 parallel source components → single
disk buffer sink (they all share the same `Arc<Mutex<BufferWriter>>`).
Enable Antithesis CPU throttle on the Vector process. Run for a
duration long enough to observe throughput variance. Assert
`Sometimes(event_throughput > floor)` over a moving window. Combine with
`writer-eventually-makes-progress` to distinguish degenerate-but-alive
from deadlocked: if throughput is always below floor AND writer-eventually-
makes-progress fires, the system is degenerate; if writer-eventually-makes-
progress never fires, the system is deadlocked.

**Why It Matters:** The internal buffers GA design doc doc documents a known
throughput ceiling of ~90 MiB/s under 10-thread lock contention. Under
Antithesis CPU throttle (which can reduce effective parallelism and introduce
scheduling jitter), the same lock contention can drive throughput toward zero
without triggering a deadlock. This regression is hard to detect in CI because
it requires sustained parallel writes under resource pressure — exactly the
conditions Antithesis can provide. The value is catching a *regression to
near-zero progress* that the deadlock detection (`writer-eventually-makes-
progress`) misses.

---

## Code Verification

### `Arc<Mutex<BufferWriter>>` — the single lock bottleneck (sender.rs:24)

```rust
// lib/vector-buffers/src/topology/channel/sender.rs:24
DiskV2(Arc<Mutex<disk_v2::BufferWriter<T, ProductionFilesystem>>>),
```

Every send operation (whether from `SenderAdapter::send`, `try_send`, or
`flush`) acquires this mutex:

```rust
// sender.rs:46-48
Self::DiskV2(writer) => {
    let mut writer = writer.lock().await;  // contended by all parallel senders
    writer.write_record(item).await...
}
```

Multiple topology components sharing the same disk-buffer sink all clone
the `Arc` and contend on the single `Mutex`. This is a serialization point
with no read/write splitting.

### Writer sends via `Arc` clone (sender.rs:35-37)

```rust
// sender.rs:33-37
impl<T: Bufferable> From<disk_v2::BufferWriter<T, ProductionFilesystem>> for SenderAdapter<T> {
    fn from(v: disk_v2::BufferWriter<T, ProductionFilesystem>) -> Self {
        Self::DiskV2(Arc::new(Mutex::new(v)))
    }
}
```

The `Arc::new(Mutex::new(v))` wrapping happens at topology construction time.
When multiple sources share the same sink, the `Arc` is cloned and all
contend on the same underlying `Mutex`.

### `write_record` hold time (writer.rs — the critical section)

Each sender holds the mutex for the duration of:

1. `encode` (protobuf serialization of the event).
2. `write_record` (copy into the 256KB `TrackingBufWriter`).
3. Conditionally, `flush_inner` (page-cache flush; sometimes `sync_all`).

Under CPU throttle, any of these steps can be extended arbitrarily, holding
the mutex and blocking all other senders.

### `DEFAULT_WRITE_BUFFER_SIZE` = 256KB (common.rs:37)

```rust
// lib/vector-buffers/src/variants/disk_v2/common.rs:37
pub const DEFAULT_WRITE_BUFFER_SIZE: usize = 256 * 1024;
```

The `TrackingBufWriter` buffers 256KB before issuing a write syscall. If the
mutex holder is encoding a large event, other senders wait for the full
encode + copy cycle. Under CPU throttle, this can be hundreds of milliseconds
per acquisition.

### Lock contention documented in GA doc

Per `_external-references-digest.md`: "A major lock-contention performance
issue affected all disk-buffer users (writer throughput ~90 MiB/s capped by
contention)." This is a known ceiling; this property detects regression
*below* a minimum floor, not measurement of the ceiling.

### Relationship to `ensure_ready_for_write` (writer.rs:1001-1019)

The writer's `ensure_ready_for_write` loop holds the mutex while waiting for
the reader to signal progress (`wait_for_reader().await`). Under the underflow
deadlock (#21683), this await never resolves — mutex held forever. Under lock
starvation (this property's target), the await resolves but other senders
cannot acquire the mutex at a meaningful rate.

---

## Distinguishing Degenerate from Deadlocked

| State | `writer-eventually-makes-progress` | `throughput-progresses-under-contention` |
|---|---|---|
| Healthy | fires (Sometimes) | fires (Sometimes) |
| Degenerate-but-alive | fires (Sometimes) | does NOT fire (throughput below floor) |
| Permanently deadlocked | does NOT fire | does NOT fire |

Antithesis can distinguish states 2 and 3 by checking BOTH properties in the
same run: if `writer-eventually-makes-progress` fires but
`throughput-progresses-under-contention` does not, the system is degenerate.

---

## SUT-Side Instrumentation

The Antithesis SDK is a committed dependency under the `antithesis` feature, and three `assert_always_greater_than_or_equal_to!` underflow detectors already ship (ledger.rs:271, ledger.rs:313, reader.rs:529 — see `existing-assertions.md`). None of them addresses lock-contention throughput, so the assertions below are genuine still-to-add suggestions.

### Assertion 1 — Sometimes: throughput above floor

Placed in a workload-side heartbeat that samples written-event count over a
rolling window:

```rust
// workload heartbeat, every 10 seconds
let events_written_this_window = EVENTS_WRITTEN_COUNTER.swap(0, Ordering::Relaxed);
antithesis_sdk::assert_sometimes!(
    events_written_this_window > THROUGHPUT_FLOOR_EVENTS_PER_WINDOW,
    "throughput-progresses-under-contention: write throughput above floor",
    &serde_json::json!({
        "events_written": events_written_this_window,
        "floor": THROUGHPUT_FLOOR_EVENTS_PER_WINDOW,
        "window_seconds": 10,
        "parallel_senders": N_SENDERS,
    })
);
```

`THROUGHPUT_FLOOR_EVENTS_PER_WINDOW` should be set conservatively (e.g.,
1,000 events per 10-second window) to avoid flakiness under heavy CPU
throttle while still detecting regression to near-zero.

### Assertion 2 — Reachable: mutex acquisition completes under contention

```rust
// sender.rs, after writer.lock().await completes
antithesis_sdk::assert_reachable!(
    "throughput-progresses-under-contention: mutex acquired under parallel contention",
    &serde_json::json!({
        "sender_id": SENDER_ID,  // set per-thread
    })
);
```

This confirms that the async mutex is being acquired (not permanently blocking
due to a deadlock), allowing Antithesis to distinguish "lock never acquired"
from "lock acquired but slowly."

### SUT-side metric: expose lock wait time

The existing `tracing` instrumentation at `sender.rs:46-48` does not measure
mutex acquisition latency. Adding a `histogram` metric at this callsite would
allow both Antithesis assertions and production observability:

```rust
// sender.rs, SenderAdapter::send, DiskV2 arm
let lock_start = Instant::now();
let mut writer = writer.lock().await;
let lock_wait_ms = lock_start.elapsed().as_millis();
// existing metrics infra:
histogram!("disk_buffer_writer_lock_wait_ms", lock_wait_ms as f64);
```

---

## Why Existing Tests Cannot Catch This

- The model-based proptest serializes all operations (single thread, no
  parallel senders) — lock contention is structurally absent.
- Unit tests do not configure multiple parallel sources to the same buffer.
- The disabled `writer_waits_when_buffer_is_full` test (`size_limits.rs,
  #[ignore]`) is the closest existing test to this scenario, but it tests the
  blocking behavior (progress after backpressure), not throughput degradation
  under contention.
- CPU throttle is not available in the existing test environment.

---

## Framing: Performance vs. Correctness

This property borders on performance testing, which is unusual for Antithesis
properties (which typically target safety/liveness, not throughput numbers).
The framing is justified because:

1. The floor is set to catch *regression to near-zero*, not to measure the
   ceiling. A floor of 1,000 events/10s is several orders of magnitude below
   the ~90 MiB/s ceiling; crossing below it indicates a structural problem
   (lock starvation, scheduling fairness failure), not a performance degradation.
2. The value is catching a failure mode that the existing deadlock detection
   misses: "barely alive but functionally useless" is operationally equivalent
   to deadlocked from the customer's perspective.
3. Lock contention under CPU throttle is an interleaving-sensitive bug class
   that Antithesis's execution model is specifically designed to explore.

---

## Open Questions

- What is the appropriate floor threshold? Setting it too high causes flakiness
  under heavy CPU throttle; setting it too low makes the property trivially
  pass even in degenerate states. Recommend calibrating against a baseline run
  (no throttle, single sender) and setting the floor at 0.1% of the baseline
  throughput.

- Does `tokio`'s async mutex (`tokio::sync::Mutex`) provide fairness
  guarantees under CPU throttle? If the mutex uses FIFO queuing (it does),
  all senders should eventually acquire the lock, making permanent starvation
  unlikely. However, FIFO ordering means a slow holder blocks all waiting
  senders for its full hold duration — amplifying the effect of CPU throttle.

- Should the throughput counter be placed at the `write_record` callsite
  (inside the mutex, measuring successful writes) or at the sender entry point
  (measuring attempted writes)? Counting attempted writes exposes the lock
  wait time; counting successful writes exposes the downstream write rate.
  Both are useful; recommend both counters with separate names.

- The GA doc's ~90 MiB/s ceiling was measured with 10 threads. With N=4
  parallel sources, the ceiling is lower. What is the expected throughput
  floor at N=4 under moderate CPU throttle? Needs a calibration run in the
  Antithesis environment.

- If the floor is crossed, does Antithesis need SUT-side instrumentation to
  distinguish "waiting for reader to free space" (correct backpressure
  behavior) from "starved on lock contention" (regression)? The distinction
  requires exposing whether the slow path is `ensure_ready_for_write`
  (waiting for reader, logged at TRACE) or `writer.lock().await` (no current
  instrumentation).
