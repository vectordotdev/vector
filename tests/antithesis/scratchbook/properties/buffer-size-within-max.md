---
slug: buffer-size-within-max
type: Safety / Always
sut_path: lib/vector-buffers/src/variants/disk_v2/
commit: b7aae737cef5dd37d1445915443a1eb97b584f85
updated: 2026-05-28
linked_claims:
  - INV-7 in sut-analysis.md §5 ("Buffer never exceeds max_size")
  - INV-3 in sut-analysis.md §5 (per-record overshoot caveat)
linked_bugs:
  - vectordotdev/vector#21683 (underflow makes this vacuously true via deadlock)
---

# Property: buffer-size-within-max

## Catalog Entry

**Type:** Safety / Always

**Property:** The total on-disk buffer size (sum of all `buffer-data-N.dat` file
sizes) never exceeds the configured `max_buffer_size`, except for the documented
single-record overshoot allowance (up to `max_record_size` bytes past the
data-file 128MB limit, per INV-3).

More precisely, two sub-invariants must hold simultaneously:

**INV-A (accounting):** The in-memory `total_buffer_size` accurately reflects the
true on-disk content at all stable points (i.e., when no write or delete is
in-progress). `|total_buffer_size - actual_disk_bytes| <= max_record_size`.

**INV-B (backpressure):** The writer never successfully writes a record that would
cause `total_buffer_size > max_buffer_size`. The writer blocks instead.

**Invariant:** For all points in time when the writer has completed a write and
the ledger has been updated:

```
actual_on_disk_data_bytes <= max_buffer_size + max_record_size
```

And the gate enforcing this is never bypassed:

```rust
// writer.rs:793-798  — can_write_record
self.can_write() && total_buffer_size + potential_write_len <= self.config.max_buffer_size
```

**Antithesis Angle:** Fill the buffer to capacity under fault conditions. Verify
the actual on-disk byte total (measured by the workload or a watchdog process
enumerating `.dat` files) remains bounded. Confirm the writer blocks rather than
over-commits. Most importantly, verify that "holds within max" is a meaningful
result by cross-checking that the writer is *still making progress* (ruling out
vacuous satisfaction via deadlock). See the deadlock-vacuity subtlety below.

**Why It Matters:** Users configure `max_buffer_size` to control disk space usage.
A buffer that silently exceeds this limit causes disk-full errors for the host,
filling OS buffers and potentially starving other processes. The inverse failure
(deadlock via underflow) is equally harmful: the buffer appears not to exceed
the limit because no new data is being written — a false negative.

---

## Source-Level Enforcement

### Write-side gate

Two independent checks gate every write:

**1. `can_write_record` (writer.rs:793-798):**

```rust
fn can_write_record(&self, amount: usize) -> bool {
    let total_buffer_size = self.ledger.get_total_buffer_size() + self.unflushed_bytes;
    let potential_write_len =
        u64::try_from(amount).expect("Vector only supports 64-bit architectures.");
    self.can_write() && total_buffer_size + potential_write_len <= self.config.max_buffer_size
}
```

- `get_total_buffer_size()` loads the `total_buffer_size` atomic (ledger.rs:276-278).
- `self.unflushed_bytes` is a writer-local counter (not atomic) tracking bytes
  written to the `TrackingBufWriter` not yet flushed to the data file.
- The combined sum must not exceed `max_buffer_size` for the write to proceed.

**2. `is_buffer_full` (writer.rs:993-996):**

```rust
fn is_buffer_full(&self) -> bool {
    let total_buffer_size = self.ledger.get_total_buffer_size() + self.unflushed_bytes;
    let max_buffer_size = self.config.max_buffer_size;
    total_buffer_size >= max_buffer_size
}
```

Called by `ensure_ready_for_write` (writer.rs:1001-1019) which blocks the writer
until `is_buffer_full()` is `false`.

Note: `can_write_record` uses `<= max_buffer_size` (allows writing right up to the
limit) while `is_buffer_full` uses `>= max_buffer_size` (blocks at the limit).
These are logically equivalent at the boundary but represent two separate call
sites with duplicate logic — a maintainability concern if either diverges.

### Accounting update on write

`track_write` (ledger.rs:386-390) calls `increment_total_buffer_size(record_size)`,
where `record_size` is the full serialized record length (header + payload).

```rust
pub fn track_write(&self, event_count: u64, record_size: u64) {
    self.increment_total_buffer_size(record_size);
    // ...
}
```

The increment uses `fetch_add` with no overflow guard (ledger.rs:282). In theory
a record_size could be up to `max_record_size` (128MB); `fetch_add` of 128MB to
a value near u64::MAX wraps — a separate theoretical overflow path (not observed
in practice given max_buffer_size is bounded, but worth noting for completeness).

### Per-record overshoot (INV-3 / documented)

`can_write` (writer.rs:789-791) additionally checks `data_file_size < max_data_file_size`.
A writer may write a record that *individually* pushes a single data file beyond
128MB by up to `max_record_size`. This is documented behavior, not a bug.
The effective bound on any one data file is therefore `max_data_file_size + max_record_size`.
The buffer-level bound `max_buffer_size` is enforced independently.

---

## The Deadlock Vacuity Subtlety

**This is the critical subtlety for this property.**

When the `total_buffer_size` underflow bug (#21683) fires:

1. `total_buffer_size` wraps to ~u64::MAX.
2. `is_buffer_full()` returns `true` permanently (writer.rs:993-996).
3. The writer stops writing. No new data reaches any `.dat` file.
4. The actual on-disk buffer size stops growing and eventually shrinks as the
   reader drains and deletes files.
5. **`buffer-size-within-max` trivially passes: no data is written, so the bound
   is never violated.**

This is a classic safety-liveness interaction: the safety property is upheld, but
only because the system has deadlocked. A passing `buffer-size-within-max` result
under these conditions is a false negative — it signals "correct behavior" when
the system is actually completely broken.

**Mitigation:** Always evaluate `buffer-size-within-max` jointly with
`writer-eventually-makes-progress`. The semantically meaningful result requires:

- `buffer-size-within-max` is `Always` satisfied **AND**
- `writer-eventually-makes-progress` (`Sometimes`) is also satisfied.

If `buffer-size-within-max` passes AND `writer-eventually-makes-progress` fails,
the correct diagnosis is: the underflow bug fired and the bound is vacuously held.

### Antithesis cross-assertion

Add a combined assertion in the workload or a watchdog:

```
after STOP_FAULTS:
  assert that:
    (1) max(disk_bytes_observed during run) <= max_buffer_size + max_record_size
    (2) total_writes_after_last_fault > 0
```

If (1) holds but (2) fails, report both findings together.

---

## What a Genuine Violation Looks Like

A non-vacuous violation would require the write-side gate to be bypassed. Known
paths:

**Path 1: Accounting drift on startup.** `update_buffer_size` (ledger.rs:653-697)
adds *all* `.dat` file sizes, including files that the reader has already
processed but whose deletion race-lost against the restart. If `total_buffer_size`
is over-seeded, `can_write_record` blocks early (writer thinks buffer is larger
than it is). This makes the bound hold more conservatively — a false-positive
backpressure, not a violation.

**Path 2: Foreign `.dat` files.** If a foreign file matching `buffer-data-*.dat`
exists in the data directory, `update_buffer_size` includes its size in
`total_buffer_size`. This inflates the apparent buffer size without any real
data being present. The bound could be violated if the foreign file is *not*
counted in the gate check but *is* on disk — but since the gate uses the same
`total_buffer_size` atomic as the seeding, this path inflates the gate too, so
the bound still holds. However, the false inflation can cause premature blocking.

**Path 3: Accounting drift from config-reload race.** If the old writer and new
writer both have the same data directory briefly, the new writer's
`update_buffer_size` counts files still being written by the old writer. Both
writers may then attempt to write, potentially exceeding `max_buffer_size`. This
requires the advisory lock (`buffer.lock`) to be ineffective intra-process
(which it is on Linux — `fcntl` locks are per-process, not per-`fd`). This is
a live safety risk under config reload.

---

## SUT-Side Instrumentation (MISSING — must be added)

All Antithesis SDK calls below are absent from the codebase.

### Assertion 1 — Always: write gate is never bypassed

```rust
// writer.rs, inside write_record, after can_write_record returns true
// and before the actual write to TrackingBufWriter
let total_buffer_size = self.ledger.get_total_buffer_size() + self.unflushed_bytes;
antithesis_sdk::assert_always!(
    total_buffer_size <= self.config.max_buffer_size,
    "buffer_size_within_max: total_buffer_size does not exceed max at write time",
    &serde_json::json!({
        "total_buffer_size": total_buffer_size,
        "max_buffer_size": self.config.max_buffer_size,
        "unflushed_bytes": self.unflushed_bytes,
        "ledger_total": self.ledger.get_total_buffer_size(),
    })
);
```

### Assertion 2 — Always: gate logic is self-consistent

Verify `is_buffer_full` and `can_write_record` agree at the moment a write
proceeds (they should both say "not full" simultaneously):

```rust
// writer.rs, inside ensure_ready_for_write, just before returning Ok(())
// (i.e., when the loop breaks because is_buffer_full() returned false)
antithesis_sdk::assert_always!(
    !self.is_buffer_full(),
    "buffer_gate_consistent: writer exits wait loop only when not full",
    &serde_json::json!({
        "total_buffer_size": self.ledger.get_total_buffer_size(),
        "unflushed_bytes": self.unflushed_bytes,
        "max_buffer_size": self.config.max_buffer_size,
    })
);
```

### Assertion 3 — Unreachable: write proceeds when is_buffer_full is true

```rust
// writer.rs, in write_record, gated by can_write_record returning true
antithesis_sdk::assert_unreachable!(
    "write_while_full: wrote a record while is_buffer_full() was true",
    &serde_json::json!({
        "is_full": self.is_buffer_full(),
        "can_write_record_result": true,  // by definition we just passed the gate
    })
);
```

This would catch any race where `is_buffer_full` changes between the `ensure_ready_for_write`
exit and the actual `write_record` execution (unlikely given single-writer design,
but defensive).

### Watchdog process assertion (workload-side)

A separate watchdog process (not inside Vector) should periodically enumerate
all `buffer-data-*.dat` files in the configured data directory and assert:

```
sum(file_sizes) <= max_buffer_size + max_record_size
```

This is workload-observable without SUT modification and provides an independent
check that the gate is actually working.

---

## Open Questions

- **What is the configured `max_buffer_size` in the Antithesis harness?** The
  minimum is ~256MB (from the docs/spec). A smaller value makes the buffer fill
  faster and the property is exercised more frequently. Recommend using the
  minimum for harness efficiency.

- **Is the single-record overshoot (INV-3) tested?** Write a record whose
  serialized size is `max_data_file_size - 1` bytes (nearly 128MB). Verify the
  data file exceeds 128MB but the buffer-level `max_buffer_size` is still
  respected by blocking the *next* write, not the current one.

- **Config-reload race: is the intra-process advisory-lock gap actually reachable
  in Antithesis?** This requires two Vector topology instances to briefly share
  the same data directory. Confirm whether the harness exercises config-reload
  scenarios. If so, the per-process `fcntl` lock gap is a live safety issue for
  this property.

- **Does `ANTITHESIS_STOP_FAULTS` actually prevent node kills during the
  verification window?** If node kills continue during the post-fault check,
  the watchdog process may itself be killed before it can report a violation.
  Confirm with Antithesis documentation.

- **Is the `total_buffer_size` atomic observable to the watchdog via metrics
  export?** The `buffer_byte_size` metric is derived from `total_buffer_size`
  via the usage reporter. If the watchdog reads this metric rather than measuring
  files directly, it will see PR #23561's `saturating_sub` output (which caps at
  zero rather than showing u64::MAX) and will miss the underflow. The watchdog
  must measure actual file sizes, not the metric.

- **Relationship to `drop_newest` accounting bug (#24606/#24144):** When
  `when_full = drop_newest` fires, events are dropped but `component_discarded_events_total`
  does not increment. The buffer size accounting is updated (decrement happens),
  but the metric is wrong. Does this affect the `buffer-size-within-max` check?
  No — drops reduce the size, they don't violate the upper bound. But the metric
  discrepancy means the watchdog should measure files, not metrics.

- **Overflow on `increment_total_buffer_size`:** `fetch_add` at ledger.rs:282
  has no overflow guard. Is `max_buffer_size + max_record_size < u64::MAX`?
  Yes by a wide margin for any practical configuration. Document as out of scope.
