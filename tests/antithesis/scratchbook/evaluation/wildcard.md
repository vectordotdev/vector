---
sut_path: /home/ssm-user/src/vector
commit: b7aae737cef5dd37d1445915443a1eb97b584f85
updated: 2026-05-28
external_references: []
---

# Wildcard Evaluation: Disk Buffer v2 Property Catalog

Lens: **deliberately unconstrained**. The other three lenses cover Antithesis
Fit (unit-test vs. chaos territory), Coverage Balance (right portfolio vs. SUT
risks), and Implementability (can assertions be placed). This lens asks: what
did the framing miss, what failure scenarios are unmodeled, what joint conditions
do the individual properties not compose, and what is simply odd?

---

## Section 1 — Framing Questions

### W-F1: The persistent-volume assumption is load-bearing but unguarded by any property

**What the framing assumes:** The deployment topology (`deployment-topology.md`)
explicitly requires that `<data_dir>` reside on storage that survives node-kill.
Without this, every crash-recovery property (Categories 2–6) either passes
vacuously (buffer wiped on kill → clean init every time, no recovery exercised)
or fails spuriously (data the workload expected to survive is gone).

**What the framing misses:** No property in the catalog detects or guards against
this scenario. If Antithesis's tenant configuration recreates the container
filesystem on kill (a common default for stateless workloads), the test suite
produces a **perfectly green run** that is entirely meaningless: 15 of 19
properties are exercising fresh-start paths, not crash-recovery paths.

**The specific vacuity:** `durable-unacked-events-survive-crash` passes because
after kill+wipe there are no surviving data files for the reader to re-read; the
reader starts from scratch and the workload's "produced" set is empty (workload
was also killed). `recovery-completes-after-crash` passes because there is
nothing to recover. `partial-write-at-rotation-recovers` is never triggered
because files are gone. `writer-eventually-makes-progress` passes because
`total_buffer_size` starts at 0, so the underflow bug is never triggered.

**Missing guard:** A workload-level sentinel that proves persistence is working
before faults are injected. Concrete approach: before the first kill, write N
events and fsync (explicit flush_interval=0 run), assert that after restart the
buffer directory exists and contains the expected `.dat` files with non-zero
size. If the post-restart buffer directory is empty, emit
`assert_unreachable!("buffer_dir_empty_after_kill")` and abort the run with a
clear harness-configuration failure. This is not a Vector bug — it is a
harness-integrity check, but its absence makes the entire test suite's results
untrustworthy.

**Scope:** catalog-wide (Categories 2–6), harness design
**Evidence:** deployment-topology.md §"CRITICAL — persistent buffer storage";
implementability.md §"persistent-volume assumption"; no guard property exists in
any of the 19 property evidence files.
**Suggested action:** Add a sentinel workload step: before fault injection begins,
write-then-fsync N events, kill, assert `.dat` and `buffer.db` files survive.
Fail fast with a harness error if they do not. Gate all Category 2–6 assertions
on this sentinel passing.

---

### W-F2: The workload's "durably written" oracle conflates three distinct conditions

**What the framing assumes:** The catalog repeatedly asks "how does the workload
establish 'durably written'?" (property-catalog.md File-Level Open Questions;
`durable-unacked-events-survive-crash` Open Question 1), offering three
candidates: e2e acks, flush_interval=0 tracing, sync_all callsite tracing.

**What the framing misses:** These three candidates have **fundamentally different
meanings** and are not interchangeable. Understanding which one the workload
uses determines whether the property is even testable from outside.

**The confusion:**

1. **Source-HTTP-returns-200 with e2e acks enabled.** This is the strongest
   signal — it means the downstream HTTP sink has acknowledged receipt. The
   source returns 200 only after `BatchStatus::Delivered` propagates back through
   `handle_batch_status` (`src/sources/util/http/prelude.rs:309`). "200 returned"
   means the event traversed: disk buffer write → disk buffer read → HTTP sink
   delivery → sink ACK → BatchNotifier dropped → BatchStatus::Delivered → source
   response. This is NOT a durability marker; it is an end-to-end delivery marker.
   An event can be fsynced to disk (durable) but not yet return 200 (not yet
   delivered). Conversely, an event can return 200 while a different event — the
   one currently being fsynced when Vector is killed — is lost within the 500ms
   window.

2. **Source-HTTP-returns-200 without e2e acks.** Here 200 is returned immediately
   after the event is accepted into the channel (before buffer write, let alone
   fsync). This is the WEAKEST marker — not durable at all.

3. **flush_interval=0 / sync_all tracing.** The only option that actually marks
   fsync completion is to either set `flush_interval` so low that every `flush()`
   call triggers a full `sync_all`, or to instrument `sync_all` directly. Neither
   is exposed to the workload without SUT-side instrumentation.

**The consequence for property design:**

For `durable-unacked-events-survive-crash`, the correct oracle is condition 3
(sync_all fired, event is in the synced set). Condition 1 conflates buffer
durability with downstream delivery and makes the property measure the
`every-written-event-eventually-delivered` invariant instead. Using condition 2
makes the property vacuously true (0ms window, anything written at all survives
the 0ms window).

**A specific gap this creates:** With e2e acks (condition 1), the workload's
"durably written" set is empty during the deadlock scenario (no events ever reach
the downstream sink → no 200s returned → workload concludes nothing was durably
written → `durable-unacked-events-survive-crash` passes vacuously despite the
buffer being deadlocked). The deadlock thus masks the durability property.

**Suggested action:** Pick a single oracle for "durably written" across all
Category 3 properties: set `flush_interval` to a very short but non-zero value
(e.g., 50ms) so fsync fires frequently, and define "durably written" as "the
event was written AND at least one `sync_all` has completed after the write."
Use a workload-side timer: if the event was sent more than `2 * flush_interval`
milliseconds ago, assume it has been fsynced. This is imprecise but conservative
and does not require SUT instrumentation for the oracle itself.

**Scope:** `durable-unacked-events-survive-crash`, `every-written-event-eventually-
delivered`, `partial-write-at-rotation-recovers` — all Category 3 properties.
**Evidence:** `src/sources/util/http/prelude.rs:283-321` (source ack = sink
delivery, not buffer durability); `sut-analysis.md §2` (flush model; fsync only
on rotation or every ≥500ms).

---

### W-F3: The "single-process, just use node-kill" framing hides a split-brain window at the ledger/data boundary

**What the framing assumes:** The SUT analysis correctly identifies the data-file
fsync and ledger msync as "two separate, non-atomic syscalls" (`sut-analysis.md
§3`). The recovery logic (`validate_last_write`) is designed to handle the two
canonical divergence states: data ahead of ledger (`Ordering::Less`, fast-forward)
and ledger ahead of data (`Ordering::Greater`, skip).

**What the framing underweights:** The **directionality of the non-atomicity
depends on which syscall the kill interrupts**, and the two outcomes have
asymmetric consequences:

- Kill between `sync_all` (line 1314) and `ledger.flush()` (line 1317): data is
  durable, ledger is stale. Recovery: `Ordering::Less` → ledger fast-forwards to
  match data. **Safe** for the data that was synced; a duplicate may be delivered
  (at-least-once semantics).

- Kill between `ledger.flush()` and the return of `flush_inner`: ledger is
  current, data is not yet changed (this window is narrow — ledger.flush is the
  last call). Practically safe.

- Kill during the `sync_all` itself (kernel-level partial fsync): the data file
  may be only partially durable. Recovery path depends on whether the last record
  in the partially-fsynced file is valid.

**The unmodeled case:** Kill *after* `ledger.flush()` updates
`writer_next_record` in the mmap'd region but *before* the data file fsync
propagates the corresponding bytes to persistent media. This is possible on some
block devices and virtual disks where msync (via mmap dirty page writeback) is
faster than `fsync`. In this case: ledger says "record N was written"; data file
does not contain record N's bytes. Recovery: `Ordering::Greater` → "Events have
likely been lost" log, skip to next file. The skip is intentional, but **the
events were not actually lost — they were never durably written in the first
place**. The "events lost" log is technically wrong; the events were page-cache
writes that never reached storage. Downstream impact: the ledger's
`writer_next_record` is advanced past a gap, and the reader will eventually read
those events from subsequent writes (which restart from the new ID), creating an
ID gap that the workload may interpret as event loss.

**What no property models:** Whether the `Ordering::Greater` path fires correctly
only for genuine data loss (events that reached fsync but whose data is
unreadable) vs. spuriously for events that were only in the page cache (never
reached fsync). The property `partial-write-at-rotation-recovers` covers the
recovery path but does not distinguish these two causes of `Ordering::Greater`.

**Scope:** `partial-write-at-rotation-recovers`, `durable-unacked-events-survive-crash`
**Evidence:** `writer.rs:1312-1317` (sync_all then ledger.flush, no atomicity);
`sut-analysis.md §3` ("data file fsync and the ledger msync are two separate,
non-atomic syscalls").
**Suggested action:** Add a sub-case to `partial-write-at-rotation-recovers`
explicitly noting the two directionalities and which Antithesis kill windows
target each. A SUT-side assertion at the `Ordering::Greater` path could emit
structured data distinguishing "no data in file" (page-cache-only loss) from
"data in file but corrupt" (genuine partial fsync loss).

---

## Section 2 — Missing Angles

### W-M1: The double-wrap in `is_buffer_full` creates intermittent write-through, not permanent deadlock — an unmodeled inconsistency state

**Discovered via:** Code inspection of `writer.rs:993-996`; arithmetic
verification.

**The issue:** When `total_buffer_size` wraps to `u64::MAX` (the underflow bug),
the deadlock is not strictly permanent. At line 994:

```rust
let total_buffer_size = self.ledger.get_total_buffer_size() + self.unflushed_bytes;
```

This is a plain `u64` addition. In Rust release mode, `u64::MAX + unflushed_bytes`
wraps: if `unflushed_bytes >= 1`, the result is `unflushed_bytes - 1`, a small
non-negative number. If `small_number < max_buffer_size`, then `is_buffer_full()`
returns `false` — the writer is NOT seen as full, and it proceeds to write.

This means: after the underflow, the writer may make **exactly one additional
write** (the one during which `unflushed_bytes > 0` at the check point), after
which `flush_write_state()` zeroes `unflushed_bytes` and the next `is_buffer_full`
check sees `u64::MAX + 0 = u64::MAX` again, blocking permanently.

The intermittent write-through is not just a curiosity:

1. The writer accepts a new record, updates `unflushed_bytes`, writes to the
   `TrackingBufWriter`, then calls `flush()`. After flush, `unflushed_bytes` goes
   to 0 and `total_buffer_size` (still at `u64::MAX`) blocks the writer again.
   But the flushed record is now in the OS page cache and is readable by the
   reader. The reader reads it, attaches a `BatchNotifier`, delivers to sink. The
   ack returns. The finalizer calls `decrement_total_buffer_size(record_bytes)` —
   on a value already at `u64::MAX` — which wraps again to `u64::MAX -
   record_bytes`. Still near `u64::MAX`. The accounting is permanently poisoned
   regardless.

2. The workload may observe write throughput recovering briefly after the
   underflow, then stalling again. A `Sometimes(writer_unblocked_after_full)`
   assertion fires during the brief recovery window — even in the bug scenario.
   This makes the `writer-eventually-makes-progress` property a **false negative**
   under this specific input-timing combination: the `Sometimes` is satisfied by
   the brief window, and Antithesis reports success while the bug is present.

3. The inconsistency state — `total_buffer_size` at `u64::MAX` but writer making
   occasional writes — is not covered by any property. `total-buffer-size-never-
   underflows` catches the underflow at the decrement site (correct). But after
   the underflow is detected, the SUT continues running in a poisoned state. No
   property asserts that `total_buffer_size` stays sane after a detected underflow.

**Scope:** `writer-eventually-makes-progress` (false negative risk),
`total-buffer-size-never-underflows` (detects root but not downstream inconsistency)
**Evidence:** `writer.rs:993-996` (unchecked `u64 +`); `writer.rs:784`
(`unflushed_bytes -=` after flush); arithmetic: `u64::MAX + 1 = 0` (wraps to
near 0, below any `max_buffer_size`).
**Suggested action:** The `assert_unreachable!` inside the stall-count loop in
`writer-eventually-makes-progress` must fire after N consecutive wakeup-with-no-
net-progress cycles, not just on any wakeup. The "progress" check must verify
that `total_buffer_size` has actually decreased from a previous measurement, not
just that the writer exited `ensure_ready_for_write`. Additionally: add an
`assert_always(total_buffer_size <= max_buffer_size || just_underflowed)` at the
ledger level so that any post-underflow state is visible even when the writer
appears to make progress.

---

### W-M2: The `should_flush` CAS loser silently skips fsync — clock jitter can make this permanent

**Discovered via:** `ledger.rs:485-497` code inspection + the Antithesis clock
fault capability mentioned in `sut-analysis.md §10`.

**The mechanism:** `should_flush()` uses an `AtomicCell<Instant>` CAS:

```rust
pub fn should_flush(&self) -> bool {
    let last_flush = self.last_flush.load();
    if last_flush.elapsed() > self.config.flush_interval
        && self.last_flush.compare_exchange(last_flush, Instant::now()).is_ok()
    {
        return true;
    }
    false
}
```

When two callers race, only one wins the CAS. The loser returns `false` and skips
the fsync — this is the intended design (deduplicate concurrent flushers). But:

If Antithesis's clock jitter **slows down wall clock time** as seen by
`Instant::elapsed()`, the `elapsed() > flush_interval` condition may never become
true. The result: `should_flush()` permanently returns `false`, and `flush_inner`
never calls `sync_all`. Data accumulates in the OS page cache indefinitely. A
kill at any point loses all unsynced events.

This is separate from the 500ms documented window: that window assumes fsync
eventually fires. If `Instant::elapsed()` is frozen by the Antithesis clock
scheduler, the window is infinite.

**The interaction with `force_full_flush`:** `flush_inner(true)` bypasses
`should_flush()` entirely and always calls `sync_all`. It is called on file
rotation. So the only guard against permanent fsync suppression is that file
rotation fires — but file rotation is triggered by the data file reaching its
size limit, which is independent of time. With small records and high throughput,
rotations may be frequent enough that the durability window is bounded by
rotation frequency, not by the 500ms clock.

**But with large `max_data_file_size` (default 128MB):** A slow workload may
never rotate, and if the clock is frozen, the fsync never fires. The durability
window becomes "until next rotation" — which could be unbounded.

**What no property models:** The interaction between clock jitter (slowing or
freezing `Instant::elapsed()`) and the effective fsync interval. The
`durable-unacked-events-survive-crash` property specifies "loss bounded to ≤500ms
unsynced window" — this bound assumes the clock runs at real speed.

**Scope:** `durable-unacked-events-survive-crash`, `partial-write-at-rotation-
recovers`, catalog-wide (any property that depends on fsync having fired)
**Evidence:** `ledger.rs:485-497` (`should_flush` CAS); `common.rs:31`
(`DEFAULT_FLUSH_INTERVAL = 500ms`); `writer.rs:1041` (rotation triggers
`force_full_flush = true`, bypassing the clock).
**Suggested action:** Add a `flush_interval` override to the test config, setting
it to 0 (force every flush to be a full fsync). This eliminates the clock
dependency for the durability cluster and makes "durably written" = "flush() was
called." Separately, test with `flush_interval` = large value to exercise the
rotation-only fsync path.

---

### W-M3: The `WhenFull::Overflow` + disk base ordering inversion under crash is unmodeled

**Discovered via:** `sut-analysis.md §10` (noted as a "Wildcard / Cross-Cutting"
observation). No catalog property covers it.

**The scenario:** A Vector topology configured as:

```
source → disk_buffer (base) → in-memory_buffer (overflow) → sink
```

Under load, the disk buffer fills up. New events go to the in-memory overflow
buffer. At this point:

- Disk buffer: events A₁...Aₙ (fsynced, durable)
- In-memory overflow: events Aₙ₊₁...Aₙ₊ₖ (not durable, not on disk)

SIGKILL occurs. On restart:

- Disk buffer: events A₁...Aₙ are present and re-readable (correct at-least-once).
- In-memory overflow: Aₙ₊₁...Aₙ₊ₖ are **lost** (not persisted anywhere).

The ordering inversion: events A₁...Aₙ (earlier, durably written) are replayed
to the sink. Events Aₙ₊₁...Aₙ₊ₖ (later, overflow) are permanently lost. The
sink sees a gap — events up to Aₙ appear, events Aₙ₊₁...Aₙ₊ₖ never appear.

This breaks at-least-once reasoning in a subtle way: the duplicates from Aₙ's
replay mean the downstream dedup logic sees "events I already processed" AND
"events that should have come after them are missing." Standard dedup (by ID)
will correctly deliver A₁...Aₙ at-least-once but will never deliver Aₙ₊₁...Aₙ₊ₖ,
creating **permanent silent loss of events that the source already acknowledged**
(if the source ACKed Aₙ₊₁...Aₙ₊ₖ before the kill).

No property in the catalog covers this scenario. The deployment topology uses
`when_full: block` (not `overflow`), so the harness does not exercise this mode.

**Scope:** NEW — no existing property covers the overflow+disk+crash combination.
**Evidence:** `sut-analysis.md §10` ("WhenFull::Overflow + disk base");
`topology/channel/sender.rs:236-244` (overflow path); no `overflow` in
deployment-topology.md.
**Suggested action:** Add a separate topology configuration with `when_full:
overflow` and a second in-memory buffer as overflow. Add a property: "events
accepted by the source before the kill that were written to the disk buffer are
delivered after restart; events accepted into the in-memory overflow are
documented as potentially lost on crash." This is primarily a documentation and
workload-design question — the SUT's behavior is correct per design (in-memory
overflow is not durable). The risk is that operators configure this topology
believing the disk buffer provides end-to-end durability, when in fact it only
provides durability for the base buffer portion.

---

### W-M4: The `NotFound` file skip path at `reader.rs:777` calls `increment_acked_reader_file_id` without a subsequent `ledger.flush()`

**Discovered via:** Code inspection of `reader.rs:766-784` vs.
`delete_completed_data_file:548-549`.

**The asymmetry:** The `delete_completed_data_file` path calls
`ledger.increment_acked_reader_file_id()` at line 548 **followed immediately** by
`ledger.flush()` at line 549 (MS_SYNC, blocking until durable). The `NotFound`
skip path at line 777 calls `increment_acked_reader_file_id()` **without any
subsequent `ledger.flush()`**.

`increment_acked_reader_file_id` calls `self.state().increment_reader_file_id()`
which atomically stores to the mmap'd `reader_current_data_file` field in the
`LedgerState`. On Linux with MAP_SHARED, this is a dirty mmap page — durable
only after `msync(MS_SYNC)`. Without the subsequent `ledger.flush()`, a kill
immediately after the atomic store in the `NotFound` path may or may not persist
the incremented file ID to disk, depending on whether the kernel's dirty page
has been written back.

**Practical impact:** If the incremented value IS written to disk (kernel
flushed the dirty page), the restart correctly finds the reader at the new
file ID. If NOT written, restart finds the reader at the old file ID, the `NotFound`
path fires again, and the skip is idempotent — safe.

However, this is NOT idempotent in one case: if the skipped file is a gap that
also has outstanding `total_buffer_size` accounting (from a corrupted/externally
deleted file that was never properly size-accounted), skipping it twice could
double-decrement the accounting. The `NotFound` skip path at line 777 does NOT
call `decrement_total_buffer_size` (unlike `delete_completed_data_file` which
calls it at line 538), so this specific case is probably safe.

**The real concern:** The inconsistency between the two paths (one flushes ledger,
one does not) is a code-smell that could cause a future regression if someone
adds accounting to the `NotFound` path without also adding a ledger flush.

**Scope:** `recovery-completes-after-crash`, indirectly `total-buffer-size-never-underflows`
**Evidence:** `reader.rs:766-784` (NotFound path, no ledger.flush); `reader.rs:548-549`
(delete_completed_data_file, calls ledger.flush).
**Suggested action:** Add a `ledger.flush()` call after the `increment_acked_reader_file_id()`
at `reader.rs:777` to make the two skip paths symmetric. Add a note to the
`recovery-completes-after-crash` property that this asymmetry exists and should
be exercised by kills immediately after the NotFound branch fires.

---

### W-M5: Two properties are individually fine but jointly contradictory under the deadlock — creating a false-green composite

**Discovered via:** Cross-reading `buffer-size-within-max` with `writer-eventually-
makes-progress`; arithmetic of `u64::MAX` + `is_buffer_full()`.

**The joint contradiction:** The catalog correctly notes that `buffer-size-within-
max` is vacuously true under the deadlock: if the writer is permanently blocked,
no new data is written, so the on-disk size never exceeds `max_size`. The catalog
says "must be evaluated jointly with `writer-eventually-makes-progress`."

But consider adding the double-wrap finding (W-M1): if the deadlock is
*intermittent* (writer makes occasional writes due to the double-wrap), then:

- `buffer-size-within-max`: `Always(on_disk_bytes <= max_size + max_record_size)`.
  The writer is mostly blocked; occasionally writes one record. On-disk size
  stays near `max_size`. **Passes**.
- `writer-eventually-makes-progress`: `Sometimes(writer_unblocked)`. The writer
  occasionally escapes due to double-wrap. **Passes** (the `Sometimes` fires on
  the escape).
- `total-buffer-size-never-underflows`: FAILS (the underflow IS detected by the
  `assert_always(amount <= current)` before `fetch_sub`).

So with the double-wrap active: the two liveness properties both pass, the safety
root-cause property fails. The system is in a broken state that the liveness
properties' `Sometimes` assertions cannot distinguish from a healthy bounded-
backpressure cycle.

**The missing joint assertion:** A compound property that is `Unreachable` when:

```
is_buffer_full() == true
  AND total_buffer_size >= max_buffer_size * 0.99  // truly full, not just overflow artifact
  AND elapsed_since_last_write > 30s               // writer has been stalled
  AND recent_deletes_without_writer_progress > N   // reader is deleting but writer stays blocked
```

None of the 19 properties captures this compound condition. It is observable from
the workload (write throughput + sink delivery throughput both drop to zero
simultaneously while the buffer gauge shows "full" and the reader continues
processing acks).

**Scope:** `buffer-size-within-max`, `writer-eventually-makes-progress`, joint
**Evidence:** `writer.rs:993-996` (double-wrap arithmetic); property-catalog.md
§`buffer-size-within-max` ("Must be evaluated jointly"); W-M1 above.
**Suggested action:** Add a workload-level compound liveness check: if write
throughput ≈ 0 AND sink delivery throughput ≈ 0 AND buffer gauge shows ≥ 90% full
AND the condition persists for > 2× the drain-time bound, fire
`assert_unreachable!("persistent_deadlock_detected")`. This cross-cuts the two
properties and is observable without SUT instrumentation.

---

### W-M6: The `config-reload` POSIX lock gap is intra-process — no Antithesis-native way to inject it

**Discovered via:** `sut-analysis.md §5` (INV-10: "advisory lock does NOT protect
intra-process"); `deployment-topology.md §Custom faults`.

**The issue:** POSIX `fcntl` locks are per-process on Linux. If the old and new
topology open the same buffer directory during config reload, both get the lock
(they are in the same process). The catalog notes this but treats it as "may make
the lock gap a live safety issue." This is more specific:

On config reload, the old topology's `BufferWriter` is dropped (calling `close()`
but not `flush()`). The new topology immediately opens the same buffer directory.
Between the old `close()` and the new `open()`, there is no inter-process lock
because both are in the same process. But: the old finalizer task (spawned as a
`tokio::spawn` detached task, `ledger.rs:701-710`) holds an `Arc<Ledger>` and
continues running. If the new topology's reader opens the buffer while the old
finalizer is still calling `increment_pending_acks` or `notify_writer_waiters`,
there are two concurrent coroutine paths touching the same `Ledger` via separate
`Arc` references.

The Antithesis fault — SIGHUP to trigger reload — is a custom fault. But the
**race** between old finalizer and new reader init is not a fault to inject: it
is an inherent timing race in the reload sequence. Antithesis's scheduler can
explore the interleaving naturally if the reload is driven by the workload during
live writes. The issue is whether the harness actually keeps the write load high
during reload (maximizing finalizer task backlog at the moment of reload).

**Missing from the catalog:** An explicit assertion at the finalizer task shutdown
point: does the old `Arc<Ledger>` get dropped before the new topology's `from_config_inner`
runs? If `Arc::strong_count(ledger) > 1` at the start of the new init sequence,
the old finalizer may still be running.

**Scope:** `config-reload-no-silent-loss`
**Evidence:** `ledger.rs:701-710` (`spawn_finalizer` spawns detached tokio task);
`sut-analysis.md §5` (POSIX lock, per-process); `writer.rs:1366-1374` (Drop calls
close() not flush()).
**Suggested action:** Add an assertion at the start of `Buffer::from_config_inner`:
`assert_always(Arc::strong_count(&ledger_would_be) == 1)` (i.e., no other reference
to this directory's ledger exists when init begins). This requires knowing whether
the old Arc has been dropped, which requires the finalizer to have exited — a
condition only met when the old `OrderedFinalizer` sender side is dropped.

---

## Section 3 — Cross-Lens Composite Findings

### W-C1: Antithesis-Fit calls `durable-unacked-events-survive-crash` high-value, but Implementability calls the oracle definition "needs a decision" — the reformulation

**The tension:** `durable-unacked-events-survive-crash` is correctly in
Antithesis-Fit's high-value zone (timing-sensitive, test-impossible otherwise).
But the workload oracle for "durably written" is open (catalog: "options are e2e
acks, tracing the sync_all callsite, or flush_interval=0; needs a decision").
Implementability flags this as a decision gate.

**The reformulation that captures the same risk feasibly:**

Instead of tracking "which events are in the fsync'd set," track "which events
are NOT in the definitely-lost set." Specifically:

The 500ms durability window is a property of the timer, not of individual events.
Any event written more than `2 × flush_interval` milliseconds before the kill
has (with high probability) been included in at least one full fsync cycle. Any
event written within the `flush_interval` window before the kill may or may not
have been fsynced (depends on rotation).

Workload-side reformulation:

1. Send events continuously with unique IDs.
2. Maintain a "written more than 2×flush_interval ago" set (events definitely
   past the fsync window).
3. After kill+restart+drain, assert: every event in the "past-window" set is
   delivered. Events in the "within-window" set may or may not be delivered (no
   assertion, loss is expected).
4. The `Sometimes` assertion: confirm that at least one past-window event
   survived the kill (proves fsync is actually working, not just that the test
   never wrote anything durable).

This reformulation requires only: (a) a workload timer, (b) `flush_interval`
set to a known short value, (c) event IDs. No SUT instrumentation needed for
the oracle. The `Sometimes` variant confirms the positive case; the `Always`
variant (past-window events survive) catches durability regressions.

**Scope:** `durable-unacked-events-survive-crash` reformulation
**Evidence:** `common.rs:31` (`DEFAULT_FLUSH_INTERVAL = 500ms`); `writer.rs:1312`
(`should_flush` timer gate); property-catalog.md §durable-unacked-events-survive-crash
Open Question 1.

---

### W-C2: Coverage-Balance flags "skip-rest-of-file data loss" as thin — a feasible reformulation exists

**The tension:** Coverage Balance finds the "reader skips entire file after first
bad record" loss surface (SUT §6 item 7) to have no dedicated property. Antithesis
Fit would rate it high-value (exact point of bad-read detection + corruption extent
depend on timing). Implementability notes `corruption-is-detected-and-recovered`
partially covers it but quantifies nothing.

**The reformulation:** Add a SUT-side counter: `records_abandoned_due_to_corruption`
(a simple atomic incremented each time `roll_to_next_data_file` is called after a
bad read). An `assert_always(records_abandoned <= 0)` would be wrong (abandonment
is by design). The correct property: `assert_always(records_abandoned_per_file <=
expected_max)` where `expected_max` is `max_data_file_size / min_record_size`. In
other words: abandonment is bounded by the file size. This is always trivially true,
but the observation that matters is: were any valid records abandoned that the
workload knows should have survived?

Workload approach: instead of counting abandoned records (invisible to workload),
use unique event IDs. If an event with ID K was written (workload confirmed it was
accepted into the source), but the workload also injected corruption into the file
containing K's data, and K is never delivered — that IS an acceptable loss (K was
on the corrupted segment). But if events K+1, K+2, ... K+N (known to be in valid
records AFTER the corruption point in the same file) are also never delivered —
that IS the skip-rest-of-file loss.

This is implementable if the workload can correlate corruption injection timing
with event IDs (doable with a shared timestamp).

**Scope:** NEW property (or sub-case of `corruption-is-detected-and-recovered`)
**Evidence:** `reader.rs` `roll_to_next_data_file` call site; `sut-analysis.md §6`
item 7; coverage-balance.md §F-Missing "skip-rest-of-file quantification."

---

## Section 4 — Oddities

### W-O1: The `debug_assert!` at `writer.rs:~396` for `max_data_file_size >= max_record_size` is compiled out of release — creating a silent infinite loop risk in production

**The property `record-never-spans-files` notes this in its Open Questions** ("Is
the `debug_assert(max_data_file_size >= max_record_size)` compiled out of release?
If so, a `max_record_size > max_data_file_size` misconfig silently makes every
write return `DataFileFull` → writer loops forever").

This is not just a config issue — Antithesis runs production builds (not debug).
If someone configures `max_record_size > max_data_file_size` (even accidentally by
setting both to the same value and then a record header pushes it over), the writer
enters an infinite loop with no error log and no crash. This is the same operational
signature as the deadlock from #21683 and would be completely invisible to the
operator.

No property tests this specific misconfiguration. The catalog's `record-never-spans-
files` focuses on the spanning-record data loss, not the deadlock-from-config-mismatch.

**This is odd** because: the `debug_assert` exists (author knew it was important),
it is not a release assertion (release safety check is absent), and the result is
identical to the known-highest-value bug.

**Suggested action:** Promote the `debug_assert` to a validated configuration check
in `Buffer::from_config_inner` that returns an `Err(BufferError)` instead of silently
looping. This is a one-line fix independent of Antithesis. As a property: add a
harness configuration fuzzing step — include configs with edge-case `max_record_size`
values — and assert Vector either rejects the config or makes write progress.

---

### W-O2: The catalog lists `every-written-event-eventually-delivered` as a liveness `Sometimes` — but liveness under faults usually requires `Always(eventually)`

**The oddity:** The catalog uses `Sometimes(all_produced_delivered)` for the
end-to-end at-least-once property. In Antithesis semantics, `Sometimes` fires
when the condition is true on any one execution. But "at-least-once delivery" is
not a "sometimes" claim — it is an "always eventually" claim: for every event
produced, it is eventually delivered (possibly after many retries/crashes).

The catalog's choice of `Sometimes` is explained by "progress milestone" framing
— confirm the happy path fires at least once. But this means Antithesis will stop
reporting a failure once it finds a single timeline where all events are delivered,
even if 99% of timelines have data loss.

**A stronger formulation:** Use a workload-level `assert_always` on a per-event
basis: for every event ID in the "produced" set, assert it eventually appears in
the "delivered" set within the quiet period. The `Sometimes` wrapper is only needed
for the `Sometimes(at_least_one_event_delivered)` reachability check — separately
from the `Always(all_produced_delivered)` safety check.

**Scope:** `every-written-event-eventually-delivered`
**Evidence:** property-catalog.md §`every-written-event-eventually-delivered`
("`Sometimes(all_produced_delivered)` as a progress milestone").
**Suggested action:** Split into: (a) `assert_sometimes!("at_least_one_delivery",
  delivered_count > 0)` to confirm the delivery path is exercised; (b) workload
`assert_always!("all_produced_eventually_delivered", produced_set ⊆ delivered_set)`
checked after each quiet period. The `Sometimes` becomes a reachability check on
the delivery path, not the primary safety assertion.

---

### W-O3: The workload-observable deadlock signal depends on write throughput dropping — but WhenFull::Block backpressure looks identical to the deadlock from outside

**The oddity:** The `writer-eventually-makes-progress` property proposes observing
"write throughput drops to zero" as the deadlock signal. But normal, correct
backpressure (`WhenFull::Block`, buffer full, reader hasn't caught up) also drops
write throughput to zero. The two states are operationally identical from outside
the process:

- Healthy full buffer: write throughput zero, sink delivery continues, eventually
  writer unblocks.
- Deadlocked buffer: write throughput zero, sink delivery also stops (reader
  drains but accounting is wrong so writer never unblocks).

The distinguishing signal is: **does sink delivery rate also drop to zero during
the stall?** In the deadlock, the reader drains (sink delivers events that were
already in the buffer), but eventually the buffer is empty and sink delivery also
stops. In healthy backpressure, sink delivery continues throughout and eventually
the writer unblocks.

The workload must observe **both** write throughput AND sink delivery throughput
simultaneously. The property as written ("write throughput drops to zero") is
insufficient — a single-metric check gives a false positive for healthy
backpressure.

**The joint condition needed:** `write_throughput ≈ 0 AND sink_throughput ≈ 0
AND duration > drain_time_bound`. This is never written in any property.

**Scope:** `writer-eventually-makes-progress` workload design.
**Evidence:** `writer.rs:1001-1019` (backpressure loop vs. deadlock loop look
identical externally); `reader.rs:553-555` (reader does notify_reader_waiters in
both cases).
**Suggested action:** Modify the workload to emit a deadlock-detection assertion
only when write throughput AND sink delivery throughput are both near zero for
more than the drain-time bound. Also add sink delivery throughput as a continuous
metric.

---

## Summary Table

| ID | Property/Slug | Concern | Scope | Evidence brief | Suggested action |
|----|--------------|---------|-------|----------------|-----------------|
| W-F1 | catalog-wide (Cat 2–6) | Persistent-volume assumption unguarded: fresh-filesystem kills produce vacuously green runs | Harness design | deployment-topology.md §CRITICAL; no sentinel property in catalog | Add workload sentinel before fault injection: write+fsync N events, kill, assert `.dat` files survive; gate Cat 2–6 on sentinel |
| W-F2 | `durable-unacked-events-survive-crash`, `every-written-event-eventually-delivered`, `partial-write-at-rotation-recovers` | Three oracle candidates for "durably written" conflate fsync, source-ack, and sink-delivery — e2e ack is NOT a durability marker | Category 3 | `prelude.rs:309` (200 = sink delivered, not fsynced) | Pick: set flush_interval short, define "durable" = "sent >2×flush_interval ago"; no SUT instrumentation needed |
| W-F3 | `partial-write-at-rotation-recovers`, `durable-unacked-events-survive-crash` | `Ordering::Greater` path fires for both genuine data loss AND page-cache-only loss — not distinguished by any property | Specific | `writer.rs:1312-1317` (sync_all then ledger.flush); sut-analysis §3 | Add SUT-side annotation at `Ordering::Greater` distinguishing "no bytes in file" from "bytes corrupted" |
| W-M1 | `writer-eventually-makes-progress`, `total-buffer-size-never-underflows` | Double-wrap in `is_buffer_full` (`u64::MAX + unflushed_bytes` wraps to small number) creates intermittent write-through, not permanent deadlock; `Sometimes` may fire during the brief escape, producing false-green liveness result | `writer.rs:993-996` | Arithmetic: `u64::MAX + 1 = 0`; brief escape then re-deadlock | Stall-count counter must require consecutive no-net-progress cycles; add compound workload assertion (write=0 AND sink=0 AND duration>bound) |
| W-M2 | `durable-unacked-events-survive-crash`, Cat 3 | `should_flush` CAS + frozen Antithesis clock can permanently suppress fsync — "≤500ms window" claim requires clock running at real speed | `ledger.rs:485-497` | `Instant::elapsed()` drives 500ms gate; rotation is the only clock-independent fsync trigger | Set flush_interval=0 in test config (rotation-triggered fsync always fires regardless of clock) |
| W-M3 | NEW — no existing property | `WhenFull::Overflow` + disk base: later in-memory events silently lost, earlier durable events replayed — creates permanent gap in at-least-once reasoning | `topology/channel/sender.rs:236-244` | sut-analysis §10 | Add second topology config with overflow; add property documenting which events survive (disk-buffer portion) vs. which are lost (overflow portion) |
| W-M4 | `recovery-completes-after-crash`, `total-buffer-size-never-underflows` | `NotFound` skip path at `reader.rs:777` calls `increment_acked_reader_file_id` without subsequent `ledger.flush()`, unlike delete path (asymmetry is a future regression risk) | `reader.rs:766-784` vs `reader.rs:548-549` | Structural: delete path flushes, skip path does not | Add `ledger.flush()` after line 777; add kill-during-NotFound test case to `recovery-completes-after-crash` |
| W-M5 | `buffer-size-within-max`, `writer-eventually-makes-progress` (jointly) | Both individually pass under the double-wrap intermittent deadlock; no joint compound property detects the broken state | Both properties | W-M1 arithmetic; property-catalog.md §`buffer-size-within-max` vacuity note | Add compound workload assertion: write≈0 AND sink≈0 AND buffer≥90% full AND duration>drain_time |
| W-M6 | `config-reload-no-silent-loss` | Old finalizer `Arc<Ledger>` may overlap with new topology init (POSIX lock is per-process; detached tokio task survives Drop) | `ledger.rs:701-710` | sut-analysis §5 (INV-10); detached tokio task holds Arc after Drop | Assert `Arc::strong_count == 1` at start of new `from_config_inner`; ensure finalizer exits before new init |
| W-C1 | `durable-unacked-events-survive-crash` | High Antithesis-Fit but open oracle — reformulation: use event-timestamp-based "past-window" set instead of fsync tracing | Oracle design | `common.rs:31` (500ms interval); no SUT instrumentation needed | Events sent >2×flush_interval ago = "past window"; assert all past-window events delivered after restart |
| W-C2 | NEW (sub-case of `corruption-is-detected-and-recovered`) | Skip-rest-of-file loss is unquantified; valid records after a corruption point are abandoned silently | Corruption cluster | reader `roll_to_next_data_file`; coverage-balance §F-Missing | Track abandonment via event-ID correlation: events written after the corruption-injected offset should survive; absent → loss |
| W-O1 | `record-never-spans-files` | `debug_assert` for `max_data_file_size >= max_record_size` compiled out in release → misconfig produces infinite write loop identical to the known deadlock | `writer.rs:~396` | debug_assert only; no release-mode check | Promote to validated config check returning `BufferError`; add harness config fuzzing |
| W-O2 | `every-written-event-eventually-delivered` | `Sometimes(all_produced_delivered)` is insufficient for at-least-once — `Sometimes` fires on a single good timeline, hiding loss on all others | property-catalog.md §this property | Antithesis `Sometimes` semantics | Split: `Sometimes(delivery_path_reachable)` + `Always(per_event: produced ⊆ delivered)` per quiet-period drain |
| W-O3 | `writer-eventually-makes-progress` | Deadlock signal (write throughput → 0) is identical to healthy backpressure from outside; single-metric check gives false positive | workload design | `writer.rs:1001-1019`; reader `notify_reader_waiters` fires in both cases | Assert write≈0 AND sink≈0 AND duration>drain_time (not just write≈0 alone) |

---

## Passes (no concern)

- The 19-property count and the cluster structure are sound. No cluster is
  internally contradictory in its intended semantics (the jointly-contradictory
  issue at W-M5 is about runtime behavior, not definitional contradiction).
- The `record-id-monotonicity-holds` `Unreachable` assertion type is correct: the
  monotonicity panic is a guardrail that must never trip, and `Unreachable` is the
  right SDK type.
- The `no-corrupted-record-delivered` `AlwaysOrUnreachable` assertion type is
  correct: corruption detection is an optional path (acceptable if never triggered),
  but any execution that does enter the path must satisfy the invariant.
- The `sink-failure-not-silently-acked` property correctly identifies a known-
  violated invariant as an expected-to-fail property — valuable for tracking when
  the bug is fixed.
- The `file-id-rollover-stays-coordinated` property is correctly scoped to the
  test-time `MAX_FILE_ID = 6` constant that makes rollover reachable.
- The `buffer.lock` advisory-lock design is correctly scoped to intra-process
  being unprotected (INV-10). No property overclaims protection.

---

## Uncertainties

1. **Whether Antithesis's `Instant` time is virtual or real.** If Antithesis's
   scheduler does not intercept `std::time::Instant::now()` / `elapsed()`, the
   clock-jitter concern (W-M2) may not apply. If it does intercept, the concern
   is active and `flush_interval=0` in test config is the mitigation.

2. **Whether the persistent-volume requirement (W-F1) is met in the target tenant.**
   This is the most operationally critical uncertainty: without it, the entire
   crash-recovery test suite is meaningless.

3. **Whether the double-wrap intermittent escape (W-M1) actually produces a
   distinguishable execution trace in Antithesis.** The timing window for
   `unflushed_bytes > 0` at the `is_buffer_full` check point may be very short
   (microseconds), and Antithesis may not systematically explore it unless the
   scheduler specifically targets the `fetch_sub` → `is_buffer_full` interleaving.

4. **Whether the `Ordering::Greater` path (W-F3) is reachable without the F5
   torn-tail.** If genuine `Greater` cases only arise from hardware media errors
   (which Antithesis doesn't inject directly), this sub-case is unreachable in
   practice and the distinction from F5 false-positives is moot.

5. **Whether the config-reload finalizer race (W-M6) is actually concurrent.**
   If the tokio runtime guarantees that the detached finalizer task is drained
   before the `Arc<Ledger>` is dropped, the race does not exist. This requires
   confirming the tokio shutdown semantics for detached tasks.
