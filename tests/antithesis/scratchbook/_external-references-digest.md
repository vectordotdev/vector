# External References Digest (working note for discovery agents)

This is scaffolding for the antithesis-research run on **disk buffers v2**
(`lib/vector-buffers/src/variants/disk_v2/`). User scope answer: *"Whatever you
have access to. You have your MCPs."* — so in-repo docs/RFCs plus Datadog
internal doc/Jira were consulted. Key findings condensed below so per-focus agents
don't need to re-fetch.

## In-repo references

- `rfcs/2021-10-14-9477-buffer-improvements.md` — original buffer-rework RFC.
- `docs/specs/buffer.md` — buffer component spec / claimed behavior.
- `lib/vector-buffers/src/variants/disk_v2/mod.rs` — authoritative design doc
  (module-level comment): on-disk format, ledger, record IDs, recovery.

## Claimed guarantees (from `mod.rs` design doc + buffer spec + internal doc)

- Data files never exceed 128MB; ≤ 65,536 files; buffer ≤ ~8TB.
- All records checksummed with **CRC32C**; records written
  sequentially/contiguously; a record never spans two data files.
- Writers create+write data files; readers read+delete them. Reader deletes a
  data file **only after all records in it are acknowledged** (whole-file
  deletion, never partial truncation).
- Ledger (`buffer.db`, memory-mapped) tracks `writer_next_record_id`,
  `writer_current_data_file_id`, `reader_current_data_file_id`,
  `reader_last_record_id`. Fields updated atomically, but **not** atomically
  w.r.t. reader/writer activity.
- Record IDs are monotonic and encode event count: record ID N with next record
  M means the record holds M−N events. Used to compute buffer event count and to
  detect gaps / dropped events after corruption.
- **Durability:** data is fsync'd every **500ms** (`DEFAULT_FLUSH_INTERVAL`).
  Page-cache flush happens on every `flush()` (readers see data immediately on
  Linux); full fsync only every 500ms. **Data-loss window on crash = up to 500ms
  of unsynced writes** (when e2e acks off). Graceful shutdown flushes everything
  → no loss.
- Min buffer `max_size` ~256MB; `DEFAULT_MAX_DATA_FILE_SIZE` 128MB;
  `DEFAULT_MAX_RECORD_SIZE` = 128MB; `DEFAULT_WRITE_BUFFER_SIZE` 256KB.
- Endianness: files are host-endian; not portable across architectures.
- Delivery semantics with e2e acks + disk buffer = **at-least-once**: crash after
  buffer write but before downstream ack → replay on restart → **possible
  duplicates** (downstream must dedup).

## Known bugs / incidents (HIGH-VALUE Antithesis targets)

1. **Ledger `total_buffer_size` AtomicU64 underflow → permanent writer deadlock**
   (Vector #21683, partially mitigated by PR #23561 on the *reporter* side only;
   the ledger atomic still wraps).
   - `decrement_total_buffer_size` (ledger.rs, `fetch_sub` at ledger.rs:319) does
     raw `fetch_sub(amount, AcqRel)` with **no saturation**. If `amount >
     current_value`, the atomic wraps to ≈ 2^64. Under the `antithesis` feature
     this site now carries a committed `assert_always_greater_than_or_equal_to!(total_buffer_size, amount)`
     detector at ledger.rs:313 — it reports the wrap, it does not prevent the subtraction.
   - Then `total_buffer_size + unflushed_bytes` is always astronomical →
     `is_buffer_full()` returns true forever → `can_write_record()` false forever
     → writer's `ensure_ready_for_write()` (writer.rs ~1001-1020) loops on
     `ledger.wait_for_reader().await` and never recovers. **Writer deadlocks
     permanently.**
   - Trigger: crash/reboot/abrupt-shutdown that leaves a data file whose on-disk
     size and readable-record bytes disagree, combined with the reader running
     through that file on restart. Partial writes at file-rotation boundaries are
     the most plausible cause. Not deterministic per-restart, but not exotic.
   - Reporter-side gauges use `saturating_sub` (PR #23561) so the *dashboard*
     no longer shows 2^64, but the ledger control-path atomic is unfixed.

2. **Disk buffer stall + silent event drops during config reload**
   (Vector #24948, PR #24949; directly implicated in the **internal config-reload incident non-prod
   incident**).
   - Old writer dropped while events still in-flight → events lost without
     accounting.
   - `track_dropped_events` passes `0` for `byte_size` → permanent drift in
     buffer-size metrics.
   - `synchronize_buffer_usage()` re-seeds metrics while the old reporter may
     still run → double-counted metric spikes; then a metrics gap between old
     reporter teardown and the first tick (2s) of the new reporter.

3. **`component_discarded_events_total` blind to buffer drops** (Vector #24606,
   #24144). When a disk buffer fills and `drop_newest` fires, only
   `buffer_discarded_events_total` increments; the component-level discarded
   counter stays 0 → silent data loss on dashboards. `BufferEventsDropped::emit()`
   in `lib/vector-buffers/src/internal_events.rs` never calls
   `ComponentEventsDropped`.

4. **Buffer size gauges stuck non-zero / negative** (Vector #23995, #17666,
   #21683). Reporter `current() = total_entered.saturating_sub(total_left)`;
   stuck-at-non-zero still open.

5. **Component tags lost for sinks using disk buffers** (OPA-5380): components
   paused for IO at init time lose `component_*` labels on later-registered
   metrics (utilization, etc.).

## Existing test strategy (so we don't duplicate it)

- In-repo: extensive `proptest` + **model-based testing** under
  `variants/disk_v2/tests/model/` (a reference model + action sequencer +
  in-memory filesystem). Unit tests for acknowledgements, initialization,
  known_errors, size_limits, invariants, record.
- Datadog internal: an E2E **chaos test** that SIGKILLs the worker 3× with e2e acks
  enabled and asserts every event is delivered end-to-end. Antithesis should go
  beyond: explore fault *timing/interleavings* (partial writes at rotation,
  fsync-vs-crash windows, reader/writer races on the mmap'd ledger) that a fixed
  3×SIGKILL test cannot.
- A **major lock-contention performance issue** affected all disk-buffer users
  (writer throughput ~90 MiB/s capped by contention) — points at writer/reader
  coordination hot paths.

## Notes on faults

- Crash-recovery properties require **node termination faults** (often disabled
  by default in Antithesis tenants) — flag this in the catalog.
- The disk buffer is **single-process** (intra-Vector reader+writer sharing an
  mmap'd ledger). Network/partition faults are largely irrelevant to the buffer
  itself; the strong levers are node kill/restart, node hang, CPU throttling
  (exposes the fsync/flush timing windows and lock contention), and filesystem
  state across restart.
