---
sut_path: /home/ssm-user/src/vector
commit: b7aae737cef5dd37d1445915443a1eb97b584f85
updated: 2026-05-28
external_references: []
---

# Antithesis Fit Evaluation: Disk Buffer v2 Property Catalog

Evaluation lens: does Antithesis add unique value over deterministic tests for
each property, or is the property really unit/integration-test territory? Are
`Sometimes` / `Always` / `Unreachable` assertion types matched to what is
actually reachable and what Antithesis's scheduler explores? Are any properties
undervalued because the catalog underestimates how far Antithesis goes into
the unhappy paths?

---

## Section 1 — Findings (properties that need reconsideration)

### Finding 1: `record-id-wraparound-accounting-holds` — split into two distinct cases with very different Antithesis fit

**Property:** `record-id-wraparound-accounting-holds`
**Concern:** The catalog bundles two sub-cases whose Antithesis fit differs
sharply. The empty-buffer equality case (`next_writer == last_reader` → `0 - 1
= u64::MAX`) is a **pure unit-test bug**: no faults, no concurrency, no timing
sensitivity. Write one batch to a fresh buffer, ack all events, call
`get_total_records()` — the bug fires deterministically on every clean restart
of a drained buffer. The evidence file (`record-id-wraparound-accounting-holds.md`)
confirms this explicitly: "On a fresh start both IDs start at 0; `wrapping_sub(0,
0) = 0`, then `0 - 1 = u64::MAX`." It also confirms that in debug builds this
panics rather than wrapping silently. A single-line unit test exposing this
would run in milliseconds and catch the regression forever.

The true u64 wrap (writer at `u64::MAX`, reader at `u64::MAX`, writer wraps to
0) is a different matter: it requires writing `2^64 / avg_event_count` records,
which at any realistic event rate is astronomically unreachable in any timeline
— Antithesis or otherwise. There is no test-build hook that fast-forwards
record IDs to near `u64::MAX` in a running production binary (the
`unsafe_set_writer_next_record_id` helpers are `cfg(test)`-gated and unavailable
in production binaries, which is what Antithesis will run). So for an Antithesis
run against a production build, the true-wrap case is **vacuously unreachable**.

**Scope:** property-specific
**Evidence:** Evidence file §"When does the intermediate result equal 0?";
§"Antithesis Angle" item 2 ("requires injection via test-only helpers");
property-catalog.md "Drain the buffer completely; restart; assert buffer-size/
event-count gauges are near 0 (not ~2^64)."
**Suggested action:** Split into two: (a) Fix the empty-buffer `0 - 1` bug with
a unit test — remove this sub-case from the Antithesis catalog. (b) Mark the
true u64 wrap sub-case as "out of scope for Antithesis runs on production
binaries; document as a latent, astronomically-unlikely risk." If a regression
test is needed for the fix to the empty-buffer case, the unit test suite (not
Antithesis) is the correct vehicle. Keep the `get_total_records` return-value
sanity assertion in SUT instrumentation only as a cheap guard; it is not a
primary Antithesis target.

---

### Finding 2: `record-never-spans-files` — the normal path is a pure unit test; the fault path is thin

**Property:** `record-never-spans-files`
**Concern:** The spanning guard (`RecordWriter::can_write`) is a two-level gate:
a `u64` comparison checked before every write. The gate is correct and verified
by reading the code. The only way a record can span files is if the on-disk
`metadata().len()` value seeding `current_data_file_size` at writer-open is
underreported (Antithesis filesystem fault) or if the size counter drifts. For
the counter-drift case (plain `u64 +` addition, no saturation), the maximum
practical record size is 128MB, far below `u64::MAX`, so drift is not a
realistic concern outside cosmic-ray-level events. The fault path — corrupted
`metadata().len()` — is a plausible Antithesis filesystem fault, but:

1. Antithesis's filesystem fault model primarily covers write/sync failures and
   node-kill, not arbitrary corruption of metadata return values. Whether this
   specific fault is achievable in the target tenant needs confirmation.
2. Even if `metadata().len()` is corrupted to underreport, the
   `AlwaysOrUnreachable` assertion fires at `flush_record` after the size is
   updated — not a difficult detection scenario for a deterministic test if you
   can inject the fault at open time.
3. The evidence file's `Open Questions` section notes that the single-writer
   design prevents concurrency races between `can_write` and `flush_record`.

The `AlwaysOrUnreachable` assertion type is correct (the spanning branch should
be unreachable on a correct run), but the scenarios that can actually violate it
in an Antithesis run are narrow and depend on filesystem-metadata corruption that
may not be in Antithesis's standard fault toolkit.

**Scope:** property-specific
**Evidence:** `record-never-spans-files.md` §"Timing / Fault Conditions";
sut-analysis.md §5/INV-2 ("Hard" invariant); the two-level gate analysis in
the evidence file confirms the guard is sound under normal and partial-write
faults.
**Suggested action:** Retain in catalog as a cheap `AlwaysOrUnreachable`
assertion placed in SUT instrumentation (zero search budget — the assertion fires
only if the condition is violated). Lower its priority relative to the
deadlock/durability cluster. Do not drive Antithesis fault strategy around this
property. If the harness can inject corrupted `metadata().len()` values, keep
it as a secondary reachability check; otherwise it is a passive safety net.

---

### Finding 3: `dropped-events-are-counted` — a code bug detectable by a unit test; Antithesis value is regression-detection only

**Property:** `dropped-events-are-counted`
**Concern:** The evidence file confirms the violation is a **missing function
call**: `BufferEventsDropped::emit` does not call `emit(ComponentEventsDropped
{...})`. This is not a timing-sensitive or concurrency-sensitive bug. You do not
need to kill nodes, explore interleaving, or inject faults to observe it —
configuring `when_full: drop_newest` and overfilling the buffer with any
deterministic test immediately reveals the discrepancy between
`buffer_discarded_events_total` and `component_discarded_events_total`. The
existing test suite almost certainly misses this only because no test checks the
component-level counter, not because the bug is timing-sensitive.

Furthermore, the evidence file raises a key open question (OQ-4): whether
`drop_newest` is even reachable for disk buffers at all (disk-buffer
`try_write_record` may never return the item to the caller; the `was_dropped`
branch may be unreachable for disk buffers). If `drop_newest` is not reachable
for disk buffers, this property is vacuously satisfied for this SUT and should
be removed or redirected to in-memory buffers.

The 2-second reporter lag (OQ-3 in the evidence file) is a minor timing concern
but trivially handled by waiting a few seconds — this is not the kind of
timing-sensitivity that requires Antithesis's systematic exploration.

**Scope:** property-specific
**Evidence:** `dropped-events-are-counted.md` §"Call Chain"; direct `grep` in
evidence: "There is no reference to `ComponentEventsDropped` anywhere in
`lib/vector-buffers/`."
**Suggested action:** (a) Confirm first whether `drop_newest` is actually
reachable for disk buffers (OQ-4). If not, remove from catalog. If yes: (b)
file a unit test that configures `drop_newest`, overfills the buffer, waits 3s,
and asserts `component_discarded_events_total >= buffer_discarded_events_total`.
This will catch the bug and serve as a regression test without consuming
Antithesis search budget. In Antithesis, keep only the workload-side metric
assertion as a cheap pass-through check (no additional search budget); it should
not drive Antithesis fault strategy.

---

### Finding 4: `sink-failure-not-silently-acked` — logic bug, but observability requires fault injection; Antithesis fit is moderate, not high

**Property:** `sink-failure-not-silently-acked`
**Concern:** The violation (`_status` discarded in `spawn_finalizer`) is a
confirmed logic bug, but unlike the deadlock cluster bugs it is observable
without a kill-and-restart: within a single process lifetime, making the
downstream sink return `Errored`/`Rejected` status for a sustained window and
then checking that events are retained (not dropped) is a deterministic
integration-test scenario. The key open question from the evidence file (OQ-1)
is whether any actual Vector sink emits `Errored`/`Rejected` status at all, or
whether sinks swallow errors internally. If sinks do not propagate non-Delivered
status, the bug is dormant without a custom test sink.

Antithesis does add some value here: the "make the downstream sink error for a
window" scenario benefits from fault injection (toggling the mock sink between
5xx and 2xx), and the timing of how long errors persist relative to the buffer's
ack pipeline is worth exploring. But the core bug detection does not require
Antithesis's systematic search — a fixed-sequence integration test suffices.

The catalog marks this as "currently violated" and places it in Category 5. The
catalog's own framing is accurate but the priority as an Antithesis target is
overstated relative to the deadlock/durability cluster.

**Scope:** property-specific
**Evidence:** `sink-failure-not-silently-acked.md` §"Current Status: VIOLATED";
OQ-1 ("whether any sink actually emits Errored/Rejected status"); OQ-3 ("Is
there a nack/unack path in the buffer at all?").
**Suggested action:** Confirm OQ-1 (whether sinks emit non-Delivered status).
If yes: write a focused integration test using a custom test sink. Keep the
Antithesis workload-side assertion but do not invest Antithesis fault budget
beyond the standard backpressure fault (toggle sink to error). This is a
category-3 priority, not category-1.

---

### Finding 5: `buffer-size-within-max` assertion type mismatch — `Always` is vacuously true under the deadlock

**Property:** `buffer-size-within-max`
**Concern:** The catalog notes this property "must be evaluated jointly with
`writer-eventually-makes-progress`" and that the underflow deadlock makes the
bound "vacuously hold." This is correct but the concern is understated. An
`Always` assertion on disk-size <= max_size will never fire in the deadlock
scenario — the writer stalls and no new data is written, so the bound holds
trivially. The assertion is not wrong, but it provides zero signal on the most
important failure mode (the deadlock). An Antithesis run could report this
`Always` as passing even during a permanent writer deadlock.

The catalog's recommended check (watchdog summing `.dat` file sizes via the
workload, not the masked gauge) is correct, but the framing as an independent
Antithesis `Always` property is misleading — it should explicitly be a compound
check: `(disk_size <= max) AND (writer throughput > 0)`. Neither condition alone
is sufficient.

**Scope:** property-specific
**Evidence:** property-catalog.md §`buffer-size-within-max` "Antithesis Angle:
Must be evaluated jointly with `writer-eventually-makes-progress`"; writer-
eventually-makes-progress.md §"Relationship to buffer-size-within-max."
**Suggested action:** Remove as a standalone Antithesis `Always` property or
explicitly mark it as a derived signal: "passes vacuously when writer is
deadlocked; only meaningful in conjunction with `writer-eventually-makes-progress
` passing." In the workload, combine the disk-size check with a write-throughput
check to avoid false-green reporting under the deadlock.

---

### Finding 6: `corruption-is-detected-and-recovered` — `Sometimes` reachability concern; value depends on fault toolkit

**Property:** `corruption-is-detected-and-recovered`
**Concern:** This is a `Sometimes` reachability property — it asserts the
`is_bad_read() → roll_to_next_data_file` branch actually fires. The value is
legitimate: confirming that the fault injection is reaching the right code path
and that the recovery branch is not dead code. However, the
`AlwaysOrUnreachable` sibling property (`no-corrupted-record-delivered`) is
the safety property; this `Sometimes` is purely confirmational.

The risk is that if Antithesis's fault model does not have direct disk-data-
corruption (bit-flip on a specific file byte) in its standard toolkit, the
`Sometimes` may never be satisfied and Antithesis will report a reachability
failure — not because the recovery path is broken, but because the fault type
needed to reach it was never injected. Antithesis's standard fault suite covers
node-kill, CPU throttle, and clock jitter; direct disk byte-corruption is
described as a "bit-flip / partial-write / torn-tail fault" in the catalog but
the tenant fault availability must be confirmed.

**Scope:** property-specific
**Evidence:** property-catalog.md §`corruption-is-detected-and-recovered`
"Antithesis Angle: Inject faults while the reader's BufReader has the file open";
catalog-wide Open Question "Are node-termination faults enabled?"; evidence file
OQ-1 "Does the seek_to_next_record init loop invoke roll_to_next_data_file?"
**Suggested action:** Confirm whether the tenant supports direct disk-byte-
corruption faults. If not: scope this property down to "partial write / torn
tail from node-kill" only (node-kill at a write boundary leaves a torn record,
which exercises `PartialWrite` detection and recovery). Partial-write detection
is reachable via standard node-kill faults. The deeper bit-flip scenarios
(testing the `CheckBytes` surface) require explicit filesystem corruption.

---

### Finding 7: `graceful-shutdown-flushes-all` assertion type mismatch — `Sometimes` may never be satisfied without careful workload design

**Property:** `graceful-shutdown-flushes-all`
**Concern:** The property is typed as `Sometimes(graceful_shutdown_lossless)`,
meaning Antithesis must find at least one execution where graceful shutdown
completes with zero loss. The evidence file's analysis reveals that whether this
`Sometimes` is satisfiable at all depends on unresolved open questions: does
the topology's `stop()` drop `inputs` (and thus the `BufferWriter`) before the
write-loop task's final `flush()` completes? If `inputs` is always dropped
synchronously inside `stop()` before the future is returned, the race is always
present, and a `Sometimes` "graceful shutdown lossless" may never be achieved if
the workload sends events right up to the SIGTERM boundary. Alternatively, if
the write loop always fully flushes before `inputs` is eligible for drop (because
the channel drains before `stop()` proceeds), the `Sometimes` is always satisfied
and Antithesis provides no additional insight.

The `Sometimes` type is also in tension with the "this is a safety property"
framing: a `Sometimes` says "it happens at least once," not "it always
happens." The intended claim is that graceful shutdown is *always* lossless
(an `Always` property), but the catalog defers to `Sometimes` because the
execution reaching a graceful stop at all needs to be confirmed.

**Scope:** property-specific
**Evidence:** `graceful-shutdown-flushes-all.md` §"Does stop() call flush before
dropping inputs?"; §"Critical ordering question"; OQ-1 "Does stop() call flush
before dropping inputs?" (marked "Verification required"); INV-6 in sut-analysis
"graceful shutdown flushes (but see §10 — BufferWriter::Drop does NOT flush)."
**Suggested action:** Resolve OQ-1 (trace the drop order in `running.rs`) before
committing to this as an Antithesis property. If graceful shutdown is always
lossless on the happy path, convert to `Always`. If it is sometimes lossy
(race between `inputs` drop and write-loop flush), this is a bug to fix
first and then an `Always` regression test. The current `Sometimes` framing
defers a design question that should be answered by code inspection, not
Antithesis exploration.

---

## Section 2 — Underestimated Properties (marked lower-value but actually high)

### Underestimate A: `record-id-monotonicity-holds` is higher-value than implied

The catalog lists this as a `Safety / Unreachable` property — a panic that must
never fire. The catalog text is correct but the placement in Category 1 alongside
corruption properties undersells its connection to the deadlock cluster. The
monotonicity panic at `reader.rs:480-484` can be triggered by: (1) the file-ID
rollover non-wrap-aware comparison at `reader.rs:932`; (2) a wrong `next_record_
id` fast-forward from `validate_last_write` in the `Ordering::Greater` branch
after a torn-tail. Both triggers require node-kill faults and timing-sensitive
state that deterministic tests cannot reach (the model FS never produces real
partial writes). If the panic triggers, the process crashes and loops on restart
if the persisted state is wrong — same operational impact as the deadlock (#21683).

The catalog's `Unreachable` assertion is correctly placed, but the property
should be explicitly cross-referenced as part of the deadlock/durability cluster,
not isolated in the corruption category. Its Antithesis fit is high because the
triggering conditions (torn tail, file-ID rollover at restart) require exactly
the systematic timing exploration Antithesis provides.

### Underestimate B: `file-id-rollover-stays-coordinated` is high-value for Antithesis specifically

The catalog notes this is a "latent bug" reachable in test builds (`MAX_FILE_ID=
6`). This is exactly the kind of property Antithesis is uniquely positioned to
find: with `MAX_FILE_ID=6`, the Antithesis harness will naturally cycle through
all 6 file IDs in a sustained run (small `max_data_file_size` helps), and with
systematic fault timing it will hit the rollover boundary under crash conditions
that a fixed-sequence test misses. The raw `u16 >` comparison at `reader.rs:932`
is a latent production bug (at `MAX_FILE_ID=65535` requires ~8TB written) but is
continuously exercisable in test builds. No existing test covers the rollover
scenario under crash conditions. This property's Antithesis fit is high and the
catalog should flag it as a primary target for test-build runs.

### Underestimate C: `every-written-event-eventually-delivered` benefit extends beyond the deadlock scenario

The catalog frames this primarily as an at-least-once e2e check. Its secondary
value — exposing the `unlink-before-ledger-flush` window at `reader.rs:546-549`
and the in-flight finalizer task loss on SIGKILL — is also high. These windows
are precisely the kind of narrow, timing-sensitive data-loss paths that
Antithesis's systematic scheduler explores. The catalog is correct but the
framing should more prominently note that the at-least-once liveness property
is also the primary vehicle for surfacing the three silent-loss paths identified
in the SUT analysis (§6 items 3–5).

---

## Section 3 — What Looks Correct

**The deadlock/durability cluster is correctly identified as the high-value
core.** The five properties `total-buffer-size-never-underflows`,
`writer-eventually-makes-progress`, `durable-unacked-events-survive-crash`,
`partial-write-at-rotation-recovers`, and `recovery-completes-after-crash` are
all genuine Antithesis targets: they require node-kill faults, their triggering
conditions are timing-sensitive (exact byte offset of the kill relative to the
fsync window and file-rotation boundary), they cannot be reached by the existing
test suite (model FS has no-op sync), and they have a confirmed known-unfixed
production bug (#21683). The `Unreachable` and `Sometimes` assertion types are
correctly chosen for these properties.

**`no-corrupted-record-delivered` is correctly in Antithesis territory.** The
hand-written `CheckBytes` (unsafe, rkyv ICE workaround) is an unusual validation
surface that benefits from systematic bit-flip exploration. The CRC32C collision
risk is documented and residual. The `AlwaysOrUnreachable` type is correct: on
a clean run the assertion is unreachable (no corruption injected), and under
faults it must always hold.

**`acked-files-eventually-deleted` and `reader-drains-and-terminates-cleanly`
correctly target the finalizer-task dependency chain.** Killing the finalizer
task mid-run (node-kill) and confirming the dependent liveness chain breaks
(or correctly recovers) is exactly the kind of multi-step concurrent-failure
path that unit tests cannot reach. The disabled test `#23456` is a direct
prior art data point for why deterministic testing failed here.

**`config-reload-no-silent-loss` is correctly identified as requiring a custom
fault.** The SIGHUP-driven reload path is not covered by Antithesis's standard
node-kill or network-partition faults. The property correctly notes this
dependency and the need to confirm feasibility with the tenant team. The
existing `topology_disk_buffer_config_change_does_not_stall` test explicitly
does NOT assert zero event loss (only liveness), confirming that Antithesis
adds something the test suite doesn't have.

**The `Unreachable` on `record-id-monotonicity-holds` is correctly typed.**
The panic at `reader.rs:480-484` is a guardrail that must never trip; any
execution that reaches it is a bug. `Unreachable` is the correct Antithesis
assertion type — Antithesis reports a failure the first time the point is hit.

**The catalog's catalog-wide fault dependency flag is critical and correct.**
Flagging that "nearly every high-value property requires node-termination faults,
which are often disabled by default" is the most operationally important warning
in the catalog. Without kill-and-restart faults enabled, Categories 2–6 show
as either unfound (liveness) or vacuously passing (safety under no-crash
condition). This must be confirmed before any Antithesis run is submitted.

---

## Section 4 — Uncertainties

1. **Antithesis bit-flip / disk-byte-corruption fault availability.** Several
   Category 1 properties (`no-corrupted-record-delivered`,
   `corruption-is-detected-and-recovered`, `record-never-spans-files` under
   corrupted metadata) depend on direct filesystem byte-corruption, not just
   node-kill. Whether this is available in the target tenant is unconfirmed.
   If not available, Category 1 properties reduce to "torn tail from node-kill"
   coverage, which is less comprehensive but still valuable.

2. **Production binary vs. test binary.** `file-id-rollover-stays-coordinated`
   and (to a lesser extent) `record-id-wraparound-accounting-holds` depend on
   test-only constants (`MAX_FILE_ID=6`, `unsafe_set_*_record_id` helpers). If
   Antithesis runs production binaries, the rollover is unreachable without
   sustained high-volume writes exceeding 8TB. Clarify which binary type the
   Antithesis harness uses and whether test-build constants can be compiled in
   without `#[cfg(test)]` gating.

3. **Filesystem persistence across node-kill.** The deployment-topology document
   correctly flags this as critical: if Antithesis node-termination recreates
   the container with a fresh filesystem, all crash-durability properties pass
   vacuously. Whether the target tenant's persistent-volume model survives node-
   kill must be confirmed before any crash-durability test is meaningful.

4. **Reachability of `Sometimes` under fault injection timing.** For
   `writer-eventually-makes-progress`, the `Sometimes(writer_unblocked_after_full
   )` assertion must fire on at least one non-fault execution (buffer fills and
   drains normally) before Antithesis considers it satisfied. The catalog confirms
   this should be reachable on the happy path, which is correct design. But the
   assertion only demonstrates liveness failure if it is never satisfied across
   all executions — including fault executions. Confirm that the test run
   duration and workload throughput allow buffer-full → drain cycles to occur
   frequently enough for the `Sometimes` baseline to establish before fault-
   injection scenarios begin.

5. **`sink-failure-not-silently-acked`: whether sinks emit non-Delivered status.**
   OQ-1 in the evidence file is unresolved: if Vector sinks internally retry and
   only surface success to the finalizer, `Errored`/`Rejected` status may never
   reach `spawn_finalizer` in any workload, making the property unreachable
   without a custom test sink. This must be answered before committing harness
   resources to this property.
