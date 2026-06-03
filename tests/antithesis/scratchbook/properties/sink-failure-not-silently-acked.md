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

`_status` (leading underscore) is the Rust idiom for an intentionally unused value.
Every termination status — `Delivered`, `Errored`, `Rejected` — causes an
unconditional `increment_pending_acks(amount)`, advancing the reader ack cursor and
eventually causing `delete_completed_data_file` to unlink the file. The events are
permanently gone without successful delivery — silent loss, no metric, no error
log, no retry.

Within a process lifetime the property is violated. At-least-once is restored only
by crash+replay: a SIGKILL before `delete_completed_data_file` leaves the file on
disk to replay on restart; if the ack and deletion complete before the crash, the
events are gone. The window is small but real, and under sustained errors (a
permanently-erroring sink) the violation is continuous.

**Antithesis Angle:**

Configure a disk buffer and a sink that can return `Errored`/`Rejected` on demand
(HTTP sink to a workload-controlled endpoint, or a flag-reading test sink):

1. Write N events.
2. Force all deliveries to error (kill downstream, return 500s) for a sustained window.
3. Without crashing Vector, assert the events are either (a) still in the buffer
   (retry pending) or (b) visible as failed in the error log with a matching count.
4. Shut down cleanly and verify a fresh restart replays and delivers once the
   downstream is restored.

A SUT-side assert would branch on `_status` in `spawn_finalizer`, firing on the
`Errored`/`Rejected` arm (today it falls through to `increment_pending_acks`). Not
committed. Workload signal: total delivered < total written with no
`component_discarded_events_total` increment.

**Why It Matters:**

A direct violation of the at-least-once guarantee disk buffer is sold on. A customer
buffering to prevent loss on transient downstream errors gets the opposite: silent,
permanent, unmetered loss with no log or alert. Worst under sustained errors
(network partition, outage, quota exhaustion) where the buffer drains itself instead
of accumulating for replay.

## Open Questions / findings

- **Sinks DO emit `Errored`/`Rejected` in practice (CONFIRMED).** The HTTP sink's
  `DriverResponse::event_status` returns `Errored` for transient and `Rejected`
  for permanent non-2xx; `src/sinks/http/tests.rs` asserts `BatchStatus::Rejected`.
  So the `_status` discard is a LIVE violation on any non-2xx downstream, not only
  under synthetic faults. `OrderedFinalizer<T>` already yields `(BatchStatus, T)`,
  so the discard is a call-site choice fixable purely in `spawn_finalizer`.
- **No nack/unack path exists.** `handle_pending_acknowledgements` only moves the
  ack cursor forward; true retry would mean rewinding `reader_last_record` or a
  separate retry queue — a significant redesign. The `_status` naming (leading
  underscore) reads as deliberate, with no comment explaining the rationale.
  **Needs human input:** is the fix "at minimum log + metric" or "redesign the ack
  path"? Either way, at-least-once is violated within a process lifetime under
  sustained sink errors.
- Graceful-shutdown finalizer-drain is a separate liveness concern from the same
  finalizer design (see finalizer entry in `_shelved.md`).
