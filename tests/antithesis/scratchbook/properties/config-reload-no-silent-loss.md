---
slug: config-reload-no-silent-loss
type: Safety / Always
status: missing
sut_commit: 049eec79b737450c4669b7f8aa1dd814551ec466
updated: 2026-06-02
related_issues:
  - "vectordotdev/vector#24948 (internal config-reload incident config-reload stall)"
  - "vectordotdev/vector PR#24949 (partial fix)"
related_files:
  - lib/vector-buffers/src/variants/disk_v2/writer.rs
  - lib/vector-buffers/src/variants/disk_v2/ledger.rs
  - src/topology/running.rs
  - src/topology/test/reload.rs
---

# Property: config-reload-no-silent-loss

## Catalog Entry

| Field | Value |
|---|---|
| **Type** | Safety / Always |
| **Property** | `assert_always("config_reload_no_silent_loss", "No event accepted into the disk buffer is silently dropped during or after a config reload")` |
| **Invariant** | Every event that received a successful `send` acknowledgement from the disk buffer writer before or during a config reload must either (a) be read and forwarded by the reader, or (b) be explicitly accounted in `buffer_discarded_events_total`. No event may vanish without trace. |
| **Antithesis Angle** | Trigger a config reload (SIGHUP or workload-driven) while events are streaming under sustained write load with an active reader. After reload completes and a quiet period elapses, drain the buffer and downstream sink, then assert: `events_in == events_out + events_explicitly_discarded`. |
| **Why It Matters** | This was the direct cause of the an internal config-reload incident (#24948). Silent loss on config reload defeats the entire durability guarantee disk buffers provide. |

## The Bug: `Drop` Calls `close()` But Not `flush()`

### What the code does

`BufferWriter::Drop` (writer.rs:1366-1374) calls only `self.close()`:

```
impl<T, FS> Drop for BufferWriter<T, FS> {
    fn drop(&mut self) {
        self.close();  // writer.rs:1372
    }
}
```

`close()` (writer.rs:1358-1363) calls only `ledger.mark_writer_done()` and
`ledger.notify_writer_waiters()`. It does NOT call `self.flush()` or
`self.flush_inner(true)`.

`BufferWriter::flush()` (writer.rs:1336-1340) calls `flush_inner(false)`, which
calls `writer.flush().await?` on the `TrackingBufWriter` (writer.rs:1307-1308).
`Drop` is synchronous and cannot `.await`, so this flush path is structurally
unavailable in `Drop`.

### The `TrackingBufWriter` and its 256KB buffer

`TrackingBufWriter` (writer.rs:239-343) holds an internal `Vec<u8>` with
capacity `DEFAULT_WRITE_BUFFER_SIZE` = 256 * 1024 bytes
(common.rs:37). Records are staged into this buffer and only written to the
underlying OS file handle when the buffer fills (auto-flush on overflow,
writer.rs:279-282) or when `.flush().await` is called explicitly
(writer.rs:313-331).

If fewer than 256KB of records have arrived since the last explicit flush, the
entire pending batch sits in `TrackingBufWriter::buf` at the time of `Drop`.
When `Drop` runs:

1. `close()` is called — marks the writer done in the ledger, notifies reader.
2. `TrackingBufWriter` is dropped — its `buf` is freed without ever calling
   `inner.write_all(&buf)`. The OS file handle is closed with unflushed data
   silently discarded.

This is not a crash scenario; it is the normal code path whenever the topology
tears down a sink (reload or graceful stop) without the sink having first called
`flush()`.

### When does `flush()` actually get called before `Drop`?

The normal write loop (the `BufferSender`-level `send` → writer task loop)
calls `writer.flush().await` after every successful `write_record` call
(topology/channel/sender.rs:86-98: `SenderAdapter::DiskV2` calls
`writer.flush().await`). So as long as the write loop is running, every
dispatched event is flushed before the loop proceeds.

The hazard window is events that have been **enqueued into the channel feeding
the writer task** but not yet dequeued and processed by the writer — i.e., events
sitting in the mpsc channel between the source/transform and the sink's write
loop — when the detach trigger fires. These are not lost in the buffer; they
are lost upstream.

More critically for the disk buffer specifically: if the sink task is
mid-batch (e.g., it dequeued a batch from the channel, encoded some records
into `TrackingBufWriter`, but the topology fires the `tripwire`/detach trigger
before the next `flush()` call completes), those staged bytes are dropped with
`Drop`.

Whether the topology calls `flush()` before dropping the `BufferSender`/writer
on graceful component shutdown vs. config reload is an open question (see
below), but the `Drop` impl itself provides no safety net.

### Metric drift: `track_dropped_events` charges `byte_size = 0`

When the reader later detects a gap (records present in the data file according
to the ledger but not actually written), or when the buffer is re-initialized and
`synchronize_buffer_usage` re-seeds accounting from file sizes, there is a
mismatch. More directly, if the writer calls `track_dropped_events`
(ledger.rs:553-564) for any reason during reload, the implementation explicitly
passes `byte_size = 0`:

```rust
pub fn track_dropped_events(&self, count: u64) {
    // We don't know how many bytes are represented by dropped events because we never
    // actually had a chance to read them...
    self.usage_handle
        .increment_dropped_event_count_and_byte_size(count, 0, false);  // ledger.rs:563
}
```

The comment acknowledges this is a permanent drift: the byte-size accounting
for the dropped events is zeroed, so `buffer_byte_size` gauges will be wrong
for the lifetime of the buffer instance.

### The advisory-lock gap: per-process, not per-thread

`load_or_create` (ledger.rs:600-601) uses `fslock::LockFile::try_lock()`.
POSIX `fcntl`/`flock` advisory locks are per-process on Linux: a second open
from the **same process** succeeds even if the first lock is still held. During
a config reload, if the old sink task is still running (being waited on by the
topology rebuild loop, running.rs:677-685 / 688-710) while the new sink task
tries to open the same buffer directory, both will hold "the lock" from the
OS's perspective. The topology's reload logic (`changed_disk_buffer_sinks`,
running.rs:589-601) attempts to wait for the old sink to fully shut down before
starting the new one — but this sequencing is best-effort under tokio's
cooperative scheduler and depends on the detach trigger being wired correctly.
The stall bug in #24948 was precisely a case where the detach trigger was NOT
cancelled for the changing sink, causing the wait to hang indefinitely. PR
# 24949 fixed the trigger cancellation for `changed_disk_buffer_sinks`, but the
per-process lock semantics remain — if any future code path reorders the
teardown/startup sequence, the old and new writers can open the same buffer
concurrently with no OS-level protection.

### Finalizer `Arc<Ledger>` retention during reload

`spawn_finalizer` (ledger.rs:728-737) moves an `Arc<Ledger>` clone into a
tokio task:

```rust
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

The `Arc<Ledger>` is held for as long as the spawned task lives. The task exits
only when the `OrderedFinalizer` sender side is dropped (which happens when the
`BufferWriter` is dropped and the `finalizer` field goes out of scope). However,
if the writer `Drop` runs before all in-flight `BatchNotifier`s are dropped
(i.e., events that are "in transit" in a sink's delivery pipeline), the
finalizer task remains alive past the writer's lifetime, holding a reference to
the ledger. This means the buffer directory cannot be safely fully reset until
the finalizer task drains, which happens asynchronously. In practice, the new
writer's `load_or_create` starts before this drain completes, creating a window
of overlapping ledger use.

## Antithesis Test Design

### Fault requirement

Config reload requires a **custom fault** or workload-driven trigger, not a
built-in Antithesis network/node fault. The two mechanisms available in Vector:

1. **SIGHUP** — Vector's signal handler (signal.rs:200, signal.rs:218) converts
   SIGHUP to `SignalTo::ReloadFromDisk`, which triggers
   `reload_config_from_result` (app.rs:382-386) → `topology_controller.reload()`
   → `RunningTopology::reload_config_and_respawn()`. This is the production
   reload path.
2. **Workload-driven API reload** — Vector's GraphQL/REST API (if enabled) can
   trigger a reload programmatically from the test workload container.

SIGHUP is preferable because it exercises the exact production code path.
**Flag to Antithesis team: custom signal injection capability needed.**

### Test scenario

```
Setup:
  - Vector with a source (e.g., HTTP source or socket) feeding into
    a sink with disk buffer (type: disk, when_full: block).
  - A downstream sink endpoint (HTTP mock or file sink) that records
    every event received.
  - A workload driver that:
      (a) sends a steady stream of events and tracks every event ID sent,
      (b) waits for send acknowledgement from the Vector HTTP source before
          recording the event as "accepted",
      (c) after N seconds, sends SIGHUP to Vector to trigger reload,
      (d) continues sending events through the reload,
      (e) after reload completes (detected via health endpoint), enters
          a quiet period with no new events,
      (f) waits for the downstream sink to drain (buffer empties),
      (g) asserts: accepted_event_ids == received_event_ids.

Antithesis fault injection:
  - Concurrent CPU throttle on the Vector process during reload
    (widens the window between TrackingBufWriter staging and flush).
  - Disk I/O slowdown during reload (lengthens the old sink teardown,
    increasing overlap with new sink startup).
  - Clock jitter (stretches/shrinks the 500ms fsync window).

Key assertion point:
  - assert_always("no_silent_loss_on_reload",
      accepted_count == forwarded_count + explicitly_discarded_count,
      {accepted_count, forwarded_count, explicitly_discarded_count,
       reload_timestamp})
```

### What to look for in Antithesis output

- Events with IDs in the "accepted" set that appear in neither the downstream
  sink's log nor in any `buffer_discarded_events_total` increment.
- A spike in `buffer_discarded_events_total` at reload time that does not
  correspond to a `when_full: drop_newest` event (i.e., the buffer was not
  full — the discard was from the unflushed `TrackingBufWriter` data).
- A non-zero delta in `buffer_byte_size` immediately after reload that does
  not reconcile with the byte sizes of events known to be in-flight.

## Open Questions

1. **Does the topology caller flush before dropping the writer on config reload?**
   The `SenderAdapter::DiskV2` flush path (sender.rs:86-98) is called from the
   write loop, not from teardown. The detach trigger (`tripwire`) fires to close
   the `rx.take_until_if(tripwire)` stream in the sink task (builder.rs:693).
   Once the stream closes, the sink task returns `TaskOutput::Sink(rx)` and the
   `BufferSender` (which owns the `Arc<Mutex<BufferWriter>>`) is dropped as part
   of `rx` going out of scope. There is no explicit `flush()` call at this point.
   **Verification needed: trace the drop chain from `TaskOutput::Sink(rx)` to
   `BufferWriter::drop` to confirm no flush occurs in between.**

2. **Is the per-process advisory-lock gap actually reachable under current
   topology reload sequencing?** The `changed_disk_buffer_sinks` wait
   (running.rs:688-710) awaits the old sink task completing before `buffers`
   is returned and the new pieces are built. If the new sink's `load_or_create`
   is called strictly after the old task's `await` completes and the old
   `BufferWriter` is dropped, the lock file is released before the new open.
   However: does tokio guarantee that the spawned finalizer task (which holds
   `Arc<Ledger>`) has exited before the new `load_or_create` runs? The finalizer
   runs on the tokio runtime and is not awaited during teardown — **a race
   exists if the finalizer task has not yet exited when the new writer opens.**

3. **Does the internal config-reload incident PR #24949 fully close this property?** PR #24949
   fixed the detach-trigger cancellation for `changed_disk_buffer_sinks` (the
   stall), and may have addressed some accounting. Whether it fixed the
   unflushed-`TrackingBufWriter`-data loss specifically is unclear from the
   in-repo test coverage (the existing `topology_disk_buffer_conflict` and
   `topology_disk_buffer_config_change_does_not_stall` tests do not assert
   zero event loss, only liveness/no-stall). **Antithesis can answer this
   definitively by checking the loss invariant, not just liveness.**

4. **What is the actual loss bound?** Up to 256KB (`DEFAULT_WRITE_BUFFER_SIZE`,
   common.rs:37) of staged-but-unflushed data per reload. For small events
   (e.g., 100-byte log lines), this is ~2,600 events silently dropped per reload.
   For large events near `DEFAULT_MAX_RECORD_SIZE` (128MB), the auto-flush on
   capacity overflow (writer.rs:279-282) means staging is bounded, so the loss
   per reload may be smaller in practice. The exact loss depends on the arrival
   rate and event size at the moment of the reload tripwire.

5. **Custom fault requirement flag:** The Antithesis standard fault suite
   (network partitions, node kill) does not include SIGHUP or API-driven config
   reload. This property requires either (a) the workload driver to issue
   SIGHUP/API calls on a schedule, or (b) a custom Antithesis fault that sends
   SIGHUP to the Vector process. Confirm with the Antithesis team that
   process-signal delivery is supported in the tenant configuration.

---

## SUT-Side Instrumentation (not yet committed)

The Antithesis Rust SDK is now a committed dependency under the `antithesis`
feature, and three underflow `assert_always_greater_than_or_equal_to!` detectors
exist (ledger.rs:271/313, reader.rs:529; see existing-assertions.md). None
covers the config-reload loss path. The SIGHUP config-reload fault this property
needs is, moreover, the committed harness's other Antithesis target: the
scenario's `anytime_reload.sh` swaps the two config files (`$VECTOR_CONFIG` ↔
`$VECTOR_CONFIG_ALT`, i.e. `head.yaml`/`head.b.yaml`) and sends `kill -HUP 1`
(the #24948 reload-from-disk path), so the reload trigger is already realized in
the harness — what remains uncommitted is the loss-detecting instrumentation
below.

**Where to assert `accepted == forwarded + discarded`:**

1. **`BufferWriter::close()` (writer.rs:1358)** — add an `assert_always!` that `self.unflushed_bytes == 0` at the point `close()` is called. A non-zero value means staged bytes will be silently dropped with the `TrackingBufWriter`. This is the primary loss site on config reload.

   ```rust
   antithesis_sdk::assert_always!(
       "writer_unflushed_bytes_zero_on_close",
       self.unflushed_bytes == 0,
       &serde_json::json!({
           "unflushed_bytes": self.unflushed_bytes,
           "unflushed_events": self.unflushed_events,
       })
   );
   ```

2. **`track_dropped_events` (ledger.rs:553)** — `byte_size = 0` is passed unconditionally (confirmed at ledger.rs:563). Add an `assert_always!` that `count == 0` if the drop is unexpected (i.e., not during a known `when_full: drop_newest` policy invocation), or at minimum emit an `assert_reachable!` so that Antithesis confirms the site is exercised during a reload test and the count can be tracked.

3. **Workload-level** — after reload quiet period, assert:

   ```
   assert_always("config_reload_no_silent_loss",
       accepted_count == forwarded_count + explicitly_discarded_count,
       { accepted_count, forwarded_count, explicitly_discarded_count })
   ```

   where `accepted_count` is events acknowledged by the Vector HTTP source before reload, `forwarded_count` is events received by the downstream mock sink, and `explicitly_discarded_count` is the delta in `buffer_discarded_events_total` during the reload window.

---

### Investigation Log

#### Does PR #24949 fix the loss or only the stall?

**Examined:** `src/topology/running.rs:589–710` (config-reload path, `changed_disk_buffer_sinks`, detach/remove-inputs logic), `lib/vector-buffers/src/variants/disk_v2/writer.rs:1358–1374` (`close()` and `Drop`).

**Found:** PR #24949 (referenced in related_issues as "partial fix") addressed the stall in #24948 by fixing detach-trigger cancellation for `changed_disk_buffer_sinks` — specifically the sequencing of old-sink teardown so the topology does not hang indefinitely waiting for a sink that never receives its detach signal. The `changed_disk_buffer_sinks` path at running.rs:629–668 detaches inputs from changed sinks and waits for the old sink task to complete at running.rs:656–668. This fix ensures the old task actually terminates.

**Not found:** Any change to `BufferWriter::Drop` (writer.rs:1366–1374) that calls `flush()` before `close()`. `Drop` at this commit still calls only `self.close()` (writer.rs:1372), which calls only `ledger.mark_writer_done()` and `ledger.notify_writer_waiters()` — no `flush_inner` call. The `TrackingBufWriter` internal buffer is freed without flushing if any staged bytes remain at drop time. No code added by PR #24949 (or observable at this commit) closes the unflushed-`TrackingBufWriter` loss path.

**Conclusion:** PR #24949 fixed the stall (detach-trigger cancellation) and partially addressed accounting. The `Drop`-without-flush silent loss path is not fixed at this commit (049eec79b). Whether PR #24949 also added a pre-drop `flush()` call in the sink task teardown path (rather than in `Drop` itself) could not be confirmed without reviewing the PR diff directly — the topology teardown code at running.rs:656–668 awaits the old task completing, but does not call `writer.flush()` explicitly from that site. Loss is still possible for any bytes staged in `TrackingBufWriter` at the moment the write loop receives the tripwire signal and exits without a final flush.
