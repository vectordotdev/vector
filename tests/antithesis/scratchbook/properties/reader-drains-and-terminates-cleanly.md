---
slug: reader-drains-and-terminates-cleanly
property_id: 13
type: Liveness
antithesis_assertion: Sometimes(reader_returned_none_clean)
sut_path: lib/vector-buffers/src/variants/disk_v2/
commit: 049eec79b737450c4669b7f8aa1dd814551ec466
updated: 2026-06-02
cross_refs:
  - total-buffer-size-never-underflows   # termination condition uses total_buffer_size == 0
  - writer-eventually-makes-progress      # writer must be done before reader can terminate
  - acked-files-eventually-deleted        # deletion drives total_buffer_size to 0
related_issues:
  - "vectordotdev/vector #23456"  # the exact flaky test this property covers
  - "vectordotdev/vector #21683"  # total_buffer_size underflow breaks termination
disabled_tests:
  - "lib/vector-buffers/src/variants/disk_v2/tests/basic.rs::reader_exits_cleanly_when_writer_done_and_in_flight_acks (ignore = \"flaky. See https://github.com/vectordotdev/vector/issues/23456\")"
---

# Property 13: reader-drains-and-terminates-cleanly

## Invariant (informal)

When the writer is done (i.e., `mark_writer_done()` has been called) and every
record that was written has been read **and** acknowledged downstream, the
reader's `next()` coroutine returns `Ok(None)` — a clean `None` that signals
end-of-stream — within finite time. Two failure modes must both be excluded:

- **Hang**: `next()` blocks indefinitely despite no more data or acks pending.
- **Premature None**: `next()` returns `None` before all records have been
  delivered, truncating the stream and silently dropping undelivered events.

---

## Termination Condition and Its Fragility

The termination check is at `reader.rs:991-993`:

```rust
// reader.rs:991-993
if self.ledger.is_writer_done() {
    let total_buffer_size = self.ledger.get_total_buffer_size();
    if total_buffer_size == 0 {
        return Ok(None);
    }
}
```

Both conditions must be simultaneously true:

1. `is_writer_done()` reads the `writer_done: AtomicBool` at ledger.rs:437-439
   with `Ordering::Acquire`. This is set by `mark_writer_done()` (ledger.rs:430-434)
   which is called when the writer is dropped/closed.

2. `get_total_buffer_size()` reads `total_buffer_size: AtomicU64`. This is
   decremented by `track_reads` (ledger.rs:420-424) when acks are processed in
   `handle_pending_acknowledgements` (reader.rs:634-635).

**Three distinct ways this condition can fail:**

### Failure A — Premature None (notify-before-ledger-update race)

The termination check runs at the **top of each loop iteration**, before
`ensure_ready_for_read()`. The sequence inside a single `next()` call is:

```
loop {
    handle_pending_acknowledgements(force_check)   // may decrement total_buffer_size
    CHECK: is_writer_done() && total_buffer_size == 0  ← termination exit here
    ensure_ready_for_read()
    try_next_record()
    ...
    wait_for_writer()  ← woken by finalizer calling notify_writer_waiters()
}
```

The finalizer task (ledger.rs:730-734) calls:

```rust
self.increment_pending_acks(amount);   // ledger.rs:732: pending_acks += amount
self.notify_writer_waiters();          // ledger.rs:733: wakes reader
```

These two operations are **not atomic**. The `notify_writer_waiters()` wakes
the reader before `increment_pending_acks` is visible to the reader OR, more
precisely, both atomics use `AcqRel` but between the two calls the reader may
be scheduled and run the loop iteration:

1. Finalizer: `pending_acks.fetch_add(amount, AcqRel)` — commit succeeds.
2. Reader wakes up (scheduled).
3. Reader: `handle_pending_acknowledgements` → `consume_pending_acks()` → sees
   the new `pending_acks`. Processes acks. Decrements `total_buffer_size`.
4. Reader: termination check: `is_writer_done() == true`,
   `total_buffer_size == 0` → **returns `Ok(None)`**.

This path is actually the *intended* correct path. The race described in the
`#23456` test is subtler: the test at basic.rs:116-143 demonstrates that the
reader must enter `wait_for_writer()` **twice** before the ack arrives, because
the writer's close sends one spurious wakeup. The flakiness arises from:

- The reader receives the spurious wakeup from writer close.
- It loops back to the termination check with `total_buffer_size > 0` (ack not
  yet processed). Correctly, it continues.
- It waits again on `wait_for_writer()`.
- The ack arrives. Finalizer fires `notify_writer_waiters()`.
- Reader wakes. But this time, it must **consume the pending ack** in
  `handle_pending_acknowledgements` *before* hitting the termination check,
  because otherwise `total_buffer_size` is still non-zero and it waits again.

The flakiness is in how tokio's `Notify` (edge-triggered, one-permit store)
interacts with the reader's poll frequency. If the reader's second
`wait_for_writer()` call races with the finalizer's `notify_writer_waiters()`:

- If `notify_writer_waiters()` fires **before** `wait_for_writer()` is called,
  the permit is stored. The next `wait_for_writer()` returns immediately.
- If the reader has already entered `.notified()` and is parked, the notify
  wakes it.
- In both cases the reader eventually processes the ack and terminates.

The "flaky" failure mode most likely involves the reader waking, *not* finding
the ack (a timing window where `pending_acks` is 0 because the finalizer task
hasn't yet run), and going back to sleep, missing the stored permit. But
`Notify` stores at most one permit — if the notify arrived while the reader was
awake, the permit may have been consumed by the previous wakeup. The reader
parks again, but now no notify is coming (the finalizer already fired its one
permit). **Hang.**

This is exactly the "missed wakeup" concern noted in sut-analysis.md §4.

### Failure B — Hang (total_buffer_size stuck non-zero due to underflow)

From sut-analysis.md §5 (L8) and §6 (root cause #1):

If `total_buffer_size` has wrapped to ≈ 2^64 due to the unguarded `fetch_sub`
in `decrement_total_buffer_size` (ledger.rs:306-320):

```rust
// ledger.rs:306-320 — no saturation
pub fn decrement_total_buffer_size(&self, amount: u64) {
    let last_total_buffer_size = self.total_buffer_size.fetch_sub(amount, Ordering::AcqRel);
    ...
}
```

A committed `assert_always_greater_than_or_equal_to!` now precedes the `fetch_sub` at ledger.rs:313, so Antithesis flags the underflow as a finding — but it is a detector, not a guard: the `fetch_sub` itself still wraps.

Then `total_buffer_size` equals ≈ 2^64, never reaches 0, and the termination
condition `total_buffer_size == 0` is never satisfied. The reader loops forever
in `wait_for_writer()`. **Permanent hang.**

This is a distinct trigger from Failure A: A wraps once and is done; B is an
arithmetic bug whose trigger is crash/partial-write discrepancies on restart.
Both lead to the same observable symptom: `next()` never returns.

### Failure C — Premature None (total_buffer_size reaches 0 while undelivered records exist)

This is the *opposite* direction. If `total_buffer_size` is decremented more
than it should be — for example, if a double-decrement occurs, or if the
startup `update_buffer_size` under-seeds the initial value — then
`total_buffer_size` reaches 0 while there are still unread or un-acked records
on disk. The termination check fires prematurely. Records are silently dropped.

The startup seeding path (ledger.rs, `update_buffer_size`): seeds
`total_buffer_size` from the sum of `.dat` file sizes. The reader then
decrements by the number of bytes in each **record** (not the full file size).
If the file contains padding, partial writes at the tail, or gap markers, the
per-record decrement total may be less than the file size, leaving
`total_buffer_size > 0` at end. But if the accounting goes the other way
(over-counting the decrements somehow), premature termination is possible.

---

## The Exact `#23456` Race Path

The disabled test `reader_exits_cleanly_when_writer_done_and_in_flight_acks`
(basic.rs:72-152) exercises the following sequence under a **single-event**
buffer:

1. Write one `SizedRecord::new(32)`. Flush. Close writer.
2. `read_next_some(&mut reader)` — reads the record; does NOT ack yet.
3. `reader.next()` is polled. It must **not** return `None` here (the record
   has been read but `total_buffer_size` is still non-zero because the ack
   hasn't been processed). The test asserts it enters `wait_for_writer()` at
   least **twice** (line 120) — once consuming the spurious wakeup from writer
   close, once blocking for real.
4. `acknowledge(first_read).await` — drops the `BatchNotifier`, which triggers
   the finalizer task.
5. Finalizer: `pending_acks += 1; notify_writer_waiters()`.
6. Reader wakes. `handle_pending_acknowledgements`: `consume_pending_acks() = 1`.
   Processes the ack. `track_reads(...)` → `decrement_total_buffer_size(...)`.
   `total_buffer_size` becomes 0.
7. Termination check: `is_writer_done() && total_buffer_size == 0` → `Ok(None)`.
8. Assert `blocked_read.is_woken()`. Assert `second_read == Ok(None)`.

The flakiness is the window between steps 5 and 6: if the reader is scheduled
to run between `notify_writer_waiters()` (step 5) and when the reader actually
calls `consume_pending_acks()` (step 6), and the reader's `Notify` permit was
already consumed by a prior spurious wakeup in step 3, then the reader may park
in `wait_for_writer()` indefinitely after step 5 fires — because the permit was
already spent and the finalizer won't fire again.

Antithesis can **deterministically explore** this scheduling window, whereas
the unit test relies on tokio-test mock timers and `spawn` polling, which is
non-deterministic in timing.

---

## Cross-Reference: `total-buffer-size-never-underflows` and `writer-eventually-makes-progress`

This property depends on both:

- **`total-buffer-size-never-underflows`**: If `total_buffer_size` wraps to 2^64,
  the termination condition `total_buffer_size == 0` can never be satisfied
  (Failure B). The clean-termination property is broken by the same arithmetic
  bug that breaks writer liveness.

- **`acked-files-eventually-deleted`**: File deletion is what drives
  `total_buffer_size` to 0 in the normal path (Failure A requires the deletion
  to have occurred to decrement the counter). If files are never deleted (e.g.,
  finalizer task died), `total_buffer_size` stays positive and the reader hangs.

- **`writer-eventually-makes-progress`**: Less direct. But if the writer is
  deadlocked, it cannot call `close()` / `mark_writer_done()`, so
  `is_writer_done()` stays false, and the termination check never fires even
  if `total_buffer_size == 0`. This is a distinct hang path: the reader waits
  for the writer to be done, the writer waits for the reader to free space, the
  reader waits for acks — circular dependency if `total_buffer_size` wrapped.

---

## Antithesis Experimental Design

### Target scenario

1. Configure a small buffer (one or two data files). Write N records. Flush.
   Close the writer (call `writer.close()`).
2. Read all N records via `reader.next()`. Collect all `BatchNotifier` handles
   without yet acking.
3. Assert that `reader.next()` does **not** return `None` at this point (it
   should be waiting, since `total_buffer_size > 0`).
4. Drop all notifiers (ack all records). Wait for the finalizer task to fire.
5. Assert that `reader.next()` returns `Ok(None)` within a timeout.
6. Assert `total_buffer_size == 0` via ledger introspection.

### Antithesis scheduling exploration

- **Interleave the ack between the two `wait_for_writer()` calls** (steps 3
  and 4 in the `#23456` race path above). This is precisely the window that
  makes the unit test flaky. Antithesis's scheduler can deterministically force
  this interleaving.
- **Delay finalizer task scheduling** relative to the reader polling. With
  Antithesis's virtual scheduling, the finalizer task can be held off until
  after the reader has re-entered `wait_for_writer()`, testing whether the
  `Notify` permit is correctly stored or dropped.
- **SIGKILL between writer close and ack arrival.** On restart, the buffer
  should replay the un-acked record. The reader should read it again, ack it,
  and then terminate cleanly. Assert no duplication in the downstream oracle
  (requires idempotent downstream).

### Fault-specific assertions

- **Premature None detection** (workload oracle): Track the total number of
  events enqueued by the writer. Assert that the total number of events
  delivered to the downstream sink equals the number enqueued (minus any
  crash-window losses that are expected). A premature `None` silently truncates
  this count.
- **Hang detection** (workload oracle): After `writer.close()`, assert that
  `reader.next()` returns within a finite bound (e.g., 10× the
  `flush_interval`). If it does not, capture the ledger state:
  `is_writer_done`, `total_buffer_size`, `pending_acks`, `reader_last_record`,
  `writer_next_record`.

### Assertions to add (SUT-side)

The Antithesis SDK is a committed dependency under the `antithesis` feature, and three `assert_always_greater_than_or_equal_to!` underflow detectors already ship (ledger.rs:271, ledger.rs:313, reader.rs:529 — see `existing-assertions.md`). The ledger.rs:313 detector sits on `decrement_total_buffer_size`, directly relevant to Failure B's wrap-to-2^64 termination break — but it is a detector, not a guard (the `fetch_sub` still wraps). None of the three covers the clean-termination check below, so these are genuine still-to-add suggestions:

```rust
// At the top of the next() loop, before the termination check, assert
// that if we are about to return None, total_buffer_size is truly 0 and
// there are no pending_acks that haven't been consumed yet:
if self.ledger.is_writer_done() {
    let total_buffer_size = self.ledger.get_total_buffer_size();
    let pending_acks = self.ledger.pending_acks.load(Ordering::Acquire);
    if total_buffer_size == 0 {
        antithesis_sdk::assert_always!(
            pending_acks == 0,
            "reader returns None only when no pending acks remain",
            &serde_json::json!({
                "total_buffer_size": total_buffer_size,
                "pending_acks": pending_acks,
            })
        );
        antithesis_sdk::assert_sometimes!(
            true,
            "reader returned None cleanly after writer done and buffer empty",
            &serde_json::json!({
                "reader_last_record": self.last_reader_record_id,
            })
        );
        return Ok(None);
    }
}
```

```rust
// In finalizer task (ledger.rs:730-734), after increment and notify,
// confirm reachability:
tokio::spawn(async move {
    while let Some((_status, amount)) = stream.next().await {
        self.increment_pending_acks(amount);
        antithesis_sdk::assert_sometimes!(
            true,
            "finalizer task delivered ack to pending_acks",
            &serde_json::json!({ "amount": amount })
        );
        self.notify_writer_waiters();
    }
});
```

---

## Why This Matters

Clean reader termination is the **shutdown contract** of the disk buffer. A
Vector topology performs graceful shutdown by: (1) stopping sources, (2)
waiting for all in-flight events to be delivered and acked, (3) tearing down
sinks. If the disk buffer's reader never returns `None`, step (2) hangs
forever. The operator must send SIGKILL, which:

- Loses up to 500ms of unflushed writes (the documented window).
- Leaves the buffer in a partially-acknowledged state that triggers
  re-processing on restart (duplicates).
- In the `#21683` scenario, leaves `total_buffer_size` in a state that causes
  a permanent deadlock on the next startup.

A premature `None` is equally bad: events that have been written to disk but
not yet delivered are silently abandoned. The customer loses data they expected
the disk buffer to protect.

The `#23456` test being **disabled as flaky** is a direct signal that this
property is not reliably upheld even under the deterministic conditions of
`tokio-test`. Antithesis, by exploring all scheduler interleavings, should be
able to (a) reproduce the flaky failure deterministically, (b) isolate the
exact race window, and (c) verify any fix actually closes the window.

---

## Open Questions

1. **What is the root cause of `#23456` flakiness?** The test's comment
   (basic.rs:107-115) explains the two-wait requirement but does not identify
   what scheduling interleaving causes it to fail. Is it the `Notify` permit
   being spent before the second park, or a more subtle ordering between the
   finalizer task and the reader task? Antithesis can answer this definitively.

2. **Is there a window between `mark_writer_done()` and the reader checking
   `is_writer_done()` where `total_buffer_size` drops to 0 for an unrelated
   reason?** If so, a premature `None` could occur before the writer has
   finished all writes. The `mark_writer_done()` is called from
   `BufferWriter::close()` or `BufferWriter::drop()`. If the writer is dropped
   mid-write (e.g., topology tear-down), this window exists.

3. **Does `notify_writer_waiters()` at ledger.rs:733 correctly wake the reader
   in all cases?** The `Notify` API stores at most one permit. If multiple acks
   arrive before the reader wakes, multiple calls to `notify_writer_waiters()`
   collapse into one permit. The reader wakes once, processes all pending acks
   in one `consume_pending_acks()` call (which atomically swaps to 0), and
   terminates. This appears correct but needs verification under high-concurrency
   ack delivery (many sink workers acking simultaneously, each triggering the
   finalizer).

4. **Does the graceful shutdown sequence guarantee `writer.close()` is called
   before the reader is asked to terminate?** If the topology drops the reader
   before the writer is marked done, `is_writer_done()` stays false and the
   reader hangs. This is an integration question about topology shutdown
   ordering, not visible from the disk-buffer code alone.

5. **Can `total_buffer_size` reach 0 before all in-flight acks are processed
   due to the gap-marker path?** If records are skipped due to corruption
   (`events_skipped > 0` at reader.rs:610-612), `track_dropped_events` is
   called (reader.rs:650) but does NOT decrement `total_buffer_size`. The
   decrement only happens in `track_reads` (via `bytes_acknowledged`). If
   corruption causes a gap that accounts for the last remaining bytes in
   `total_buffer_size`, and the gap is processed by `events_skipped` rather
   than `bytes_acknowledged`, `total_buffer_size` may not reach 0 when it
   should. Or vice versa, it may reach 0 prematurely if gap markers account
   for bytes that were already decremented elsewhere. This interaction deserves
   careful tracing.
