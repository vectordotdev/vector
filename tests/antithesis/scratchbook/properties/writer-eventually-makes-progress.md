---
slug: writer-eventually-makes-progress
type: Liveness / Sometimes(writer_unblocked_after_full)
sut_path: lib/vector-buffers/src/variants/disk_v2/
commit: b7aae737cef5dd37d1445915443a1eb97b584f85
updated: 2026-05-28
linked_bugs:
  - vectordotdev/vector#21683 (root cause: see total-buffer-size-never-underflows)
  - L1 / L8 in sut-analysis.md §5 (liveness claims that fail under underflow)
---

# Property: writer-eventually-makes-progress

## Catalog Entry

**Type:** Liveness / Sometimes(writer_unblocked_after_full)

**Property:** A writer that blocked because the buffer was full (i.e.
`is_buffer_full()` returned `true`) eventually performs another successful
`write_record` after the reader acknowledges and deletes at least one data file.
The state where `is_buffer_full()` returns `true` permanently (writer deadlock)
never persists indefinitely.

**Invariant:** After every `delete_completed_data_file` invocation that calls
`notify_reader_waiters()` (reader.rs:555), the writer unblocks and completes at
least one successful `write_record` within a bounded time. Equivalently: the
tuple `(is_buffer_full() == true, buffer_is_drained == false)` is not a
permanent fixed point.

**Antithesis Angle:** This is the *user-visible manifestation* of #21683 — a
silent pipeline stall. The workload should:

1. Fill the buffer to capacity (writes block; `is_buffer_full()` true).
2. Inject a node-kill fault at a file-rotation or partial-write boundary.
3. Restart Vector.
4. Resume the reader workload so it acks and deletes files normally.
5. Call `ANTITHESIS_STOP_FAULTS` (quiet period with no further faults).
6. Assert `Sometimes`: the writer completes at least one additional successful
   write after the reader deletes a file post-restart.

If the underflow bug fires at step 2-3, the writer never unblocks at step 6,
and `Sometimes` is never satisfied — Antithesis reports a liveness failure.

**Why It Matters:** A pipeline stall with no error, no crash, and no high-level
alert is the worst possible operational failure mode. Dashboards may appear
normal (PR #23561 makes the buffer-size gauge saturate at zero instead of
showing u64::MAX). The sink stops delivering events, but no alert fires. The
customer experiences silent data loss without any signal to investigate.
This is exactly the scenario disk buffer is supposed to prevent.

---

## The Deadlock Chain (traced through source)

### Step 1: underflow fires (see `total-buffer-size-never-underflows.md`)

`decrement_total_buffer_size` at ledger.rs:292 wraps `total_buffer_size` to ~u64::MAX.

### Step 2: `is_buffer_full` permanently returns `true`

```rust
// writer.rs:993-996
fn is_buffer_full(&self) -> bool {
    let total_buffer_size = self.ledger.get_total_buffer_size() + self.unflushed_bytes;
    let max_buffer_size = self.config.max_buffer_size;
    total_buffer_size >= max_buffer_size  // u64::MAX + any >= any max_size: always true
}
```

`get_total_buffer_size()` loads `total_buffer_size` (ledger.rs:276-278) with
`Ordering::Acquire`. The wrapped value is visible to the writer immediately.

Note: `self.unflushed_bytes + u64::MAX` wraps a second time back near 0 on some
inputs, potentially causing intermittent false negatives. The behaviour is
input-dependent — another source of non-determinism.

### Step 3: `ensure_ready_for_write` enters an infinite sleep loop

```rust
// writer.rs:1001-1019
async fn ensure_ready_for_write(&mut self) -> io::Result<()> {
    loop {
        if !self.is_buffer_full() || !self.ready_to_write {
            break;  // never taken
        }
        // Logs at trace! only — not visible in production by default
        self.ledger.wait_for_reader().await;  // woken by notify_reader_waiters()
    }
    // ...
}
```

`wait_for_reader()` awaits `self.reader_notify.notified()` (ledger.rs:361-363).
Every time the reader deletes a file it calls `notify_reader_waiters()`
(reader.rs:555), which calls `self.reader_notify.notify_one()` (ledger.rs:376).
The writer wakes, calls `is_buffer_full()` (which returns `true`), and blocks
again — the wakeup is real but the accounting is wrong.

The `Notify` primitive is edge-triggered with a single permit. If the writer
misses a wakeup (e.g. it was not yet waiting when `notify_one()` was called),
the next write attempt will observe the false-full state and block for the next
notification from the reader. In the underflow scenario, the reader eventually
drains the entire buffer and stops calling `notify_reader_waiters()`, so the
writer sleeps indefinitely.

### Step 4: `can_write_record` also poisoned

```rust
// writer.rs:793-798
fn can_write_record(&self, amount: usize) -> bool {
    let total_buffer_size = self.ledger.get_total_buffer_size() + self.unflushed_bytes;
    let potential_write_len = u64::try_from(amount)...;
    self.can_write() && total_buffer_size + potential_write_len <= self.config.max_buffer_size
    //                  ^^^^^^^^^^^^^^^^ wrapped value ≫ max_buffer_size: always false
}
```

Even if `ensure_ready_for_write` were somehow bypassed, `can_write_record`
would return `false` for every non-zero write, blocking the write at the
outer layer too.

### Step 5: reader shutdown also broken (L8)

The reader's `next()` uses `total_buffer_size == 0` as the "buffer empty" signal
to return `None` and shut down. With `total_buffer_size` at u64::MAX, `next()`
never sees zero and loops indefinitely trying to read records. The pipeline
cannot shut down cleanly.

### Step 6: wakeup chain dependency diagram

```
sink delivers event
  → BatchNotifier dropped
    → finalizer task (ledger.rs:701-709) calls increment_pending_acks + notify_writer_waiters
      → wakes READER (naming is misleading: notify_writer_waiters wakes the reader's wait_for_writer loop)
        → reader calls handle_pending_acknowledgements
          → delete_completed_data_file
            → decrement_total_buffer_size  ← UNDERFLOW HERE on post-restart run
            → notify_reader_waiters
              → wakes WRITER from wait_for_reader
                → writer checks is_buffer_full → still true → re-blocks
```

Break any link in this chain (underflow, finalizer task dead, reader not polled)
and the writer stalls. Antithesis should kill at multiple points in this chain.

---

## Observable Signals

### Workload-observable (external to SUT)

- **Write throughput drops to zero** after a node-kill-and-restart during
  buffer-full conditions. The workload can measure this by counting successful
  `write_record` completions per time window. After STOP_FAULTS, if throughput
  does not recover within a grace period, liveness fails.
- **Sink delivery throughput drops to zero** — nothing is being read or forwarded.
- **No error logs at ERROR or WARN level** — the stall is silent at those levels;
  the trace! at writer.rs:1013-1016 is only emitted at `trace` level.

### SUT-side (requires Antithesis SDK instrumentation — all MISSING)

- `assert_sometimes!` immediately after a successful `write_record` completes,
  conditional on `had_been_full` (a local flag set when `is_buffer_full()` was
  true at the start of the preceding `ensure_ready_for_write` call). This fires
  once per "blocked writer later unblocks" cycle.
- `assert_unreachable!` inside `ensure_ready_for_write`'s `wait_for_reader`
  branch that counts consecutive wait cycles: if the writer has woken N times
  (say N=100) without making progress, fire. This is an operational proxy for
  the permanent deadlock.

---

## SUT-Side Instrumentation (MISSING — must be added)

### Assertion 1 — Sometimes: writer unblocks after being full

```rust
// writer.rs, in ensure_ready_for_write, after the loop exits
// (i.e., when is_buffer_full() becomes false and the writer is about to write)
if was_buffer_full {  // local bool set to true when we entered the wait
    antithesis_sdk::assert_sometimes!(
        true,
        "writer_unblocked_after_full: writer made progress after buffer was full",
        &serde_json::json!({
            "total_buffer_size": self.ledger.get_total_buffer_size(),
            "max_buffer_size": self.config.max_buffer_size,
            "unflushed_bytes": self.unflushed_bytes,
        })
    );
}
```

### Assertion 2 — Sometimes: writer_unblocked_after_restart

A stronger variant, scoped to "writer unblocked in the same process lifetime
as a restart recovery":

```rust
// After validate_last_write completes (writer.rs, end of the method)
// and the writer subsequently completes a write:
antithesis_sdk::assert_sometimes!(
    true,
    "writer_made_progress_after_recovery",
    &serde_json::json!({ "recovered": true })
);
```

### Assertion 3 — Unreachable: stale-full detected

Inside `ensure_ready_for_write`, count loops without progress:

```rust
let mut stall_count = 0u32;
loop {
    if !self.is_buffer_full() || !self.ready_to_write {
        break;
    }
    stall_count += 1;
    antithesis_sdk::assert_unreachable!(
        "writer stalled waiting for reader without progress",
        &serde_json::json!({
            "stall_count": stall_count,
            "total_buffer_size": self.ledger.get_total_buffer_size(),
            "max_buffer_size": self.config.max_buffer_size,
        })
    );
    self.ledger.wait_for_reader().await;
}
```

Note: `assert_unreachable!` fires on first execution; a stall counter threshold
is not needed — Antithesis tracks whether the unreachable point is ever reached.
But a threshold (e.g. fire only after 10 wakeups with no progress) avoids
false-positive noise during brief legitimate back-pressure.

---

## Antithesis Fault Strategy

### Recommended fault sequence

1. **Fill phase:** Send enough events to bring `total_buffer_size` near
   `max_buffer_size`. Writer should block (normal backpressure).
2. **Fault injection:** Node-kill (SIGKILL) at a file-rotation boundary. The most
   reliable trigger is killing during the `fsync` of a data file just before or
   just after it reaches 128MB, so the tail is partial.
3. **Restart:** Vector restarts; `update_buffer_size` re-seeds from file sizes.
4. **Reader drain:** Let the reader seek, ack, and delete files normally.
5. **STOP_FAULTS:** Call `ANTITHESIS_STOP_FAULTS` or equivalent quiet period.
6. **Verify progress:** Assert `Sometimes(writer_unblocked_after_full)` is
   satisfied within the quiet period. If the assertion is never seen, the test fails.

### Why Antithesis over a fixed chaos test

The internal chaos test uses SIGKILL ×3 at fixed points. The underflow bug depends on
the *exact byte offset* of the crash relative to the file boundary. Antithesis's
systematic exploration of fault timing finds the specific windows (e.g. kill
during file rename/open at rotation, or during the first `write_all` to the new
file) that a fixed-timing test misses.

### CPU throttling amplification

CPU throttling extends the time between `fetch_sub` and the subsequent
`is_buffer_full` check, increasing the window where a reader wakeup arrives
between the underflow and the re-check. Throttling the writer process specifically
may surface race-condition variants.

---

## Relationship to `buffer-size-within-max` (property #7)

If the underflow bug fires, `is_buffer_full()` is permanently `true`, meaning no
more data is written. The on-disk buffer size technically stays within `max_size`
— not because the invariant is upheld, but because the writer is dead.
`buffer-size-within-max` must explicitly note that a passing result under
permanent deadlock conditions is vacuously true and does not indicate health.
Cross-reference: test both properties together; if `buffer-size-within-max` holds
AND `writer-eventually-makes-progress` fails, the combined result exposes the bug.

---

## Open Questions

- **Is the `Sometimes` assertion reachable under normal (non-fault) operation?**
  Yes — any time the buffer fills and then drains normally. This means the
  `Sometimes` property is satisfiable without faults, which is desirable: it
  shows the non-fault path is covered before testing the fault path.

- **What is the grace period for the quiet-phase progress check?** The writer
  may be slow to unblock if the reader takes time to delete files. A grace period
  of ~10s after STOP_FAULTS should be sufficient for a non-buggy system. Tune
  based on `max_buffer_size` and simulated throughput.

- **Does the `Notify` miss-wakeup window matter here?** `notify_one()` stores one
  permit; if the writer is not yet waiting when `notify_one` fires, the next
  `notified().await` returns immediately with the stored permit. This means there
  is no missed-wakeup issue *in the healthy case*. In the underflow case, the
  reader eventually stops notifying (buffer is empty), and the writer sleeps
  indefinitely — this is the bug, not a spurious wakeup race.

- **Does `wait_for_reader` have a timeout?** No (ledger.rs:361-363). The writer
  will sleep indefinitely in the underflow case with no watchdog. A timeout-based
  health check (e.g. emit a WARN log if waiting > 30s) would be a useful
  diagnostic addition independent of Antithesis.

- **Is the finalizer task shutdown correctly?** If the finalizer task (spawned by
  `spawn_finalizer`, ledger.rs:701-709) is dropped before all in-flight acks are
  processed, pending acks are silently lost. This could cause the reader to
  stall waiting for acks that never arrive, which the writer then interprets as
  "reader made no progress." This is a separate liveness bug from #21683 but
  observable via the same `Sometimes` property.

- **Node-termination faults enabled?** Essential for this property. Confirm with
  Antithesis tenant operator. Without kill-and-restart faults, the underflow
  trigger is unreachable and this property will always be satisfied trivially.
