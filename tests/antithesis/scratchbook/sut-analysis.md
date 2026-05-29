---
sut_path: /home/ssm-user/src/vector
commit: b7aae737cef5dd37d1445915443a1eb97b584f85
updated: 2026-05-28
external_references:
  - path: lib/vector-buffers/src/variants/disk_v2/mod.rs
    why: Module-level doc comment is the authoritative design spec (format, ledger, recovery)
  - path: rfcs/2021-10-14-9477-buffer-improvements.md
    why: Original buffer-rework RFC; intended design and guarantees
  - path: docs/specs/buffer.md
    why: Buffer component spec / claimed behavior
  - path: (internal design doc, not linked)
    why: Authoritative description of fsync/durability window, ack flow, at-least-once + duplicate semantics
  - path: (internal design doc, not linked)
    why: Detailed root-cause writeups of disk-buffer bugs (#21683 ledger underflow, #24948 config-reload stall, #24606 discarded-metric blind spot)
  - path: (internal design doc, not linked)
    why: Existing internal chaos test (SIGKILL x3 + e2e acks) and a lock-contention performance issue affecting all disk-buffer users
  - path: GitHub issues vectordotdev/vector #21683 #24948 #24606 #24144 #23995 #17666 #23456 and PRs #23561 #24949
    why: Bug/regression context for property and evidence files
---

# SUT Analysis: Disk Buffer v2 (`lib/vector-buffers/src/variants/disk_v2/`)

## 1. Summary

Disk buffer v2 is Vector's durable, single-process, ring-buffer-style sink buffer.
A user opts into it per-sink (`type: disk`, `max_size` bytes, `when_full`) to get
durability across crashes/restarts. It is the durability backbone for
mission-critical pipelines and (with end-to-end acknowledgements) provides
**at-least-once** delivery. ~4,900 lines of Rust across 9 modules, with an
extensive in-repo model-based proptest suite.

The dominant risk, surfaced independently by **8 of 12** discovery focuses, is a
single class of bug: **unsaturated `u64` arithmetic on the in-memory
`total_buffer_size` accounting atomic, triggered by crash/partial-write
discrepancies, leading to a permanent writer deadlock** (Vector #21683). The
control-path atomic remains unfixed (PR #23561 only fixed the metrics reporter).
Secondary risks cluster around crash-time durability/recovery windows, the
non-atomic memory-mapped ledger, the file-rotation boundary, and silent data
loss on config reload / `drop_newest` / sink-error acks.

This is an **Antithesis-ideal** target: the bugs are crash-timing- and
interleaving-sensitive, externally hard to observe (silent stalls and silent
loss), and the existing tests explicitly cannot reach them (the model test's
in-memory filesystem makes `sync_all`/`flush` no-ops, so no crash-in-fsync-window
state is reachable).

## 2. Architecture and Data Flow

### Components

- **`DiskV2Buffer` / `Buffer::from_config`** (`mod.rs:233-365`) — entrypoint;
  wired into Vector topology via `IntoBuffer` → `SenderAdapter::DiskV2` (an
  `Arc<Mutex<BufferWriter>>`) and `ReceiverAdapter::DiskV2` (single
  `BufferReader`). The writer mutex is the **lock-contention bottleneck** noted
  in the GA doc.
- **`Ledger`** (`ledger.rs`) — `Arc`-shared between reader and writer. Wraps the
  memory-mapped `buffer.db` (`LedgerState`) plus in-memory coordination state.
- **`BufferWriter` / `RecordWriter` / `TrackingBufWriter`** (`writer.rs`) — write
  path: encode → CRC32C-wrap → rkyv-serialize → 256KB `TrackingBufWriter` →
  data file. Owns data-file rotation and `validate_last_write` recovery.
- **`BufferReader` / `RecordReader`** (`reader.rs`) — read path: read length
  delimiter → validate archive + checksum → zero-copy decode → attach
  `BatchNotifier` → emit. Owns acknowledgement processing, data-file deletion,
  and `seek_to_next_record` recovery.
- **Finalizer** — `spawn_finalizer` (`ledger.rs:701-710`) spawns an
  `OrderedFinalizer<u64>` tokio task that turns dropped `BatchNotifier`s into
  `pending_acks` increments and wakes the reader-side ack machinery.

### Buffer directory layout

`<data_dir>/buffer/v2/<id>/` contains `buffer.db` (mmap'd ledger),
`buffer.lock` (advisory lock), and `buffer-data-{u16}.dat` data files.

### On-disk record format

```
[8 bytes: record_len (u64 BIG-endian)] [rkyv-archived Record { checksum:u32, id:u64, metadata:u32, payload:[u8] }]
```

- CRC32C covers `BE(id) || BE(metadata) || payload` (NOT the checksum field).
- **Mixed endianness**: the length delimiter is big-endian; the rkyv body is
  **host-native** (little-endian on x86-64). Files are not portable across
  architectures (documented). `rkyv`'s `archived_root` reads the root from the
  **end** of the buffer — relevant to torn-tail recovery (see §10 F5).
- `CheckBytes` for `ArchivedRecord` is **hand-written** (`record.rs:75-117`) due
  to an upstream rkyv ICE — a manual unsafe validation surface.

### Write → read → ack → delete flow

1. `write_record` → encode + checksum + rkyv → `TrackingBufWriter` (256KB). Record
   ID = `writer_next_record + unflushed_events` and **encodes event count** (ID N
   to next ID M ⇒ M−N events).
2. `flush()` always flushes `TrackingBufWriter` to the **OS page cache**
   (readers see it immediately on Linux) and `notify_writer_waiters()`; it calls
   `sync_all()` (fsync) + `ledger.flush()` (msync) **only** when `should_flush()`
   says ≥500ms elapsed, or on rotation (`force_full_flush`).
3. Reader reads from page cache, validates, attaches a `BatchNotifier`, emits.
4. Sink delivers downstream, drops the notifier → finalizer → `pending_acks`.
5. Reader's `handle_pending_acknowledgements` consumes acks, advances
   `reader_last_record`, and when **all records in a data file are acked**,
   `delete_completed_data_file` unlinks the whole file (never partial), persists
   `reader_current_data_file`, `ledger.flush()`, and `notify_reader_waiters()` to
   unblock a full writer.

## 3. State Management and Persistence

### Persisted (mmap'd `buffer.db`, individually atomic, NOT a transactional group)

- `writer_next_record: AtomicU64`
- `writer_current_data_file: AtomicU16`
- `reader_current_data_file: AtomicU16` (the durable acked reader position)
- `reader_last_record: AtomicU64`

### In-memory only (lost on crash, reconstructed at startup)

- `total_buffer_size: AtomicU64` — **re-seeded at startup by summing `.dat` file
  sizes** (`update_buffer_size`), then decremented as the reader seeks/acks.
  This reconstruction-from-file-size vs. decrement-by-record-bytes mismatch is
  the underflow trigger.
- `pending_acks`, `writer_done`, `unacked_reader_file_id_offset`,
  `last_flush: AtomicCell<Instant>`, the two `Notify`s.
- Writer-local `unflushed_bytes`/`unflushed_events` (plain integers).

### Two-level flush model and the durability window

- **Page-cache flush** (every `flush()`): data visible to same-host reader.
- **fsync + ledger msync** (every ≥500ms `DEFAULT_FLUSH_INTERVAL`, or rotation):
  the only durable point.
- **Data-loss window = up to 500ms** of page-cached-but-unsynced writes on crash
  (documented). The data file fsync and the ledger msync are **two separate,
  non-atomic syscalls** — a crash between them leaves data and ledger diverged,
  repaired (with assumptions) by `validate_last_write` on restart.

### Ordering of ledger updates relative to durability

- Writer updates `writer_next_record` **after** the page-cache write, lazily on
  flush; a crash leaves the ledger lagging the data → `validate_last_write`
  fast-forwards (`Ordering::Less`). The reverse (ledger ahead of data,
  `Ordering::Greater`) logs "Events have likely been lost" and skips to the next
  file — **detected loss, counted as gap markers**, not silent.
- Reader updates `reader_last_record` after acks, flushed lazily; file deletion
  unlinks **before** the ledger msync — a crash in that window leaves the ledger
  pointing at a deleted file (handled on restart via NotFound→skip).

## 4. Concurrency Model

- Single writer (behind a topology `Mutex`), single reader, plus the finalizer
  task. mmap'd ledger fields use `Acquire`/`Release`/`AcqRel`; in-memory atomics
  likewise. Orderings were judged **correct** (focus 3) — the bugs are
  arithmetic/logic, not memory-ordering.
- Coordination via tokio `Notify`: writer waits on `wait_for_reader()`; reader
  waits on `wait_for_writer()`. **Naming is misleading**: the finalizer calls
  `notify_writer_waiters()` which wakes the *reader*; the reader then frees space
  and calls `notify_reader_waiters()` to wake the *writer*. The writer's progress
  is therefore **transitively dependent** on: sink acks → finalizer task alive →
  reader being actively polled → file deletion. Break any link and the writer
  can stall.
- `Notify` is edge-triggered with a one-permit store; generally tolerant but a
  potential source of missed-wakeup delays.
- `should_flush` uses an `AtomicCell<Instant>` CAS so only one caller fsyncs;
  under CPU-throttle the winner can be descheduled between winning the CAS and
  actually fsyncing, silently extending the 500ms window.
- Lock contention on the writer `Mutex` is the known throughput ceiling (~90
  MiB/s with 10 threads).

## 5. Claimed Guarantees

### Safety ("a bad thing never happens")

- **INV-1** Every record CRC32C-checksummed; corrupted records detected and never
  returned as valid (`record.rs`, `reader.rs`). Bypass only via CRC collision or
  a bug in the hand-written `CheckBytes`.
- **INV-2** A record never spans two data files (`writer.rs:433-436` gate). Hard.
- **INV-3** Data files ≤ 128MB — **soft**: a record may overshoot by up to
  `max_record_size` (documented); real bound is ~2×.
- **INV-4** Record IDs strictly monotonic and encode event count; violation
  **panics** (`reader.rs:480-484`). Crash can create a *gap* (detected loss), not
  a duplicate.
- **INV-5** A data file is deleted only after all its records are acked;
  whole-file deletion only, never partial truncation.
- **INV-6** Durability: synced data survives crash; **best-effort within 500ms**;
  graceful shutdown flushes (but see §10 — `BufferWriter::Drop` does NOT flush).
- **INV-7** Buffer never exceeds `max_size` — **broken by the underflow bug**:
  the writer deadlocks (vacuously upholding the bound by never writing again).
- **INV-9** No double-counting / no silent loss — **broken** for sink-error acks:
  the finalizer discards `BatchStatus`, so `Errored`/`Rejected` deliveries are
  credited as acknowledged and the events are dropped from the buffer with no
  replay (within a process lifetime).
- **INV-10** Single-process exclusivity via advisory `buffer.lock` — does NOT
  protect intra-process (POSIX `fcntl` locks are per-process), so a config-reload
  overlap of old+new topology can open the same buffer twice.

### Liveness ("a good thing eventually happens")

- **L1** A blocked (full) writer eventually unblocks once the reader frees a data
  file — **fails permanently under the underflow bug**.
- **L2** Written+flushed records eventually become readable (page-cache flush per
  send; no timer needed). Strong.
- **L3** Fully-acked data files eventually deleted / space reclaimed — depends on
  finalizer task alive + reader polled + delete I/O succeeding.
- **L4** On restart, reader eventually catches up (`seek_to_next_record`) —
  vulnerable to torn-tail and to the file-ID rollover ordering bug.
- **L6** Buffer eventually initializes after crash — vulnerable if the writer
  must open a not-yet-created next file, or if `update_buffer_size` over-seeds.
- **L8** Reader terminates (`next()→None`) when writer done and buffer empty —
  uses `total_buffer_size == 0`, so the underflow bug also breaks clean shutdown.

## 6. Failure-Prone Areas (ranked — these drive the property catalog)

1. **`total_buffer_size` underflow → permanent writer deadlock (#21683).**
   Root: `ledger.rs:291-298` raw `fetch_sub`; two trigger paths:
   `reader.rs:524` `metadata.len() - bytes_read` (also unguarded) and the
   startup `update_buffer_size`(file sizes) vs. seek-decrement(record bytes)
   mismatch. Manifests as a silent pipeline stall; also breaks reader shutdown
   (L8). **Highest-value target.** Requires node-kill + restart faults.
2. **Crash-time durability/recovery windows.** fsync-vs-crash, data-file-fsync
   vs. ledger-msync non-atomicity, torn last record, `validate_last_write`
   `Greater`/`Less` reconciliation, partial write at file rotation. Tests cannot
   reach these (no-op fsync in model FS).
3. **Config-reload silent loss & metric drift (#24948).** `BufferWriter::
   Drop` calls `close()` but **not** `flush()` → up to 256KB of buffered events
   silently dropped; `track_dropped_events` charges `byte_size=0` → permanent
   accounting drift; finalizer task may still hold the `Arc<Ledger>` / lock during
   reload; double-counted then gapped metrics. PR #24949 addressed parts.
4. **`drop_newest` silent loss vs. metrics (#24606/#24144).** Buffer-level
   discarded counter increments but `component_discarded_events_total` stays 0.
5. **Sink-error acks discarded** (`ledger.rs:717` `_status` ignored) → silent loss
   under at-least-once.
6. **File-ID rollover ordering bug** (`reader.rs:932` raw `u16 >`), reachable in
   tests where `MAX_FILE_ID=6`; production at 65536-file rollover.
7. **Reader skips the rest of a file on first bad record** — valid records after a
   corrupt one in the same 128MB file are silently abandoned.
8. **`get_total_records` `- 1` non-wrapping** at record-ID equality/rollover →
   ~2^64 phantom event count into metrics.
9. **mmap SIGBUS / external file tampering** (foreign `.dat` files inflate
   `total_buffer_size`; truncation under read → underflow).

## 7. Existing Test Strategy and the Antithesis Gap

- In-repo: model-based proptest (`tests/model/`) with a reference model + action
  sequencer + **in-memory `TestFilesystem`**; per-area unit tests
  (initialization, acknowledgements, basic, known_errors, size_limits,
  invariants, record); 9 saved proptest regression seeds (tiny size limits / ack
  ordering).
- **Critical limitation:** `TestFilesystem::sync_all` and mmap `flush` are
  **no-ops**, `flush_interval` is hardcoded to 10s, and the sequencer serializes
  ops — so the model suite **cannot** exercise crash-in-fsync-window, real
  partial writes, true reader/writer preemption, or the underflow trigger. The
  model's own `LedgerModel::decrement_buffer_size` even mirrors the unguarded
  `fetch_sub`, so it would reproduce the underflow if the trigger were reachable
  — but the fake FS prevents the trigger.
- Two telling **disabled tests**: `reader_exits_cleanly_when_writer_done_and_in_flight_acks`
  (`basic.rs`, `#[ignore = "flaky #23456"]`) and `writer_waits_when_buffer_is_full`
  (`size_limits.rs`, `#[ignore]`) — both sit exactly on the deadlock/backpressure
  path. High-value Antithesis targets.
- internal E2E **chaos test**: SIGKILL ×3 with e2e acks, asserts all events delivered.
  Antithesis goes further by exploring fault *timing/interleavings* a fixed
  3×-kill test cannot.

## 8. External Dependencies / Integration Points

- **OS/filesystem** via `io.rs` (`open/write/fsync/unlink/mmap`). Relies on Linux
  page-cache read-after-write (acknowledged Linux-specific assumption).
- **rkyv** zero-copy (host-endian, alignment-sensitive, manual `CheckBytes`).
- **memmap2** for the ledger (`msync` on flush; **SIGBUS** if `buffer.db` is
  truncated/unmapped — unhandled, crashes process; misaligned atomics UB on some
  non-x86 arches).
- **crc32fast** (hardware-accelerated CRC32C).
- **fslock** advisory lock — per-process on Linux (no intra-process protection).
- **vector-common finalization** (`OrderedFinalizer`, `BatchNotifier`) for acks;
  topology channel adapters treat all writer/reader errors as unrecoverable
  (reader I/O error → `panic!` in `receiver.rs`).

## 9. Product Context

- Disk buffer is opt-in per sink; sold as "data synchronized to disk will not be
  lost if Vector is restarted forcefully or crashes; data synchronized every
  500ms." Used by customers needing durability for mission-critical pipelines.
- With e2e acks: at-least-once; crash between buffer-write and downstream-ack →
  **duplicate delivery on replay** (downstream must dedup).
- User-visible failures, by severity: (1) **silent pipeline stall** (writer
  deadlock — no crash, no error, dashboards may look healthy); (2) **silent data
  loss** (config reload, `drop_newest`, sink-error acks, crash window); (3)
  **duplicate delivery**; (4) **lying buffer metrics** (stuck/negative gauges).
  The stall and the silent loss are what a durability-seeking customer cares
  about most.

## 10. Wildcard / Cross-Cutting Observations

- **F5 (torn-tail mis-recovery):** rkyv `archived_root` reads the root offset from
  the last 8 bytes; crash-left trailing bytes could be misread as a plausible
  offset, yielding a `Valid` record with the wrong `id`, fast-forwarding the
  ledger to a wrong ID and synthesizing a phantom gap.
- **`WhenFull::Overflow` + disk base:** unbiased `select!` over base+overflow
  reorders events across the overflow boundary; if overflow is in-memory, a crash
  loses the *later* in-memory events while the *earlier* disk events survive —
  breaks dedup-based at-least-once reasoning (a gap, not just duplicates).
- **`DiskBufferV1CompatibilityMode` flag inversion** (`vector-core/event/ser.rs`):
  `can_decode` requires the V1-compat flag on every record; a future "V2-native"
  flag scheme would be rejected as incompatible — a forward-compat foot-gun.
- **Clock jitter × `should_flush`:** `Instant::elapsed` drives the 500ms gate;
  Antithesis clock faults could stretch/shrink the durability window.
- **mmap'd ledger torn write under crash:** four independent atomics, no group
  atomicity; a crash mid-multi-field-update is exactly what recovery must handle
  and what the model FS never produces.

## Assumptions

- Disk buffer is single-process; network/partition faults are largely irrelevant.
  The strong fault levers are **node termination (kill/restart)**, **node hang**,
  **CPU throttling**, **clock jitter**, and **filesystem state across restart**.
- Antithesis runs x86-64 Linux (matches production); cross-arch endianness is out
  of scope except as a "don't move buffer files" caveat.

## Open Questions (catalog-wide)

- Are **node-termination faults enabled** in the target Antithesis tenant? Nearly
  every high-value property needs them. Flag to the user.
- Does Vector's topology shutdown call `writer.flush()` before dropping the writer
  on graceful shutdown (vs. the unflushed `Drop`)? Determines whether graceful
  shutdown is actually lossless.
- Does the finalizer task get drained by the tokio runtime before shutdown, or can
  in-flight acks be lost (stranding the reader)?
- Is the config-reload old/new topology overlap actually concurrent (making the
  per-process advisory-lock gap a live safety issue)?
</content>
