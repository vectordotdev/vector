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
| **Property** | A stray `.dat` placed in the buffer data dir before startup (operator, prior process, symlink) inflates `total_buffer_size` at init but is never read or decremented; the writer must still eventually make progress, not deadlock. |
| **Invariant** | `Always(writer_makes_progress_after_drain)`: with a foreign `.dat` present, after the reader acks its legitimate files the writer is not permanently stalled. The over-seeded `total_buffer_size` does not hold `is_buffer_full()` true when actual content is below `max_buffer_size`. |
| **Antithesis Angle** | (1) fill + partially drain for a baseline; (2) inject `foreign.dat` into the data dir; (3) restart; (4) `ANTITHESIS_STOP_FAULTS` quiet period; (5) assert the writer resumes within a bounded time. The foreign file must be large enough to push `total_buffer_size` above `max_buffer_size`. No node-kill — a non-crash operator-error path. |
| **Why It Matters** | A distinct non-crash path to the #21683 stall, needing only an operator mistake (or a leftover `.dat`) plus a restart — no crash, race, or timing luck. Silent: the writer hangs, `is_buffer_full()` forever true, dashboards healthy (gauge 0 post-PR-#23561), no error. |

---

## What Led to This Property

`update_buffer_size` runs once during `Ledger::load_or_create` to seed
`total_buffer_size`. It sums the size of **every file whose name ends in `.dat`**:

```rust
if file_name.ends_with(".dat") {
    let metadata = dir_entry.metadata().await.context(IoSnafu)?;
    total_buffer_size += metadata.len();
```

The predicate is `ends_with(".dat")` — a suffix check with no validation against
the expected `buffer-data-{id}.dat` pattern. The comment acknowledges this:
lowercase `.dat` only, no prefix filter, any compliant extension accepted.

The sum is applied unconditionally via `increment_total_buffer_size` (`fetch_add`,
no saturation), feeding `is_buffer_full()`:

```rust
fn is_buffer_full(&self) -> bool {
    let total_buffer_size =
        self.ledger.get_total_buffer_size() + self.unflushed_bytes;
    total_buffer_size >= self.config.max_buffer_size
}
```

and `can_write_record()`. If the foreign `.dat` pushes `total_buffer_size >=
max_buffer_size` at startup, the writer enters the `ensure_ready_for_write` wait
loop and never exits — nothing decrements the foreign file's contribution.

The reader decrements `total_buffer_size` only by **record bytes actually read**
(`track_reads`) and by the **file-size minus bytes-read** delta when deleting
completed data files (`decrement_total_buffer_size`). Neither reaches the foreign
file: it doesn't match the `buffer-data-{id}.dat` ID sequence the reader follows,
so it's never opened, read, or deleted. The inflated seed never decrements.

The writer's wakeup chain (`notify_writer_waiters` → `wait_for_reader` →
`notify_reader_waiters`) is sound but conditioned on reader acks through the
finalizer. If the foreign inflation exceeds `max_buffer_size - actual_content`, the
writer blocks forever even after legitimate data fully drains.

A pure non-crash operator-error path: no kill, no timing luck, no concurrent fault
— a stray `.dat` and a restart.

---

## What Breaks

The writer hangs permanently in `ensure_ready_for_write` →
`wait_for_reader().await` with only a `trace!`, no crash — same user-visible
impact as #21683 (silent stall, gauge masked to 0 by PR #23561). Threshold: a
foreign `.dat` totaling ≥ the buffer's free space below `max_buffer_size` (≥256 MiB
for a 256 MiB empty buffer).

Difference from #21683: that path WRAPS toward `u64::MAX` (unrecoverable without a
fresh buffer); the foreign-file path over-seeds to a large-but-FINITE value, so
removing the file and restarting resolves it — a Safety violation, not permanent
corruption. Fix: a `buffer-data-{N}.dat` prefix filter in `update_buffer_size` (or
assert+reject unknown `.dat`). Confirmed no path decrements the foreign
contribution — `delete_completed_data_file` only runs for file ids in the
reader-to-writer range, outside which a foreign file sits.

---

## Fault Conditions and coverage

No node-kill needed — normal startup with a stray file triggers it; a crash leaving
a temp `.dat` is just the most realistic delivery. The `data_dir` is user-configurable
and often writable, so an operator `cp`, symlink, or version-migration leftover can
cause it. Antithesis delivers it as pure workload logic (write the file before
restart, mount the buffer volume into the workload container) — no special fault
primitive. SUT-side covered by the committed underflow detectors plus the
post-recovery liveness probe (`_shelved.md`); a post-seed `total_buffer_size <=
max_buffer_size` `assert_always` in `update_buffer_size` would catch the over-seed
directly and is not committed.

Interaction caveat for the shelved `buffer-size-within-max` watchdog: a foreign
`.dat` inflates the file-sum ground truth too, so it could falsely fail — measure
only `buffer-data-{N}.dat` files.
