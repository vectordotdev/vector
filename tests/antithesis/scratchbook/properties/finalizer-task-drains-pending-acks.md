---
slug: finalizer-task-drains-pending-acks
catalog_category: 4 — Space Reclamation & Clean Termination
type: Liveness / Sometimes(all_acks_drained)
status: cataloged (Category 7)
related:
  - acked-files-eventually-deleted
  - reader-drains-and-terminates-cleanly
  - every-written-event-eventually-delivered
  - writer-eventually-makes-progress
  - total-buffer-size-never-underflows
commit: b7aae737cef5dd37d1445915443a1eb97b584f85
---

### finalizer-task-drains-pending-acks — Finalizer Task Drains All In-Flight Acks Before Exit

| | |
|---|---|
| **Type** | Liveness |
| **Property** | All in-flight `BatchNotifier` acknowledgements are eventually processed by the finalizer task and reflected in `pending_acks` — both (a) during steady-state operation after a quiet period, and (b) on graceful shutdown before the process exits. Acks are never permanently stranded in the `FuturesOrdered` queue of the finalizer task without being consumed by the `Arc<Ledger>` ack machinery. |
| **Invariant** | `Sometimes(all_acks_drained)`: after a write-then-ack-then-quiet-period sequence, `pending_acks` reflects all delivered acks and `total_buffer_size` has been decremented correctly; no acks are left un-processed in the finalizer stream. Pair with an `Unreachable` assertion on "finalizer task died while acks were still pending" for a sharper fault-injection signal. |
| **Antithesis Angle** | (1) **SIGKILL with acks in flight**: write records, begin ack flow, SIGKILL the process mid-drain; assert on restart that events are replayed (ack progress was not persisted) rather than silently stranded. (2) **Finalizer task panic**: inject a panic into the `tokio::spawn` body (`ledger.rs:703`); assert that the reader detects the dead finalizer and takes a recovery action rather than hanging indefinitely. (3) **Graceful shutdown ordering**: SIGTERM with acks in flight; assert `pending_acks == 0` and `total_buffer_size == 0` (after full drain) before the process exits; replay check after restart. |
| **Why It Matters** | The finalizer task is an **unmonitored, detached** `tokio::spawn` that holds the only reference capable of calling `increment_pending_acks` and `notify_writer_waiters`. If it panics, is killed, or is not drained before shutdown, the entire write-read-ack-delete chain breaks: `pending_acks` never advances → `handle_pending_acknowledgements` never fires → no file deletion → `total_buffer_size` never decremented → eventual writer deadlock. Additionally, events already delivered to the sink but not yet acked in the buffer are silently lost (no replay on restart) — a distinct silent-loss path from the arithmetic underflow. |

---

## What Led to This Property

The `spawn_finalizer` function (`ledger.rs:701–710`) spawns a detached tokio
task that acts as the bridge between sink delivery and buffer accounting:

```rust
pub(super) fn spawn_finalizer(self: Arc<Self>) -> OrderedFinalizer<u64> {
    let (finalizer, mut stream) = OrderedFinalizer::new(None);  // ledger.rs:702
    tokio::spawn(async move {                                     // ledger.rs:703
        while let Some((_status, amount)) = stream.next().await { // ledger.rs:717
            self.increment_pending_acks(amount);                   // ledger.rs:705
            self.notify_writer_waiters();                          // ledger.rs:706
        }
    });
    finalizer
}
```

Key observations:

1. **The task is detached**: `tokio::spawn` returns a `JoinHandle` that is
   immediately discarded (no `let _ = tokio::spawn(...)` or any join/abort
   handle stored). There is no supervision: if the task panics or the runtime
   drops it without draining, the caller has no way to detect this.

2. **The `_status` discard**: the matched `BatchStatus` is explicitly ignored
   (`_status` at `ledger.rs:717`). Every ack — regardless of whether it is
   `Delivered`, `Errored`, or `Rejected` — increments `pending_acks` by
   `amount`. This is the sink-failure silent-loss bug documented in
   `sink-failure-not-silently-acked`. For this property, what matters is that
   acks that **do** flow through the task are counted; the discard is a
   correctness issue, not a liveness issue.

3. **The `stream` and its drain semantics**: `OrderedFinalizer::new(None)`
   creates a `FinalizerSet` plus a `BoxStream<'static, (BatchStatus, u64)>`.
   The stream is driven by the `finalizer_stream` function (`finalizer.rs:114–167`).
   Its `None`-from-`new_entries` branch (`finalizer.rs:150–151`) breaks the
   main loop and falls into the drain-loop at `finalizer.rs:159–161`:

   ```rust
   while let Some((status, entry)) = status_receivers.next().await {
       yield (status, entry);
   }
   ```

   The drain loop runs only if the tokio task is still alive and being polled.
   The `sender` half of the `FinalizerSet`'s internal channel is held by the
   `OrderedFinalizer` handle (`finalizer.rs:49`), which is stored in
   `BufferReader` (`reader.rs:411`). When `BufferReader` is dropped, the
   `sender` drops, the channel closes, the task sees `None`, the drain loop
   runs, and the stream terminates — **in theory**.

4. **SIGKILL cuts the drain**: on SIGKILL, the entire tokio runtime is
   terminated without running any Drop or drain logic. In-flight
   `BatchNotifier`s that have been detached from their events (event delivered
   downstream, notifier dropped by the sink) but whose `Future` in the
   `FuturesOrdered` has not yet been polled to completion are lost. On restart,
   those events are not in `pending_acks`, so `handle_pending_acknowledgements`
   never advances the reader position past them, and the file containing them
   is not deleted. Because `total_buffer_size` is re-seeded from file sizes at
   startup (not from persisted ack state), the file's bytes are counted again
   in the seed, and those records **will be re-read and re-delivered** — which
   is the correct at-least-once behavior. The stranded-ack scenario therefore
   results in **duplicate delivery, not silent loss**, as long as the reader
   re-reads those records correctly.

   However: there is a **silent-loss path** if a record was read and a
   `BatchNotifier` was attached (`reader.rs:1117–1119`), the event was
   delivered to the sink, the `BatchNotifier` was dropped, the `BatchStatusReceiver`
   future completed in the `FuturesOrdered` — but the task loop had not yet
   called `stream.next()` to yield it. At SIGKILL, this completed future is
   in the `FuturesOrdered` but unpolled. On restart, `total_buffer_size` is
   re-seeded from file sizes, so the file appears to still be present. But
   `reader_last_record` (persisted in the ledger) reflects the reader's acked
   position from the last `ledger.flush()` call, which may lag behind the
   in-memory ack state. If `reader_last_record` was flushed at a position
   before those records, the reader will re-read them (safe replay). If
   `reader_last_record` was flushed at or beyond those records (unlikely given
   the lazy flush interval), they are skipped. The exact outcome depends on
   the flush timing — this is the interleaving Antithesis is designed to explore.

5. **Graceful shutdown ordering**: `FinalizerSet` has no `Drop` impl
   (`finalizer.rs` — no `impl Drop for FinalizerSet` found). The drain of
   in-flight acks on graceful shutdown depends on:
   (a) Vector's topology shutdown calling `writer.close()` (which marks the
       writer done via `ledger.mark_writer_done()`, `writer.rs:1359`), followed
       by the read loop draining, followed by `BufferReader` drop, which drops
       the `OrderedFinalizer`, which closes the internal channel, which triggers
       the `finalizer_stream` drain loop, which the tokio task must be polled
       to complete.
   (b) The tokio runtime not shutting down before the finalizer task completes
       its drain. `tokio::runtime::Runtime::drop` waits for all spawned tasks
       to complete by default only with `block_on` semantics; a detached spawn
       may be abandoned if the runtime shuts down before the task is polled
       through the drain.
   This is the "does the runtime drain finalizer tasks before shutdown" open
   question from `sut-analysis.md §Open Questions`.

---

## Code References

| Location | Relevance |
|---|---|
| `lib/vector-buffers/src/variants/disk_v2/ledger.rs:701–710` | `spawn_finalizer`: detached `tokio::spawn`, `_status` discard, no join handle retained |
| `lib/vector-buffers/src/variants/disk_v2/ledger.rs:717` | `_status` discard — `BatchStatus` ignored; every ack counted as `Delivered` |
| `lib/vector-buffers/src/variants/disk_v2/ledger.rs:415–416` | `increment_pending_acks`: `fetch_add` on `pending_acks` — the only path that unblocks the reader ack machinery |
| `lib/vector-buffers/src/variants/disk_v2/ledger.rs:381` | `notify_writer_waiters`: wakes the reader, which then advances acks and frees files |
| `lib/vector-buffers/src/variants/disk_v2/mod.rs:262–264` | `spawn_finalizer` call site: `Arc<Ledger>` cloned, `finalizer` passed to `BufferReader` |
| `lib/vector-buffers/src/variants/disk_v2/reader.rs:411` | `finalizer: OrderedFinalizer<u64>` — stored in `BufferReader`; drop triggers sender close |
| `lib/vector-buffers/src/variants/disk_v2/reader.rs:1117–1119` | `BatchNotifier::new_with_receiver` + `finalizer.add(record_events.get(), receiver)` — ack registration per record |
| `lib/vector-common/src/finalizer.rs:70–82` | `OrderedFinalizer::new(None)`: creates sender+stream; `shutdown=None` means drain-only-on-channel-close |
| `lib/vector-common/src/finalizer.rs:114–167` | `finalizer_stream`: main loop (`None` branch breaks at L151) + drain loop (L159–161) |
| `lib/vector-common/src/finalizer.rs:48–52` | `FinalizerSet`: no `Drop` impl — drain is implicit via sender-drop, not explicit |
| `lib/vector-buffers/src/variants/disk_v2/writer.rs:1358–1363` | `close()`: marks writer done + wakes reader; called from `Drop` (`writer.rs:1371–1373`) |

---

## What Breaks

**Path A — SIGKILL with acks in `FuturesOrdered` but unpolled:**
The tokio runtime is killed. In-flight `BatchStatusReceiver` futures that
completed (event delivered, notifier dropped) but whose completion has not yet
been polled by `finalizer_stream` are lost. `pending_acks` on the next startup
starts at 0. `total_buffer_size` is re-seeded from file sizes. The reader seeks
to `reader_last_record` (persisted). Records after the acked position are
re-read. **Result: duplicate delivery** (at-least-once holds, but duplicates
are generated from records that were already delivered downstream). This is the
expected and correct behavior for SIGKILL.

**Path B — Finalizer task panic:**
If the `tokio::spawn` body panics (e.g., due to a bug in the ack counting
arithmetic or in `notify_writer_waiters`), the task terminates. The
`OrderedFinalizer` handle in `BufferReader` is still alive; `add` calls
(`reader.rs:1119`) attempt to send on the internal channel. With the task gone,
the receiver is dropped, and `sender.send(...)` returns `Err`. The `add` method
logs `error!(message = "FinalizerSet task ended prematurely.", %error)`
(`finalizer.rs:105`) but does not return an error or signal the reader. The
reader continues reading and attaching notifiers; those notifiers complete when
events are delivered, but `increment_pending_acks` is never called.
`handle_pending_acknowledgements` in the reader receives 0 acks. No file is
ever deleted. `total_buffer_size` never decrements. **Result: no file deletion,
eventual writer deadlock** — same user-visible manifestation as #21683, but
with a different trigger path. The reader continues making forward progress
until the buffer fills; then the writer stalls.

**Path C — Graceful shutdown with acks in flight:**
Topology shuts down; `BufferWriter` is dropped, `close()` is called. The
reader's drain loop runs. At some point `BufferReader` is dropped, dropping
the `OrderedFinalizer`, closing the internal channel. The `finalizer_stream`
in the tokio task sees `None`, breaks its main loop, and enters the drain loop
(`finalizer.rs:159–161`). If the tokio runtime shuts down (e.g., `Runtime::drop`)
before the drain loop completes, in-flight acks are abandoned.
**Result: records that were delivered downstream but not yet counted in
`pending_acks` are not reflected in `reader_last_record` at shutdown. On
restart, they are replayed. Duplicates, not silent loss.** But if
`reader_last_record` was flushed past those records (because the reader's ack
machinery had already advanced the position before the finalizer drained), those
records would be skipped on restart — silent loss.

**Path D — Steady-state liveness:**
Under heavy load, if the finalizer task falls behind (e.g., CPU throttling,
the `FuturesOrdered` queue grows faster than it drains), `pending_acks` is
not incremented promptly. The reader's `handle_pending_acknowledgements` loop
does not fire. File deletions are delayed. The writer, if blocked full, waits
longer for the reader to free a file. **Result: head-of-line blocking,
throughput degradation, eventual writer stall under sustained CPU pressure.**
This is not a permanent failure but a performance degradation that becomes
permanent if the CPU throttle is extreme enough.

---

## Fault Conditions

1. **SIGKILL with acks in flight** — requires node-termination fault. Flag if
   disabled in the Antithesis tenant.

2. **Finalizer task panic** — can be injected via a custom Antithesis fault that
   sends a panic to the spawned task, or by adding a fault-injection point in
   the task body. Does not require node-kill.

3. **CPU throttling** — available as an Antithesis resource fault. Slows the
   finalizer task relative to the reader, growing the `FuturesOrdered` queue
   and extending ack latency.

4. **Shutdown ordering race** — requires Antithesis to explore the interleaving
   between the topology shutdown path and the finalizer drain. No special fault
   primitive needed; Antithesis will explore these orderings automatically.

---

## OrderedFinalizer Drop/Drain Semantics Summary

Based on `finalizer.rs`:

- `FinalizerSet::new(shutdown: Option<ShutdownSignal>)` with `None` (as used
  at `ledger.rs:702`): the stream terminates only when the `sender` is dropped.
- **The drain loop at `finalizer.rs:159–161` runs only if the tokio task is
  scheduled** after the sender closes. There is no blocking synchronous drain
  — it is an async drain that depends on the runtime continuing to poll the
  task.
- **`FinalizerSet` has no `Drop` impl** — it performs no synchronous drain on
  drop. Dropping the `OrderedFinalizer` only closes the channel; the actual
  drain is async and may not complete before the runtime exits.
- The `flush` method (`finalizer.rs:109–111`) calls `flush.notify_one()` which
  drops all pending `status_receivers` (`finalizer.rs:131–133`). This is a
  discard path, not a drain path. It would abandon in-flight acks — relevant if
  called during shutdown.

---

## Missing SUT Instrumentation

No Antithesis SDK assertions exist. Needed:

1. **Finalizer task health check**: an `Unreachable` assertion inside the
   `add` method's `Err` branch (`finalizer.rs:104–106`) would fire when the
   finalizer task terminates prematurely. This is already signaled by the
   `error!` log, but an SDK assertion makes it an explicit test failure.

2. **`Sometimes` drain completion assertion**: at the point where the
   `finalizer_stream` drain loop exits (`finalizer.rs:161`), an SDK
   `Sometimes(drain_completed)` assertion confirms the drain ran to completion
   at least once per test run.

3. **`pending_acks` ground-truth assertion**: at `ledger.rs:416` (after
   `increment_pending_acks`), an `Always(pending_acks > 0)` assertion would
   confirm that the ack is actually registered before `notify_writer_waiters`
   wakes the reader. This catches the race where `notify_writer_waiters` fires
   but `pending_acks` is 0 due to the ack being dropped.

4. **Workload-level replay detection**: after SIGKILL with known-delivered
   events, the workload asserts that on restart those events are re-delivered
   exactly once (duplicate, not silent loss). No SUT modification needed for
   this path.

---

## Open Questions

- Does the tokio runtime's shutdown sequence (`Runtime::drop` or
  `Runtime::shutdown_background`) drain spawned tasks before exiting, or does
  it abandon them? The answer determines whether graceful-shutdown acks are
  reliably processed. `(partial: tokio's default`Runtime::drop` gives spawned
  tasks a 2-second grace period to complete; if the drain takes longer — e.g.,
  many unresolved `BatchStatusReceiver`futures — some acks may be abandoned)`

- Is there any mechanism in Vector's topology shutdown that explicitly joins or
  awaits the finalizer task? If `spawn_finalizer` stored the `JoinHandle` and
  the topology awaited it during shutdown, graceful-shutdown acks would be
  reliable. Currently the handle is discarded.

- If the finalizer task panics (path B above), the `error!` at `finalizer.rs:105`
  fires on every subsequent `add` call. Does the Vector metrics/alerting
  infrastructure surface this log as an actionable signal, or is it buried? An
  operator would not naturally look for this log as the signal for a deadlock-in-progress.

- Can the finalizer task grow its `FuturesOrdered` queue without bound under
  a stalled sink that never delivers/acks? If so, the queue is a memory leak
  vector under backpressure — distinct from the write-path `max_buffer_size`
  guard.

- Is `OrderedFinalizer::new(None)` the correct call for the disk buffer's
  finalizer, given that `None` means "no shutdown signal"? If a `ShutdownSignal`
  were passed, the stream would terminate immediately on shutdown (discarding
  in-flight acks per `finalizer.rs:65–68`), which is worse. But with `None`,
  the drain depends on the sender being dropped, which is implicit and
  ordering-sensitive. Neither option provides a synchronous, guaranteed drain —
  this is a structural gap.

---

### Investigation Log

#### Does the tokio runtime drain spawned tasks before exit?

**Examined:** `lib/vector-buffers/src/variants/disk_v2/ledger.rs:701–710` (`spawn_finalizer`), `lib/vector-common/src/finalizer.rs:114–167` (`finalizer_stream`, drain loop at lines 159–161).

**Found:** `spawn_finalizer` at ledger.rs:703 calls `tokio::spawn(async move { ... })` and discards the returned `JoinHandle` — no handle is stored and no join/abort is called anywhere in the codebase. The finalizer task is fully detached. The drain loop in `finalizer_stream` at finalizer.rs:159–161 (`while let Some((status, entry)) = status_receivers.next().await { yield (status, entry); }`) runs only after the `new_entries` channel closes (when the `OrderedFinalizer` sender, held in `BufferReader`, is dropped). This drain is async and depends on the tokio runtime continuing to poll the task. There is no explicit join on the finalizer task handle — neither in `spawn_finalizer` nor in any topology shutdown path.

**Found — ~2s window:** Tokio's `Runtime::drop` (the default `block_on`/drop path) does not have a documented fixed timeout for completing spawned tasks. The evidence file's claim of "~2s window" was sourced from prior analysis of tokio internals and the Vector topology graceful-shutdown deadline configuration; it is not a hard tokio guarantee. Tokio's shutdown does allow tasks to run to completion *if* the runtime is shut down via `Runtime::shutdown_timeout` or similar — but `Runtime::drop` without an explicit shutdown_timeout may abandon incomplete tasks immediately on drop. The exact behavior depends on how the tokio runtime is constructed and whether `shutdown_timeout` is configured in Vector's main binary.

**Not found:** No explicit `join`, `await`, or `abort` on the finalizer task's `JoinHandle` at any site in `lib/vector-buffers/`. No `impl Drop for FinalizerSet` at finalizer.rs. No `ShutdownSignal` passed to `OrderedFinalizer::new(None)` at ledger.rs:702 that would give the stream an explicit termination signal.

**Conclusion:** The tokio runtime does not provide a guaranteed synchronous drain of the finalizer task before exit. The ~2s window is a best-effort estimate based on topology shutdown configuration, not a hard guarantee from tokio. In-flight acks at the time of process exit or runtime drop may be abandoned, resulting in duplicate delivery on restart (correct at-least-once) rather than silent loss — provided `reader_last_record` has not been flushed past those records.
