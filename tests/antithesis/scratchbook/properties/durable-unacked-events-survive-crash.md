# Property: durable-unacked-events-survive-crash

## Catalog Entry

**Type:** Safety / Always

**Property:** Every event that was durably synced (fsync'd) to a data file and
not yet acknowledged is readable after an ungraceful crash + restart. No synced
event is skipped or lost by recovery. Data-loss is bounded to the documented
≤500ms unsynced window.

**Invariant:** Let `S` be the set of events whose writes were confirmed durable
by a completed `sync_all` call (data file fsync) and a completed ledger `flush`
(msync) before the kill signal. After restart and full recovery
(`from_config_inner` returns), draining the buffer to the quiet-period boundary
must yield every event in `S` at least once. No event in `S` may be silently
absent.

**Antithesis Angle:**

1. Workload writes events with monotonically increasing unique IDs. It only
   marks an event as "durably written" (adds its ID to the expected-set `S`)
   after receiving an application-level confirmation that an fsync window has
   closed — concretely, after the workload observes that an e2e ack has come
   back for an event written ≥500ms ago (demonstrating that at least one fsync
   cycle has elapsed and the data is committed), OR the workload uses an
   out-of-band fsync signal (see Open Question OQ-1).
2. Antithesis injects a SIGKILL at an arbitrary point — before, during, or
   after the 500ms fsync window.
3. Vector restarts. The workload waits for the quiet period (no new events
   produced, buffer drains to empty).
4. The workload computes `S_delivered` (set of IDs that came out downstream)
   and asserts `S ⊆ S_delivered` (at-least-once; duplicates are allowed and
   expected).
5. Events written within the 500ms unsynced window before the kill are
   explicitly excluded from `S` — their loss is within the documented contract.

**Why It Matters:** This is the core durability promise marketed to customers:
"data synchronized to disk will not be lost if Vector crashes." If a synced
event is lost silently during recovery, the product's fundamental safety
guarantee is violated. The 500ms window is documented; what is not documented
(and not currently tested) is the possibility that even synced data is lost due
to:

- A crash between the data `sync_all` and the ledger `msync` (two separate
  non-atomic syscalls: `writer.rs:1314` `writer.sync_all()` then
  `ledger.rs:534-535` `self.state.get_backing_ref().flush()`), leaving the
  ledger lagging the data — handled by `validate_last_write` `Ordering::Less`
  path (`writer.rs:922-944`), which fast-forwards the ledger. If this path has
  a bug, synced data silently disappears.
- The `validate_last_write` `Ordering::Greater` path (`writer.rs:910-919`)
  logs "Events have likely been lost" and rolls to the next file — the correct
  detection path, but the roll-over leaves an intentional gap that should not
  include synced data.
- `update_buffer_size` at startup (`ledger.rs:680-724`) sums `.dat` file sizes
  and seeds `total_buffer_size`. If this overseeds relative to what
  `seek_to_next_record` will decrement, the underflow path (#21683) can be
  triggered and the writer deadlocks, causing zero more events to be delivered
  — indistinguishable from loss from the caller's perspective.

**Crash Windows (code-precise):**

| Window | Code location | Risk |
|--------|--------------|------|
| Write committed to page cache, `sync_all` not yet called | `writer.rs:1308` `writer.flush()` succeeded; `writer.rs:1314` `writer.sync_all()` not reached | Loss is expected and in-contract (≤500ms) |
| `sync_all` done, ledger `flush` not done | `writer.rs:1314` done; `ledger.rs:534` not done | Ledger lags data → `Ordering::Less` recovery path fast-forwards; synced data must survive |
| Ledger `flush` done; file-rotation increment not yet done | `ledger.rs:534` done; `writer.rs:1138` `increment_writer_file_id` not reached | File ID in ledger still points to old file; recovery opens the same file; data must be re-readable |
| Kill inside `delete_completed_data_file` | `reader.rs:557` unlink done; `reader.rs:559` `increment_acked_reader_file_id` and `ledger.flush` not done | Ledger still points at a deleted file; handled by `NotFound`→skip on restart; relevant unacked events in that file survive on the next undeleted file or are in-ack-flight |

**Recovery Branches Exercised:**

- `validate_last_write` `Ordering::Less` (ledger lags data): `writer.rs:922-944`
  — fast-forwards `next_record_id` using `increment_next_writer_record_id`.
- `validate_last_write` `Ordering::Greater` (data lags ledger): `writer.rs:910-919`
  — emits error log, sets `should_skip_to_next_file = true`.
- `seek_to_next_record` fast-path (different reader/writer file IDs):
  `reader.rs:851-907` — mmap-validates last record of each reader file and
  deletes already-acked files.
- `seek_to_next_record` slow-path: `reader.rs:915-953` — reads records via
  `next()` until `last_reader_record_id` matches ledger.

**Fault Requirements:** Node-termination faults (SIGKILL) required. These are
often disabled by default in Antithesis tenants — confirm enabled. CPU
throttling (stretching the 500ms window beyond its nominal boundary) is a
useful secondary lever.

**Antithesis SDK Assertion (SUT-side):**

The Antithesis SDK is a committed dependency under the `antithesis` feature, and three `assert_always_greater_than_or_equal_to!` underflow detectors already ship (ledger.rs:271, ledger.rs:313, reader.rs:529 — see `existing-assertions.md`). None of them covers the durable-survives-crash paths, so the assertions below are genuine still-to-add suggestions:

```rust
// In validate_last_write, after Ordering::Less fast-forward:
antithesis_sdk::assert_always!(
    record_next >= ledger_next,
    "validate_last_write: fast-forwarded ledger to data file; no synced data lost",
    json!({ "ledger_next": ledger_next, "record_next": record_next })
);

// In seek_to_next_record, after returning Ok(()):
antithesis_sdk::assert_reachable!(
    "seek_to_next_record completed after crash recovery"
);
```

**Workload-level set-difference check:**
The workload maintains a set `DURABLE` (event IDs confirmed synced before
kill) and a set `DELIVERED` (IDs received downstream post-restart). After
quiet period: assert `DURABLE.difference(DELIVERED).is_empty()`. Duplicates
from `DELIVERED` that are not in `DURABLE` are acceptable (at-least-once).

---

## Open Questions

**OQ-1 (Critical): How does the workload establish "durably written" without
access to Vector internals?**
The 500ms fsync window makes it hard for an external workload to know which
events were synced before the kill. Options:

- (a) Rely on e2e acks: an event is considered durable if it was acked by the
  downstream sink before the kill (i.e., it completed the full
  write→read→deliver→ack→delete cycle). This is conservative but correct.
- (b) Instrument Vector to emit a structured log line after each `sync_all`
  completes (noting the `writer_next_record_id` at that point), and have the
  workload parse it to determine the durable frontier. More precise.
- (c) Use a configurable `flush_interval=0` (force fsync on every flush) so
  every write is immediately durable; then all written events are in `S` and
  only the final partial write before the kill is excluded. Cleanest but
  changes the production code path.
Option (c) is recommended for initial testing; option (b) for production-
representative timing.

**OQ-2: Does `validate_last_write` `Ordering::Greater` path always correctly
skip to a clean state, or can it produce a gap that overlaps synced records?**
The `Ordering::Greater` case emits an error log and sets `should_skip =
true` (`writer.rs:983-986`). This causes `reset()` + `mark_for_skip()` and
defers the actual skip to the next `ensure_ready_for_write`. If the writer
rolls to the next file and there are valid synced records in the old file that
the reader hasn't yet read, are those records still accessible? They should be
(the old file is not deleted), but the gap in `writer_next_record_id` means
the reader might interpret them as a monotonicity violation — verify against
`reader.rs:482` `MonotonicityViolation` panic.

**OQ-3: Is the data `sync_all` and ledger `msync` ordered (data first, ledger
second) or can they be reordered under the Tokio executor?**
`flush_inner` at `writer.rs:1314` calls `writer.sync_all().await` then
`self.ledger.flush()` (synchronous msync). The async await creates a yield
point between them. The Antithesis scheduler can exploit this yield to inject a
kill between the two, which is the exact scenario we want. Confirmed this is
reachable.

**OQ-4: Does `BufferWriter::drop` (`writer.rs:1371-1374`) call `flush()` before
`close()`?**
Reading the source: `Drop::drop` only calls `self.close()` (which marks
`writer_done` and notifies) — it does NOT call `flush()`. On graceful topology
shutdown the caller is expected to call `flush()` first. If it does not, up to
256KB of `TrackingBufWriter` data plus any unsynced page-cache data can be lost
even on clean shutdown. This is the silent-loss-on-config-reload vector (#24948).
For this property, it means the "graceful shutdown is fully lossless" claim is
only true if the topology's shutdown path calls `writer.flush()` explicitly
before dropping — confirm.
