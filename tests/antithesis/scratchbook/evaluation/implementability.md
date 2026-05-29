---
sut_path: /home/ssm-user/src/vector
commit: b7aae737cef5dd37d1445915443a1eb97b584f85
updated: 2026-05-28
external_references: []
---

# Implementability Evaluation: Disk Buffer v2 Property Catalog

19 properties evaluated across 6 categories. The central implementation risk is
the **instrumented-build burden**: zero Antithesis SDK instrumentation exists in
the codebase today (confirmed by repo-wide grep), so every SUT-side assertion
must be added from scratch by adding `antithesis-sdk` as a new Cargo dependency
to `lib/vector-buffers`. This is a one-time setup cost shared across many
properties, but it is a real precondition for roughly half the catalog.

A second structural risk is the **persistent-volume assumption**: the deployment
topology explicitly requires that the `data_dir` survives node-termination faults.
If the Antithesis tenant recreates the container with a fresh filesystem on kill,
every crash-recovery property either passes vacuously (buffer wiped → clean init)
or fails spuriously (data expected that no longer exists on disk). This must be
confirmed before any Category 2–6 property is run.

---

## Category 1 — Data Integrity and Corruption

### no-corrupted-record-delivered

**Implementability: Feasible — requires instrumented build.**

The safety invariant (`AlwaysOrUnreachable` at `reader.rs:~1131`) is invisible
from the workload: the workload only sees events delivered downstream; it cannot
distinguish "record validated and passed" from "record bypassed validation." The
SUT-side `assert_always_or_unreachable!` is necessary and its placement is clean
(one choke point at `Ok(Some(record))` in `BufferReader::next`).

The instrumented build adds the SDK to `lib/vector-buffers/Cargo.toml` — a
one-time change. The assertion itself is well-posed. Fault injection (bit-flip on
`.dat` files, partial write via kill during flush) is supported by Antithesis.

One complication: the startup path (`seek_to_next_record`) calls
`validate_record_archive` directly at `reader.rs:850`, not through `try_next_record`.
The `Ok(Some(record))` assertion placement at `reader.rs:1131` may miss corruption
detected (and recovered) during startup replay — a second assertion at the startup
validation call site may be needed. This is an implementability wrinkle, not a
blocker: the assertion is straightforwardly placeable at both sites.

A secondary concern is that `receiver.rs` panics on `ReaderError` from `next()`,
meaning any detected corruption immediately crashes the process. The workload
cannot distinguish "corruption detected and recovered" from "process died" without
also checking restart behavior. The workload needs a liveness check alongside the
safety assertion.

**Verdict:** Implementable with instrumented build. Flag the two-site placement
and the panic-on-detection observation.

---

### corruption-is-detected-and-recovered

**Implementability: Feasible — requires instrumented build, with reachability
dependency on Antithesis fault injection timing.**

The `Sometimes(corruption_detected_and_recovered)` at `reader.rs:~1035`
(`is_bad_read()` branch) requires Antithesis to inject a bit-flip or truncation
while the reader's `BufReader` has the relevant file open. This is a timing
constraint: Antithesis must inject the fault during an active read, not while the
file is closed. Antithesis's filesystem fault injection is designed for exactly
this use case and should handle it.

The assertion is SUT-side (instrumented build required), but straightforwardly
placed. The companion concern is whether the `Sometimes` is reachable at all in
the absence of faults: it is not (the is_bad_read path requires actual corruption
or a partial write). This is correct for a `Sometimes` — it will be satisfied only
when fault injection reaches the live reader, which is the intended signal.

**Verdict:** Implementable with instrumented build and filesystem fault injection
enabled.

---

### record-id-monotonicity-holds

**Implementability: Feasible — requires instrumented build.**

The `Unreachable` at the monotonicity panic site (`reader.rs:~480-484`) is
trivial to implement: the panic is already there; replacing the panic with an
`assert_unreachable!` before the panic preserves the existing behavior while
adding Antithesis telemetry. (Alternatively, add the SDK call immediately before
the `panic!` macro.)

This property is a pure SUT-side assertion with no workload-observable equivalent.
The precondition — reaching the panic — requires one of: crashed `validate_last_write`
fast-forward producing a wrong `next_record_id`, torn-tail mis-recovery (F5),
or file-ID rollover misclassification. All require node-termination faults and
persistent-volume survival (see structural risks above).

One subtle point: the property evidence notes that `OrderedAcknowledgements::
add_marker` may not use wrap-aware comparison, making a fresh-buffer ID 0
a false-trigger candidate. This is a testability concern (the assertion might
fire spuriously on a clean buffer without faults). Needs verification before
asserting `Unreachable`.

**Verdict:** Implementable with instrumented build, node-kill faults, and
persistent volume. Verify the wrap-aware comparison question before committing
to `Unreachable`.

---

### record-never-spans-files

**Implementability: Mostly workload-observable; SUT-side optional.**

The workload can observe spanning indirectly: if a record is written successfully
(`Ok`) but never appears downstream, a span is a plausible cause. A more direct
check is a watchdog that monitors `.dat` file sizes and flags any single data file
exceeding `max_data_file_size + max_record_size`. This is a simple filesystem
enumeration, not a SUT-side assertion, and does not require the instrumented build.

The SUT-side assertion (in `RecordWriter::flush_record`) provides a direct, same-
process check, but the watchdog approach is implementable immediately with zero
SDK changes.

The `debug_assert` at `writer.rs:~396` being compiled out of release is a real
concern for the `max_record_size > max_data_file_size` misconfiguration path: in
release builds, the writer loops forever on `DataFileFull`. This is observable
from the workload (write throughput drops to zero) and does not require SDK
instrumentation to detect.

**Verdict:** Implementable without instrumented build via file-size watchdog.
SUT-side assertion is optional but would provide tighter detection.

---

## Category 2 — Buffer Accounting and Writer Liveness

This is the highest-value cluster. The implementability of all five properties
depends on the **persistent-volume assumption** and **node-termination faults**.
If either is unavailable, the entire cluster becomes unimplementable or vacuous.

### total-buffer-size-never-underflows

**Implementability: Requires instrumented build. The trigger requires a specific
crash-boundary scenario that Antithesis must find, not the workload construct.**

The internal state (`total_buffer_size` atomic) is completely invisible to the
workload. The only observable signal is downstream throughput collapse (which
could have many causes). The SUT-side assertions at `ledger.rs:~291` and
`reader.rs:~521-535` are necessary.

Regarding the "specific deterministic trigger" concern: the underflow requires a
kill at a file-rotation or partial-write boundary where file-on-disk bytes exceed
record-decoded bytes. The workload cannot reliably position this kill timing; it
relies on Antithesis's systematic exploration across timelines. With a small
`max_data_file_size` (1MB configuration), rotations happen frequently, making
the trigger window large relative to run time. This is a good fit for Antithesis
exploration (not a single needle-in-a-haystack moment).

One complication: the `trace!` log at `ledger.rs:295` includes
`last_total_buffer_size - amount`, which also wraps. In a debug build this would
panic before the `fetch_sub` wraps, preventing the bug from being observable in
release semantics. The harness should use a release build (or a build that
disables debug overflow checks) to observe the wrapping behavior rather than
catching it as a debug-mode panic. This affects harness build profile selection.

**Verdict:** Implementable with instrumented build, persistent volume, and
node-kill faults. Harness must use release mode (or explicitly allow wrapping
arithmetic in the `trace!` log). Antithesis exploration drives trigger discovery.

---

### writer-eventually-makes-progress

**Implementability: Requires instrumented build for the sharp signal; partially
observable from the workload.**

The workload can detect the deadlock symptom: write throughput drops to zero
after a node-kill-and-restart. The workload drives: fill buffer → kill → restart
→ resume reader → quiet period → assert write throughput recovered. This is
implementable without SDK instrumentation, though the signal is delayed (the
workload must wait for a grace period before concluding deadlock vs. slow recovery).

The SUT-side `Sometimes(writer_unblocked_after_full)` provides a precise signal
at the recovery point. The `Unreachable` on repeated no-progress wakeups is a
sharper deadlock indicator that requires the instrumented build.

The `Sometimes` assertion must be reachable on the non-fault path (any normal
full→drain cycle) to establish a baseline before fault testing. This is confirmed
by the property evidence: the normal fill/drain cycle satisfies the `Sometimes`.

**Verdict:** Workload-observable without instrumented build (throughput signal).
SUT-side instrumentation sharpens the signal. Both paths implementable.

---

### buffer-size-within-max

**Implementability: Workload-observable via file-size watchdog, no instrumented
build required. The deadlock-vacuity problem makes this property misleading in
isolation.**

The property's critical note is that it passes vacuously under the underflow
deadlock. The workload must check both:

1. `sum(.dat file sizes) <= max_buffer_size + max_record_size` (the bound)
2. `write_throughput > 0` after the quiet period (rules out deadlock)

Both are workload-observable. The property evidence correctly notes this must be
evaluated jointly with `writer-eventually-makes-progress`.

One subtle point: the workload must read actual file sizes, not the buffer metric
gauge, because PR #23561's `saturating_sub` makes the gauge show 0 (normal) even
under the deadlock. The watchdog must enumerate `.dat` files directly.

**Verdict:** Implementable without instrumented build. Must always be evaluated
jointly with `writer-eventually-makes-progress` to avoid vacuous pass.

---

## Category 3 — Crash Durability and Recovery

All five properties in this category require node-termination faults (SIGKILL)
and the persistent-volume assumption. If the persistent volume is unavailable,
all five become vacuous or spuriously failing.

### durable-unacked-events-survive-crash

**Implementability: Workload-observable in principle; the "durably written"
definition is the main challenge.**

The workload maintains `DURABLE` (confirmed-synced events) and `DELIVERED`
(post-restart downstream events) sets and asserts `DURABLE ⊆ DELIVERED`. This
is implementable at the workload level.

The open question — how to establish "durably written" without Vector internals
access — has a clean answer: use `flush_interval=0` (force fsync on every flush)
so every write that succeeds at the source level is durably synced. The workload
can then treat all successfully-accepted events as `DURABLE`, excluding only
events sent within the final window before the kill. This is the recommended
option (c) from the property evidence file.

The 500ms window exclusion must be handled carefully: the workload should stop
producing events some time (e.g. 2 seconds) before any kill to clear the window
boundary, or use `flush_interval=0` to eliminate the window.

**Verdict:** Implementable. Use `flush_interval=0` to eliminate the durability-
window ambiguity. Requires node-kill faults and persistent volume.

---

### every-written-event-eventually-delivered

**Implementability: Workload-observable; the most comprehensive test of the
at-least-once contract.**

The `PRODUCED` vs `DELIVERED_SET` comparison is straightforward workload logic.
The workload assigns unique IDs (e.g. monotonic counter embedded in event
payload), records all submitted IDs, records all downstream-received IDs, and
asserts the difference is empty after a quiet period.

One complication: this property intentionally includes the `_status`-discard bug
(`sink-failure-not-silently-acked`). To test pure crash durability without
conflating the two bugs, the workload should use a reliable downstream stub that
always returns a successful response (never `Errored`/`Rejected`), isolating the
crash-durability signal.

The `unlink-before-ledger-flush` window at `reader.rs:546-549` is the most
subtle latent loss path. The workload cannot control this timing; Antithesis
must find it. With many crash timings explored, the probability of hitting this
specific window is reasonable.

An `assert_always` variant (workload fires if `PRODUCED \ DELIVERED` is non-empty
after quiet period) combined with a `Sometimes` (at least one timeline achieves
full delivery) is the recommended assertion structure.

**Verdict:** Implementable as a workload-level check. Requires node-kill faults,
persistent volume, and a reliable downstream stub. Antithesis exploration drives
discovery of the loss windows.

---

### recovery-completes-after-crash

**Implementability: Partially workload-observable; SUT-side assertion provides
a cleaner signal.**

The workload can proxy init completion by measuring time from process start to
first event being deliverable (first event appears downstream). This is
workload-observable but has noise (the throughput signal conflates init completion
with the post-init deadlock described in OQ-1).

The critical subtlety (also noted in `writer-eventually-makes-progress`): init
can return `Ok` while the writer is immediately deadlocked by the underflow bug.
The property must assert post-init progress, not just `Ok` return. Keeping init-
completion separate from liveness (as the evidence recommends) means two separate
assertions, with the combined result telling the full story.

The SUT-side `assert_sometimes!` at the end of `from_config_inner` is clean and
confirms recovery is actually exercised. The `assert_always!` bounding
`update_buffer_size` output is a useful diagnostic addition.

**Verdict:** Implementable. Workload proxy is sufficient. SUT-side assertion
(instrumented build) sharpens diagnosis. Requires node-kill faults and persistent
volume.

---

### partial-write-at-rotation-recovers

**Implementability: Requires instrumented build; very high fault-timing
sensitivity, but well-supported by Antithesis's approach.**

The property requires kills specifically during the rotation sequence (8 distinct
windows at `writer.rs:1041-1138`). Setting `max_data_file_size` to 1MB makes
rotations happen every few seconds, giving Antithesis many opportunities. This is
a well-structured target: the fault windows are well-defined code paths, not a
single rare coincidence.

The F5 torn-tail risk (rkyv `archived_root` reading garbage footer bytes as a
valid root pointer, yielding a plausible-looking-but-wrong `Valid` record) is the
subtlest path. The workload cannot provoke F5 directly; it relies on Antithesis
hitting the specific byte position during crash. CRC32C is the practical backstop
(F5 only matters if garbage bytes also produce a CRC32C collision, probability
~1/2^32 per check). Antithesis runs many timelines and can empirically explore
whether F5 is reachable before CRC32C catches it.

The `Sometimes(torn_tail_recovered)` assertion (SUT-side, in `validate_last_write`)
confirms the torn-tail detection path is actually reached. Without it, rotation-
boundary kills might always hit a non-torn window and never exercise the recovery
path. This makes the instrumented build important for this property.

The safety sub-properties (`Always` no-garbage, `Unreachable` monotonicity panic)
are covered by `no-corrupted-record-delivered` and `record-id-monotonicity-holds`
respectively, avoiding duplication.

**Verdict:** Implementable with instrumented build, node-kill faults, persistent
volume, and small `max_data_file_size`. Antithesis exploration drives fault timing
discovery. CRC32C backstop makes F5 a low-probability but non-zero concern.

---

## Category 4 — Space Reclamation and Clean Termination

### acked-files-eventually-deleted

**Implementability: Partially workload-observable; best checked jointly with
writer liveness.**

The workload can check: after fully acknowledging all events in a data file (no
new writes, quiet period), enumerate `.dat` files and assert the acked file is
gone. The workload controls downstream ack delivery (it controls the HTTP stub
that Vector's HTTP sink delivers to); acknowledging all events is workload-
controllable.

The finalizer-task kill scenario (kill the spawned finalizer tokio task) is not
an independent Antithesis node-fault; the finalizer lives inside the Vector
process. A node kill kills everything, including the finalizer. To specifically
kill the finalizer without killing the whole process, the workload would need to
inject a panic inside the finalizer, which requires SUT-side modification. This
specific sub-scenario (finalizer task dead, rest of process alive) is an edge
case best deferred to a unit test rather than an Antithesis test.

The `bytes_read > metadata.len()` underflow path (noted in the open questions)
is the same underflow bug from Category 2, exercisable through the same fault
sequence.

**Verdict:** Implementable. File-enumeration watchdog covers the deletion check.
The finalizer-task-kill sub-scenario requires SUT modification to isolate and is
better treated as a unit test.

---

### reader-drains-and-terminates-cleanly

**Implementability: Workload-observable; directly addresses two disabled tests.**

The workload can detect both failure modes:

- Hang: reader does not return `None` within a bounded time after the writer stops
  and all events are acked. The workload measures time-to-drain.
- Premature `None`: events are missing from the delivered set (cross-reference
  `every-written-event-eventually-delivered`).

This property directly targets the root cause of the disabled
`reader_exits_cleanly_when_writer_done_and_in_flight_acks` test (`basic.rs`,
`#[ignore = "flaky #23456"]`). The Antithesis version of this test is
straightforward: stop writes, deliver and ack all events, assert drain completes.

The `total_buffer_size == 0` termination condition is broken by the underflow bug.
This makes the property's interaction with Category 2 critical: if the deadlock
fires, the reader never terminates cleanly. Running this property jointly with
`writer-eventually-makes-progress` identifies the cause.

**Verdict:** Implementable without instrumented build. Directly exercises the
flaky `#23456` path. Requires node-kill faults and persistent volume to test the
fault-then-drain scenario.

---

## Category 5 — Delivery Semantics and Boundary Conditions

### sink-failure-not-silently-acked

**Implementability: Feasible. The workload controls the downstream stub; the bug
is confirmed and observable. The custom fault is workload-driven, not Antithesis
node-fault-driven.**

The workload controls an HTTP endpoint that Vector's HTTP sink delivers to. To
inject sink errors, the workload simply returns 5xx responses for a window. This
is a workload-driven custom "fault," not a built-in Antithesis fault type. It is
fully implementable without any special Antithesis tenant capabilities.

The bug is confirmed (`ledger.rs:717` discards `_status`). The observable signal:
workload sends N events, forces the sink to error, waits for a quiet period, then
restarts Vector and checks whether those events are replayed. Under the bug, they
are not replayed (silently lost). This is a workload-observable end-to-end check.

One concern: the property catalog notes that whether sinks actually emit
`Errored`/`Rejected` status in Vector's internal BatchStatus plumbing (not just
returning HTTP 5xx at the transport level) needs verification. Vector's HTTP sink
may internally retry on 5xx responses, eventually timing out, and only then
surfacing `Errored`. The workload-observable test works regardless: make the sink
permanently error for the test window and observe that events are lost without
replay.

**Verdict:** Implementable without instrumented build. Workload-driven sink-error
injection is the fault mechanism, not a built-in Antithesis fault. The bug is
confirmed and the expected behavior is well-defined.

---

### dropped-events-are-counted

**Implementability: Workload-observable via metrics scraping. The
`drop_newest`-on-disk-buffer question needs verification.**

The workload can compare `buffer_discarded_events_total` (expected to increment
on drops) against `component_discarded_events_total` (expected to also increment
but currently stays 0). Vector exposes both via its `prometheus_exporter` source,
which the workload can scrape.

One critical concern: whether `drop_newest` applies to disk buffers at all. The
`try_write_record` for the disk buffer (`writer.rs:1166-1168`) returns `Some(item)`
(i.e., the item bounced back) when `is_buffer_full()` is true. The
`BufferSender::send` with `WhenFull::DropNewest` checks `try_send` and drops the
item if it is returned (`sender.rs:231-234`). This code path exists for disk
buffers: `SenderAdapter::DiskV2` implements `try_send` via `try_write_record`.
So the path is reachable.

However, the 2-second reporter lag (buffer metrics tick every 2 seconds) means
the workload assertion must allow for a brief delay between drops occurring and
`buffer_discarded_events_total` incrementing. The workload needs a wait period
before asserting the metric comparison.

**Verdict:** Implementable without instrumented build, via metrics scraping.
Must use `when_full: drop_newest` configuration. The 2-second reporter lag
requires a wait in the workload assertion. The disk-buffer `try_write_record`
path is verified as reachable.

---

### file-id-rollover-stays-coordinated

**Implementability: Requires a test binary (with `MAX_FILE_ID=6`) or synthetic
state injection (cfg(test)-gated). Production binary with default `MAX_FILE_ID=65535`
makes rollover unreachable in any practical Antithesis run.**

This is a structural implementability issue. In production binaries,
`MAX_FILE_ID = u16::MAX = 65535` (`common.rs:43`). Reaching rollover requires
65535 file rotations. At 1MB per file (test configuration), that is 65GB of data
throughput. At 128MB per file (default), it is ~8TB. No Antithesis run reaches
this threshold in normal test time.

In test binaries, `MAX_FILE_ID = 6` (`common.rs:45`, `#[cfg(test)]`). Rollover
is reached after 6 files, which at 1MB each takes seconds. However, the
`unsafe_set_writer_next_record_id` and `unsafe_set_reader_last_record_id` helpers
that could synthetically place the state near rollover are also `#[cfg(test)]`-
gated and unavailable in production builds.

Two options exist:

1. Run Vector in test build mode (which enables `MAX_FILE_ID=6`). This is non-
   standard for an Antithesis run (test builds may have other cfg-gated behavior).
2. Add a configuration knob (e.g. an env var or hidden config option) that sets
   `MAX_FILE_ID` at runtime, usable in a production build.

Option 1 is simpler but requires the Antithesis harness to build Vector with
`--cfg test` (or equivalent), which may interact with other test-only code paths.
Option 2 requires a Vector code change.

The raw `u16 >` comparison bug at `reader.rs:932` is real (confirmed by source
inspection). The question is only whether it can be triggered in the harness.

**Verdict:** Not directly implementable with a standard production Vector binary
without code changes or a test build. Requires either test binary mode or a new
configuration hook for `MAX_FILE_ID`. Flag as needing decision on harness build
mode before implementation.

---

### record-id-wraparound-accounting-holds

**Implementability: The empty-buffer equality case (the realistic bug path) IS
workload-observable without any instrumentation. The u64-wrap case is practically
unreachable.**

This is the most important implementability note in the entire catalog. The
property title suggests testing u64 record-ID wraparound (a ~2^64 write
threshold — completely unreachable by real traffic), but the actual bug that
fires on every clean restart with an empty buffer is the `0 - 1 = u64::MAX`
case in `get_total_records` at `ledger.rs:266`. This case is trivially reachable:

1. Write N events into the buffer.
2. Read and acknowledge all N events (drain completely).
3. Restart Vector with the same buffer directory.
4. Immediately scrape the `buffer_events_received_total` or `buffer_byte_size`
   gauge from the `prometheus_exporter`.
5. Assert the gauge value is near 0, not `u64::MAX = 1.844e19`.

Step 4 requires Vector to expose metrics, which the topology already does via
`internal_metrics` → `prometheus_exporter`. The workload can scrape this without
any SUT-side SDK instrumentation.

The test-helper `unsafe_set_writer_next_record_id` / `unsafe_set_reader_last_record_id`
for the true u64-wrap case are `#[cfg(test)]`-gated and unavailable in production
builds. The near-u64-MAX record ID scenario is not reachable by real traffic in
any test run.

The property as stated ("holds across u64 wraparound AND the empty-buffer equality
case") is therefore split:

- Empty-buffer equality case: **reachable, workload-observable, no instrumentation
  needed.** This is the real bug.
- u64 wrap case: **not reachable** with production binary (requires 2^64 writes).
  The property statement claiming both are tested is misleading for the wrap case.

The debug-build consideration: `0u64 - 1` panics in Rust debug mode. If the
Antithesis harness uses a debug build, the bug surfaces as a panic rather than a
silent `u64::MAX`. The property evidence notes this is a stronger signal.

**Verdict:** The empty-buffer equality case is fully implementable without
instrumented build via metric scraping after a drain+restart cycle. The u64 wrap
case is practically unreachable with a production binary and should be explicitly
descoped or relabeled. The property should be renamed/refocused to
`record-id-empty-buffer-accounting-holds` to reflect what is actually testable.

---

## Category 6 — Lifecycle and Config Reload

### config-reload-no-silent-loss

**Implementability: Requires custom fault (workload-driven SIGHUP); feasible
but with architectural dependency on Antithesis tenant capabilities.**

The `SIGHUP` mechanism is confirmed: `signal.rs:200, 218-219` handles `SIGHUP`
and converts it to `SignalTo::ReloadFromDisk`. Sending `SIGHUP` from the workload
container to the Vector process is straightforward: the workload knows the Vector
process PID (or uses `kill -HUP $(pidof vector)`). This does not require a
special Antithesis fault injection capability — the workload can send SIGHUP
directly to the Vector process using a standard OS signal.

This is a workload-driven trigger, not a built-in Antithesis fault. The Antithesis
scheduler does not need to be aware of reload semantics; the workload fires SIGHUP
on a schedule while Antithesis's node-kill and CPU-throttle faults run concurrently.

The loss-observable signal is workload-level: `accepted_event_ids == received_event_ids`
after a quiet period. The workload tracks every event accepted by the HTTP source
and every event received at the downstream stub.

The `BufferWriter::Drop` calling `close()` but not `flush()` means events
staged in the 256KB `TrackingBufWriter` at reload time are silently dropped. The
workload-level check catches this directly. No SUT-side instrumentation is needed
to observe the bug.

One concern: whether the per-process advisory lock gap (two Vector topologies
briefly sharing the buffer directory) is exercisable. The evidence suggests
`running.rs:688-710` attempts sequencing, but the finalizer task's `Arc<Ledger>`
retention may extend the overlap window. This is a background race observable
only if the workload drives reload under concurrent write load and the file
deletion lag causes corruption. The workload-level end-to-end assertion catches
any resulting loss.

**Verdict:** Implementable. Workload sends SIGHUP (no special tenant capability
required). The loss is workload-observable without instrumentation. Prioritize
alongside `graceful-shutdown-flushes-all` as they share the `Drop`-without-flush
root cause.

---

### graceful-shutdown-flushes-all

**Implementability: Workload-observable; the critical `stop()` drop-order question
needs a one-time code inspection or logging addition to resolve.**

The workload sends SIGTERM (graceful stop), waits for the Vector process to exit,
restarts with the same buffer directory, drains the buffer to empty, and asserts
all pre-shutdown accepted events appear downstream. This is a workload-level
end-to-end check requiring no SDK instrumentation.

The central uncertainty is whether `RunningTopology::stop()` at `running.rs:145`
drops `self.inputs` (and thus the `BufferSender` / `Arc<Mutex<BufferWriter>>`)
before or after the write-loop task completes its final `flush()`. The property
evidence analysis suggests `inputs` is dropped synchronously inside `stop()` before
the returned future is polled, which could mean the `BufferWriter` is dropped
(unflushed) while the write loop is still running. However, the write loop calls
`flush()` after every `write_record` (`sender.rs:86-98`), so if the last event
was processed before SIGTERM, `TrackingBufWriter` should be empty.

This is resolvable without code changes: add a `tracing::debug!` log in
`TrackingBufWriter::drop` showing `buf.len()` and run a graceful-shutdown test
to confirm it is 0. This is a diagnostic addition, not an SDK assertion.

The SUT-side `assert_always!(unflushed_bytes == 0)` in `close()` (proposed in
the evidence file) is a cleaner check but requires the instrumented build.

A subtle note: graceful shutdown may rely on OS page-cache flush-on-process-exit
for the 500ms fsync window, not an explicit Vector `sync_all`. This is OS-
dependent behavior (Linux guaranteed, but not Vector-guaranteed). The workload
test implicitly covers this by comparing events on restart.

**Verdict:** Implementable without instrumented build via workload-level end-to-end
assertion. The `stop()` drop ordering should be verified with a diagnostic log
or code trace before asserting zero-loss expectations.

---

## Cross-Cutting Implementability Concerns

### 1. Persistent-Volume Assumption (CRITICAL)

**Properties affected: ALL of Category 2 (5 properties), ALL of Category 3 (5
properties), Category 4 (2 properties), Category 6 (2 properties). Total: 14 of
19 properties.**

The deployment topology requires that the disk buffer `data_dir` survives a
container node-kill fault. If the Antithesis tenant's node-termination recreates
the container with a fresh filesystem, the buffer is wiped on every restart, and:

- Crash-recovery properties (Category 3) pass vacuously (no data to recover).
- Deadlock properties (Category 2) are unreachable (clean state on restart).
- Liveness/reclamation properties (Category 4) are unreachable (no pre-existing
  data to drain).

**This is the single most important implementation prerequisite to confirm.** The
deployment topology document explicitly calls this out as a requirement. Without
a persistent volume, 14 of 19 properties are either vacuous or spuriously failing.

### 2. Node-Termination Faults (CRITICAL)

**Properties affected: All 14 listed above (same set as persistent-volume).**

Node-termination faults (SIGKILL) are often disabled by default in Antithesis
tenants. Without them, the underflow bug (#21683), torn-tail recovery, and all
at-least-once crash-durability properties are unreachable. This must be
confirmed with the Antithesis tenant operator before any Category 2–6 property
is declared implementable.

### 3. Instrumented Build Burden

**Properties requiring SUT-side SDK assertions (instrumented build):**

- `total-buffer-size-never-underflows` (CRITICAL — internal atomic invisible)
- `writer-eventually-makes-progress` (SDK sharpens; workload signal exists)
- `record-id-monotonicity-holds` (CRITICAL — panic path, no workload equivalent)
- `no-corrupted-record-delivered` (CRITICAL — emission point invisible externally)
- `corruption-is-detected-and-recovered` (CRITICAL — branch not observable externally)
- `partial-write-at-rotation-recovers` (SDK confirms recovery path reached)
- `recovery-completes-after-crash` (SDK at `from_config_inner` end)
- `graceful-shutdown-flushes-all` (SDK optional; workload signal exists)

The instrumented build requires:

1. Adding `antithesis-sdk` to `lib/vector-buffers/Cargo.toml` (one-time change).
2. Rebuilding Vector with the SDK dependency.
3. Inserting assertion calls at ~10 locations across `ledger.rs`, `reader.rs`,
   `writer.rs`, and `mod.rs`.

The SDK assertions are no-ops outside Antithesis, so the instrumented build is
safe for normal use. This is a one-time setup cost, not per-property.

**Properties fully workload-observable without instrumented build:**

- `writer-eventually-makes-progress` (throughput signal)
- `buffer-size-within-max` (file-size watchdog)
- `durable-unacked-events-survive-crash` (set-difference check)
- `every-written-event-eventually-delivered` (set-difference check)
- `recovery-completes-after-crash` (time-to-deliverable proxy)
- `reader-drains-and-terminates-cleanly` (time-to-drain)
- `sink-failure-not-silently-acked` (workload controls sink errors)
- `dropped-events-are-counted` (metrics scraping)
- `record-id-wraparound-accounting-holds` (metrics scraping after drain+restart)
- `config-reload-no-silent-loss` (workload controls SIGHUP + end-to-end count)
- `graceful-shutdown-flushes-all` (workload end-to-end count)
- `record-never-spans-files` (file-size watchdog)

### 4. `record-id-wraparound-accounting-holds` Descoping

As analyzed above, the u64 wrap case (requires 2^64 writes) is not practically
reachable with any production binary. The property as stated conflates two
distinct scenarios:

- **Empty-buffer equality** (trivially reachable, workload-observable, is a real
  bug at `ledger.rs:266`).
- **True u64 wraparound** (not reachable without `#[cfg(test)]`-gated test
  helpers or 2^64 actual writes).

The implementable portion is the empty-buffer equality check. This should be
split into its own concrete test case and the u64 wrap portion explicitly
acknowledged as out of scope for production-binary Antithesis testing.

### 5. `file-id-rollover-stays-coordinated` Binary Mode Decision

The property is only implementable with `MAX_FILE_ID=6` (test build mode) because
production binary rollover is unreachable. Decision required from the user:

- Run test binary mode in Antithesis (enables `#[cfg(test)]` constants including
  `MAX_FILE_ID=6`, but also enables other test-only code).
- Add a runtime-configurable `MAX_FILE_ID` override (new Vector code change).
- Descope the property to "latent bug documented but not tested in this run."

### 6. Custom Fault Summary

| Property | Fault type | Mechanism | Tenant capability needed? |
|---|---|---|---|
| `config-reload-no-silent-loss` | SIGHUP to Vector process | Workload sends signal | No (workload OS call) |
| `sink-failure-not-silently-acked` | HTTP 5xx responses | Workload controls stub | No (workload HTTP) |
| `file-id-rollover-stays-coordinated` | `MAX_FILE_ID=6` + node kill | Test binary + node kill | Yes (node kill) |
| All Category 2–3 | SIGKILL | Node termination | Yes (must confirm enabled) |

---

## Summary Table

| Property | Workload-Observable? | Instrumented Build Required? | Node-Kill Required? | Persistent Volume Required? | Verdict |
|---|---|---|---|---|---|
| `no-corrupted-record-delivered` | No | YES (primary) | No (faults inject corruption) | No | Implementable — instrumented build |
| `corruption-is-detected-and-recovered` | No | YES | No | No | Implementable — instrumented build |
| `record-id-monotonicity-holds` | No | YES | Yes | Yes | Implementable — instrumented build + node kill |
| `record-never-spans-files` | Yes (watchdog) | Optional | No | No | Implementable — no instrumented build |
| `total-buffer-size-never-underflows` | No | YES (critical) | Yes | Yes | Implementable — instrumented build + node kill |
| `writer-eventually-makes-progress` | Yes (throughput) | Optional (sharpens) | Yes | Yes | Implementable — node kill; instrumented improves |
| `buffer-size-within-max` | Yes (watchdog) | No | Yes | Yes | Implementable — must pair with liveness check |
| `durable-unacked-events-survive-crash` | Yes | No | Yes | Yes | Implementable — use flush_interval=0 |
| `every-written-event-eventually-delivered` | Yes | No | Yes | Yes | Implementable |
| `recovery-completes-after-crash` | Yes (proxy) | Optional | Yes | Yes | Implementable |
| `partial-write-at-rotation-recovers` | Partial | YES (Sometimes) | Yes | Yes | Implementable — instrumented build + small file size |
| `acked-files-eventually-deleted` | Yes (watchdog) | No | Yes | Yes | Implementable |
| `reader-drains-and-terminates-cleanly` | Yes | No | Yes | Yes | Implementable |
| `sink-failure-not-silently-acked` | Yes | No | No | No | Implementable — workload controls sink |
| `dropped-events-are-counted` | Yes (metrics) | No | No | No | Implementable — metrics scraping |
| `file-id-rollover-stays-coordinated` | Yes | No | Yes | Yes | BLOCKED — needs test binary or MAX_FILE_ID knob |
| `record-id-wraparound-accounting-holds` | Yes (empty case) | No | No | No | Implementable (empty case only); u64 wrap descope |
| `config-reload-no-silent-loss` | Yes | No | No (SIGHUP) | No | Implementable — workload sends SIGHUP |
| `graceful-shutdown-flushes-all` | Yes | Optional | No | No | Implementable |

---

## Findings (Concerns)

### F1 — Persistent-volume assumption unconfirmed (BLOCKER for 14 properties)

- **Scope:** Categories 2, 3, 4, 6 (14 of 19 properties)
- **Concern:** If the Antithesis tenant recreates the container filesystem on
  node-kill, crash-recovery properties pass vacuously and deadlock properties are
  unreachable.
- **Evidence:** Deployment topology document §CRITICAL note; buffer `data_dir`
  must survive kill/restart for any crash-recovery property to be meaningful.
- **Action:** Confirm with Antithesis tenant operator that the buffer `data_dir`
  (on a mounted persistent volume) is not wiped on node-termination fault before
  beginning any Category 2–6 testing.

### F2 — Node-termination faults may be disabled (BLOCKER for same 14 properties)

- **Scope:** Categories 2, 3, 4, 6
- **Concern:** Node-kill faults are often disabled by default in Antithesis tenants.
  Without SIGKILL, the underflow trigger, torn-tail recovery, and all at-least-once
  crash-durability properties are unreachable.
- **Evidence:** Property catalog file-level open questions; sut-analysis.md §Assumptions.
- **Action:** Confirm node-termination faults are enabled in the target tenant.

### F3 — Zero Antithesis SDK instrumentation (BLOCKER for 5 properties, burden for 3 more)

- **Scope:** `total-buffer-size-never-underflows`, `record-id-monotonicity-holds`,
  `no-corrupted-record-delivered`, `corruption-is-detected-and-recovered`,
  `partial-write-at-rotation-recovers`; optional for `writer-eventually-makes-progress`,
  `recovery-completes-after-crash`, `graceful-shutdown-flushes-all`
- **Concern:** Every SUT-side assertion must be added from scratch. The 5
  critical properties have internal state that is entirely invisible from the
  workload without SDK instrumentation.
- **Evidence:** `existing-assertions.md` confirms zero SDK usage; repo-wide grep
  returns no matches.
- **Action:** Add `antithesis-sdk` to `lib/vector-buffers/Cargo.toml` and insert
  assertions at the ~10 identified sites. This is a one-time build setup shared
  across all affected properties.

### F4 — `file-id-rollover-stays-coordinated` unreachable with production binary (BLOCKER)

- **Scope:** `file-id-rollover-stays-coordinated`
- **Concern:** `MAX_FILE_ID=65535` in production binary requires ~8TB of data
  throughput to trigger rollover. Not achievable in any practical Antithesis run.
  `MAX_FILE_ID=6` is only compiled in `#[cfg(test)]` builds.
- **Evidence:** `common.rs:43-45` confirms the conditional constant; test helpers
  for synthetic state injection are also `#[cfg(test)]`-gated.
- **Action:** Choose one: (a) run Vector in test binary mode (enabling `MAX_FILE_ID=6`),
  (b) add a runtime-configurable override for `MAX_FILE_ID`, or (c) descope to
  "latent bug documented but not exercised in this Antithesis run."

### F5 — `record-id-wraparound-accounting-holds` as stated is half-vacuous

- **Scope:** `record-id-wraparound-accounting-holds`
- **Concern:** The u64 wrap case (the property's primary stated focus) is
  unreachable with any production binary. The testable bug is the empty-buffer
  equality case (`wrapping_sub(0,0) - 1 = u64::MAX`), which triggers on every
  clean restart with a drained buffer.
- **Evidence:** `ledger.rs:266` confirmed; `#[cfg(test)]` gate on
  `unsafe_set_writer_next_record_id` / `unsafe_set_reader_last_record_id` at
  `ledger.rs:173-196`; no path to u64::MAX record ID via real writes.
- **Action:** Refocus the property on the empty-buffer equality case (rename and
  re-scope). Implement as: drain buffer completely → restart → scrape
  `buffer_byte_size` or `buffer_events_received_total` → assert near 0. This
  is workload-observable without any SDK instrumentation. Explicitly descope the
  u64 wrap case.

### F6 — `total-buffer-size-never-underflows` debug-build conflict

- **Scope:** `total-buffer-size-never-underflows`
- **Concern:** The `trace!` macro at `ledger.rs:295` includes `last_total_buffer_size - amount`
  as a Rust expression, which panics in debug mode on overflow before the
  `fetch_sub` wrapping behavior is observable.
- **Evidence:** `ledger.rs:291-298` source; Rust debug arithmetic overflow semantics.
- **Action:** Use release build for Antithesis testing of this property. Document
  this as a harness build requirement. Separately, fix the `trace!` to use
  `wrapping_sub` to avoid the debug-mode panic.

### F7 — `buffer-size-within-max` vacuous under deadlock (design concern)

- **Scope:** `buffer-size-within-max`
- **Concern:** The safety property passes trivially when the writer is deadlocked
  (no writes → no overflow). A passing result for this property alone is not
  evidence of correct behavior.
- **Evidence:** `buffer-size-within-max.md` deadlock-vacuity section; `sut-analysis.md`
  §5 INV-7.
- **Action:** Always assert this property jointly with `writer-eventually-makes-progress`.
  The combined result (size holds AND liveness holds) is the meaningful signal.

### F8 — `graceful-shutdown-flushes-all` stop() drop-order unresolved

- **Scope:** `graceful-shutdown-flushes-all`
- **Concern:** Whether `RunningTopology::stop()` drops `inputs` (and thus the
  `BufferWriter`) before the write-loop task completes its final `flush()` is
  unresolved. If the drop precedes the final flush, up to 256KB of staged events
  are silently lost even on graceful shutdown.
- **Evidence:** `graceful-shutdown-flushes-all.md` §stop() analysis; `running.rs`
  code path trace.
- **Action:** Add a `debug!` log in `TrackingBufWriter::drop` to show `buf.len()`
  at drop time. Run a single graceful-shutdown test to confirm whether `buf.len()`
  is 0 at drop. This resolves the uncertainty without requiring the full Antithesis
  harness.

---

## Passes (No Implementability Concerns)

- **`sink-failure-not-silently-acked`**: workload-driven sink errors, bug confirmed,
  no special tenant capabilities needed.
- **`dropped-events-are-counted`**: metrics scraping with `when_full: drop_newest`
  config, bug confirmed, straightforward assertion.
- **`config-reload-no-silent-loss`**: workload sends SIGHUP (standard OS call),
  bug plausible, end-to-end count is the assertion.
- **`record-never-spans-files`**: file-size watchdog covers it without
  instrumented build.
- **`durable-unacked-events-survive-crash`**: use `flush_interval=0` to eliminate
  the durability-window ambiguity; clean workload-level set-difference check.
- **`every-written-event-eventually-delivered`**: same structure as above; use a
  reliable downstream stub to isolate crash-durability from sink-error bugs.

---

## Uncertainties

1. **Does `receiver.rs` swallow or panic on `ReaderError::Checksum`/`Deserialization`?**
   Affects whether `no-corrupted-record-delivered` needs a crash-detection signal
   in addition to the SUT-side assertion. (SUT analysis says panic; confirm.)

2. **Is `drop_newest` actually reachable for disk buffers?** The `try_write_record`
   path exists in `writer.rs:1166-1178` and `sender.rs:231-234` calls it for
   `WhenFull::DropNewest`. Needs an end-to-end path trace to confirm the disk
   buffer variant is reached (not just the in-memory `LimitedSender`).

3. **Does the `unacked_reader_file_id_offset` context make the `reader.rs:932`
   `u16 >` comparison more correct than the raw comparison suggests?** The
   evidence file acknowledges this open question. A deeper code trace of
   `get_current_reader_file_id` (`ledger.rs:305-308`) is needed before asserting
   the bug is definitively triggerable.

4. **Does any sink actually emit `Errored`/`Rejected` `BatchStatus` in normal
   operation, or only the `_status`-discard path?** The
   `sink-failure-not-silently-acked` end-to-end test works regardless (make the
   downstream HTTP permanently return 5xx and observe non-replay after quiet
   period), but understanding whether the `BatchStatus` plumbing actually carries
   `Errored` affects the SUT-side assertion design.

5. **Finalizer task drain on shutdown:** Does the tokio runtime drain in-flight
   `BatchNotifier` finalizer tasks before process exit on SIGKILL? If not, acked-
   in-flight events are lost without ledger update, creating a loss window that
   the `every-written-event-eventually-delivered` property would catch but that
   may be confused with the in-contract 500ms fsync window loss.
