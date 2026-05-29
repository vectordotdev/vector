---
slug: graceful-shutdown-flushes-all
type: Liveness / Sometimes(graceful_shutdown_lossless)
status: missing
sut_commit: 049eec79b737450c4669b7f8aa1dd814551ec466
updated: 2026-06-02
related_issues:
  - "vectordotdev/vector#24948 (config-reload stall, partially overlapping concern)"
  - "lib/vector-buffers/src/variants/disk_v2/mod.rs (design doc: 'graceful shutdown flushes everything → no loss')"
  - "antithesis/scratchbook/sut-analysis.md §5 INV-6"
related_files:
  - lib/vector-buffers/src/variants/disk_v2/writer.rs
  - lib/vector-buffers/src/variants/disk_v2/ledger.rs
  - lib/vector-buffers/src/topology/channel/sender.rs
  - src/topology/builder.rs
  - src/topology/running.rs
---

# Property: graceful-shutdown-flushes-all

## Catalog Entry

| Field | Value |
|---|---|
| **Type** | Liveness / Sometimes(graceful_shutdown_lossless) |
| **Property** | `assert_sometimes("graceful_shutdown_lossless", "Vector completes a graceful shutdown without losing any buffered events that were accepted before shutdown began")` |
| **Invariant** | On a graceful stop (SIGTERM, not SIGKILL), all events that were written into the disk buffer and acknowledged at the source level must survive to the downstream sink. No event that is durable (i.e., fsync'd to disk) may be lost; no event that was accepted but only in the 256KB `TrackingBufWriter` page-cache stage may be silently dropped. The buffer spec (mod.rs design doc, `_external-references-digest.md`) claims "graceful shutdown flushes everything → no loss." |
| **Antithesis Angle** | Send graceful stop (SIGTERM/`vector stop`) under sustained write load; restart Vector; assert all pre-shutdown accepted events are present at the downstream sink. Compare with the ungraceful-crash (SIGKILL) property where the 500ms fsync window is the known loss bound. |
| **Why It Matters** | The disk buffer is sold as "no data loss on graceful shutdown." If this is violated, the entire durability value proposition fails for the most common operational scenario (planned restarts, deploys, scaling events). |

## The Core Question: Does `flush()` Get Called Before `Drop`?

### What `Drop` does (and does not do)

`BufferWriter::Drop` (writer.rs:1366-1374):

```rust
impl<T, FS> Drop for BufferWriter<T, FS> {
    fn drop(&mut self) {
        self.close();  // writer.rs:1372
    }
}
```

`close()` (writer.rs:1358-1363) only calls `ledger.mark_writer_done()` and
`ledger.notify_writer_waiters()`. It does NOT call `flush()` or
`flush_inner(true)`. The `BufferWriter::flush()` method is `async`
(writer.rs:1336-1340), and `Drop` is synchronous, so flush-on-drop is
structurally impossible in the current design.

This means: if any data is staged in the `TrackingBufWriter`'s 256KB internal
buffer (writer.rs:239-251, capacity = `DEFAULT_WRITE_BUFFER_SIZE` = 256 * 1024,
common.rs:37) at the moment `BufferWriter` is dropped, that data is lost
silently. The OS closes the file handle without ever calling `write_all` on the
buffered bytes.

### The claimed "graceful shutdown flushes" path — does it exist?

The external references digest and the mod.rs design doc assert: "Graceful
shutdown flushes everything → no loss." This claim must come from somewhere in
the shutdown code path calling `writer.flush()` before the writer is dropped.

Tracing the shutdown path:

1. `RunningTopology::stop()` (running.rs:145) sends the shutdown signal to all
   sources via `shutdown_coordinator.shutdown_all(deadline)` (running.rs:259).

2. Sources stop producing events. The channel feeding the sink drains naturally.

3. The sink task runs until its input stream is exhausted (the stream closes
   because all sender halves are dropped as the source/transform tasks finish).

4. The sink's async function (builder.rs:666-704) returns
   `TaskOutput::Sink(rx)` when `sink.run(...)` completes.

5. `rx` is a `Utilization`-wrapped `BufferReceiverStream`. The `BufferSender`
   (which holds `Arc<Mutex<BufferWriter>>`) is NOT held by `rx` — the sender
   side is held by the upstream component's output channel (`self.inputs` in
   `RunningTopology`, running.rs). The inputs are dropped when
   `RunningTopology::stop()` drops `self` (running.rs:145 takes ownership of
   `self`, moving `detach_triggers` and `tasks` into the returned future).

6. When does the `BufferSender` (and thus `Arc<Mutex<BufferWriter>>`) get
   dropped? It is stored in `self.inputs: HashMap<ComponentKey, BufferSender<EventArray>>`
   (running.rs). When `stop()` moves `self`, `inputs` is dropped as part of
   `RunningTopology`. But `stop()` moves only `self.tasks` and
   `self.source_tasks` into the returned future; `self.inputs` (and other
   fields) are dropped synchronously when `stop()` is called.

   **Critical ordering question:** Does `self.inputs` (containing the
   `BufferSender`) get dropped before the tasks finish? If yes, the
   `Arc<Mutex<BufferWriter>>`'s reference count drops to zero before the sink
   task has drained the reader side — meaning `BufferWriter::drop` runs while
   events are still being processed. If no, the `Arc` outlives the tasks
   somehow.

   Looking at running.rs:145-267: `stop()` takes `self` by value. The fields
   that are NOT moved into the returned future are dropped at the end of the
   `stop()` function body before it returns the future. `self.inputs` is one
   such field. The `tasks` future only polls the already-spawned JoinHandles;
   `inputs` is gone. So: **`inputs` (and the `BufferSender`) are dropped when
   `stop()` returns the future, which is before the future is polled/completed.**

   This means `BufferWriter` is dropped (and `Drop::drop` calls `close()` but
   not `flush()`) potentially while the sink task is still running its final
   drain. However, because the sink task holds `rx` (the reader side), not the
   writer side, this may be acceptable for the reader but means any
   still-staged writes are lost.

7. The write loop (`SenderAdapter::DiskV2` flush path, sender.rs:86-98) calls
   `writer.flush().await` after every successful `write_record` via the
   `send` → `write_record` → `flush` cycle. So every event that was
   successfully processed by the write loop has been flushed to the OS page
   cache (and possibly fsync'd to disk if 500ms elapsed). The residual in
   `TrackingBufWriter` at shutdown time would only be from a batch that was
   encoded but for which the async flush had not yet been awaited.

   Under graceful shutdown: the source stops, the mpsc channel closes, the
   write loop's `next()` returns `None` and the loop exits. At that point,
   the last call to `send` should have already triggered a `flush`. If the
   write loop always calls `flush` after each `write_record`, and the source
   sent its last event, and `flush` was awaited successfully, then
   `TrackingBufWriter` should be empty when the loop exits and the
   `BufferSender` is dropped.

   **But:** is there a race between the write loop's final `flush` completing
   and the drop of `inputs`? The write loop runs in an async task. The
   `inputs` drop happens when `stop()` is called (synchronously, before the
   future is polled). If `stop()` is called before the write loop task
   completes its final `flush`, the `BufferWriter` can be dropped mid-flush.

### The `should_flush` / 500ms fsync gate

Even if `flush()` is called (moving data from `TrackingBufWriter` to the OS
page cache), the full fsync (`sync_all`) + ledger msync is only done if
`should_flush()` returns true (writer.rs:1312), which requires ≥500ms since
the last full flush. On graceful shutdown, if the last periodic fsync was
recent, the OS-page-cache data is present and readable (Linux guarantees
this), but is not fsync'd to disk. If the process exits gracefully (normal
process exit), the OS will flush page cache to disk on process exit. So for
graceful shutdown (not SIGKILL), the OS page cache flush on process exit likely
covers this gap. **But this is OS-behavior-dependent, not Vector-guaranteed.**

For SIGKILL: the 500ms window is a documented known data-loss bound.
For graceful stop: Vector relies on OS process-exit page-cache flush, not on
an explicit `sync_all` before exit. This is an implicit guarantee that could
break under certain OS configurations or if the process is OOM-killed after
receiving SIGTERM.

### The `flush_inner(force_full_flush=true)` path

`flush_inner(force_full_flush=true)` (writer.rs:1299-1321) is called during
file rotation (writer.rs:1041) to force a full fsync. It is NOT called on
graceful shutdown through any currently observable code path. The normal
`flush()` → `flush_inner(false)` path skips `sync_all` if the 500ms gate
hasn't fired.

## Contrast with the SIGKILL / Crash Property

| Scenario | Expected loss bound | Mechanism |
|---|---|---|
| SIGKILL / abrupt crash | Up to 500ms of page-cache writes | fsync only every 500ms |
| Graceful stop (SIGTERM) | Claimed: zero | Depends on final flush being called before Drop |
| Config reload | Up to 256KB of TrackingBufWriter data | Drop calls close() not flush() |

The Antithesis property for graceful shutdown specifically tests the "claimed:
zero" row. It is a **liveness / sometimes** property (not always, because
Antithesis needs to observe at least one graceful shutdown completing
successfully to confirm the claim is reachable — hence `Sometimes`).

## Antithesis Test Design

### Test scenario

```
Setup:
  - Vector with a source feeding into a sink with disk buffer.
  - A downstream HTTP mock sink that records every event received and
    its sequence ID.
  - A workload driver that:
      (a) sends events with sequence IDs, tracking every ID sent and
          every ID acknowledged by the Vector HTTP source,
      (b) after N events, sends SIGTERM to Vector (graceful stop),
      (c) polls the Vector health endpoint until it stops responding
          (shutdown complete),
      (d) restarts Vector with the same configuration,
      (e) waits for buffer drain (reader catches up on restart),
      (f) asserts: acknowledged_ids == downstream_received_ids.

Antithesis fault injection:
  - CPU throttle during shutdown (slow the final flush race).
  - Disk I/O slowdown during shutdown (test whether fsync completes
    before process exit).
  - Clock jitter (shrink/expand the 500ms fsync window relative to
    shutdown timing).

Key assertion:
  - assert_sometimes("graceful_shutdown_lossless",
      acked_before_sigterm_count == downstream_count,
      {acked_count, downstream_count, flush_completed, sigterm_timestamp})

Secondary assertion (contrast):
  - assert_always("graceful_beats_crash",
      graceful_shutdown_loss_count <= crash_loss_count)
    (Graceful shutdown must never lose more than a crash, and should
     ideally lose zero.)
```

### What to look for in Antithesis output

- Any event with a sequence ID in the "acknowledged before SIGTERM" set that
  does not appear in the downstream sink after drain.
- Whether `buffer_discarded_events_total` or `component_discarded_events_total`
  increments during shutdown (indicating the buffer itself is accounting for
  the drop) — vs. a completely silent drop (no counter increments, event just
  gone).
- Whether the 500ms fsync window creates any detectable loss on graceful
  shutdown when CPU/IO throttling slows the shutdown sequence.

### Instrumentation to add (not yet committed)

The SDK is now wired and the three #21683 underflow detectors are present (see
`existing-assertions.md`); the two assertions below are additional and not yet
committed.

Add to `BufferWriter::close()` (writer.rs:1358) or to the point where the
write loop exits cleanly:

```rust
antithesis_sdk::assert_always!(
    "writer_unflushed_bytes_zero_on_close",
    self.unflushed_bytes == 0,
    &json!({
        "unflushed_bytes": self.unflushed_bytes,
        "unflushed_events": self.unflushed_events,
    })
);
```

This assertion fires if `close()` is called while the `TrackingBufWriter`
still has staged bytes, directly surfacing the bug.

Add at the workload level after drain completes:

```rust
antithesis_sdk::assert_sometimes!(
    "graceful_shutdown_lossless",
    acked_count == downstream_count,
    &json!({
        "acked_count": acked_count,
        "downstream_count": downstream_count,
    })
);
```

## Open Questions

1. **Does `stop()` call flush before dropping `inputs`?** This is the central
   unknown. Reading running.rs:145-267: `stop()` moves `self` fields into the
   returned future selectively. `inputs` appears to be dropped synchronously.
   If `inputs` is dropped before the write loop task finishes its final `flush`,
   the `BufferWriter` is dropped with staged data. **Verification required:
   add a tracing log at `TrackingBufWriter::drop` showing `buf.len()` and
   confirm it is 0 on graceful shutdown.**

2. **Does the tokio runtime drain all tasks before the process exits?** If
   `stop()` is awaited and all task JoinHandles complete, then all async work
   (including the write loop's final flush) is done before `stop()` returns.
   But the `inputs` `HashMap` is dropped inside `stop()` before the future is
   returned, creating the race described above. The question is whether the
   drop order within `stop()` puts the write-loop task completion before the
   `inputs` drop or after.

3. **Does the OS page-cache flush on process exit cover the gap?** For graceful
   shutdown (clean process exit after SIGTERM), the Linux kernel flushes dirty
   page cache on process exit. This would cover the case where the 500ms
   fsync window has not yet fired but data is in the page cache. However, if
   the process is forcefully killed (OOM killer, watchdog) after receiving
   SIGTERM but before flushing, the page-cache data is lost. This scenario
   is intermediate between graceful and crash and is not currently tested.

4. **Does the finalizer task complete before the buffer is re-opened on
   restart?** On graceful shutdown + restart, the finalizer task
   (ledger.rs:728-737, spawned as a tokio task holding `Arc<Ledger>`) must
   exit before the new process opens the same buffer directory. Since the
   finalizer task exits when the `OrderedFinalizer` sender side is dropped
   (which happens when `BufferWriter` is dropped), and the old process
   completes before the new one starts, this is safe for restart — but
   not for in-process config reload (see config-reload-no-silent-loss.md).

5. **`topology_disk_buffer_flushes_on_idle` test (src/topology/test/mod.rs:822):**
   This test confirms events are readable before shutdown fires, but it
   explicitly stops the topology only **after** receiving both copies of the
   event (line 870: `topology.stop().await`). It does not test whether the
   stop itself would have flushed pending events. It is not a loss test.

6. **Relationship to INV-6 (sut-analysis.md §5):** INV-6 states "graceful
   shutdown flushes (but see §10 — `BufferWriter::Drop` does NOT flush)."
   This property operationalizes that open question into a testable Antithesis
   `Sometimes` assertion.

---

### Investigation Log

#### Does Vector topology call `writer.flush()` before dropping the writer (`running.rs` `stop()` drop-order)?

**Examined:** `src/topology/running.rs:145–268` (`stop()` body), `lib/vector-buffers/src/variants/disk_v2/writer.rs:1358–1374` (`close()` and `Drop`).

**Found:** `stop()` at running.rs:145 takes `self` by value. It moves `self.tasks` and `self.source_tasks` into the `wait_handles` / `check_handles` futures at lines 157–161, and drops `self.inputs` (the `HashMap<ComponentKey, BufferSender<EventArray>>`) implicitly when `stop()` returns the future at line 267. Because `inputs` is not moved into the returned future, it is dropped synchronously when `stop()` is called — before the future is awaited, and therefore before the JoinHandles in `wait_handles` complete. The `BufferSender` (which holds `Arc<Mutex<BufferWriter>>`) lives in `inputs`; when `inputs` drops, the `Arc` reference count decrements. If this is the last reference, `BufferWriter::drop` fires, calling only `close()` (writer.rs:1372), not `flush()`.

**Found — write-loop flush behavior:** The write loop (`SenderAdapter::DiskV2` path in sender.rs) calls `writer.flush().await` after each `write_record`. Under graceful shutdown, the source stops producing events, the mpsc channel feeding the write loop drains, the loop's `next()` returns `None`, and the loop exits. If the loop called `flush()` after the last record before exiting, `TrackingBufWriter` should be empty at the time of drop. The race is: does the `inputs` drop (which drops `BufferSender` → `BufferWriter`) occur before or after the write-loop task's final `flush().await` completes?

**Not found:** An explicit `flush()` call in the topology teardown path between the write-loop task completing and `BufferWriter::drop`. The write loop runs in a spawned async task; `inputs` is dropped synchronously in `stop()` before the future is polled. If the write-loop task has not yet returned when `stop()` discards `inputs`, the `Arc<Mutex<BufferWriter>>` may still have a reference held by the write-loop task — in which case `BufferWriter::drop` fires when the write-loop task finally exits, not when `inputs` drops. Whether `TrackingBufWriter` is empty at that point depends on whether the write loop's final `flush()` completed before the task returned.

**Conclusion:** The race is unresolved by code inspection alone. The most likely safe scenario is: source stops → mpsc channel closes → write loop's final `flush()` completes → write-loop task returns → `Arc` reference drops → `BufferWriter::drop` fires with empty `TrackingBufWriter`. But if `inputs` drops the `Arc` before the write-loop task returns and it was the last reference, `Drop` fires early. Verification requires either adding a `buf.len()` trace at `TrackingBufWriter::drop` and observing it under graceful shutdown, or reading the write-loop task's exact `Arc` lifetime. This is flagged as an unresolved race requiring code tracing or an instrumented test.
