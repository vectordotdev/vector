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

`BufferWriter::Drop` calls only `self.close()`. `close()` calls only
`ledger.mark_writer_done()` and `ledger.notify_writer_waiters()` — NOT
`self.flush()` / `flush_inner(true)`. `flush()` routes to `flush_inner(false)` →
`writer.flush().await?` on the `TrackingBufWriter`; `Drop` is synchronous and
cannot `.await`, so this flush is structurally unavailable in `Drop`.

### The `TrackingBufWriter` 256KB buffer

`TrackingBufWriter` holds a `Vec<u8>` of capacity `DEFAULT_WRITE_BUFFER_SIZE` =
256 * 1024 (common.rs). Records stage there and reach the OS file handle only on
buffer-fill auto-flush or explicit `.flush().await`.

With fewer than 256KB staged since the last flush, the whole pending batch sits in
`TrackingBufWriter::buf` at `Drop`. On `Drop`: `close()` marks the writer done and
notifies the reader, then `TrackingBufWriter` drops — `buf` freed without
`inner.write_all(&buf)`, OS handle closed, unflushed data silently discarded. Not
a crash — the normal path whenever the topology tears down a sink (reload or
graceful stop) without a prior `flush()`.

### When does `flush()` run before `Drop`?

The write loop calls `writer.flush().await` after every successful `write_record`
(`SenderAdapter::DiskV2`, topology/channel/sender.rs). While the loop runs, every
dispatched event flushes before the loop proceeds.

The hazard window: the sink task is mid-batch (dequeued a batch, encoded some
records into `TrackingBufWriter`) when the topology fires the `tripwire`/detach
trigger before the next `flush()` completes — those staged bytes drop. Whether the
topology calls `flush()` before dropping the writer on graceful shutdown vs.
reload is an open question (below); the `Drop` impl provides no safety net. (Events
still in the mpsc channel feeding the writer task are lost upstream, not in the
buffer.)

### Metric drift: `track_dropped_events` charges `byte_size = 0`

If the writer calls `track_dropped_events` during reload (or the reader detects a
ledger/data gap, or `synchronize_buffer_usage` re-seeds from file sizes), it passes
`byte_size = 0`:

```rust
pub fn track_dropped_events(&self, count: u64) {
    // We don't know how many bytes are represented by dropped events because we never
    // actually had a chance to read them...
    self.usage_handle
        .increment_dropped_event_count_and_byte_size(count, 0, false);
}
```

The zeroed byte-size is permanent drift — `buffer_byte_size` gauges stay wrong for
the buffer instance lifetime.

### The advisory-lock gap: per-process, not per-thread

`load_or_create` uses `fslock::LockFile::try_lock()`. POSIX `fcntl`/`flock`
advisory locks are per-process on Linux — a second open from the **same process**
succeeds while the first lock is held. On reload, if the old sink task still runs
(awaited by the topology rebuild loop) while the new sink opens the same buffer
dir, both hold "the lock". `changed_disk_buffer_sinks` waits for the old sink to
shut down first, but this is best-effort under tokio's cooperative scheduler and
depends on the detach trigger being wired correctly. #24948 was exactly an
uncancelled detach trigger hanging the wait. PR #24949 fixed trigger cancellation
for `changed_disk_buffer_sinks`, but the per-process lock semantics remain — any
future reorder of teardown/startup lets old and new writers open the buffer
concurrently with no OS-level protection.

### Finalizer `Arc<Ledger>` retention during reload

`spawn_finalizer` moves an `Arc<Ledger>` into a detached task that exits only when
the `OrderedFinalizer` sender drops (i.e. the writer drops). If writer `Drop` runs
before all in-flight `BatchNotifier`s drop, the finalizer outlives the writer
holding the ledger, so the new writer's `load_or_create` can start before the old
finalizer drains — overlapping ledger use on top of the per-process-lock gap.
(Finalizer detail in `_shelved.md`.)

## Antithesis Test Design and committed harness

The reload trigger is ALREADY committed: the scenario's `anytime_reload.sh` swaps
the two config files (`$VECTOR_CONFIG` ↔ `$VECTOR_CONFIG_ALT`, i.e.
`head.yaml`/`head.b.yaml`) and sends `kill -HUP 1` — Vector's signal handler maps
SIGHUP to `ReloadFromDisk` → `reload_config_and_respawn`, the exact #24948
production path. Amplify with CPU throttle (widens the staging-to-flush window),
disk-IO slowdown (lengthens old-sink teardown overlap), and clock jitter.

Workload oracle: track event ids acknowledged by the source before/through the
reload, drain after a quiet period, and assert
`accepted == forwarded + explicitly_discarded` (the discarded term being the
`buffer_discarded_events_total` delta in the reload window). Look for accepted ids
that appear in neither the downstream log nor any discard-counter increment, and
for a reload-time discard spike with the buffer NOT full (i.e. unflushed
`TrackingBufWriter` data, not a `drop_newest` drop).

SUT-side: the committed underflow detectors do not cover this loss path; the
loss-detecting asserts are uncommitted. The sharpest SUT site is an
`assert_always!(unflushed_bytes == 0)` in `BufferWriter::close()` — a non-zero
value at close means staged bytes are about to be dropped. The verdict is the
workload oracle above.

## Open Questions / findings

- **PR #24949 fixed the STALL, not the loss (CONFIRMED).** It fixed detach-trigger
  cancellation for `changed_disk_buffer_sinks` so old-sink teardown no longer
  hangs. It did NOT add a pre-drop `flush()`: `Drop` still calls only `close()`,
  and the teardown path (`running.rs` awaits the old task) calls no `writer.flush()`.
  Existing topology reload tests assert liveness, not zero loss.
- **Loss bound:** up to 256 KiB (`DEFAULT_WRITE_BUFFER_SIZE`) staged-unflushed per
  reload — ~2,600 100-byte events. Large events auto-flush on overflow, bounding
  their loss smaller.
- **Per-process advisory-lock race still latent.** Reload waits for the old task
  before the new `load_or_create`, but the detached finalizer holding `Arc<Ledger>`
  is not awaited — if still alive when the new writer opens, both share the dir
  with no OS-level protection.
- **Fault flag:** SIGHUP via the committed `anytime_reload.sh` (`kill -HUP 1`);
  confirm the tenant permits process-signal delivery / in-container script exec.
