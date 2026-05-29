---
slug: overflow-chain-no-unaccounted-gap
type: Safety / Always
sut_path: lib/vector-buffers/src/variants/disk_v2/
commit: 049eec79b737450c4669b7f8aa1dd814551ec466
updated: 2026-06-02
---

# Property: overflow-chain-no-unaccounted-gap

## Catalog Entry

**Type:** Safety / Always

**Property:** When `WhenFull::Overflow` is configured with a disk buffer as
the base and an in-memory buffer as the overflow, a crash during an
overflow-active period does not create a silent middle gap in the delivered
event stream. Either (a) all events from both base and overflow are accounted
for (delivered or explicitly loss-reported), or (b) if overflow events are
lost on crash, the loss is reported via the existing accounting path
(`increment_dropped_event_count_and_byte_size`), not silently dropped. Event
ordering guarantees must also be honored or explicitly documented as absent.

**Invariant:** `Always`: after a crash-during-overflow-active and subsequent
drain, the set of delivered event IDs has no unaccounted gap relative to the
produced set. Specifically: if event ID N was produced, placed on disk (base),
and survived the crash, and event ID M > N was produced before the crash and
placed in the overflow (in-memory), M may be lost but N must not be silently
reclassified as lost. The delivered set does not "skip over" durable disk
events due to overflow-induced reordering confusion.

**Antithesis Angle:** Configure a topology with `disk` base buffer +
`WhenFull::Overflow` pointing to an in-memory buffer. Fill the base to
capacity to trigger overflow. Crash while overflow is active (events in both
buffers). Restart and drain. Assert: (1) no silent middle gap — events known
to be on disk before the crash are present in the drain; (2) the receiver-side
unbiased `select!` does not deliver a LATER overflow event as if it were an
EARLIER disk event (reordering), violating monotonicity assumptions.

**Why It Matters:** The overflow configuration is entirely uncovered in the
existing test suite. The asymmetry between base (durable disk) and overflow
(ephemeral in-memory) creates a unique crash shape: EARLIER events survive on
disk, LATER events are in the overflow and lost. This is not a simple
"duplicates at the tail" scenario — it is a gap in the middle of the
chronological stream. Downstream dedup logic typically assumes at-least-once
(duplicate tails are fine) and does not account for middle gaps. An
unaccounted gap means the downstream consumer permanently misses events that
the source believed were accepted.

Additionally, the unbiased `tokio::select!` in the receiver means that even
during non-crash steady state, events from overflow can interleave with events
from the disk base, breaking ordering.

---

## Code Verification

### Overflow dispatch on send (sender.rs:236-244)

```rust
// lib/vector-buffers/src/topology/channel/sender.rs:236-244
WhenFull::Overflow => {
    if let Some(item) = self.base.try_send(item).await? {
        was_dropped = true;
        self.overflow
            .as_mut()
            .unwrap_or_else(|| unreachable!("overflow must exist"))
            .send(item, send_reference)
            .await?;
    }
}
```

When the base buffer is full (`try_send` returns `Some(item)` = the item was
not accepted), the item is forwarded to the overflow buffer. The `was_dropped`
flag is set, which triggers the instrumentation path
(`increment_dropped_event_count_and_byte_size`) — but this counts the item as
"dropped from the base" for backpressure purposes, NOT as silently lost.

### Unbiased `select!` in receiver (receiver.rs:133-138)

```rust
// lib/vector-buffers/src/topology/channel/receiver.rs:133-138
Some(mut overflow) => {
    select! {
        Some(item) = overflow.next() => (item, false),
        Some(item) = self.base.next() => (item, true),
        else => return None,
    }
}
```

`tokio::select!` with no `biased` keyword uses pseudo-random branch selection.
When both the overflow and base receivers have items ready simultaneously,
either can be selected. This means:

- A LATER event (placed in overflow after the base was full) can be delivered
  before an EARLIER event (already on disk in the base).
- After a crash: the overflow in-memory buffer is gone; the base disk buffer
  retains events up to the crash point. On restart, only the base is drained.
  But during the pre-crash period, the delivery order was already interleaved.

### Crash asymmetry: disk base survives, in-memory overflow does not

The disk base (`ReceiverAdapter::DiskV2`) stores events in `buffer-data-N.dat`
files, fsync'd per the `flush_interval` model. These survive a crash (subject
to the ≤500ms loss window).

The in-memory overflow (`ReceiverAdapter::InMemory`, backed by a
`LimitedReceiver<T>` / `LimitedSender<T>` channel) holds events only in heap
memory. A crash (SIGKILL) loses all in-memory channel contents with no
recovery path and no on-disk trace.

### `WhenFull::Overflow` topology configuration

`WhenFull::Overflow` is wired in `BufferSender::with_overflow`
(sender.rs:158-170):

```rust
// sender.rs:158-170
pub fn with_overflow(base: SenderAdapter<T>, overflow: BufferSender<T>) -> Self {
    Self {
        base,
        overflow: Some(overflow),
        when_full: WhenFull::Overflow,
        ...
    }
}
```

The overflow `BufferSender` is a recursive structure — it may itself have an
`overflow`, enabling chained overflow. For this property, the relevant case is
a disk base + in-memory overflow (the standard two-level chain).

---

## Crash Scenario Walkthrough

1. Source produces events E1…EN sequentially. E1…EK fit in the disk base
   buffer and are accepted; E(K+1)…EN overflow to the in-memory buffer.
2. The disk base receives `sync_all` for E1…EJ (J ≤ K, the last fsync
   boundary). E(J+1)…EK are page-cached but not yet fsync'd.
3. Crash (SIGKILL). In-memory overflow (E(K+1)…EN) is lost. Page-cached
   E(J+1)…EK may also be lost (within the ≤500ms durability window).
4. On restart, Vector opens the disk base. `validate_last_write` recovers to
   E1…EJ (or possibly EK if the page cache was flushed by the OS on kill).
   The overflow buffer is not reopened — it has no recovery path.
5. Drain: E1…EJ are delivered. E(J+1)…EN are never delivered.

**Gap shape:** E(J+1)…EN is a suffix gap (standard crash loss). This is
expected and documented.

**Non-obvious gap shape (the property target):** Consider a second, subtler
scenario where the source numbers events globally. If E1…EJ are on disk and
E(K+1)…EN are in overflow, and the drain delivers only E1…EJ, a workload
that expects *all IDs from 1 to J* to be present may observe a gap at
E(K+1)…EN — but since those IDs were never durably written, this is expected.

The *actual* risk this property guards against is:

- A bug in the `select!` dispatch that causes the receiver to skip a disk
  event (treating it as consumed) because an overflow event arrived
  simultaneously.
- A bug in the overflow sender that miscounts `total_buffer_size` on the
  base after an overflow-and-drain cycle, triggering the underflow deadlock
  (cross-ref `total-buffer-size-never-underflows`).
- Silent loss from the instrumentation path: when an overflow item is sent,
  `was_dropped=true` on the base, and
  `increment_dropped_event_count_and_byte_size` is called. But after the
  overflow send succeeds, the item is NOT lost — it is in the overflow. The
  base-side "drop" counter is misleading and may be misread by operators as
  data loss.

---

## SUT-Side Instrumentation (not yet committed — the SDK is wired and the three #21683 underflow asserts are present; these are additional)

The Antithesis SDK is a committed dependency under the `antithesis` feature, and
three underflow `assert_always_greater_than_or_equal_to!` detectors exist
(ledger.rs:271/313, reader.rs:529; see existing-assertions.md). None covers the
overflow chain, so the assertions below remain genuine still-to-add suggestions.

### Assertion 1 — Reachability: overflow path is exercised

```rust
// sender.rs, inside the WhenFull::Overflow arm, after overflow.send() succeeds
antithesis_sdk::assert_reachable!(
    "overflow-chain: item dispatched to overflow buffer",
    &serde_json::json!({
        "base_was_full": true,
        "overflow_buffer_type": "in-memory",  // or determined dynamically
    })
);
```

### Assertion 2 — Always: no durable disk event is skipped post-drain

This is a workload-level assertion. The workload assigns sequential IDs to
produced events and tracks which IDs were accepted into the base disk buffer
(via a confirmation callback or a secondary log channel). After drain:

```rust
// workload-side, post-drain
antithesis_sdk::assert_always!(
    delivered_ids.contains_all(&durable_ids),
    "overflow-chain: all durable disk events delivered after crash",
    &serde_json::json!({
        "durable_count": durable_ids.len(),
        "delivered_count": delivered_ids.len(),
        "missing_ids": durable_ids.difference(&delivered_ids).collect::<Vec<_>>(),
    })
);
```

### Assertion 3 — Always: base-side drop counter matches overflow dispatches

The `increment_dropped_event_count_and_byte_size` call on the base side when
`WhenFull::Overflow` fires should equal the number of items forwarded to the
overflow buffer (not items permanently lost). This is a metric-accuracy
assertion, not a data-loss assertion.

---

## Why Existing Tests Cannot Catch This

- The model-based proptest (`tests/model/`) does not configure
  `WhenFull::Overflow`. All test runs use a single buffer level.
- No integration test exercises the disk-base + in-memory-overflow topology.
- The internal chaos test (SIGKILL ×3) uses a single-level disk buffer.
- The `select!` reordering risk is only present when both branches are
  simultaneously ready — this is a timing/interleaving sensitivity that unit
  tests with synchronous scheduling cannot explore.

---

## Requires a Second Topology Config

Testing this property requires a Vector topology configured as:

```yaml
# config/antithesis-overflow-test.yaml
sinks:
  my_sink:
    type: blackhole  # or mock sink
    inputs: [my_transform]
    buffer:
      type: disk
      max_size: 268435488  # 256MB minimum
      when_full: overflow  # triggers the overflow chain
```

The overflow buffer (in-memory) is automatically allocated when
`when_full: overflow` is set. This is distinct from the standard
single-level harness config and must be a separate test scenario.

---

## Open Questions

- Does `WhenFull::Overflow` actually chain to a second independently-sized
  in-memory buffer, or does it overflow to the same topology channel? Verify
  that `BufferSender::with_overflow` is called with a separate
  `LimitedSender`/`LimitedReceiver` pair and that this second buffer has its
  own capacity configuration.

- The `was_dropped = true` flag in the overflow arm (sender.rs:238) triggers
  `increment_dropped_event_count_and_byte_size` even though the item is NOT
  dropped — it is forwarded to the overflow. Is this intentional? Does the
  instrumentation path distinguish "dispatched to overflow" from "permanently
  dropped"? If not, this is a metrics-accuracy bug independent of the crash
  scenario.

- After a crash, when the topology restarts, is the overflow `BufferSender`
  recreated with an empty in-memory buffer? If so, there is no re-delivery of
  overflow events — confirmed expected behavior, but worth asserting explicitly
  in the harness.

- Is the unbiased `select!` in receiver.rs:133 intentional (the comment at
  lines 120-124 explains the rationale: avoid stalling the base while draining
  overflow), or is strict ordering expected? If ordering is not guaranteed, the
  documentation should say so explicitly. This affects whether reordering is a
  "violation" or a "known trade-off."

- What happens to `total_buffer_size` on the base when the overflow is active?
  If the base is "full" (the item was rejected via `try_send`), the base's
  `total_buffer_size` remains at `max_buffer_size`. When the overflow drains
  and the reader acks base events, can the combined accounting get confused
  about which buffer's bytes are being freed? This is a potential secondary
  trigger for the `total-buffer-size-never-underflows` bug.
