---
slug: sink-failure-not-silently-acked
type: Safety / Always
status: CURRENTLY VIOLATED (within a process lifetime; at-least-once only restored by crash+replay)
sut_commit: 049eec79b737450c4669b7f8aa1dd814551ec466
---

# Property 14: sink-failure-not-silently-acked

## Catalog Entry

**Type:** Safety / Always

**Property:** An event whose downstream delivery status is `Errored` or `Rejected` is NOT
silently treated as acknowledged and removed from the buffer. At-least-once semantics require
that errored/rejected deliveries are retried (either by the same process or by replay after
restart) and are never silently discarded within a live process.

**Invariant:** For every event batch delivered to a downstream sink, if the batch completes with
`BatchStatus::Errored` or `BatchStatus::Rejected`, the buffer retains the events for retry.
Equivalently: the buffer never silently credits a failed delivery as a successful acknowledgement.

**Current Status: VIOLATED.** The finalizer in `ledger.rs:731` discards the `BatchStatus`
entirely:

```rust
// ledger.rs:728-737
pub(super) fn spawn_finalizer(self: Arc<Self>) -> OrderedFinalizer<u64> {
    let (finalizer, mut stream) = OrderedFinalizer::new(None);
    tokio::spawn(async move {
        while let Some((_status, amount)) = stream.next().await {  // <-- _status discarded
            self.increment_pending_acks(amount);
            self.notify_writer_waiters();
        }
    });
    finalizer
}
```

The variable is named `_status` with a leading underscore, which is the Rust idiom for
explicitly acknowledging that a value is intentionally unused. Every termination status —
`BatchStatus::Delivered`, `BatchStatus::Errored`, and `BatchStatus::Rejected` — causes an
unconditional `increment_pending_acks(amount)`, which advances the reader's acknowledgement
cursor and eventually causes `delete_completed_data_file` to unlink the data file. The events
are then permanently gone from the buffer without having been successfully delivered. This is
silent data loss with no metric, no log at error level, and no retry.

Within a single process lifetime the property is violated. At-least-once is only restored by
crash+replay: a SIGKILL before `delete_completed_data_file` runs means the file survives on
disk and is replayed on restart; but if the finalizer processes the ack and the file deletion
completes before the crash, the events are gone. The window is small but real, and under
sustained error conditions (e.g. a sink that permanently errors) the violation is continuous.

**Antithesis Angle:**

Configure a pipeline with a disk buffer and a sink that can be made to return `Errored` or
`Rejected` status on demand (e.g. an HTTP sink pointed at an endpoint controlled by the
Antithesis workload, or a custom test sink that reads a flag). Then:

1. Write N events into the buffer.
2. Force the sink to error all deliveries (via fault injection: kill the downstream, return 500s,
   etc.) for a sustained window.
3. Without crashing Vector, assert that the events are either:
   a. Still present in the buffer (retry pending), or
   b. Visible as failed in the error log with a count matching the dropped count.
4. Terminate Vector cleanly (graceful shutdown) and verify that a fresh restart replays events
   and delivers them when the downstream is restored.

The assertion to add SUT-side would sit inside `spawn_finalizer`, branching on `_status`:

```rust
// Proposed instrumentation site (ledger.rs:731)
while let Some((status, amount)) = stream.next().await {
    match status {
        BatchStatus::Delivered => {
            self.increment_pending_acks(amount);
            self.notify_writer_waiters();
        }
        BatchStatus::Errored | BatchStatus::Rejected => {
            // assert_always!(false, "errored delivery silently acked — at-least-once violated",
            //     { "amount": amount, "status": format!("{:?}", status) });
            // TODO: implement retry / nack path instead of silently acking
            self.increment_pending_acks(amount); // BUG: this line drops the events
            self.notify_writer_waiters();
        }
    }
}
```

Workload-observable signal: `buffer_discarded_events_total` or `buffer_sent_events_total`
increments, but `component_discarded_events_total` does not. The total events delivered to the
downstream sink is less than total events written to the buffer.

**Why It Matters:**

This is a direct violation of the at-least-once guarantee that disk buffer is sold on. A
customer enabling disk buffering specifically to prevent data loss on transient downstream errors
gets the opposite: errors cause silent, permanent, unmetered data loss with no log or alert.
This matters most in production scenarios where the downstream sink experiences sustained errors
(network partition, service outage, quota exhaustion) and Vector's buffer silently drains itself
rather than accumulating events for replay.

## Open Questions

1. **Does any sink actually emit `Errored` or `Rejected` status under normal operation (non-fault
   conditions)?** If sinks always return `Delivered` on internal retry success and only surface
   errors by panicking or logging, the violation may only be reachable under active fault
   injection. This needs to be verified by checking a representative sink (e.g.
   `src/sinks/http/`) to see what `BatchStatus` values it emits under various error conditions.
   If sinks swallow errors internally and never propagate `Errored` to the finalizer, the
   violation is latent rather than continuously exercised, which affects priority but not
   correctness.

2. **Is the `OrderedFinalizer` typed to drop the status or does it give it to the caller?**
   `vector-common`'s `OrderedFinalizer<T>` yields `(BatchStatus, T)` from the stream (confirmed
   at `ledger.rs:731`). The `_status` binding at the call site is the discard, not an upstream
   limitation. A fix can be made purely in `spawn_finalizer` without changing the finalizer
   library.

3. **Is there a nack / unack path in the buffer at all?** The current ack machinery
   (`handle_pending_acknowledgements` in `reader.rs`) only moves the acknowledgement cursor
   forward; there is no mechanism to "un-ack" or requeue a record. Implementing true retry for
   errored deliveries would require a significant design change (e.g. moving back
   `reader_last_record`, or maintaining a separate retry queue). The fix may require scoping to
   "at minimum, emit an error-level log and a metric" rather than true retry within the same
   process.

4. **Does a graceful shutdown drain the finalizer task before dropping the buffer?** If the
   tokio runtime drops the finalizer task while in-flight acks are still pending, an errored
   delivery may never increment `pending_acks`, leaving the reader stranded. This is a separate
   liveness concern (the reader never deletes the file) rather than the silent-loss concern, but
   both stem from the same finalizer design.

---

### Investigation Log

#### Do sinks actually emit `Errored` or `Rejected` status in practice?

**Examined:** `src/sinks/util/http.rs:928–936` (`DriverResponse for HttpResponse`), `src/sinks/http/tests.rs` (test assertions on `BatchStatus`).

**Found:** The `DriverResponse::event_status` implementation for `HttpResponse` at sinks/util/http.rs:929–936 explicitly returns `EventStatus::Errored` for transient HTTP errors and `EventStatus::Rejected` for permanent HTTP errors (non-2xx, non-transient responses). The test file `src/sinks/http/tests.rs` confirms this at lines 536 and 566 (`assert_eq!(receiver.try_recv(), Ok(BatchStatus::Rejected))`) and line 956 (`Ok(BatchStatus::Rejected)`). The discard at `ledger.rs:731` (`_status` binding) is therefore exercisable in real workloads wherever an HTTP sink receives a 4xx/5xx response. The finalizer unconditionally increments `pending_acks` regardless of which status value reaches it.

**Not found:** No code path in `spawn_finalizer` (ledger.rs:728–737) that branches on `BatchStatus::Errored` or `BatchStatus::Rejected` — the match arm is `_status` with unconditional `increment_pending_acks`. No retry or nack path exists in the buffer.

**Conclusion:** Sinks do emit `Errored`/`Rejected` in practice (confirmed in HTTP sink). The `_status` discard at ledger.rs:731 is therefore a live violation during normal operation with any non-2xx downstream, not only under synthetic fault injection.

#### Is the `_status` discard at ledger.rs:731 intentional or a bug?

**Examined:** `ledger.rs:728–737` (`spawn_finalizer`), git history context (commit 049eec79b), no inline comment explaining the intent.

**Not found:** No comment in the code or adjacent documentation explaining why `_status` is discarded. The naming convention `_status` (leading underscore) is the Rust idiom for "intentionally unused," which suggests the discard is deliberate rather than an oversight — but no comment or design document explains the rationale. There is no nack/retry path in the buffer; implementing one would require reversing `increment_acked_reader_file_id` or maintaining a separate retry queue, which is a significant design change.

**Conclusion:** Whether this is intentional design (the buffer does not support nack/retry within a process lifetime, so status is irrelevant to the acknowledgement cursor) or an unaddressed bug (the buffer should retain errored events for replay) requires human input from the owning team. The effect is clear: at-least-once is violated within a process lifetime under sustained sink errors. This item is flagged as **needs human input** to determine whether the fix scope is "add error log + metric" or "redesign ack path."
