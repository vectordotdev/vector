---
sut_path: /home/ssm-user/src/vector
commit: 049eec79b737450c4669b7f8aa1dd814551ec466
updated: 2026-06-02
external_references: []
---

# Coverage Balance Evaluation: Disk Buffer v2 Property Catalog

Lens: **Is this the right set of properties?** Evaluated section-by-section against
`sut-analysis.md`, cross-checking each failure-prone area against the catalog,
then examining five specific cross-cutting gaps called out in the evaluation prompt,
and finally assessing the Safety/Liveness/Reachability portfolio balance.

---

## 1. Mapping: SUT Failure-Prone Areas → Catalog Coverage

The SUT analysis ranks 9 failure-prone areas (§6). The table below shows catalog
coverage density per area and flags imbalances.

| Rank | SUT §6 area | Catalog properties | Coverage adequacy |
|------|-------------|-------------------|------------------|
| 1 | `total_buffer_size` underflow → writer deadlock | `total-buffer-size-never-underflows`, `writer-eventually-makes-progress`, `buffer-size-within-max`, `reader-drains-and-terminates-cleanly` (4 properties, 3 from different vantage points) | **Well-covered** — the highest-value cluster has the most properties |
| 2 | Crash-time durability/recovery windows | `durable-unacked-events-survive-crash`, `recovery-completes-after-crash`, `partial-write-at-rotation-recovers`, `every-written-event-eventually-delivered` (4 properties) | **Well-covered** |
| 3 | Config-reload silent loss (#24948) | `config-reload-no-silent-loss`, `graceful-shutdown-flushes-all` (2 properties) | **Adequately covered** — both Cluster F properties address the Drop/flush root cause |
| 4 | `drop_newest` metric blindness (#24606/#24144) | `dropped-events-are-counted` (1 property) | **Adequate** — single, targeted; the bug is self-contained |
| 5 | Sink-error acks discarded (`_status` ignored) | `sink-failure-not-silently-acked` (1 property) | **Adequate** — single, targeted |
| 6 | File-ID rollover (`reader.rs:932` raw `u16 >`) | `file-id-rollover-stays-coordinated` (1 property) | **Adequate** — single, targeted |
| 7 | Reader skips rest of file on first bad record | `corruption-is-detected-and-recovered` (captures skip path), `no-corrupted-record-delivered` | **Thin** — the skip-rest-of-file data-loss dimension is noted in open questions but has no dedicated property quantifying or bounding the abandonment loss |
| 8 | `get_total_records` non-wrapping `- 1` → ~2^64 | `record-id-wraparound-accounting-holds` (1 property) | **Adequate** |
| 9 | mmap SIGBUS / external file tampering / foreign `.dat` | **No property in the catalog** | **GAP — see Finding F1** |

---

## 2. Cross-Cutting Gap Analysis (Specific Prompt Items)

### Finding F1 — UNCOVERED: mmap SIGBUS / external file tampering / foreign `.dat` file

**SUT evidence:** sut-analysis §6 item 9; §8 (memmap2 bullet: "SIGBUS if `buffer.db` is
truncated/unmapped — unhandled, crashes process"); §3 (foreign `.dat` files inflate
`total_buffer_size`; truncation under read → underflow).

**Catalog check:** No property addresses:

1. `buffer.db` truncated while mmap'd → SIGBUS → unhandled crash (no property on
   SIGBUS resistance or graceful degradation of the mmap'd ledger).
2. A foreign/unexpected `.dat` file in the buffer directory inflating
   `update_buffer_size` over-seed → feeding directly into the underflow bug via a
   non-crash path (operator error, symlink attack, leftover file from a previous
   process).
3. External truncation of a `.dat` file under an active read → `bytes_read >
   metadata.len()` → underflow at `reader.rs:524` via a filesystem-level fault, not
   a crash.

**Scope:** These are distinct from all existing properties. The SIGBUS path is an
unhandled signal → process crash → restart loop. The foreign `.dat` path is a
silent, crash-free over-seed of `total_buffer_size` that produces the same
deadlock as #21683 without a crash trigger.

**Suggested action:** Add a property `ledger-resists-external-filesystem-corruption`
with two sub-cases: (a) `assert_unreachable` at any unhandled SIGBUS (equivalently,
assert the process does not crash after a filesystem truncation fault on `buffer.db`);
(b) `assert_always(total_buffer_size == sum_of_valid_own_dat_files)` at startup, using
a workload-side injection of a foreign `.dat` file before restart to confirm the
over-seed path is either prevented or does not cause deadlock. This property
requires a filesystem-level fault capability (Antithesis can truncate files or inject
foreign files via the deterministic filesystem model).

---

### Finding F2 — UNCOVERED: throughput-under-lock-contention (the performance issue)

**SUT evidence:** sut-analysis §4 ("The writer mutex is the **lock-contention
bottleneck** noted in the GA doc… ~90 MiB/s with 10 threads"); §10 not listed but
§4 explicitly notes this; the GA doc external reference (internal buffers GA design doc)
calls it a known throughput ceiling.

**Catalog check:** No property covers throughput degradation under writer `Mutex`
contention. The catalog has liveness properties for the deadlock case
(`writer-eventually-makes-progress`) but nothing for the performance-under-contention
case: does the buffer's throughput remain within an acceptable bound when multiple
sources/transforms compete for the `Arc<Mutex<BufferWriter>>`? Antithesis CPU
throttling is the lever to exacerbate contention without deadlock, which is a
distinct failure mode.

**Scope:** This is a liveness/performance gap. The known ceiling (~90 MiB/s with 10
threads) may not be a correctness bug per se, but: (a) CPU throttle + mutex
contention can drive throughput to zero without triggering any of the existing
deadlock properties (the writer is making progress but at 0.01 MiB/s); (b) there is
no property that would catch a regression that makes contention worse; (c) the GA doc
explicitly flags this as a gap in the existing chaos test.

**Suggested action:** Add a property `throughput-under-contention-acceptable` as a
Liveness property: during CPU-throttle faults with N parallel writer sources
(N >= 4), write throughput must stay above a configurable floor (e.g., 10% of no-fault
baseline) within a bounded time window. This requires the workload to measure
throughput independently of whether the writer is deadlocked. It would distinguish
"barely making progress" from "deadlocked" — a distinction the current catalog cannot
make.

---

### Finding F3 — UNCOVERED: clock-jitter × 500ms `should_flush` deadline interaction

**SUT evidence:** sut-analysis §4 ("Under CPU-throttle the winner can be descheduled
between winning the CAS and actually fsyncing, silently extending the 500ms window");
§10 ("Clock jitter × `should_flush`: `Instant::elapsed` drives the 500ms gate;
Antithesis clock faults could stretch/shrink the durability window").

**Catalog check:** The `durable-unacked-events-survive-crash.md` mentions CPU throttle
as a secondary lever and the deployment topology notes clock jitter as a fault lever.
But no property directly asks: when clock jitter compresses the `should_flush` gate to
fire more frequently (effectively increasing fsync frequency), or stretches it to fire
much less frequently (widening the durability window beyond 500ms), does the buffer
remain correct? Specifically:

- **Clock runs fast:** `should_flush` fires every few ms → excessive fsync contention
  → does the CAS at `ledger.rs:last_flush` create a miss-and-extend window?
- **Clock runs slow (or frozen):** `should_flush` never fires → the 500ms window is
  actually "forever" → crash can lose data that the customer expected to be durable
  because `flush_interval` has elapsed in wall time but not in `Instant` time.
- **Clock jitter × `should_flush` CAS winner descheduled:** The winner of the CAS
  marks the time slot but then is preempted before calling `sync_all`; the 500ms
  window implicitly restarts, extending the loss window by up to one full CPU
  quantum. No property currently asserts a bound on the maximum loss window under
  this condition.

**Scope:** Medium. The 500ms guarantee is part of the product SLA ("data synchronized
every 500ms"). Clock jitter violating that SLA in both directions is a safety concern
(for the slow-clock direction) and a performance regression concern (for the fast-
clock direction). Neither is covered by any existing property.

**Suggested action:** Add a property `fsync-deadline-respected-under-clock-jitter`:
with Antithesis clock faults active, assert that either (a) every fsync completes
within 2× the configured `flush_interval` in wall time (allowing some jitter margin),
or (b) the maximum data-loss window never exceeds `flush_interval × jitter_factor`.
This requires workload-side wall-clock timestamping of events alongside `Instant`-
based timing inside the SUT. The property would be `Always` for the upper bound and
`Sometimes` for confirming the fast-clock path is reached.

---

### Finding F4 — UNCOVERED: `WhenFull::Overflow` + disk base — reordering and in-memory loss

**SUT evidence:** sut-analysis §10 ("**`WhenFull::Overflow` + disk base:** unbiased
`select!` over base+overflow reorders events across the overflow boundary; if overflow
is in-memory, a crash loses the *later* in-memory events while the *earlier* disk
events survive — breaks dedup-based at-least-once reasoning (a gap, not just
duplicates)").

**Catalog check:** The catalog covers `drop_newest` (`dropped-events-are-counted`) and
`block` (via the deadlock cluster), but has **no property** for the
`WhenFull::Overflow` mode. This is a structurally distinct behavior: events are
accepted into an overflow buffer (in-memory), the disk buffer drains, then overflow
events are replayed — but a crash during this window loses the overflow-only events
while preserving the disk events. Because the disk events arrived *earlier* but are
read *after* the overflow events' crash-loss, the downstream sees a gap in the middle
of the stream, not just at the tail. This breaks dedup-based at-least-once because
the downstream may have already acked the "earlier" disk events and now skips the
gap.

**Scope:** This is a distinct `WhenFull` mode that is not exercised by the catalog
at all. The topology file acknowledges only `block` (the default) and `drop_newest`
(one config variant). The `Overflow` mode with a disk base is a real customer
configuration (chaining in-memory overflow onto disk for bursts).

**Suggested action:** Add a property `overflow-chain-no-reorder-loss`: configure
`when_full: overflow` with the disk buffer as the base and an in-memory buffer as
the overflow. Crash during a period when the overflow is active (in-memory events
exist but have not yet been replayed to disk). Assert (a) no gap exists in the
delivered event IDs post-recovery (the disk events are delivered, and the
overflow-lost events are explicitly counted in `component_discarded_events_total`),
and (b) the event ordering at the downstream does not place disk events *after*
in-memory events that were delivered pre-crash. This property requires the workload
to track sequence numbers across the overflow boundary.

---

### Finding F5 — UNCOVERED: `DiskBufferV1CompatibilityMode` flag inversion / forward-compat foot-gun

**SUT evidence:** sut-analysis §10 ("**`DiskBufferV1CompatibilityMode` flag inversion**
(`vector-core/event/ser.rs`): `can_decode` requires the V1-compat flag on every
record; a future 'V2-native' flag scheme would be rejected as incompatible — a
forward-compat foot-gun").

**Catalog check:** No property in the catalog covers this. There is no "format
version upgrade" or "compatibility mode correctness" property. The existing catalog
entirely omits the `DiskBufferV1CompatibilityMode` layer.

**Scope:** Two related gaps:

1. **Flag inversion:** If the V1-compat flag is written incorrectly (inverted), a
   reader that expects the flag present would reject all records as incompatible,
   silently abandoning the buffer. No property detects this.
2. **Format/version upgrade:** A rkyv layout change (e.g., a dependency bump that
   changes field ordering or alignment) would make old buffer files unreadable.
   No property asserts that a buffer written by version N is readable by version N+1,
   or that a version mismatch is detected gracefully rather than silently producing
   garbage.

**Suggested action (flag inversion):** Add a property `v1-compat-flag-correct`:
`assert_always_or_unreachable` at the `can_decode` check in `ser.rs` that every record
read from the disk buffer has the V1-compat flag set (until the flag scheme is
changed). This is a cheap invariant that would catch flag inversion immediately.

**Suggested action (format/version upgrade):** Add a property
`buffer-survives-version-upgrade`: the workload writes N events with version N, shuts
down, upgrades the binary (or swaps the binary in the container), restarts, and asserts
all N events are readable. This requires the harness to support binary swap — a non-
trivial workload design, but the risk is real: a rkyv bump with silent layout change
is the most plausible path to catastrophic "all existing buffers unreadable" regression.

---

## 3. Low-Investment / Over-Investment Assessment

### Potentially over-invested: `record-never-spans-files`

**Catalog entry:** Safety / `AlwaysOrUnreachable` — a record never spans two data files.

**SUT context:** The `can_write` gate (`writer.rs:433-437`) enforces this statically
before every write. The only way to bypass it is a corrupted `metadata().len()` size-
seed or a `max_record_size > max_data_file_size` misconfig. The former requires a
specific filesystem fault on the stat call, and the latter is gated by a `debug_assert`
(albeit compiled out of release).

**Balance concern:** This property addresses a violation that requires an extremely
specific fault sequence (filesystem fault between `open` and `metadata`, plus a
write that lands precisely at the file boundary), and a violation would manifest as a
`PartialWrite` that the reader handles correctly (rolls to next file). The data-loss
consequence is real but bounded (one record). Compared to the unlimited silent loss
from the underflow deadlock or config-reload unflushed data, this is a lower-value
target. The existing `can_write` gate is a compile-time static check in the common
path; its bypass path is narrow. Time spent reaching this property might be better
spent on F1–F5 above.

**Verdict:** Not over-invested per se (the property is correct and targeted), but if
the catalog needs to be pruned, this is the first candidate. It is the lowest-impact
safety property in the set.

---

## 4. Property-Type (Safety / Liveness / Reachability) Balance Assessment

### Current distribution

| Type | Properties | Slugs |
|------|-----------|-------|
| Safety (`Always` / `Unreachable`) | 11 | `no-corrupted-record-delivered`, `record-never-spans-files`, `total-buffer-size-never-underflows`, `buffer-size-within-max`, `durable-unacked-events-survive-crash`, `sink-failure-not-silently-acked`, `dropped-events-are-counted`, `file-id-rollover-stays-coordinated`, `record-id-wraparound-accounting-holds`, `config-reload-no-silent-loss`, `partial-write-at-rotation-recovers` (safety component) |
| Liveness (`Sometimes` / progress) | 7 | `writer-eventually-makes-progress`, `every-written-event-eventually-delivered`, `recovery-completes-after-crash`, `acked-files-eventually-deleted`, `reader-drains-and-terminates-cleanly`, `graceful-shutdown-flushes-all`, `partial-write-at-rotation-recovers` (liveness component) |
| Reachability (`Sometimes` confirming path fires) | 3 | `corruption-is-detected-and-recovered`, `partial-write-at-rotation-recovers` (reachability component), `recovery-completes-after-crash` (reachability component) |

Note: `partial-write-at-rotation-recovers` and `recovery-completes-after-crash` each
span multiple types; the counts above are approximate.

### Assessment

**Safety (11/19): appropriately dominant.** The disk buffer's primary failure modes
are silent-safety violations (underflow deadlock, config-reload loss, corrupted records,
metric blindness). Safety properties dominating the set is correct.

**Liveness (7/19): adequate but slightly thin on the space-reclamation / wakeup-chain
dimension.** `acked-files-eventually-deleted` and `reader-drains-and-terminates-cleanly`
cover the main liveness paths. The wakeup-chain transitivity (finalizer → reader → writer)
is analyzed in `writer-eventually-makes-progress.md` but there is no standalone property
confirming that breaking the chain at the **finalizer** step (killed finalizer task,
not the reader or writer) alone triggers the deadlock. This is noted as a concern in
open questions but has no dedicated property.

**Reachability (3/19): thin, possibly deliberately.** The three reachability
properties confirm that key recovery code paths are actually exercised. The catalog
correctly notes that `AlwaysOrUnreachable` is used where "never executed is acceptable,
but any execution must satisfy the invariant." However, there are additional recovery
branches that should be confirmed reachable but currently have no `Sometimes` check:

- `validate_last_write` `Ordering::Less` path (ledger lags data) — covered in
  `partial-write-at-rotation-recovers.md` assertions but not as a standalone catalog
  property.
- `validate_last_write` `Ordering::Greater` path (data lags ledger) — same.
- `seek_to_next_record` fast-path file deletion during recovery — same.

These are listed as SDK assertions in the evidence files but the catalog does not
elevate them to standalone properties. Given that `validate_last_write`'s branches
are the most crash-sensitive code in the buffer, a property
`validate-last-write-both-branches-reachable` with two `Sometimes` assertions would
close this gap.

---

## 5. Component Blind Spots

### The `OrderedFinalizer` task is a single point of failure with no dedicated liveness property

**SUT evidence:** The wakeup chain documented in `writer-eventually-makes-progress.md`
shows: sink acks → `BatchNotifier` dropped → finalizer task → `pending_acks` →
reader → file deletion → writer unblocks. The finalizer task is a `tokio::spawn`
that holds `Arc<Ledger>` and is never awaited or monitored.

**Catalog gap:** `writer-eventually-makes-progress` covers the full-chain deadlock,
and `acked-files-eventually-deleted` covers the file deletion. But no property
specifically asks: "if the finalizer task panics or exits early (e.g., due to a
tokio runtime shutdown racing with an in-flight ack), are events permanently stranded
in the buffer?" The gap is especially relevant because the finalizer task uses
`stream.next().await` without a timeout, and any panic in that task is unobserved.

**Suggested action:** Add a property `finalizer-task-drains-on-shutdown`:
`assert_always` that after `BufferWriter::drop`, all pending acks that were in-flight
when drop occurred are eventually processed (i.e., no events are stranded because the
finalizer task exited before processing all items). This requires a SUT-side counter
of unprocessed finalizer items at the moment the writer task exits.

### The `should_flush` CAS winner / descheduling gap has no coverage

**SUT evidence:** sut-analysis §4 ("Under CPU-throttle the winner can be descheduled
between winning the CAS and actually fsyncing, silently extending the 500ms window").

**Catalog gap:** This is distinct from the clock-jitter gap (F3 above). Even without
clock jitter, CPU throttling can cause the `should_flush` CAS winner to hold the
"slot" for the 500ms window without actually fsyncing, effectively disabling fsync for
the duration of the throttle. No property covers this specific race. Covered implicitly
by `durable-unacked-events-survive-crash` (which should catch data loss from an
extended fsync gap), but only if the CPU throttle also co-occurs with a kill.

---

## 6. Catalog-Wide Structural Observations

### Missing: a "buffer metrics are not lying" umbrella property

The catalog has `record-id-wraparound-accounting-holds` (metrics level) and
`buffer-size-within-max` (control path), and the `dropped-events-are-counted`
(component-level metric). But there is no umbrella property that asserts, after
any operation, that the set of buffer metrics (`buffer_events`, `buffer_byte_size`,
`buffer_discarded_events_total`, `component_discarded_events_total`) are mutually
consistent. Several known bugs produce metrics that contradict each other (e.g.,
`buffer_byte_size = 0` via saturating_sub while `total_buffer_size = u64::MAX`
in the control path; `buffer_discarded_events_total` increments while
`component_discarded_events_total` stays 0). A workload-level property
`buffer-metrics-are-internally-consistent` would catch these disparities more
broadly than individual point properties.

### Catalog correctly identifies fault dependency as a global risk

The catalog-level note that "nearly every high-value property requires node-termination
faults" is accurate and critical. Of the 19 properties, 14 require SIGKILL to produce
the triggering state. If node termination is disabled in the Antithesis tenant, 14/19
properties will either never fire (Liveness `Sometimes` vacuously unfound) or pass
trivially (Safety `Always` never challenged). This is the single largest operational
risk for the catalog as deployed. The catalog correctly flags it but does not propose
a fallback strategy.

**Suggested addition:** For each property that requires node-termination, add a
complementary reachability property that confirms the fault trigger is actually being
reached (i.e., confirm the process is being killed, not that the property is vacuously
satisfied). Without this, a run where node-termination is disabled silently reports
"all liveness properties satisfied" with zero information content.

---

## 7. Summary Verdict

The catalog is **well-structured and appropriately weighted** toward the highest-risk
failure areas (underflow cluster, crash durability, config-reload loss). The
Safety/Liveness/Reachability distribution is roughly correct for the SUT's failure
mode profile (mostly silent safety violations). Five specific gaps need new properties,
one existing property is lower-value than the rest, and two structural additions would
improve the catalog's meta-coverage.

**Priority order for gap closure:**

1. F1 (mmap SIGBUS / foreign `.dat` files) — triggers the same deadlock as #21683 via
   a non-crash path; no existing property catches it.
2. F4 (`WhenFull::Overflow` chain) — an entire `WhenFull` mode is completely absent.
3. F2 (throughput-under-contention) — known performance ceiling, no regression test.
4. F3 (clock-jitter × `should_flush`) — SLA-level concern, no property.
5. F5 (V1-compat flag / format upgrade) — low probability but catastrophic when hit.

---

## Findings (Structured)

### GAPS (properties/areas with no catalog coverage)

| # | Property/Slug | Concern | Scope | Evidence | Suggested action |
|---|--------------|---------|-------|----------|-----------------|
| F1 | *(missing)* `ledger-resists-external-filesystem-corruption` | mmap SIGBUS on `buffer.db` truncation crashes process unhandled; foreign `.dat` files inflate `update_buffer_size` → underflow deadlock without a crash trigger | sut-analysis §6 item 9, §8 (memmap2), §3 | No catalog property; sut-analysis calls both out explicitly; the foreign-`.dat` path reaches the same `fetch_sub` site as #21683 via a non-crash, operator-error path | Add two sub-properties: (a) assert process does not crash under `buffer.db` truncation fault; (b) assert `total_buffer_size` at startup equals sum of only self-owned valid `.dat` files, using workload-injected foreign file |
| F2 | *(missing)* `throughput-under-contention-acceptable` | Writer `Mutex` contention under CPU throttle can degrade throughput to near-zero without triggering any existing deadlock property — a regression that current properties would miss | sut-analysis §4, GA doc external reference | Known ~90 MiB/s ceiling with 10 threads; no property measures throughput floor under stress | Add Liveness property: with N>=4 parallel sources and CPU throttle, throughput stays above a configurable floor; distinguish "barely alive" from "deadlocked" |
| F3 | *(missing)* `fsync-deadline-respected-under-clock-jitter` | Clock faults stretch/shrink the `should_flush` 500ms gate; slow clock can silently extend the loss window beyond the documented SLA; fast clock causes CAS mis-timing | sut-analysis §4, §10 (clock-jitter × should_flush) | No catalog property; deployment-topology.md lists clock jitter as a fault lever without a corresponding property | Add Always property: maximum observable loss window (wall time between consecutive fsyncs) stays <= `flush_interval × jitter_factor` under clock faults |
| F4 | *(missing)* `overflow-chain-no-reorder-loss` | `WhenFull::Overflow` + disk base: crash during in-memory overflow period loses in-memory events while disk events survive; gap in the middle of the stream breaks dedup-based at-least-once | sut-analysis §10 (WhenFull::Overflow) | Catalog covers `block` and `drop_newest` only; `Overflow` mode is entirely absent | Add property: with disk base + in-memory overflow, crash during overflow active period; assert no unaccounted gap in delivered IDs and no incorrect event ordering |
| F5 | *(missing)* `v1-compat-flag-correct` + `buffer-survives-version-upgrade` | V1-compat flag inversion (`vector-core/event/ser.rs`) silently makes all records incompatible; rkyv layout change makes old buffer files unreadable without detection | sut-analysis §10 (DiskBufferV1CompatibilityMode) | No catalog property; the only format/versioning coverage is CRC32C; a rkyv bump with silent layout change is undetectable by any existing property | (a) `assert_always_or_unreachable` at `can_decode` that V1-compat flag is present; (b) binary-upgrade workload scenario asserting existing buffer readable after upgrade |
| F6 | *(missing)* `finalizer-task-drains-on-shutdown` | The `OrderedFinalizer` tokio task is a single point of failure; if it panics or exits before processing all in-flight acks, events are permanently stranded | sut-analysis §4 (finalizer), writer-eventually-makes-progress OQ | No standalone property for finalizer-task liveness; covered only implicitly via `writer-eventually-makes-progress` | Add Always property: unprocessed finalizer items at writer-drop time == 0; or Sometimes confirming all pending acks are processed within bounded time after drop |
| F7 | *(missing)* `buffer-metrics-are-internally-consistent` | Multiple known bugs produce contradictory metrics (control-path `u64::MAX` vs. gauge `0`, `buffer_discarded` increments without `component_discarded`); no umbrella consistency check | sut-analysis §6 items 1,4,8; property-relationships Cluster E | Individual metric properties exist but no cross-metric consistency invariant | Add workload-level Always property: after any operation, `{buffer_events, buffer_byte_size, buffer_discarded_events_total, component_discarded_events_total}` are mutually consistent (no pair contradicts each other) |

### IMBALANCE / THIN COVERAGE

| # | Property/Slug | Concern | Scope | Evidence | Suggested action |
|---|--------------|---------|-------|----------|-----------------|
| T1 | `corruption-is-detected-and-recovered` (partial) | The "skip rest of file on first bad record" data-loss dimension is noted in the open questions but has no dedicated property quantifying or bounding the records-abandoned rate | sut-analysis §6 item 7; catalog open question in corruption-is-detected-and-recovered | One property covers the recovery path fires, but not the loss magnitude from valid-after-corrupt records abandoned in the same 128MB file | Add a measurement property or open question resolution: assert `records_abandoned_per_corruption_event <= max_data_file_size / min_record_size` with a `Sometimes` confirming the abandon path is reached |
| T2 | *(missing standalone)* `validate-last-write-both-branches-reachable` | `validate_last_write` `Ordering::Less` and `Ordering::Greater` branches are the most crash-sensitive code; they have SDK assertions in evidence files but are not elevated to standalone catalog properties | sut-analysis §3 (recovery), partial-write-at-rotation-recovers evidence | Both branches are covered in `partial-write-at-rotation-recovers` as sub-assertions; promoting them to standalone Reachability properties would make their coverage explicit and trackable | Add two Reachability properties: `Sometimes(validate_last_write_less_branch_reached)` and `Sometimes(validate_last_write_greater_branch_reached)` |
| T3 | Reachability properties overall | Only 3 of 19 properties are Reachability type; for a SUT where the most dangerous code paths are deep recovery branches that the existing test suite provably cannot reach, confirming fault injection actually reaches those paths is underweighted | sut-analysis §7 ("model FS makes sync_all no-ops"), existing-assertions.md | The catalog correctly uses `AlwaysOrUnreachable` for optional paths, but additional `Sometimes` properties confirming the Antithesis fault strategy reaches specific code would add meta-coverage confidence | Promote at least 3 additional recovery sub-paths to standalone Reachability properties (see T2 above plus `seek_to_next_record` fast-path and `delete_completed_data_file` under fault) |

### LOW-VALUE / POTENTIAL PRUNE

| # | Property/Slug | Concern | Scope | Evidence | Suggested action |
|---|--------------|---------|-------|----------|-----------------|
| L1 | `record-never-spans-files` | The `can_write` gate enforces this statically; bypass requires a specific filesystem fault on a stat call at the exact file-boundary; consequence is bounded (one record loss handled gracefully by PartialWrite roll); lowest-impact safety property in the catalog | sut-analysis §5 INV-2 ("Hard"); catalog entry | Gate is correct in the common path; the fault scenario is narrow; `partial-write-at-rotation-recovers` and `no-corrupted-record-delivered` cover the consequences if it were bypassed | Keep, but deprioritize for initial harness implementation; address after F1-F4 |

### PASSES (well-covered areas)

| Area | Why it passes |
|------|--------------|
| `total_buffer_size` underflow / deadlock cluster (Cluster A) | 4 properties from 4 distinct vantage points (root invariant, writer liveness, reader shutdown, bound vacuity) with cross-reference to avoid missing the vacuous-pass trap |
| Crash durability / recovery (Cluster B) | 4 properties covering synced-data survival, end-to-end at-least-once, init completion, and torn-tail recovery; the `validate_last_write` fast-paths are documented with precise code locations |
| Corruption detection (Cluster C) | Correct Safety+Reachability pairing (`no-corrupted-record-delivered` + `corruption-is-detected-and-recovered`); the unsafe `CheckBytes` surface is explicitly noted |
| Boundary arithmetic (Cluster D) | Both known arithmetic bugs (`u16 >` file-ID, `- 1` record-ID) have targeted properties; the connection to the monotonicity panic is traced |
| Observability-gap bugs (Cluster E) | Both known metric-blindness bugs (#24606 `drop_newest`, `_status` discarded) have properties; the `config-reload-no-silent-loss` property covers the internal config-reload incident vector |
| Lifecycle / shutdown (Cluster F) | Both `config-reload-no-silent-loss` and `graceful-shutdown-flushes-all` address the Drop/flush root cause from different angles |
| Safety dominance in catalog | 11/19 safety properties is appropriate for a SUT whose primary failure modes are silent correctness violations |
| Fault dependency documentation | The catalog correctly identifies node-termination as a global prerequisite and flags it prominently |

### UNCERTAINTIES

| # | Question | Why it matters |
|---|----------|---------------|
| U1 | Does `drop_newest` apply to disk buffers at all? (open question in `dropped-events-are-counted.md`) | If `try_write_record` never returns the item for disk buffers, the property is unreachable and the bug untestable via Antithesis without a different workload design |
| U2 | Does the topology call `writer.flush()` before dropping the writer on graceful shutdown? | Determines whether `graceful-shutdown-flushes-all` is a bug-exposing property or a correctness confirmation; affects `config-reload-no-silent-loss` similarly |
| U3 | Is the F5 (torn-tail rkyv CRC collision) probability high enough to be reachable in Antithesis without explicit fault shaping? | If CRC32C is an effective guard, `partial-write-at-rotation-recovers` may pass vacuously on the F5 sub-path; the property needs a workload that can observe whether the `Corrupted` vs. `Valid{garbage_id}` distinction is reached |
| U4 | Are node-termination faults enabled in the tenant? | 14/19 properties are unreachable without kill/restart; if disabled, the catalog reports nothing meaningful for its highest-value cluster |
| U5 | Is the `DiskBufferV1CompatibilityMode` flag check (`can_decode`) actually on the hot path for all disk-v2 reads, or only for records written with the old serialization? | Determines whether F5 (flag inversion) is a continuous risk or only affects buffers written by older Vector versions |
