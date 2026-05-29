# Property: every-written-event-eventually-delivered

## Catalog Entry

**Type:** Liveness / Sometimes (at-least-once) — `Sometimes(all_produced_delivered)`

**Property:** With end-to-end (e2e) acks enabled, every event accepted by the
source (written into the disk buffer) is eventually delivered downstream at
least once across crashes. Duplicates are allowed; silent loss is not. This is
the at-least-once contract the product sells.

**Invariant:** Let `PRODUCED` be the set of unique IDs of all events the
workload submitted to Vector's source. After faults and a quiet period
(no new events produced, no in-flight acks), every ID in `PRODUCED` must appear
at least once in `DELIVERED` (the set of IDs observed at the downstream sink).
`|DELIVERED| ≥ |PRODUCED|` is expected (duplicates from crash+replay); the
violation is any ID in `PRODUCED \ DELIVERED`.

**Antithesis Angle:**

1. Workload assigns each event a globally unique ID (e.g., a monotonic counter
   embedded in the event payload). It records every submitted ID in `PRODUCED`.
2. The downstream sink (a workload-controlled stub or a log sink with
   structured output) records every received ID in `DELIVERED`.
3. Antithesis injects faults at arbitrary timing: SIGKILL during write, during
   fsync, during read, during ack, during file deletion, during rotation.
   Vector restarts after each kill and resumes.
4. After the quiet period (workload stops producing, Vector drains to empty,
   writer is closed), the workload compares `PRODUCED` and `DELIVERED`.
5. `Sometimes(all_produced_delivered)` fires when `PRODUCED ⊆ DELIVERED` is
   reached — i.e., the system successfully delivered all events at least once
   in at least one timeline explored by Antithesis.
6. A workload-level hard assertion (`assert_always`) fires if
   `PRODUCED \ DELIVERED` is non-empty after the quiet period — this is the
   primary falsification signal.

**Why It Matters:** This is the product's headline durability guarantee. A
customer enabling disk buffers + e2e acks is explicitly opting into at-least-
once delivery. If even one event is silently dropped across a crash, the
contract is broken. Silent loss is the hardest failure mode to detect in
production (no error, no alert, dashboards may show normal throughput). Known
silent-loss paths:

- Events in the `TrackingBufWriter` 256KB in-memory buffer on crash (not yet
  page-cache flushed): in-contract loss for events not yet synced.
- Events synced to data file but not yet acked: must survive crash via replay.
  This is the primary liveness test.
- Events synced to data file but whose data file was deleted after kill but
  before ledger flush: the deleted-file-before-ledger-msync window
  (`reader.rs:546` unlink, `reader.rs:548-549` ledger flush). On restart the
  reader file ID in the ledger still points at the now-deleted file; the code
  handles this via NotFound→advance. If the events in that file were not yet
  acked, they are genuinely lost. This is the most serious latent loss path for
  this liveness property.
- `BufferWriter::drop` does not call `flush()` (`writer.rs:1371-1374`): on
  graceful shutdown that skips an explicit flush, up to 256KB is lost silently.
- Sink-error acks: `spawn_finalizer` at `ledger.rs:701-709` discards
  `_status`, so `Errored`/`Rejected` delivery still credits the ack. This
  means a downstream error causes silent loss even with e2e acks nominally on.

**Fault Requirements:** Node-termination faults (SIGKILL) required. Confirm
enabled. The following fault sequences are especially valuable:

- Kill during the `delete_completed_data_file` window (`reader.rs:546-549`):
  unlink done, ledger flush not done.
- Kill during the page-cache-write-to-fsync window (≤500ms): tests which
  events are in-contract vs. out-of-contract loss.
- Kill during file rotation: `ensure_ready_for_write` partial rotation
  (`writer.rs:1047-1154`).
- CPU throttle: stretches the 500ms window, increasing the expected-loss set.

**Workload Implementation Notes:**

The key design decision for this property is how to handle the `PRODUCED vs
DELIVERED` comparison when duplicates are expected. The recommended approach:

```
PRODUCED: Set<u64>  -- all IDs submitted to Vector source
DELIVERED: MultiSet<u64>  -- all IDs seen at downstream sink (may repeat)
DELIVERED_SET: Set<u64> = unique(DELIVERED)

-- After quiet period:
missing = PRODUCED.difference(DELIVERED_SET)
assert missing.is_empty()  // at-least-once: every produced ID must appear

-- Count duplicates (expected; used to verify replay, not assert on):
duplicate_count = |DELIVERED| - |DELIVERED_SET|
log("duplicate deliveries observed: {}", duplicate_count)
```

Dedup responsibility is at the workload level — the downstream sink deduplicates
by ID before any downstream business logic, matching the stated contract
("downstream must dedup").

**Antithesis SDK Assertions (SUT-side, to be added):**

```rust
// In handle_pending_acknowledgements, after all acks processed:
antithesis_sdk::assert_sometimes!(
    self.ledger.get_total_buffer_size() == 0,
    "buffer drained to empty after quiet period",
    json!({ "total_buffer_size": self.ledger.get_total_buffer_size() })
);

// In spawn_finalizer closure (ledger.rs:703-707), instrument the discarded status:
antithesis_sdk::assert_always!(
    matches!(status, BatchStatus::Delivered),
    "all acked events were successfully delivered (not errored/rejected)",
    json!({ "batch_status": format!("{:?}", status) })
);
// NOTE: The above will fail under sink errors — this is intentional; it surfaces
// the known silent-loss bug (INV-9 in sut-analysis.md).
```

**Workload-level milestone assertion:**

```rust
antithesis_sdk::assert_sometimes!(
    produced_set.difference(&delivered_set).next().is_none(),
    "all produced events eventually delivered (at-least-once contract satisfied)",
    json!({
        "produced_count": produced_set.len(),
        "delivered_count": delivered_set.len(),
    })
);
```

---

## Open Questions

**OQ-1: Does the topology's shutdown path call `writer.flush()` explicitly
before dropping the writer?**
`BufferWriter::drop` calls only `close()` (marks writer done + notifies). If
the topology calls `drop` without a preceding `flush()`, in-buffer events are
lost silently. This is specifically the internal config-reload incident vector (#24948). The
liveness property must be tested with both graceful shutdown (should see zero
loss) and SIGKILL (at-most-500ms-unsynced loss). If graceful shutdown also
loses events, that is a higher-severity bug.

**OQ-2: Does the `OrderedFinalizer` task drain before the tokio runtime shuts
down on SIGKILL?**
The finalizer (`ledger.rs:701-709`) is a `tokio::spawn`'d task. On SIGKILL the
entire process dies — the finalizer task does not get to drain. In-flight
`BatchNotifier` handles that have been dropped by the sink but whose IDs have
not yet propagated to `pending_acks` are lost. This means ack-in-flight events
must be replayed on restart (they were not ledger-decremented). If the reader's
`reader_last_record` was already persisted past those events (lazy ledger
flush), the events cannot be replayed — they are lost. This interaction between
the finalizer task lifecycle, the lazy ledger flush of `reader_last_record`, and
SIGKILL timing is the most subtle loss path for this property.

**OQ-3: What is the maximum number of in-flight acks at a given moment?**
The workload should size `max_size` and batch sizes to keep many events in
various ack-flight states simultaneously. A small buffer drains too quickly for
the fault injection to hit interesting timing windows.

**OQ-4: Sink-error ack discarding — is this in scope for this property?**
The `_status` discard at `ledger.rs:704` means this property as stated will
not catch sink-error loss (since the buffer always credits the ack). A separate
property specifically testing `Errored`/`Rejected` ack propagation is
recommended. For this property, use a reliable downstream sink stub that always
returns `Delivered` to avoid conflating the two bugs.

**OQ-5: Does `delete_completed_data_file` → unlink-before-ledger-flush create
a genuine loss window?**
`reader.rs:546`: `delete_file` called (unlink). `reader.rs:548`:
`increment_acked_reader_file_id`. `reader.rs:549`: `ledger.flush()` (msync).
A kill between unlink and ledger flush: on restart, `ledger.state()
.get_current_reader_file_id()` still points to the deleted file. Code path on
restart: `seek_to_next_record` fast-path (`reader.rs:840-898`) tries to
`open_mmap_readable` the file → `NotFound` → falls through to slow path. The
slow path reads from the ledger's `reader_current_data_file_id` which is still
the deleted file. Needs careful trace to confirm no events in the deleted file
are silently abandoned if they had not been fully acked.
