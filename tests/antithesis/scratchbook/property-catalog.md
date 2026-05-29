---
sut_path: /home/ssm-user/src/vector
commit: 4ff41a0adb5240d071f30a5a43cb0d065e40f618
updated: 2026-05-29
external_references:
  - path: lib/vector-buffers/src/variants/disk_v2/mod.rs
    why: Module-level doc comment is the authoritative design spec
  - path: rfcs/2021-10-14-9477-buffer-improvements.md
    why: Original buffer-rework RFC; intended design and guarantees
  - path: docs/specs/buffer.md
    why: Buffer component spec / claimed behavior
  - path: (internal design doc, not linked)
    why: fsync/durability window, ack flow, at-least-once + duplicate semantics
  - path: (internal design doc, not linked)
    why: Root-cause writeups of #21683, #24948, #24606
  - path: (internal design doc, not linked)
    why: Existing internal chaos test + lock-contention performance issue
  - path: GitHub issues vectordotdev/vector #21683 #24948 #24606 #24144 #23995 #17666 #23456; PRs #23561 #24949
    why: Bug/regression context
---

# Property Catalog: Disk Buffer v2

29 properties across 7 categories (19 from discovery + 7 from evaluation
gap-filling — Category 7 + 3 from the 2026-05-29 data-loss expansion, in the
Category 1 "silent data-loss cluster"). No Antithesis SDK assertions exist in the codebase
today (`existing-assertions.md`); every SUT-side assertion noted below is
**missing** and must be added. Each property has an evidence file at
`properties/{slug}.md`. Evaluation refinements are recorded in
`evaluation/synthesis.md`.

**Fault dependency:** nearly every high-value property requires **node-termination
(kill/restart) faults**, which are often disabled by default in Antithesis
tenants. This must be confirmed with the user (see file-level Open Questions). A
few also need a **custom fault** (config reload via SIGHUP, downstream-sink error
injection, binary swap, filesystem truncation). Several durability properties
should run with `flush_interval=0` (every flush becomes an fsync) to remove the
500ms-window and clock-jitter ambiguity from the oracle.

**Two preconditions gate ~21/26 properties — CONFIRMED ENABLED by the user
(2026-05-28):** (1) node-termination kill/restart faults are enabled in the
tenant; (2) the buffer `data_dir` will be on storage that survives a modeled
crash (persistent volume). The crash-recovery cluster is therefore fully testable.
Still recommend a pre-fault **sentinel** (write+fsync N events, kill, assert files
survive) gating Category 2–6 assertions as a cheap guard against
misconfiguration/regression of the persistence assumption.

**Logic-bug properties — user decision (2026-05-28, Bias B1):** the ~6 deterministic
logic/metric-bug properties are **kept as workload-side secondary checks with no
dedicated fault-search budget** (refinement R-H), not promoted to first-class
fault targets and not removed.

**Currently-violated properties** (used to *expose* known/likely bugs, expected to
fail until fixed): `total-buffer-size-never-underflows`,
`writer-eventually-makes-progress` (deadlock), `sink-failure-not-silently-acked`,
`dropped-events-are-counted`, `file-id-rollover-stays-coordinated`,
`record-id-wraparound-accounting-holds` (empty-buffer case),
`foreign-data-file-no-writer-stall`, likely `config-reload-no-silent-loss`, and
likely `fsync-window-bounded-under-clock-jitter` (under clock faults).

**Build note:** `total-buffer-size-never-underflows` (and any property observing
the wrap) requires a **release build** — the `trace!` at `ledger.rs:295` evaluates
`last_total_buffer_size - amount`, which panics in a debug build before the
release-mode wrap is observable.

**Workload status (2026-05-29 data-loss battery):** the `disk_v2_lossfinder`
exerciser (`lib/vector-buffers/examples/disk_v2_lossfinder.rs`, harness
`tests/antithesis/config-lossfinder` + `test/v1/diskbuf_loss`) implements a
no-silent-loss oracle across a 7-scenario RNG fault menu, giving workload
coverage for: `every-written-event-eventually-delivered` (Baseline),
`config-reload-no-silent-loss`/#24948 (WriterDropNoFlush),
`sink-failure-not-silently-acked` (RejectDeliveries — already surfaced the
finalizer status-discard loss locally), `durable-unacked-events-survive-crash` +
`recovery-completes-after-crash` (CrashReopen), `dropped-events-are-counted`/#24606
(DropNewestOverfill — reachability only, metric oracle TODO),
`corruption-skip-loss-bounded` + `corruption-skip-record-id-accounting-consistent`
(Corruption/TruncateTail — collateral-loss oracle). The earlier `disk_v2_antithesis`
exerciser (config-direct) covers the #21683 accounting-underflow cluster
(reproduced in run D0). Both are demonstrations: assertions encode the CORRECT
no-loss invariant so they fire against current behavior.

---

## Category 1 — Data Integrity & Corruption

Records are CRC32C-checksummed and rkyv-validated. These properties verify the
buffer never hands a corrupted/garbled record to a sink and that corruption is
detected, not silently propagated. Antithesis's bit-flip / partial-write /
torn-tail faults are the core levers.

### no-corrupted-record-delivered — No Corrupted Record Delivered to Sink

| | |
|---|---|
| **Type** | Safety |
| **Property** | A record that fails CRC32C or rkyv `CheckBytes` is never decoded and returned as a valid event; the reader rolls to the next file instead. |
| **Invariant** | `AlwaysOrUnreachable` at the record-emission point (`reader.rs:~1131`, before `Ok(Some(record))`): any emitted record passed both `verify_checksum` (`record.rs:144-155`) and the hand-written `CheckBytes` (`record.rs:79-117`). AlwaysOrUnreachable because corruption is a rare/optional path — never-executed is acceptable, but any execution must satisfy the invariant. |
| **Antithesis Angle** | Bit-flips on payload / CRC field / rkyv root offset / length delimiter; mid-record truncation (→ `PartialWrite`); torn-tail after crash. Antithesis explores which corruption shapes slip past the manual `CheckBytes` and reach CRC, and whether a CRC-passing torn tail exists. |
| **Why It Matters** | The buffer's durability story depends on this guard. A bypass forwards garbled telemetry downstream and corrupts event-count accounting via a wrong record ID. Manual `CheckBytes` (rkyv ICE workaround) is an unsafe validation surface. |

**Open Questions:**

- Does the topology receiver (`receiver.rs`) panic or swallow `ReaderError::Checksum`/`Deserialization`? Determines whether a side-channel counter is needed for the workload to observe detection.
- The startup `seek_to_next_record` corruption path calls `validate_record_archive` directly, not `try_next_record` — does an assertion at the emission point miss corruption during startup replay?
- Is the `unsafe archived_root` in `read_record` (`reader.rs:375`) sound across tokio preemption between `try_next_record` and `read_record`?

### corruption-is-detected-and-recovered — Corruption Detection/Recovery Path Executes

| | |
|---|---|
| **Type** | Reachability |
| **Property** | When corruption is injected, the detection+recovery path (`is_bad_read` → `roll_to_next_data_file`) actually executes and the buffer continues reading. |
| **Invariant** | `Sometimes(corruption_detected_and_recovered)` at `reader.rs:~1035` (the `is_bad_read()` branch). Sometimes is correct: this path is only reachable under injected faults; the assertion confirms fault injection reaches live reads and the recovery branch fires. |
| **Antithesis Angle** | Inject faults while the reader's `BufReader` has the file open and is actively reading (not before/after). Distinguishes "fault reached detection logic" from "fault hit an already-closed file." |
| **Why It Matters** | Without this the recovery path may be dead code under a given fault strategy. Surfaces the "skip rest of file after first bad record" data-loss surface (valid records after a corrupt one in the same 128MB file are abandoned). |

**Open Questions:**

- Does the `seek_to_next_record` init loop invoke `roll_to_next_data_file` on bad reads, or take a different path that misses the assertion?
- What is the records-abandoned rate per corruption event (quantifies the skip-rest-of-file loss)? **Measurable angle (evaluation R-I/W-C2):** correlate corruption-injection timing with event IDs — events written *after* the injected-corruption offset in the same file should still be delivered; if they vanish, that is measurable abandonment loss. Worth elevating to its own sub-assertion in the workload.
- Do in-flight `BatchNotifier`s for records read before the corrupt one drain correctly when the file's deletion marker count excludes the skipped records?

### record-id-monotonicity-holds — Monotonicity Panic Never Fires

| | |
|---|---|
| **Type** | Safety |
| **Property** | The "record ID monotonicity violation detected; this is a serious bug" panic (`reader.rs:~480-484`) is never reached, even under crash/corruption/rollover faults. |
| **Invariant** | `Unreachable` — this is a guardrail that must never trip. A violation indicates a bug in `validate_last_write`, `seek_to_next_record`, or the ack state machine, not an acceptable rare path. |
| **Antithesis Angle** | Node-kill during `validate_last_write` fast-forward; torn-tail mis-recovery yielding a wrong `id`; file-ID rollover with the non-wrap-aware `u16 >` at `reader.rs:932`; node-kill during lazy ledger persistence in seek. |
| **Why It Matters** | The panic crashes the process; if the triggering state persists across restarts, Vector enters an infinite restart loop — same operational impact as the writer deadlock. Existing tests cannot reach it (no-op fsync in model FS). |

**Open Questions:**

- Does `OrderedAcknowledgements::add_marker` use wrap-aware ID comparison? If not, fresh-buffer ID 0 after a reset could falsely trigger the panic.
- Is the `reader.rs:932` `>` comparison intended to be wrap-aware? (Cross-ref `file-id-rollover-stays-coordinated`.)
- Can `validate_last_write`'s `Greater` branch leave `next_record_id` below what `record_acks` expects?

### record-never-spans-files — Record Never Spans Two Data Files

| | |
|---|---|
| **Type** | Safety |
| **Property** | Every record is fully contained within a single data file. |
| **Invariant** | `AlwaysOrUnreachable` in `RecordWriter::flush_record` (`writer.rs:~629`), asserting `current_data_file_size <= max_data_file_size` after update. The `can_write` gate (`writer.rs:433-437`) normally enforces this; the assertion catches a corrupted size-seed bypassing the gate. |
| **Antithesis Angle** | Corrupt the `metadata().len()` size-seed (filesystem fault between open and metadata); external append; `max_data_file_size` ≈ `max_record_size` (both default 128MB → zero margin). |
| **Why It Matters** | A spanning record is silently lost: the reader sees `PartialWrite`, rolls, and abandons it; `total_buffer_size` correction may be wrong, creating an ID gap. A write that returned `Ok` is never delivered. |

**Open Questions:**

- Is the `debug_assert(max_data_file_size >= max_record_size)` (`writer.rs:~396`) compiled out of release? If so, a `max_record_size > max_data_file_size` misconfig silently makes every write return `DataFileFull` → writer loops forever.
- Does a low `metadata().len()` under fault fail open (allows oversize write → span risk) — confirmed yes by the agent — worth an explicit fault test.

---

### Silent data-loss cluster — checksum-skip (added 2026-05-29, data-loss expansion)

These three sharpen the user's concern *"if the checksum fails we'll skip records."* The existing `corruption-is-detected-and-recovered` only checks the recovery path *executes* (`Sometimes`); these are the `Always` safety bounds on **how much** is lost and whether the loss is **observable**. Root mechanism: `roll_to_next_data_file` (reader.rs:711-759) abandons the entire remainder of a data file on the first bad read, accounting only the records actually read.

### corruption-skip-loss-bounded — Checksum-Skip Loss Bounded to the Unreadable Span

| | |
|---|---|
| **Type** | Safety |
| **Property** | When a record fails CRC32C/`CheckBytes`/partial-write detection, only that record (plus any genuinely-unparseable contiguous tail) is lost — valid records that follow it in the same 128MB data file are still eventually delivered. |
| **Invariant** | `Always` (workload-level): every durably-written, valid-checksum record positioned after a corrupt record in the same file is in the delivered set. Currently VIOLATED — `roll_to_next_data_file` (reader.rs:711) unconditionally abandons the whole file tail. |
| **Antithesis Angle** | Bit-flip an *early* record's CRC-covered region in a multi-record file; drain; compare delivered IDs vs valid IDs. Vary corruption position + file fullness to measure loss magnitude. Needs corruption in a live read. |
| **Why It Matters** | internal doc *internal buffer design notes* ((internal doc id omitted)) states the loss window is 500ms unsynced and synced events are not lost with e2e acks; a corruption roll discards synced, valid, not-yet-acked records far outside that window — contradicting the at-least-once guarantee. A single bit-flip can abandon ~a full 128MB file. |

**Open Questions:**

- ~~Is the whole-file roll an accepted product tradeoff?~~ **RESOLVED (owner ruling, 2026-05-29): it is a BUG.** Any data loss not explicitly documented in detail is a bug; the reader.rs `roll_to_next_data_file` comment ("not sure the rest of the file is valid") is a hedge, not documentation. The property is therefore a real defect to fix, not a tradeoff to accept.
- Can records be re-found after a corrupt one given the length-delimited framing? `(partial: a corrupt length delimiter can desync intra-file resync — supports the conservative roll; a CRC-valid record after a payload-corrupt one is in principle recoverable)` — note this informs *how* to fix (resync vs. abandon), not *whether* (it's a bug regardless).

### corruption-skip-loss-is-counted — Checksum-Skip Loss Is Observable

| | |
|---|---|
| **Type** | Safety |
| **Property** | Records abandoned by a corruption-triggered roll increment a loss metric (`component_discarded_events_total` and/or `buffer_discarded_events_total`) so operators can detect the loss. |
| **Invariant** | `Always`: after a corruption event abandoning N valid records, the discarded-events counter rises by N (equivalently `produced - delivered - counted_dropped == 0`). Currently VIOLATED — abandoned records hit **neither** counter; `track_dropped_events` (reader.rs:656) fires only for writer-side gap markers, not reader-side rolls, so the loss is charged only to `decrement_total_buffer_size`. Strictly more silent than #24606. |
| **Antithesis Angle** | Same early-record bit-flip; oracle scrapes the discarded counter and asserts it rose by the abandoned count after the roll. e2e acks give exact produced/delivered sets. |
| **Why It Matters** | Read-side companion to the HIGH-severity #24606 finding in the *an internal telemetry-correctness report* ((internal doc id omitted)): "silent data loss going undetected because `component_discarded_events_total`…". Undetectable loss is the worst class — operators cannot alert on what isn't counted. |

**Open Questions:**

- Is the abandoned-record count even computed internally? `(partial: roll_to_next_data_file computes the count for READ records only; the abandoned count is never materialized)`
- Intentional vs error counter semantics for corruption loss? `(needs human input)`

### corruption-skip-record-id-accounting-consistent — Skip Never Becomes Accounting Corruption

| | |
|---|---|
| **Type** | Safety |
| **Property** | A corruption roll never converts bounded data loss into accounting corruption: record-ID and `total_buffer_size` accounting stay self-consistent across the abandoned span (no underflow, no monotonicity-guard trip). |
| **Invariant** | `Always` (SUT-side): after a roll, `next_writer_record_id - reader_last_record_id == on-disk unread records`, the rolled file's `total_buffer_size` decrement equals true remaining bytes (no reader.rs:524 underflow), and the monotonicity panic (`reader.rs:~480`) never trips. |
| **Antithesis Angle** | Mid-file corruption (non-empty abandoned tail) + continue across the file boundary and a crash+restart; watch the three underflow asserts already wired + the monotonicity guard. This is where the checksum-skip path and the organically-reproduced #21683 (run D0) meet. |
| **Why It Matters** | Identifies the corruption-roll abandoned-tail as a concrete real trigger for the reader.rs:524 underflow (#21683) and the monotonicity panic — not only external truncation. Links the data-loss surface to the deadlock/crash-loop clusters. |

**Open Questions:**

- Does any path advance `reader_last_record_id` over abandoned IDs, or is the gap permanent until the next file re-anchors? `(partial: roll does not advance it; cross-file re-anchor behavior needs a read trace)`
- Is reader.rs:524 underflow reachable purely via corruption-roll without external truncation? `(needs human input / Antithesis run with mid-file corruption)`

---

## Category 2 — Buffer Accounting & Writer Liveness (the deadlock cluster)

The single highest-value cluster. The in-memory `total_buffer_size` atomic uses
unsaturated `u64` subtraction; a crash/partial-write discrepancy wraps it toward
`2^64`, making `is_buffer_full()` permanently true and deadlocking the writer
(#21683). PR #23561 fixed only the metrics reporter, not the control path.

### total-buffer-size-never-underflows — Accounting Atomic Never Wraps

| | |
|---|---|
| **Type** | Safety |
| **Property** | `decrement_total_buffer_size` is never called with `amount > current total_buffer_size`; the atomic never wraps toward `u64::MAX`. |
| **Invariant** | `Unreachable` for "underflow occurred" (equivalently `Always(amount <= current)`), placed SUT-side at the two unguarded subtraction sites: `ledger.rs:~292` (`fetch_sub`, no saturation) and `reader.rs:~524` (`metadata.len() - bytes_read`). State is invisible to the workload → requires SUT-side instrumentation (missing). |
| **Antithesis Angle** | Node-kill at file-rotation/partial-write boundary; restart; reader seeks through the partial file; `update_buffer_size` (file-size seed) vs. `track_read` (record-byte decrement) mismatch triggers the wrap. |
| **Why It Matters** | Root cause of #21683 → permanent silent writer deadlock. PR #23561 masked only the gauge; the control-path atomic is still raw `fetch_sub`. |

**Open Questions:**

- Is the double-decrement via fast-forward + `track_read` during seek fully blocked by the `!self.ready_to_read` guard (`reader.rs:~468`), or can both fire for the same bytes? `(partial: guard exists for the seek-time path; the delete-time`metadata.len()-bytes_read`path at reader.rs:524 is separate and unguarded)`
- Does the debug-build wrapping subtraction in the `trace!` at `ledger.rs:295` panic before the bug is observable in release semantics?

### writer-eventually-makes-progress — No Permanent Writer Deadlock

| | |
|---|---|
| **Type** | Liveness |
| **Property** | A writer blocked because the buffer was full eventually completes another successful write after the reader acks+deletes a file; the permanent-deadlock state never persists. |
| **Invariant** | **Compound stall detector** (refined per evaluation W-M1/W-O3): the deadlock is *intermittent*, not permanent — `u64::MAX + unflushed_bytes` wraps back to a small value, so the writer escapes for exactly one write whenever `unflushed_bytes > 0`, then re-deadlocks. A naïve `Sometimes(any_wakeup)` or "throughput→0" check therefore false-greens. Use: `write_throughput ≈ 0` AND `sink/ack_throughput ≈ 0` AND `buffer ≥ ~90% full` AND `duration > drain-time bound` ⇒ `assert_unreachable!("persistent_deadlock")`. Both throughputs must be ~0 to distinguish a real deadlock from healthy `WhenFull::Block` backpressure. Keep a `Sometimes(writer_unblocked_after_full)` as the happy-path/baseline liveness milestone. |
| **Antithesis Angle** | Fill buffer; node-kill at rotation/partial-write boundary; restart; resume reader; `ANTITHESIS_STOP_FAULTS` quiet period; assert the writer makes progress. Workload-observable: write throughput resumes. |
| **Why It Matters** | User-visible manifestation of #21683: silent pipeline stall, dashboards look healthy (gauge reads 0 post-#23561), durability promise destroyed, no watchdog. |

**Open Questions:**

- Confirm the `Sometimes` is reachable on the happy path (any normal full→drain cycle) so a clean run establishes a baseline before fault testing.
- Appropriate quiet-period grace for recovery before asserting progress.
- Can the finalizer-task drain on shutdown itself starve the wakeup chain?

### buffer-size-within-max — On-Disk Size Respects max_size

| | |
|---|---|
| **Type** | Safety |
| **Property** | The on-disk buffer never exceeds configured `max_size` (modulo the documented per-record overshoot up to `max_record_size`); the writer blocks rather than over-committing. |
| **Invariant** | `Always` checking `actual_on_disk_bytes <= max_buffer_size + max_record_size`. Best observed by a watchdog summing `.dat` file sizes (not the gauge, which is masked by `saturating_sub`). **Compound-only (refined per evaluation R-C/W-M5): never report this in isolation** — under the underflow/intermittent deadlock the bound holds *vacuously* (no writes ⇒ no overflow). Evaluate jointly with `writer-eventually-makes-progress`; a passing bound is only meaningful while the writer is demonstrably still writing. |
| **Antithesis Angle** | Fill the buffer under faults; verify the bound. **Must be evaluated jointly with `writer-eventually-makes-progress`**: the underflow deadlock makes this bound *vacuously* hold (no writes → no overflow), a false-negative if read alone. |
| **Why It Matters** | The bound itself is rarely violated; the value is detecting the vacuity (passing bound + stalled writer = the deadlock). Secondary: config-reload per-process lock gap; foreign `.dat` files inflating the total. |

**Open Questions:**

- Is the config-reload two-writers-one-dir race exercised by the harness?
- Does the watchdog read file sizes (correct) or the masked gauge?
- Is the per-record overshoot corner (record pushing a `.dat` past 128MB) worth explicit coverage?

---

## Category 3 — Crash Durability & Recovery

What the product sells: durability across crashes. These verify synced data
survives, at-least-once holds end-to-end, and recovery completes without hang,
garbage, or wrong-ID fast-forward. All require node-termination faults.

### durable-unacked-events-survive-crash — Synced, Unacked Events Survive Crash

| | |
|---|---|
| **Type** | Safety |
| **Property** | Every event durably synced (fsync'd) and not yet acknowledged is still readable after an ungraceful crash+restart; none is skipped/lost by recovery. Loss is bounded to the ≤500ms unsynced window. |
| **Invariant** | `Always` (workload-level): the set of events the workload established as durably-written is a subset of the events re-readable after restart. |
| **Antithesis Angle** | SIGKILL at arbitrary points; restart; quiet-period drain; compare delivered vs. durably-written. Both `validate_last_write` branches (`Less` fast-forward `writer.rs:~922`, `Greater` skip `writer.rs:~910`) are exercised. |
| **Why It Matters** | The core durability guarantee. A skip during recovery = silent loss of data the customer entrusted to disk. |

**Open Questions:**

- "Durably written" oracle **decided** (per evaluation R-F/W-F2): use a **wall-clock timestamp** — an event produced more than `2×flush_interval` ago is past the fsync window — and run with `flush_interval=0` so every `flush()` is a `sync_all`. Do NOT use e2e-ack *delivery* as the durability marker: it conflates delivery with fsync and is suppressed by the deadlock (→ vacuous pass).
- `BufferWriter::Drop` (`writer.rs:1371-1374`) does NOT flush — is the "graceful shutdown lossless" claim conditional on the topology calling flush explicitly? (Cross-ref `graceful-shutdown-flushes-all`.)

### every-written-event-eventually-delivered — End-to-End At-Least-Once

| | |
|---|---|
| **Type** | Liveness |
| **Property** | With e2e acks enabled, every event accepted by the source is eventually delivered downstream at least once across crashes (duplicates allowed). |
| **Invariant** | (Refined per evaluation W-O2: `Sometimes(all_produced)` is wrong for at-least-once — it passes on one good timeline, hiding loss on others.) Use **per-event `Always(produced ⊆ delivered)`** checked after each quiet-period drain (every produced ID appears ≥1 in the delivered multiset), plus a `Sometimes(delivery_path_reachable)` exploration hint. Workload tracks a `PRODUCED` set and a `DELIVERED` multiset (dedups duplicates). |
| **Antithesis Angle** | Faults injected throughout; quiet period; drain. Surfaces three silent-loss paths: unlink-before-ledger-flush window (`reader.rs:546-549`), `_status` discard (`ledger.rs:704`), in-flight finalizer tasks not draining on SIGKILL. |
| **Why It Matters** | The end-to-end at-least-once contract Datadog sells for mission-critical pipelines. |

**Open Questions:**

- Does the workload need a source that supports e2e acks, or can it observe delivery directly at a mock sink? Affects topology.
- Duplicates are expected — confirm the workload dedups and only asserts ≥1 (not exactly-once).

### recovery-completes-after-crash — Initialization Completes After Crash

| | |
|---|---|
| **Type** | Liveness |
| **Property** | `Buffer::from_config_inner` (load_or_create → validate_last_write → seek_to_next_record → synchronize_buffer_usage) completes after a kill at any point; it does not hang or fail to init. |
| **Invariant** | `Sometimes(buffer_reinitialized)` after `from_config_inner` returns `Ok` (`mod.rs:251-270`). |
| **Antithesis Angle** | Kill during write/rotation/flush; restart; assert init completes within bounded time (quiet period). |
| **Why It Matters** | If init hangs/fails, the pipeline never starts after a crash — total outage. |

**Open Questions:**

- Init can return `Ok` while the writer is *immediately* deadlocked by the underflow (init completion ≠ runtime liveness) — should this property assert post-init progress too, or leave that to `writer-eventually-makes-progress`? `(partial: agent recommends keeping them separate; init-completes is necessary but not sufficient)`
- L6 init-stall edge: writer must open a next file that doesn't exist yet if killed between `increment_writer_file_id` and file creation — is this reachable?
- Advisory lock not released on some filesystems (NFS) → init permanently fails. Out of scope for local FS?

### partial-write-at-rotation-recovers — Torn-Tail / Rotation Crash Recovers

| | |
|---|---|
| **Type** | Safety + Liveness |
| **Property** | A crash leaving a torn/partial last record, an empty just-created next file, or a ledger/data divergence at the rotation boundary recovers without deadlock and without returning garbage or fast-forwarding to a wrong record ID. |
| **Invariant** | `Sometimes(torn_tail_recovered)` to confirm the path is exercised, plus `Always(no_garbage_delivered)` (covered by `no-corrupted-record-delivered`) and no wrong-ID fast-forward (cross-ref `record-id-monotonicity-holds`). |
| **Antithesis Angle** | Kill precisely during rotation / within the fsync window; small `max_data_file_size` (e.g. 1MB) maximizes rotation frequency. Exercises the F5 torn-tail risk (`archived_root` reads root offset from the buffer's end → garbage offset may pass `CheckBytes`) and the `validate_last_write` `Greater`/`Less` branches. |
| **Why It Matters** | The most credible path to silent skip of synced records (false `Greater`), wrong ledger fast-forward (false `Less`), or the monotonicity panic. The model test cannot reach it (no-op fsync). |

**Open Questions:**

- Does a torn tail produce `RecordStatus::Valid{wrong_id}` (CRC is then the only backstop) or `FailedDeserialization` directly? Determines whether CRC32C reliably backstops torn tails.
- Is the empty-just-created-next-file path (`writer.rs:~1089`, `file_len == 0`) exercised by tests?

---

## Category 4 — Space Reclamation & Clean Termination

Liveness of the read/ack/delete chain. Progress depends on sink acks + finalizer
task alive + reader polled + delete I/O succeeding.

### acked-files-eventually-deleted — Fully-Acked Files Deleted, Space Reclaimed

| | |
|---|---|
| **Type** | Liveness |
| **Property** | Once all records in a data file are acknowledged, the file is eventually unlinked and its bytes subtracted from `total_buffer_size`, even without new writes (`force_check_pending_data_files`). |
| **Invariant** | `Sometimes(data_file_deleted)`: after a full file is acked and a quiet period elapses, the `.dat` is gone and the byte count dropped. |
| **Antithesis Angle** | Write+ack a full file under faults; SIGKILL between finalizer-fire and `delete_file`; `EIO` on delete; **kill the spawned finalizer task** (→ acks never processed → no deletion → eventual writer deadlock). |
| **Why It Matters** | File deletion is the prerequisite for the writer unblocking. No deletion = `total_buffer_size` never reaches 0 = silent stall. |

**Open Questions:**

- Does the tokio runtime drain the finalizer task before shutdown timeout?
- What `bytes_read` is passed after a bad-record roll — can it exceed `metadata.len()` and trigger the underflow? (Cross-ref `total-buffer-size-never-underflows`.)
- Is `force_check_pending_data_files` triggered when the reader is parked and not rolling?

### reader-drains-and-terminates-cleanly — Clean Reader Termination

| | |
|---|---|
| **Type** | Liveness |
| **Property** | When the writer is done and the buffer is fully drained+acked, `reader.next()` returns `Ok(None)` within finite time — no hang, no premature `None` that drops undelivered events. |
| **Invariant** | `Sometimes(reader_returned_none_clean)`. Termination uses `is_writer_done() && total_buffer_size == 0` (`reader.rs:980-985`). |
| **Antithesis Angle** | Stop writes; deliver+ack all; quiet period; force the interleaving where the finalizer fires while the reader is awake (permit consumed) vs. parked. This is exactly the disabled flaky `#23456` path. |
| **Why It Matters** | A hang blocks graceful shutdown (operator must SIGKILL → 500ms loss + sets up #21683 on restart). A premature `None` silently truncates the stream. The underflow also breaks this (`total_buffer_size == 0` never true). |

**Open Questions:**

- Exact root cause of `#23456` flakiness — permit-already-consumed missed wakeup, or something else? Antithesis can answer definitively.
- Is `writer.close()` guaranteed before the reader is asked to terminate in topology shutdown?
- Does the gap-marker `events_skipped` path interact correctly with `total_buffer_size` when skipped bytes are the last outstanding?

---

## Category 5 — Delivery Semantics & Boundary Conditions

Properties that expose known/likely-present bugs (sink-error acks, drop-newest
metric blindness) and boundary arithmetic (file-ID and record-ID rollover).

### sink-failure-not-silently-acked — Errored/Rejected Deliveries Not Silently Dropped

| | |
|---|---|
| **Type** | Safety |
| **Property** | An event whose downstream delivery status is `Errored`/`Rejected` is not silently treated as acknowledged and removed from the buffer. |
| **Invariant** | `Always`: a non-`Delivered` batch status does not advance `reader_last_record`/free the record without retry. **Currently VIOLATED**: the finalizer discards `_status` (`ledger.rs:704`); at-least-once is only restored by a full crash+replay. |
| **Antithesis Angle** | Make the downstream sink Error/Reject under faults; assert events are retained/retried, not dropped. |
| **Why It Matters** | Within a process lifetime, sink errors cause permanent silent loss of data the buffer claims to durably hold. |

**Open Questions:**

- Do sinks actually emit `Errored`/`Rejected` status under normal operation, or only under fault injection? Determines whether this is reachable without faults. `(partial: finalizer discard confirmed at ledger.rs:704; whether sinks emit non-Delivered status in practice not yet traced)`
- Is the discard intentional (retry assumed at the source layer) or a genuine bug? `(needs human input)` — design-owner question.
- **Priority note (evaluation R-H):** this is a deterministic logic bug (discarded status), arguably better caught by an integration test than Antithesis search. Keep as a **workload-side secondary check with no dedicated fault-search budget**; don't shape the fault strategy around it. See Bias B1 in `evaluation/synthesis.md`.

### dropped-events-are-counted — drop_newest Drops Are Component-Visible

| | |
|---|---|
| **Type** | Safety |
| **Property** | When `when_full=drop_newest` drops an event, it is accounted at the component level (`component_discarded_events_total`), not only `buffer_discarded_events_total`. |
| **Invariant** | `Always`: component-visible discard count matches actual buffer drops. **Currently VIOLATED** (#24606/#24144): `BufferEventsDropped::emit` (`internal_events.rs:177-243`) never calls `ComponentEventsDropped`. |
| **Antithesis Angle** | Configure `drop_newest`; overfill under backpressure faults; assert the component-visible count matches drops (workload-observable). |
| **Why It Matters** | Operators monitor `component_discarded_events_total` for data loss; silent absence means drops go undetected (internal config-reload incident-adjacent). |

**Open Questions:**

- Is there any partial fix on a branch not at this commit? `(partial: not present at this commit; grep found no ComponentEventsDropped call in vector-buffers)`
- Is `drop_newest` actually reachable for the disk-buffer variant? `(partial: implementability agent confirmed the disk`try_write_record`returns the item when full, so drop_newest fires — reachable)`
- **Priority note (evaluation R-H):** deterministic missing-emit bug; better caught by an integration test. Keep as a workload-side metric check with no dedicated fault-search budget. See Bias B1.

### file-id-rollover-stays-coordinated — u16 File-ID Rollover Stays Coordinated

| | |
|---|---|
| **Type** | Safety |
| **Property** | Across u16 file-ID rollover (`MAX_FILE_ID`; 6 in tests, 65536 in prod), reader and writer stay coordinated; the seek `reader_file_id > writer_file_id` comparison does not misclassify sync state. |
| **Invariant** | `Always`: the seek sync-gate decision is correct across rollover. **Latent bug**: `reader.rs:~932` uses a raw, non-wrap-aware `u16 >`. |
| **Antithesis Angle** | Slow reader + fast writer to force rollover; kill+restart at the boundary; assert no deadlock/regression. Reachable in tests due to `MAX_FILE_ID=6`. |
| **Why It Matters** | A misclassified sync gate can deadlock the reader (opening a wrapped low-ID file while waiting on a high-ID writer) or regress position. |

**Open Questions:**

- Is the `>` intended to be wrap-aware? Confirm against author intent. (Shared with `record-id-monotonicity-holds`.)
- What is the exact incorrect behavior at the boundary — deadlock vs. silent skip?
- **Build requirement (refined per evaluation R-E):** production `MAX_FILE_ID=65535` needs ~8TB of writes to roll; `MAX_FILE_ID=6` exists only in `#[cfg(test)]`. To exercise this within a timeline, run a **test-build** of Vector or add a runtime-configurable `MAX_FILE_ID` knob; otherwise descope. `(partial: confirmed MAX_FILE_ID is cfg-gated at common.rs:43-45)`
- Does the `unacked_reader_file_id_offset` indirection (`get_current_reader_file_id`, `ledger.rs:305-308`) make the raw `>` more correct than it looks? Needs a deeper trace before treating the bug as definitely triggerable.

### record-id-wraparound-accounting-holds — u64 Record-ID Accounting Holds

| | |
|---|---|
| **Type** | Safety |
| **Property** | At the empty-buffer equality case, event-count accounting stays correct; `get_total_records` never produces a ~2^64 phantom count. (Refocused per evaluation R-D: the *true* u64 record-ID wrap requires ~2^64 writes and is unreachable on a production binary — explicitly descoped; the reachable, real bug is the empty-buffer case below.) |
| **Invariant** | `Always`: `get_total_records()` (`ledger.rs:266`) returns a sane count. **Bug**: the outer `- 1` is a plain (non-wrapping) subtraction; when `next == last` (drained buffer), `wrapping_sub` → 0 then `0 - 1` → `u64::MAX`, poisoning `synchronize_buffer_usage` on every clean restart of a drained buffer. Workload-observable — no SUT instrumentation needed. |
| **Antithesis Angle** | Drain the buffer completely; restart Vector; scrape `buffer_size_bytes`/`buffer_size_events` and assert near 0 (not ~2^64). No node-kill required (clean restart suffices), so this is reachable even without crash faults. |
| **Why It Matters** | Poisons buffer metrics (debug: panic; release: silent 2^64), undermining all buffer-occupancy observability. |

**Open Questions:**

- The true u64 record-ID wrap is unreachable on a production binary (test-only `unsafe_set_*` helpers are `#[cfg(test)]`-gated) — descoped; only the empty-buffer case is in scope.
- Does the debug-build `synchronize_buffer_usage` path panic on the `0 - 1` before release semantics are observable (as with the `total-buffer-size` `trace!`)? Use a release build to observe the ~2^64 gauge. `(partial: empty-buffer reachability confirmed — fires on every clean restart of a drained buffer; debug-vs-release panic behavior not yet confirmed for this specific site)`

---

## Category 6 — Lifecycle / Config Reload

Operations that span lifecycle transitions rather than steady state. Config
reload is directly implicated in the internal config-reload incident.

### config-reload-no-silent-loss — Config Reload Doesn't Silently Drop Buffered Events

| | |
|---|---|
| **Type** | Safety |
| **Property** | Reloading Vector config (drop + recreate the disk-buffer writer/sink) does not silently drop events already accepted into the buffer. |
| **Invariant** | `Always`: `accepted == forwarded + explicitly_discarded` across a reload. **At risk**: `BufferWriter::Drop` calls `close()` but NOT `flush()` (`writer.rs:1366-1374`) → up to 256KB buffered-but-unflushed events discarded; `track_dropped_events` charges `byte_size=0` → accounting drift. |
| **Antithesis Angle** | Custom fault: SIGHUP/config-reload under sustained write load with a busy reader; quiet period + drain; assert no accepted event lost. Also exercises the per-process advisory-lock gap (old+new topology opening the same buffer). |
| **Why It Matters** | Directly tied to the internal config-reload incident (#24948 / PR #24949). |

**Open Questions:**

- Does PR #24949 fix the *loss* or only the *stall*/liveness? `(partial: PR addressed the stall and a detach-trigger; whether the Drop-without-flush loss is fixed not confirmed at this commit)`
- Is the old/new topology overlap actually concurrent (making the lock gap a live safety issue)?
- Reload requires a custom fault — is SIGHUP-driven reload feasible in the harness, or must the workload drive it via the API?

### graceful-shutdown-flushes-all — Graceful Shutdown Is Lossless

| | |
|---|---|
| **Type** | Liveness |
| **Property** | On graceful shutdown, all buffered data is flushed/synced before exit (the doc's "no data loss on graceful shutdown"). |
| **Invariant** | `Sometimes(graceful_shutdown_lossless)`: after a graceful stop + restart + drain, all pre-shutdown events are present. Candidate SUT-side assertion: `Always(unflushed_bytes == 0)` inside `close()`. |
| **Antithesis Angle** | Graceful stop (SIGTERM, not kill) under load; restart; drain; assert zero loss. Contrast with the ungraceful-crash 500ms-window property. |
| **Why It Matters** | The product distinguishes graceful shutdown (lossless) from crash (≤500ms loss). `Drop` cannot call async `flush()`, so the guarantee depends on the topology flushing before drop — unverified. |

**Open Questions:**

- Does Vector topology shutdown call `writer.flush()` before dropping the writer? `(partial: RunningTopology::stop drops inputs containing BufferSender; whether the write loop's final flush completes first is a race — needs tracing of the shutdown ordering)`
- Does graceful SIGTERM rely on OS page-cache flush-on-exit (not a Vector guarantee) to cover the gap?

---

---

## Category 7 — Cross-Cutting & Operational Gaps (from evaluation gap-filling)

Properties the focus-based discovery missed: non-crash paths to the #21683 stall,
clock-fault and overflow-mode coverage, version-upgrade format safety, the
finalizer single-point-of-failure, and lock-contention throughput collapse.

### foreign-data-file-no-writer-stall — Foreign `.dat` File Does Not Permanently Stall the Writer

| | |
|---|---|
| **Type** | Safety |
| **Property** | A stray/leftover/operator-placed `.dat` file in the buffer dir inflates startup `total_buffer_size` but is never read or decremented; the writer must still eventually make progress once real content is below `max_size`. |
| **Invariant** | `Always(writer_makes_progress_after_drain)`: a foreign `.dat` does not hold `is_buffer_full()` permanently true. `update_buffer_size` (`ledger.rs:~681`) sums ANY `*.dat`, not just `buffer-data-{id}.dat`. |
| **Antithesis Angle** | Workload/custom-fault places a large `foreign.dat`; restart; quiet-period; assert writes resume. **No node-kill needed** — pure operator-error path to the #21683 symptom (distinct root cause: wrong scan scope, not arithmetic). |
| **Why It Matters** | Permanent silent stall with no crash; gauge masked by `saturating_sub`. Non-crash reachability makes it testable even if node-kill faults are disabled. |

**Open Questions:**

- Fix direction: filter by `buffer-data-{N}.dat` prefix, or assert-and-reject unknown `.dat`?
- Note: a foreign file also inflates the `.dat`-summing watchdog used by `buffer-size-within-max` — risk of a false-fail there; the watchdog must filter to self-owned files.

### ledger-corruption-no-sigbus-crashloop — Ledger Corruption Is a Clean Error, Not SIGBUS/Crash-Loop

| | |
|---|---|
| **Type** | Safety |
| **Property** | External truncation/corruption of the mmap'd `buffer.db` yields a clean `LedgerLoadCreateError` at init, not a SIGBUS mid-operation or an infinite crash loop. |
| **Invariant** | `AlwaysOrUnreachable`: corruption is caught at load via rkyv `CheckBytes` (`backed_archive.rs:~73`); SIGBUS mid-operation (no handler exists, `io.rs`) and crash-loop are `Unreachable`. |
| **Antithesis Angle** | Filesystem fault: truncate/corrupt `buffer.db` while stopped or while mapped; assert clean restart or clean error, never exit-138 crash-loop. **Requires filesystem-fault capability — flag to user** (may not be available). |
| **Why It Matters** | `buffer.db` is `mmap`'d; live truncation SIGBUSes on the next field access. The init-time `CheckBytes` only guards the load, not live truncation. An all-zeros `LedgerState` may pass `CheckBytes` (silent reset). |

**Open Questions:**

- Does the all-zeros `LedgerState::default()` layout pass `CheckBytes` (→ truncation-to-zero is a silent reset, not a detected error)?
- Should `load_or_create` validate `buffer.db` is exactly `LEDGER_LEN` bytes before mmap?
- Is filesystem-fault injection available in the tenant? `(needs human input)`

### finalizer-task-drains-pending-acks — Finalizer Task Drains All In-Flight Acks

| | |
|---|---|
| **Type** | Liveness |
| **Property** | All in-flight `BatchNotifier` acks are eventually processed (steady state and on graceful shutdown) and not permanently stranded by a dead/abandoned finalizer task. |
| **Invariant** | `Sometimes(all_acks_drained)` after a quiet period; pair with `Unreachable(finalizer_died_with_acks_pending)`. The `spawn_finalizer` task (`ledger.rs:701-710`) is **unmonitored/detached** (discarded `JoinHandle`); a panic logs "FinalizerSet task ended prematurely" but the reader continues silently → no ack processing → no deletion → eventual stall, distinct from the arithmetic deadlock. |
| **Antithesis Angle** | SIGKILL with acks in flight (assert replay, not silent loss); inject a finalizer-task panic (assert no indefinite hang); SIGTERM (assert `pending_acks==0` before exit). |
| **Why It Matters** | A dead finalizer is a silent-loss/stall path that no other property isolates. |

**Open Questions:**

- Does the tokio runtime drain spawned tasks before exit? `(partial: tokio Runtime::drop has a ~2s task-completion window; no explicit join of the finalizer)`
- Can the `FuturesOrdered` queue grow unbounded under a sink that never acks?

### fsync-window-bounded-under-clock-jitter — fsync Window Bounded Under Clock Jitter

| | |
|---|---|
| **Type** | Safety |
| **Property** | Under clock-jitter faults, the durable-loss window stays bounded; a slowed clock cannot suppress `sync_all` indefinitely beyond the next file rotation. |
| **Invariant** | `Always`: time since the last `sync_all`+`ledger.flush()` pair (real wall time) stays within a bounded multiple of `flush_interval`, OR data is durable since the last rotation. `should_flush` (`ledger.rs:485-497`) gates fsync on `Instant::elapsed()`; only rotation (`force_full_flush`) is clock-independent. |
| **Antithesis Angle** | Enable clock jitter; crash; assert loss bounded by the last rotation. Also covers the CAS-winner-descheduled extension. Mitigation/oracle: `flush_interval=0` removes the clock dependence (used by the durability properties). **Requires clock-fault capability — flag to user.** |
| **Why It Matters** | The product claims a ≤500ms loss window; clock jitter can silently break it with no error/log. At low write rates rotation may never fire → unbounded window. |

**Open Questions:**

- Does Antithesis virtual-time affect `std::time::Instant::elapsed()` on the target runtime (and `crossbeam AtomicCell<Instant>`)?
- What is the max loss window when write volume is too low to trigger rotation?

### overflow-chain-no-unaccounted-gap — Overflow Chain Crash Leaves No Silent Middle Gap

| | |
|---|---|
| **Type** | Safety |
| **Property** | With `WhenFull::Overflow` (disk base → in-memory overflow), a crash during an overflow-active period does not create a silent, unaccounted middle-of-stream gap (later in-memory events lost while earlier disk events survive), and ordering is honored or documented. |
| **Invariant** | `Always`: every event accepted into the durable disk base that survives the crash is delivered; the unbiased `select!` (`receiver.rs:~133`) does not let a later overflow event replace an earlier disk event. `was_dropped=true` at `sender.rs:~238` (overflow dispatch) must not misclassify dispatched-to-overflow as permanently lost. |
| **Antithesis Angle** | Second topology config (disk + in-memory overflow); fill to overflow; crash; drain; assert no silent middle gap. |
| **Why It Matters** | The entire overflow mode is untested; the crash asymmetry produces a stream gap (not just duplicates) that dedup-based at-least-once reasoning can't handle. |

**Open Questions:**

- Does overflow chain to a distinct in-memory buffer with its own capacity? Confirm `BufferSender::with_overflow`.
- Is the unbiased `select!` ordering intentional/documented?
- Can overflow-and-drain cycles confuse the base `total_buffer_size` (secondary underflow trigger)?

### buffer-survives-version-upgrade — Buffer Files Survive Upgrade or Fail Cleanly

| | |
|---|---|
| **Type** | Safety + Liveness |
| **Property** | Buffer files written by version N are read back correctly by version N (ideally N+1); a format/layout change (rkyv) or the `DiskBufferV1CompatibilityMode` flag is handled as a clean detected error, never silent garbage. |
| **Invariant** | `Sometimes(upgrade_readback_ok)` for same-version baseline; `Always(DeserializeError)` (never `Valid{garbage}`) under a simulated layout change; `AlwaysOrUnreachable` that a current-binary record (always carries the V1-compat flag) is accepted by `can_decode` (`vector-core/event/ser.rs:~86-91`). |
| **Antithesis Angle** | Write with binary version N; simulate format change (modify `.dat` bytes or swap binaries via custom fault); restart; assert clean error or correct readback, never garbage/monotonicity-panic. **Custom fault (binary swap) needed.** |
| **Why It Matters** | No runtime mechanism detects an rkyv layout mismatch (CheckBytes validates types not version; CRC matches the new layout). A layout-changed record could pass both checks and deliver garbage. The compat-flag is a forward-compat foot-gun. |

**Open Questions:**

- Is the mmap'd `LedgerState` versioned? An upgrade changing it reads wrong offsets — a second upgrade risk not covered by record-level `CheckBytes`.
- Split into two slugs (rkyv layout vs. compat-flag)?
- Can a layout-changed small payload pass both `CheckBytes` and CRC32C → `Valid{wrong_id}` → monotonicity panic?

### throughput-progresses-under-contention — Throughput Stays Above Floor Under Contention

| | |
|---|---|
| **Type** | Liveness |
| **Property** | With N≥4 parallel sources sharing the single `Arc<Mutex<BufferWriter>>` and CPU throttle active, write throughput stays above a configurable floor — distinguishing "degenerate-but-alive" (lock starvation) from healthy and from deadlocked. |
| **Invariant** | `Sometimes(throughput_above_floor)` over an observation window. Catches a regression to near-zero progress that the permanent-deadlock property (`is_buffer_full` forever true) is blind to. |
| **Antithesis Angle** | N≥4 parallel senders → one disk sink; CPU throttle; assert `Sometimes(throughput > floor)` in a quiet window. Triangulate with `writer-eventually-makes-progress`: progress fires but throughput-floor fails → degenerate; neither → deadlocked. |
| **Why It Matters** | The ~90 MiB/s contention ceiling (GA doc) can collapse to near-zero under throttle without deadlocking — a regression CI can't catch and deadlock-detection misses. |

**Open Questions:**

- Appropriate floor (calibrate vs. an unthrottled single-sender baseline, e.g. 0.1%)?
- Borders on perf-testing — frame value as catching *near-zero* progress, not micro-benchmarking. (See Bias B1.)
- Does `tokio::sync::Mutex` FIFO fairness amplify throttle-induced starvation?

---

## File-Level Open Questions (catalog-wide)

- **Node-termination (kill/restart) faults + persistent buffer storage: CONFIRMED ENABLED by the user (2026-05-28).** The crash-recovery cluster is testable. (Resolved.)
- Config-reload (Category 6) and sink-error injection (`sink-failure-not-silently-acked`) need **custom faults** — feasibility depends on the harness/workload design.
- **`(needs human input)` — surfaced from individual properties (require a design owner / tenant operator):**
  - Is the finalizer's `BatchStatus` discard (`ledger.rs:704`) intentional (retry assumed at the source layer) or a genuine bug? — `sink-failure-not-silently-acked`.
  - Is **filesystem-fault injection** (truncating/corrupting `buffer.db` and `.dat` files) available in the tenant? Gates `ledger-corruption-no-sigbus-crashloop` and the filesystem-tamper angle of several Category-7 properties.
- Does Vector's topology call `writer.flush()` on graceful shutdown, and does the tokio runtime drain the finalizer task before exit? Both affect multiple liveness/durability properties. `(partial: tokio Runtime::drop has a ~2s task window; the running.rs stop() drop-order vs. final flush is unresolved and needs code tracing)`
- **RESOLVED — "Durably written" oracle:** use a wall-clock timestamp (event produced > 2×`flush_interval` ago) and run with `flush_interval=0` so every flush is an fsync. Do NOT use e2e-ack delivery as the durability marker (conflates delivery with fsync; suppressed by the deadlock). Reused across Category 3 (refinement R-F).
- **Build profile:** run a **release build** for underflow/wrap-observing properties (debug `trace!`/arithmetic panics first); run a **test build** (or add a runtime `MAX_FILE_ID` knob) for `file-id-rollover-stays-coordinated`.
</content>
