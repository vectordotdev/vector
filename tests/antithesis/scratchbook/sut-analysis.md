---
sut_path: /home/ssm-user/src/vector
commit: 2dae1f421
updated: 2026-06-03
external_references:
  - path: lib/vector-buffers/src/variants/disk_v2/mod.rs
    why: Module-level doc comment is the authoritative design spec (format, ledger, recovery)
  - path: rfcs/2021-10-14-9477-buffer-improvements.md
    why: Original buffer-rework RFC; intended design and guarantees
  - path: docs/specs/buffer.md
    why: Buffer component spec / claimed behavior
  - path: GitHub issues vectordotdev/vector #21683 #24948 #24606 #24144 #23995 #17666 #23456 and PRs #23561 #24949
    why: Bug/regression context for property and evidence files
---

# SUT Analysis: Disk Buffer v2 (`lib/vector-buffers/src/variants/disk_v2/`)

Connective architecture only — data flow, persisted-vs-in-memory split,
concurrency wake chain, existing-test gap. The invariant catalog and failure-mode
ranking live in the per-property files under `properties/`.

## Architecture and Data Flow

Disk buffer v2 is Vector's durable, single-process, ring-buffer-style sink buffer
(opt-in per sink: `type: disk`, `max_size`, `when_full`). ~4,900 lines, 9 modules.

### Components

- **`DiskV2Buffer` / `Buffer::from_config`** — entrypoint, wired into topology via
  `IntoBuffer` → `SenderAdapter::DiskV2` (an `Arc<Mutex<BufferWriter>>`, the
  lock-contention bottleneck) and `ReceiverAdapter::DiskV2` (single `BufferReader`).
- **`Ledger`** — `Arc`-shared between reader and writer. Wraps the memory-mapped
  `buffer.db` (`LedgerState`) plus in-memory coordination state.
- **`BufferWriter`** — encode → CRC32C → rkyv → 256KB `TrackingBufWriter` → data
  file. Owns rotation and `validate_last_write` recovery.
- **`BufferReader`** — read length delimiter → validate + checksum → zero-copy
  decode → attach `BatchNotifier` → emit. Owns ack processing, data-file deletion,
  `seek_to_next_record` recovery.
- **Finalizer** — `spawn_finalizer` runs an `OrderedFinalizer<u64>` tokio task
  turning dropped `BatchNotifier`s into `pending_acks` increments, waking the
  reader-side ack machinery.

Layout `<data_dir>/buffer/v2/<id>/`: `buffer.db` (mmap'd ledger), `buffer.lock`
(advisory lock), `buffer-data-{u16}.dat` files. Record format `[8B record_len u64
BIG-endian][rkyv Record{checksum:u32,id:u64,metadata:u32,payload}]`; CRC32C covers
`BE(id)||BE(metadata)||payload`. **Mixed endianness** (BE delimiter, host-native
rkyv body) makes files non-portable. `rkyv`'s `archived_root` reads the root from
the buffer's **end** (torn-tail recovery relevance); `CheckBytes` for
`ArchivedRecord` is hand-written (upstream rkyv ICE) — a manual unsafe validation
surface.

### Write → read → ack → delete flow

1. `write_record` → encode + checksum + rkyv → `TrackingBufWriter`. Record ID =
   `writer_next_record + unflushed_events`, **encoding event count** (ID N to next
   ID M ⇒ M−N events).
2. `flush()` always flushes `TrackingBufWriter` to the **OS page cache** (readers
   see it immediately on Linux) and `notify_writer_waiters()`. It calls `sync_all()`
   (fsync) + `ledger.flush()` (msync) **only** when `should_flush()` sees ≥500ms
   elapsed, or on rotation (`force_full_flush`).
3. Reader reads from page cache, validates, attaches a `BatchNotifier`, emits.
4. Sink delivers downstream, drops the notifier → finalizer → `pending_acks`.
5. `handle_pending_acknowledgements` advances `reader_last_record`. When **all
   records in a data file are acked**, `delete_completed_data_file` unlinks the
   whole file (never partial), persists `reader_current_data_file`, msyncs, and
   `notify_reader_waiters()` to unblock a full writer.

## State Management and Persistence

### Persisted (mmap'd `buffer.db`, individually atomic, NOT a transactional group)

`writer_next_record`, `writer_current_data_file`, `reader_current_data_file` (the
durable acked reader position), `reader_last_record` — all `AtomicU64`/`AtomicU16`.

### In-memory only (lost on crash, reconstructed at startup)

- `total_buffer_size: AtomicU64` — **re-seeded at startup by summing `.dat` file
  sizes** (`update_buffer_size`), then decremented as the reader seeks/acks. This
  reconstruct-from-file-size vs. decrement-by-record-bytes mismatch triggers the
  underflow.
- `pending_acks`, `writer_done`, `unacked_reader_file_id_offset`,
  `last_flush: AtomicCell<Instant>`, the two `Notify`s.
- Writer-local `unflushed_bytes`/`unflushed_events` (plain integers).

### Two-level flush model and the durability window

- **Page-cache flush** (every `flush()`): data visible to same-host reader.
- **fsync + ledger msync** (every ≥500ms `DEFAULT_FLUSH_INTERVAL`, or rotation):
  the only durable point.
- **Data-loss window = up to 500ms** of page-cached-but-unsynced writes on crash
  (documented). Data-file fsync and ledger msync are **two separate, non-atomic
  syscalls** — a crash between them diverges data and ledger, repaired (with
  assumptions) by `validate_last_write` on restart.

### Ordering of ledger updates relative to durability

- Writer updates `writer_next_record` **after** the page-cache write, lazily on
  flush. A crash leaves the ledger lagging the data → `validate_last_write`
  fast-forwards (`Ordering::Less`). The reverse (ledger ahead, `Ordering::Greater`)
  logs "Events have likely been lost" and skips to the next file — **detected loss,
  counted as gap markers**, not silent.
- Reader updates `reader_last_record` after acks, flushed lazily. File deletion
  unlinks **before** the ledger msync — a crash in that window leaves the ledger
  pointing at a deleted file (handled on restart via NotFound→skip).

## Concurrency Model

- Single writer (behind a topology `Mutex`), single reader, plus the finalizer
  task. mmap'd ledger fields and in-memory atomics use `Acquire`/`Release`/`AcqRel`,
  judged correct — the bugs are arithmetic/logic, not memory-order.
- Coordination via tokio `Notify`: writer waits on `wait_for_reader()`, reader on
  `wait_for_writer()`. **Naming is misleading**: the finalizer's
  `notify_writer_waiters()` wakes the *reader*; the reader frees space and calls
  `notify_reader_waiters()` to wake the *writer*. Writer progress is therefore
  **transitively dependent** on: sink acks → finalizer task alive → reader actively
  polled → file deletion. Break any link and the writer can stall.
- `Notify` is edge-triggered with a one-permit store — tolerant, but a potential
  missed-wakeup-delay source.
- `should_flush` uses an `AtomicCell<Instant>` CAS so only one caller fsyncs. Under
  CPU-throttle the winner can be descheduled between winning the CAS and fsyncing,
  silently extending the 500ms window.
- Writer `Mutex` contention is the known throughput ceiling (~90 MiB/s, 10 threads).

## Existing Test Strategy and the Antithesis Gap

- In-repo: model-based proptest (`tests/model/`) with reference model + action
  sequencer + **in-memory `TestFilesystem`**, per-area unit tests, 9 saved proptest
  regression seeds.
- **Critical limitation:** `TestFilesystem::sync_all` and mmap `flush` are
  **no-ops**, `flush_interval` is hardcoded to 10s, and the sequencer serializes
  ops — so the model suite **cannot** exercise crash-in-fsync-window, real partial
  writes, true reader/writer preemption, or the underflow trigger. Its own
  `LedgerModel::decrement_buffer_size` mirrors the unguarded `fetch_sub`, so it
  would reproduce the underflow if the trigger were reachable — but the fake FS
  prevents the trigger.
- Two telling **disabled tests** sit on the deadlock/backpressure path:
  `reader_exits_cleanly_when_writer_done_and_in_flight_acks` (`basic.rs`,
  `#[ignore = "flaky #23456"]`) and `writer_waits_when_buffer_is_full`
  (`size_limits.rs`, `#[ignore]`). High-value Antithesis targets.
- Internal E2E chaos test: SIGKILL ×3 with e2e acks, asserts all events delivered.
  Antithesis goes further, exploring fault *timing/interleavings* a fixed 3×-kill
  test cannot.

## External dependencies (brief)

OS/filesystem via `io.rs` (relies on Linux page-cache read-after-write). rkyv
zero-copy (host-endian, manual `CheckBytes`). memmap2 for the ledger (**SIGBUS** if
`buffer.db` is truncated — unhandled). crc32fast (CRC32C). fslock advisory lock
(per-process on Linux — no intra-process protection). vector-common finalization
for acks; reader I/O error → `panic!` in `receiver.rs`.

## Product context (brief)

Sold as "data synchronized to disk will not be lost on forced restart or crash;
synced every 500ms." With e2e acks: at-least-once (crash between buffer-write and
downstream-ack → duplicate on replay, downstream must dedup). User-visible failure
severity: (1) silent pipeline stall (writer deadlock — no crash, no error); (2)
silent data loss (config reload, `drop_newest`, sink-error acks, crash window);
(3) duplicate delivery; (4) lying buffer metrics. A durability-seeking customer
cares most about the stall and silent loss.

Strong fault levers: **node termination (kill/restart)**, **node hang**, **CPU
throttling**, **clock jitter**, **filesystem state across restart** — not network
partitions, to which the single-process buffer is largely indifferent.
