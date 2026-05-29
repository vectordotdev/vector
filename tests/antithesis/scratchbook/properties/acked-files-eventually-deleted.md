---
slug: acked-files-eventually-deleted
property_id: 12
type: Liveness
antithesis_assertion: Sometimes(data_file_deleted)
sut_path: lib/vector-buffers/src/variants/disk_v2/
commit: b7aae737cef5dd37d1445915443a1eb97b584f85
updated: 2026-05-28
cross_refs:
  - total-buffer-size-never-underflows   # underflow blocks deletion indirectly
  - writer-eventually-makes-progress      # deletion is the prerequisite for writer unblock
related_issues:
  - "vectordotdev/vector #21683"  # total_buffer_size underflow → permanent stall
  - "vectordotdev/vector #23456"  # flaky clean-termination test
disabled_tests:
  - "lib/vector-buffers/src/variants/disk_v2/tests/size_limits.rs::writer_waits_when_buffer_is_full (ignore = \"Needs investigation\")"
---

# Property 12: acked-files-eventually-deleted

## Invariant (informal)

Once every record in a data file has been acknowledged by the downstream sink,
the file is **eventually unlinked from the filesystem and its bytes subtracted
from `total_buffer_size`** — even if no new writes arrive. This must hold
across quiet periods and across crash-restart cycles.

## Formal Statement

**Sometimes(data_file_deleted)**: In any execution where a data file is written,
filled, and all its records are delivered and acknowledged downstream, an
`unlink(path)` of that file is observed within finite time, and
`total_buffer_size` decreases by the corresponding file size.

Equivalently as an invariant over any time-bounded window:

> For every `.dat` file F whose last record has been acked: there exists a
> future state where `filesystem::stat(F)` returns `ENOENT` and
> `ledger.total_buffer_size` has decreased by `metadata(F).len()` relative to
> the moment the last ack was processed.

---

## Dependency Chain (the full progress path for deletion)

The deletion of a data file requires **every link** of the following chain to
succeed. Breaking any single link silently stops all progress.

```
[Sink drops BatchNotifier]
        │
        ▼  (vector-common/src/finalizer.rs:FuturesOrdered::next())
[OrderedFinalizer task yields (BatchStatus, amount: u64)]
        │    ledger.rs:703-707 ── tokio::spawn loop ── stream.next().await
        │    NOTE: _status is DISCARDED here (ledger.rs:717 `let (_status, amount)`)
        ▼
[ledger.increment_pending_acks(amount)]          ledger.rs:705
[ledger.notify_writer_waiters()]                 ledger.rs:706
        │    (misleading name: wakes the *reader*, not the writer)
        ▼
[reader.next() loop wakes; calls handle_pending_acknowledgements]
        │    reader.rs:965-967
        ▼
[ledger.consume_pending_acks()]                  reader.rs:582 / ledger.rs:421
[record_acks.add_acknowledgements(consumed_acks)]  reader.rs:584
[record_acks.get_next_eligible_marker() loop]    reader.rs:586-635
        │    advances reader_last_record, accumulates bytes_acknowledged
        ▼
[data_file_acks.add_acknowledgements(records_acknowledged)]  reader.rs:633
[data_file_acks.get_next_eligible_marker() loop]  reader.rs:655-668
        │    gated by: had_eligible_records || force_check_pending_data_files
        ▼
[delete_completed_data_file(path, bytes_read)]   reader.rs:662 → reader.rs:489
        ├── [ledger.filesystem().open_file_readable(path)]  reader.rs:514-518
        │     (stat to get file size before unlink)
        ├── [metadata.len() - bytes_read → decrease_amount]  reader.rs:521-535
        │     BUG WINDOW: if metadata.len() < bytes_read → u64 underflow here (reader.rs:524)
        ├── [ledger.decrement_total_buffer_size(decrease_amount)]  reader.rs:538 / ledger.rs:291-298
        │     BUG: raw fetch_sub, no saturation (the #21683 control-path is unfixed)
        ├── [filesystem.delete_file(path)]         reader.rs:546
        │     I/O FAULT POINT: ENOSPC, EPERM, flaky disk → error propagates up
        ├── [ledger.increment_acked_reader_file_id()]  reader.rs:548 / ledger.rs:457-478
        ├── [ledger.flush()]                       reader.rs:549
        │     I/O FAULT POINT: msync failure
        └── [ledger.notify_reader_waiters()]       reader.rs:555
              (wakes the *writer*, per the inverted naming)
```

**The `force_check_pending_data_files` path** (reader.rs:1076) is the
mechanism by which deletion proceeds during a **quiet period** (no new writes).
When the reader rolls to the next data file (`roll_to_next_data_file`,
reader.rs:1075), it sets `force_check_pending_data_files = true` on the next
loop iteration. That flag bypasses the `had_eligible_records` guard at
reader.rs:651, allowing `data_file_acks.get_next_eligible_marker()` to fire
even when no new acks arrived in that iteration. Without this path, a file
whose last record was acked after the reader moved on would never be deleted
until the next record ack arrived.

---

## The Finalizer-Task-Death Scenario

The finalizer is spawned at `ledger.rs:701-710` as a detached `tokio::spawn`:

```rust
// ledger.rs:701-710
pub(super) fn spawn_finalizer(self: Arc<Self>) -> OrderedFinalizer<u64> {
    let (finalizer, mut stream) = OrderedFinalizer::new(None);
    tokio::spawn(async move {
        while let Some((_status, amount)) = stream.next().await {
            self.increment_pending_acks(amount);
            self.notify_writer_waiters();
        }
    });
    finalizer
}
```

Two observations:

1. **`_status` is silently discarded.** `BatchStatus` carries `Delivered`,
   `Errored`, or `Rejected`. The task unconditionally credits all three as
   acked events. A sink-error ack therefore advances `reader_last_record` and
   eventually triggers file deletion, removing the event from the buffer
   **without replay** (sut-analysis.md §5, INV-9 broken). This is a separate
   correctness bug, but it means the deletion path runs for all outcomes, which
   actually makes the "file deleted" observation more reachable — at the cost of
   silent loss.

2. **Task death = acks stranded.** The finalizer task holds the only consumer
   of `stream`. If the tokio task is killed (SIGKILL hitting the process),
   panics (a `BatchStatusReceiver` future panics), or the runtime is shut down
   while the task is still pending, the `OrderedFinalizer<u64>` sender
   (`finalizer`) is still alive in the reader, but the receiving task is gone.
   Subsequent calls to `finalizer.add(amount, receiver)` at reader.rs:1119
   succeed (the unbounded channel accepts messages), but nobody is consuming
   that channel. From `vector-common/src/finalizer.rs:101-107`:

   ```rust
   pub fn add(&self, entry: T, receiver: BatchStatusReceiver) {
       if let Some(sender) = &self.sender
           && let Err(error) = sender.send((receiver, entry))
       {
           error!(message = "FinalizerSet task ended prematurely.", %error);
       }
   }
   ```

   The send will only error when the receiver side of the *channel* is dropped,
   which happens when the task exits and `new_entries` (the `UnboundedReceiver`)
   is dropped. Until that point, `add()` silently succeeds while the finalizer
   task is dead. Result:
   - `pending_acks` is never incremented.
   - `notify_writer_waiters()` is never called.
   - The reader's `handle_pending_acknowledgements` loop at reader.rs:582 calls
     `ledger.consume_pending_acks()` and gets 0 every iteration.
   - `had_eligible_records` is always false.
   - `had_eligible_data_files` is always false (unless `force_check_pending_data_files`
     fires from a roll, but that only checks `data_file_acks`, which requires
     `records_acknowledged > 0` to have been accumulated, which requires
     `had_eligible_records`, which requires `consume_pending_acks() > 0`).
   - No file is ever deleted.
   - `total_buffer_size` is never decremented.
   - Writer sees `is_buffer_full()` permanently true.
   - **Permanent writer deadlock.**

   On a SIGKILL + restart, the finalizer task is recreated fresh, so this
   scenario is self-healing across restarts. But within a single process
   lifetime (e.g., the task panics due to a bug in the futures layer, or the
   tokio runtime shuts down the task before draining it during graceful
   shutdown), the pipeline stalls silently.

3. **Shutdown ordering hazard.** The sut-analysis.md §5 open question: "Does
   the finalizer task get drained by the tokio runtime before shutdown, or can
   in-flight acks be lost?" If `tokio::runtime::shutdown_timeout` fires before
   the `FuturesOrdered` inside the finalizer task drains its pending
   `BatchStatusReceiver` futures, those acks are lost. The data files are not
   deleted. On the *next* startup, `update_buffer_size` re-seeds
   `total_buffer_size` from the on-disk file sizes — potentially over-counting
   — which is the exact trigger for the #21683 underflow on subsequent reads.

---

## The `delete_completed_data_file` Underflow Window

At reader.rs:521-535:

```rust
let decrease_amount = bytes_read.map_or_else(
    || metadata.len(),
    |bytes_read| {
        let size_delta = metadata.len() - bytes_read;   // reader.rs:524
        ...
        size_delta
    },
);
```

`metadata.len()` is the on-disk file size at deletion time; `bytes_read` is the
cumulative number of bytes the reader successfully read from the file. If an
I/O fault, crash-induced partial write, or race inflated `bytes_read` above the
actual file size, `metadata.len() - bytes_read` **wraps** (both are `u64`).
`decrease_amount` becomes ≈ 2^64. The subsequent
`ledger.decrement_total_buffer_size(decrease_amount)` at reader.rs:538 calls the
raw `fetch_sub` at ledger.rs:292, wrapping `total_buffer_size` to ≈ 2^64.
Writer deadlocks permanently. This is a second trigger for the #21683 underflow
beyond the startup reconstruction path.

---

## Antithesis Experimental Design

### Target scenario

1. Configure a disk buffer with `max_buffer_size` set to exactly two data files'
   worth of records (forces the writer to block after filling two files).
2. Write enough records to fill exactly one data file. Flush. Verify file exists
   on disk and `total_buffer_size > 0`.
3. Read all records from that file. Do not yet ack.
4. **Ack all records.** The finalizer task should fire, `pending_acks` should
   be incremented, the reader loop should delete the file.
5. **Quiet period** (no new writes). Assert within a timeout that the `.dat`
   file is absent (`stat` returns `ENOENT`) and that `buffer_byte_size` metric
   gauge has dropped to 0.

### Fault injections

- **Node SIGKILL between ack and deletion** (between finalizer firing and the
  `delete_file` syscall). On restart: file should be rediscovered, reader should
  re-seek to the end, and `delete_completed_data_file` should be called again
  via the initialization path (`bytes_read = None`). Assert file eventually gone.
- **Finalizer-task kill** (simulate by pausing/killing only the finalizer goroutine,
  or by using Antithesis's process-level controls). Assert that the file is
  never deleted until the task is restored — confirming the dependency.
- **Filesystem fault on `delete_file`** (inject `EIO` or `EPERM`). The error
  propagates from `delete_completed_data_file` → `handle_pending_acknowledgements`
  → `next()` via `.context(IoSnafu)?` at reader.rs:966. The reader returns an
  error. Assert the caller (the topology) handles this gracefully and retries.
  Currently `receiver.rs` panics on reader I/O error (sut-analysis.md §8), so
  the expected behavior is a process restart.
- **CPU throttle during `should_flush`** (extend the 500ms window): the ledger
  msync after deletion may be delayed. Assert that after the throttle lifts,
  the ledger is flushed and the file is eventually absent even if it takes
  longer than normal.

### Assertions to add (SUT-side, none currently exist)

```rust
// In delete_completed_data_file, after filesystem.delete_file succeeds:
antithesis_sdk::assert_sometimes!(
    true,
    "data file was deleted after all records acked",
    &serde_json::json!({
        "data_file_path": data_file_path.to_string_lossy(),
        "bytes_read": bytes_read,
        "decrease_amount": decrease_amount,
        "total_buffer_size_after": self.ledger.get_total_buffer_size(),
    })
);

// In decrement_total_buffer_size, assert no underflow:
antithesis_sdk::assert_always!(
    amount <= self.total_buffer_size.load(Ordering::Acquire),
    "total_buffer_size decrement must not underflow",
    &serde_json::json!({ "amount": amount,
        "current": self.total_buffer_size.load(Ordering::Acquire) })
);
```

### Workload oracle

External oracle (workload container):

- After the quiet period, list all `buffer-data-*.dat` files in the buffer
  directory for the SUT. Assert that no file whose start record ID ≤ the last
  acked record ID still exists.
- Read the `buffer_byte_size` Prometheus metric. Assert it equals 0 (or matches
  the number of un-acked bytes still in flight, if any).

---

## Why This Matters

This property is the prerequisite for the writer liveness guarantee (L1,
sut-analysis.md §5). If files are not deleted, `total_buffer_size` stays
elevated. The writer's `is_buffer_full()` check at writer.rs:993-997 returns
true. `ensure_ready_for_write()` at writer.rs:1001-1020 loops forever on
`ledger.wait_for_reader()`, which will never fire because the reader is also
stuck (it has no new acks to process). **Pipeline stalls silently.** No crash,
no error log at ERROR level, dashboards may show healthy throughput (if the
pipeline has in-memory buffering upstream of the disk buffer).

The `force_check_pending_data_files` path is the only mechanism for making
progress during a quiet period. It is exercised only when the reader rolls to
the next data file. If the buffer is idle (writer done, reader waiting for
acks, no roll happening), the path is never triggered — deletion depends
entirely on `consume_pending_acks() > 0` returning true, which depends on the
finalizer task being alive and having processed the ack futures.

---

## Open Questions

1. **Does the tokio runtime drain the finalizer task before shutdown?** If
   `tokio::Runtime::shutdown_timeout` fires before the `FuturesOrdered` drains,
   in-flight acks are lost without any log or error. This is the bridge between
   "graceful shutdown" and the startup-over-seeding trigger for #21683. Needs
   investigation in the topology shutdown path.

2. **What is the `bytes_read` value passed to `delete_completed_data_file` for
   a file where the reader rolled due to a bad record (the "only partial read"
   case)?** If the reader rolled early (reader.rs:1036), `bytes_read` reflects
   only what was read before the bad record. The remainder of the file is
   charged as `size_delta = metadata.len() - bytes_read`. If a fault left the
   file larger than expected (e.g., partial write at the tail bumped the file
   size), `bytes_read` could exceed `metadata.len()`, triggering the underflow.

3. **Is `force_check_pending_data_files` sufficient for the quiet-period case
   where the reader is at the very end of the last data file and not rolling?**
   If the writer is done, the reader has read all records, all acks arrive, but
   the reader is parked in `wait_for_writer()` at reader.rs:1080 (because it
   already rolled and found an empty new file), then `notify_writer_waiters()`
   from the finalizer wakes the reader, which loops back to
   `handle_pending_acknowledgements`, which processes the acks and deletes the
   file. This appears correct, but the exact ordering of the wake → check →
   delete sequence under Antithesis scheduling pressure is worth exploring.

4. **What happens if `ledger.flush()` (the msync after delete) fails at
   reader.rs:549?** The file is already unlinked by this point (reader.rs:546
   ran first). The ledger's `reader_current_data_file` field is not yet updated.
   On restart, the reader will try to open a file that no longer exists and fall
   through the `NotFound` branch, skipping to the next file. This is the
   "handled on restart" path noted in sut-analysis.md §3. Verify under fault
   injection that the skip is handled correctly and no events are counted twice.
