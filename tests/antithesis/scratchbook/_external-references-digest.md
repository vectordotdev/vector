# External References Digest (working note for discovery agents)

Scaffolding for the antithesis-research run on **disk buffers v2**
(`lib/vector-buffers/src/variants/disk_v2/`). User scope: *"Whatever you have
access to. You have your MCPs."* — in-repo docs/RFCs plus Datadog internal doc/Jira.
Findings condensed below so per-focus agents need not re-fetch.

## In-repo references

- `rfcs/2021-10-14-9477-buffer-improvements.md` — original buffer-rework RFC.
- `docs/specs/buffer.md` — buffer component spec / claimed behavior.
- `lib/vector-buffers/src/variants/disk_v2/mod.rs` — authoritative design doc
  (module-level comment): on-disk format, ledger, record IDs, recovery.

Claimed guarantees (CRC32C, whole-file deletion, monotonic event-count IDs, 500ms
fsync window, at-least-once with duplicates on replay, size constants) live in
`sut-analysis.md` and the per-property files — not duplicated here.

## Known bugs / incidents (HIGH-VALUE Antithesis targets)

1. **Ledger `total_buffer_size` AtomicU64 underflow → permanent writer deadlock**
   (Vector #21683, mitigated by PR #23561 on the *reporter* side only; the ledger
   atomic still wraps).
   - `decrement_total_buffer_size` does raw `fetch_sub(amount, AcqRel)`, **no
     saturation**. If `amount > current_value` the atomic wraps to ≈ 2^64. Under the
     `antithesis` feature this site carries a committed
     `assert_always_greater_than_or_equal_to!(total_buffer_size, amount)` detector —
     reports the wrap, does not prevent the subtraction.
   - Then `total_buffer_size + unflushed_bytes` is always astronomical →
     `is_buffer_full()` true forever → `can_write_record()` false forever → writer's
     `ensure_ready_for_write()` loops on `ledger.wait_for_reader().await` forever.
     **Writer deadlocks permanently.**
   - Trigger: crash/reboot/abrupt-shutdown leaving a data file whose on-disk size and
     readable-record bytes disagree, plus the reader running through it on restart.
     Partial writes at file-rotation boundaries are likeliest. Not deterministic
     per-restart, but not exotic.
   - Reporter gauges use `saturating_sub` (PR #23561) so the *dashboard* no longer
     shows 2^64, but the ledger control-path atomic is unfixed.

2. **Disk buffer stall + silent event drops during config reload** (Vector #24948,
   PR #24949; implicated in the **internal non-prod config-reload incident**).
   - Old writer dropped while events still in-flight → events lost without
     accounting.
   - `track_dropped_events` passes `0` for `byte_size` → permanent drift in
     buffer-size metrics.
   - `synchronize_buffer_usage()` re-seeds metrics while the old reporter may still
     run → double-counted spikes, then a metrics gap between old-reporter teardown
     and the new reporter's first tick (2s).

3. **`component_discarded_events_total` blind to buffer drops** (Vector #24606,
   #24144). When a disk buffer fills and `drop_newest` fires, only
   `buffer_discarded_events_total` increments; the component-level discarded counter
   stays 0 → silent data loss on dashboards. `BufferEventsDropped::emit()` in
   `lib/vector-buffers/src/internal_events.rs` never calls `ComponentEventsDropped`.

4. **Buffer size gauges stuck non-zero / negative** (Vector #23995, #17666, #21683).
   Reporter `current() = total_entered.saturating_sub(total_left)`; stuck-at-non-zero
   still open.

5. **Component tags lost for sinks using disk buffers** (OPA-5380): components paused
   for IO at init time lose `component_*` labels on later-registered metrics
   (utilization, etc.).

## Existing test strategy (so we don't duplicate it)

- In-repo: extensive `proptest` + **model-based testing** under
  `variants/disk_v2/tests/model/` (reference model + action sequencer + in-memory
  filesystem). Unit tests for acknowledgements, initialization, known_errors,
  size_limits, invariants, record.
- Datadog internal: an E2E **chaos test** SIGKILLing the worker 3× with e2e acks,
  asserting every event delivered end-to-end. Antithesis goes beyond, exploring
  fault *timing/interleavings* (partial writes at rotation, fsync-vs-crash windows,
  reader/writer races on the mmap'd ledger) a fixed 3×SIGKILL test cannot.
- A **major lock-contention performance issue** affected all disk-buffer users
  (writer throughput ~90 MiB/s capped by contention) — points at writer/reader
  coordination hot paths.

(Fault-lever analysis — single-process buffer, node-kill required for
crash-recovery — lives in `sut-analysis.md` and `deployment-topology.md`.)
