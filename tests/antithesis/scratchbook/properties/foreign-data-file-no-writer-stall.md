---
slug: foreign-data-file-no-writer-stall
catalog_category: 2 — Buffer Accounting & Writer Liveness
type: Safety / Always
status: cataloged (Category 7)
related:
  - total-buffer-size-never-underflows
  - writer-eventually-makes-progress
  - buffer-size-within-max
commit: 049eec79b737450c4669b7f8aa1dd814551ec466
updated: 2026-06-02
---

### foreign-data-file-no-writer-stall — Foreign `.dat` File Does Not Permanently Stall the Writer

| | |
|---|---|
| **Type** | Safety |
| **Property** | A stray `.dat` file placed in the buffer data directory before startup (by an operator, a prior process, a symlink, etc.) inflates `total_buffer_size` at init but is never read by the reader and therefore never decremented; the writer must still eventually make progress and must not be permanently deadlocked. |
| **Invariant** | `Always(writer_makes_progress_after_drain)`: even with a foreign `.dat` file present, after the reader has processed and acked its legitimate data files, the writer is not permanently stalled. Equivalently: the over-seeded `total_buffer_size` does not hold `is_buffer_full()` permanently true when the buffer's actual content is below `max_buffer_size`. |
| **Antithesis Angle** | Custom fault / workload: (1) fill and partially drain the buffer to establish a baseline; (2) inject a stray `foreign.dat` file into the buffer data directory; (3) restart Vector; (4) `ANTITHESIS_STOP_FAULTS` quiet period; (5) assert that the writer resumes accepting writes within a bounded time. The foreign file must be large enough to push the over-seeded `total_buffer_size` above `max_buffer_size`, simulating the deadlock condition. No node-kill needed — this is a non-crash, operator-error path. |
| **Why It Matters** | This is a distinct, non-crash path to the #21683 permanent writer stall. It requires only an operator mistake (or a leftover `.dat` from a prior cleanup) and a restart — no crash, no race, no timing luck. The condition is silent: the writer hangs indefinitely, `is_buffer_full()` is forever true, dashboards may appear healthy (post-PR-#23561 the gauge reads 0 due to `saturating_sub` masking), and no error is emitted. |

---

## What Led to This Property

The `update_buffer_size` function (`ledger.rs:680–724`) is called once during
`Ledger::load_or_create` (`ledger.rs:675`) to seed `total_buffer_size`. Its
implementation reads the buffer data directory and sums
the size of **every file whose name ends in `.dat`** (`ledger.rs:708`):

```rust
if file_name.ends_with(".dat") {
    let metadata = dir_entry.metadata().await.context(IoSnafu)?;
    total_buffer_size += metadata.len();
```

The predicate is `ends_with(".dat")` — a suffix check with no further
validation against the expected `buffer-data-{id}.dat` pattern. The comment at
`ledger.rs:703–707` explicitly acknowledges this: the author wanted only
lowercase `.dat` files but made no attempt to filter by name prefix, accepting
any compliant extension.

The accumulated sum is applied unconditionally:

```rust
self.increment_total_buffer_size(total_buffer_size);  // ledger.rs:722
```

This uses `fetch_add` with no saturation (`ledger.rs:297`). The resulting
`total_buffer_size` then feeds directly into `is_buffer_full()` in `writer.rs`:

```rust
fn is_buffer_full(&self) -> bool {                          // writer.rs:993
    let total_buffer_size =
        self.ledger.get_total_buffer_size() + self.unflushed_bytes;
    let max_buffer_size = self.config.max_buffer_size;
    total_buffer_size >= max_buffer_size
}
```

and into `can_write_record()` (`writer.rs:793–798`). If the foreign `.dat` file
is large enough to push `total_buffer_size >= max_buffer_size` at startup, the
writer enters the `ensure_ready_for_write` wait loop (`writer.rs:1001–1020`)
and never exits, because there is nothing to ever decrement the contribution
from the foreign file.

The reader decrements `total_buffer_size` only by **record bytes it has actually
read** (via `track_reads`, called from `reader.rs:635`) and by the
**file-size minus bytes-read** delta when deleting completed data files
(`reader.rs:489–549`, calling `decrement_total_buffer_size` at reader.rs:549).
Neither path
reaches the foreign file, because the foreign file is not a valid `buffer-data-{id}.dat`
file (it won't match the file-ID sequence the reader follows) and will never be
opened, read, or deleted by the reader. The inflated seed never gets
decremented.

The writer's wakeup chain (`notify_writer_waiters` → `wait_for_reader` →
`notify_reader_waiters`) is sound, but it is conditioned on the reader
delivering acks that flow through the finalizer task. If the inflation from the
foreign file is larger than `max_buffer_size - actual_content`, the writer
remains blocked forever even after the buffer is completely drained of
legitimate data.

This was flagged in the SUT analysis (§6 item 9) as part of the
"mmap SIGBUS / external file tampering" cluster, but it is a pure
non-crash, operator-error path that deserves its own property: it requires no
node kill, no timing luck, and no concurrent fault — only a stray `.dat` file
and a restart.

---

## Code References

| Location | Relevance |
|---|---|
| `lib/vector-buffers/src/variants/disk_v2/ledger.rs:680–724` | `update_buffer_size` — scans `data_dir`, sums all `*.dat` files without name-prefix filtering |
| `lib/vector-buffers/src/variants/disk_v2/ledger.rs:708` | Exact predicate: `file_name.ends_with(".dat")` — no `buffer-data-` prefix check |
| `lib/vector-buffers/src/variants/disk_v2/ledger.rs:722` | `increment_total_buffer_size(total_buffer_size)` — unconditional seed |
| `lib/vector-buffers/src/variants/disk_v2/ledger.rs:297` | `fetch_add` with no saturation on `total_buffer_size` |
| `lib/vector-buffers/src/variants/disk_v2/ledger.rs:306–319` | `decrement_total_buffer_size` — raw `fetch_sub`, no saturation; committed underflow detector asserts at ledger.rs:313 |
| `lib/vector-buffers/src/variants/disk_v2/writer.rs:993–997` | `is_buffer_full` — reads `total_buffer_size` directly |
| `lib/vector-buffers/src/variants/disk_v2/writer.rs:1001–1020` | `ensure_ready_for_write` — permanent wait loop if `is_buffer_full` |
| `lib/vector-buffers/src/variants/disk_v2/reader.rs:489–549` | `delete_completed_data_file` — decrements by `metadata.len() - bytes_read` (decrement at 549; underflow detector asserts at reader.rs:529); only runs for self-owned files |
| `lib/vector-buffers/src/variants/disk_v2/reader.rs:635` | `track_reads` — decrements by record bytes; only runs for records actually read |

---

## What Breaks

**Failure mode:** the writer hangs permanently in
`ensure_ready_for_write` → `ledger.wait_for_reader().await` (`writer.rs:1019`).
No error is logged (only a `trace!` at `writer.rs:1013`). No crash. The pipeline
stalls silently.

**Severity:** same user-visible impact as #21683 — silent pipeline stall with
healthy-looking dashboards (the buffer gauge is masked by `saturating_sub` since
PR #23561). Durability promise destroyed without any observable signal.

**Threshold calculation:** for a `max_buffer_size = 256MB` buffer with no
legitimate data, placing a `foreign.dat` file of ≥ 256MB (or several smaller
ones totaling ≥ 256MB) is sufficient to trigger the stall. In a typical
production buffer with `max_buffer_size` in the GB range, the threshold is
higher, but the vector is still operator-accessible: a single misplaced large
`.dat` file (e.g., a leftover from manual inspection or a prior failed migration)
can trigger it.

**Difference from the #21683 underflow path:** the #21683 path wraps
`total_buffer_size` toward `u64::MAX` via an underflow, making the stall
essentially unrecoverable until a fresh buffer is created. The foreign-file path
over-seeds `total_buffer_size` to a large-but-finite value. If the foreign file
is subsequently removed and Vector is restarted, the stall resolves — so there
is a recovery path, but it requires operator intervention (remove the file,
restart). This makes it a Safety violation (a foreign file deadlocks the writer)
rather than a permanent corruption.

---

## Fault Conditions

1. **No node-kill needed.** The stall is triggered on a normal startup with a
   stray file present. A SIGKILL + restart sequence is the most realistic
   delivery mechanism (crash leaves a temporary file in the buffer dir), but the
   property holds even without crash faults.

2. **Operator-accessible.** The buffer data dir path is user-configurable and
   often writable. An operator `cp`ing a file into the wrong directory, a
   symlink, or a `.dat` file left by a prior Vector version or migration script
   can trigger this.

3. **Filesystem fault delivery in Antithesis.** Antithesis can place a file in
   the buffer directory at any point via workload logic (no special fault
   primitive needed — just a file-write before Vector restarts). This is a
   pure-workload exercise, not a filesystem fault.

---

## SUT Instrumentation (not yet committed)

The Antithesis SDK is a committed dependency under the `antithesis` feature, and
three underflow `assert_always_greater_than_or_equal_to!` detectors exist
(ledger.rs:271/313, reader.rs:529; see existing-assertions.md). None covers the
foreign-file over-seed path. The following SUT-side assertions would make this
property automatically testable:

1. **`Always` assertion in `update_buffer_size`** (`ledger.rs:708`): before
   accumulating a `.dat` file's size, assert that the filename matches the
   expected `buffer-data-{N}.dat` pattern. If it does not, log a `warn!` and
   skip it (which would also fix the bug). Alternatively, assert
   `total_buffer_size_after_seed <= max_buffer_size` immediately after
   `increment_total_buffer_size` at `ledger.rs:722` — an `Always` assertion
   whose violation proves the seed-overrun condition.

2. **`Sometimes` writer-progress assertion** in `ensure_ready_for_write`
   (`writer.rs` post-wait-loop): once a writer wakes from `wait_for_reader`,
   assert it makes progress (the `Sometimes` reachability assertion already in
   `writer-eventually-makes-progress`). With a permanently stalled writer this
   assertion is never reached — Antithesis will flag the `Sometimes` as
   "never observed."

3. **Workload-level observation:** the workload can assert that write throughput
   resumes after a quiet-period drain even when a foreign `.dat` was present at
   startup. No SUT modification required for this path.

---

## Open Questions

- Should `update_buffer_size` filter by the `buffer-data-{N}.dat` naming
  pattern (a fix), or assert and reject on unknown `.dat` files (a safer
  defensive posture that surfaces the operator error)? The fix and the
  assertion are different choices with different user-facing behavior.

- Is the `data_dir` directory shared with any other Vector component or
  external tool that might legitimately write `.dat` files? (If so, the
  over-seeding becomes unavoidable without a stricter naming contract.)

- Can `update_buffer_size` encounter a non-`buffer-data-{N}.dat` file during
  normal operation (e.g., from a failed atomic file creation leaving a
  partial name)? If yes, this is a normal-operation trigger, not just an
  operator-error trigger.

- After the foreign file inflates `total_buffer_size`, is there any existing
  code path that would eventually decrement it back to a correct value (e.g.,
  if the reader somehow opens the foreign file)? Confirmed no: `delete_completed_data_file`
  only runs for file IDs in `[reader_current_data_file .. writer_current_data_file]`;
  a foreign file outside that ID range is unreachable from the reader.

- How does this interact with the `buffer-size-within-max` property? That
  property uses actual `.dat` file sizes as the ground truth. With a foreign
  `.dat` in the directory, the watchdog sum would also be inflated — the
  property's `Always(actual_disk_bytes <= max_buffer_size + max_record_size)`
  could falsely fail even though the buffer's own data is within bounds.
