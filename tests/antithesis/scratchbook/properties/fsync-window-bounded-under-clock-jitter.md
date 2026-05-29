---
slug: fsync-window-bounded-under-clock-jitter
type: Safety / Always
sut_path: lib/vector-buffers/src/variants/disk_v2/
commit: 049eec79b737450c4669b7f8aa1dd814551ec466
updated: 2026-06-02
---

# Property: fsync-window-bounded-under-clock-jitter

## Catalog Entry

**Type:** Safety / Always

**Property:** Under Antithesis clock-jitter faults, the durable-loss window for
synced data stays bounded: every write that the writer accepted is either
durable (fsync'd + ledger msync'd) within a bounded multiple of
`flush_interval`, OR it is durable because a data-file rotation forced a
`force_full_flush`. No silent indefinite suppression of `sync_all` occurs.

**Invariant:** `Always`: the elapsed time since the last successful `sync_all`

+ `ledger.flush()` pair, measured in real wall time, never exceeds a
configurable bound (e.g. `K × flush_interval`, or since the last file
rotation, whichever is shorter). Violations mean the durability SLA
("≤500ms loss window") is silently extended, with no observable signal to the
operator.

**Antithesis Angle:** Enable Antithesis clock-jitter faults (virtual-time
stretch/compress). A slowed clock prevents `last_flush.elapsed()` from
exceeding `flush_interval` at the normal wall-time cadence, suppressing
`should_flush()` → `sync_all()` indefinitely (only file rotation, which calls
`flush_inner(force_full_flush=true)`, is clock-independent). Crash during the
suppressed window; verify that the data loss is bounded by the last rotation
boundary or a bounded multiple of `flush_interval`, not unbounded.

A second sub-scenario: the CAS winner of `should_flush()` is descheduled
(Antithesis can extend the descheduling window) between winning the CAS and
calling `sync_all()`. Other callers all see `should_flush()=false` (CAS
already consumed). Crash during that window; loss extends silently beyond
`flush_interval`.

**Why It Matters:** The product's stated guarantee is "data synchronized to
disk will not be lost if Vector crashes; data synchronized every 500ms." A
clock-jitter fault, which is a standard Antithesis capability, can suppress
the entire `sync_all` path indefinitely (only page-cache flushes happen),
silently extending the loss window with no error, no log at ERROR level, and
no watchdog. The only mitigant is file rotation (which is event-count driven,
not clock-driven), making the rotation frequency the de facto maximum loss
window under clock jitter.

---

## Code Verification

### `should_flush` gate (ledger.rs:512-524)

```rust
// lib/vector-buffers/src/variants/disk_v2/ledger.rs:512-524
pub fn should_flush(&self) -> bool {
    let last_flush = self.last_flush.load();
    if last_flush.elapsed() > self.config.flush_interval
        && self
            .last_flush
            .compare_exchange(last_flush, Instant::now())
            .is_ok()
    {
        return true;
    }
    false
}
```

`last_flush.elapsed()` calls `Instant::elapsed`, which is monotonic-clock
relative. Under Antithesis virtual-time compression (clock slowed), this value
advances slower than wall time, suppressing the `> flush_interval` condition.

### `flush_inner` — where sync_all is actually called (writer.rs:1299-1321)

```rust
// lib/vector-buffers/src/variants/disk_v2/writer.rs:1299-1321
async fn flush_inner(&mut self, force_full_flush: bool) -> io::Result<()> {
    if let Some(writer) = self.writer.as_mut() {
        writer.flush().await?;          // page-cache flush: always happens
        self.ledger.notify_writer_waiters();
    }
    if self.ledger.should_flush() || force_full_flush {
        if let Some(writer) = self.writer.as_mut() {
            writer.sync_all().await?;   // fsync: only when should_flush() or rotation
        }
        self.ledger.flush()             // ledger msync
    } else {
        Ok(())
    }
}
```

The page-cache path (`writer.flush()`) always runs. The durable path
(`sync_all` + `ledger.flush()`) runs ONLY when `should_flush()=true` or
`force_full_flush=true`. Under clock jitter, `sync_all` can be suppressed
indefinitely.

### `force_full_flush` is clock-independent (writer.rs:1120-1130)

File rotation calls `flush_inner(force_full_flush=true)` directly, bypassing
`should_flush()`:

```rust
// writer.rs:~1124 (inside rotate_data_file)
data_file.sync_all().await?;
```

This is the only clock-independent durability checkpoint. If the workload
generates insufficient write volume to trigger rotation, the only durability
interval is the `should_flush()` timer — which is suppressed under clock jitter.

### `DEFAULT_FLUSH_INTERVAL` (common.rs:31)

```rust
// lib/vector-buffers/src/variants/disk_v2/common.rs:31
pub const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_millis(500);
```

### Mitigation: `flush_interval=0` removes clock dependence

When `flush_interval = Duration::ZERO`, the condition
`last_flush.elapsed() > Duration::ZERO` evaluates to `true` after any
measurable time elapses, effectively making every `flush_inner()` call a
`sync_all`. Setting `flush_interval=0` in the harness configuration removes
the clock-jitter attack surface for other durability properties (e.g.,
`durable-unacked-events-survive-crash`) but is not the production default.

### CAS winner descheduling extension

`should_flush()` uses `AtomicCell::compare_exchange`: exactly one concurrent
caller wins the CAS and becomes responsible for calling `sync_all()`. If that
caller is descheduled between the CAS win (`is_ok()`) and the actual
`sync_all()` call, all other callers see `should_flush()=false` for the
duration. Antithesis can extend this descheduling window arbitrarily.

---

## Fault Conditions

| Fault | Effect |
|---|---|
| Clock jitter (slow virtual clock) | `Instant::elapsed()` advances slowly; `should_flush()` rarely/never true; `sync_all` suppressed. |
| CPU throttle + slow clock | Writer thread descheduled after CAS win; sync_all delayed past 500ms window. |
| Crash during suppressed window | Loss extends to all data since last file rotation, not just last 500ms. |
| Low write volume (no rotation) | No `force_full_flush` path; clock jitter can suppress sync indefinitely. |

---

## SUT-Side Instrumentation (not yet committed — the SDK is wired and the three #21683 underflow asserts are present; these are additional)

The Antithesis SDK is a committed dependency under the `antithesis` feature, and
three underflow `assert_always_greater_than_or_equal_to!` detectors exist
(ledger.rs:271/313, reader.rs:529; see existing-assertions.md for what is
committed). None of those covers the fsync window, so the assertions below
remain genuine still-to-add suggestions.

### Assertion 1 — Always: elapsed since last sync stays bounded

Placed at the end of `flush_inner`, after the `sync_all` branch:

```rust
// writer.rs, inside flush_inner, after sync_all completes
let elapsed_since_sync = self.ledger.last_flush.load().elapsed();
// This assertion fires on every flush_inner call that DID do sync_all.
// The bound is set generously to detect extended suppression.
antithesis_sdk::assert_always!(
    elapsed_since_sync <= MAX_ACCEPTABLE_SYNC_GAP,
    "fsync window bounded: elapsed since last sync must be within configurable bound",
    &serde_json::json!({
        "elapsed_ms": elapsed_since_sync.as_millis(),
        "flush_interval_ms": self.config.flush_interval.as_millis(),
        "bound_ms": MAX_ACCEPTABLE_SYNC_GAP.as_millis(),
    })
);
```

Note: this assertion only fires when `sync_all` executes. A complementary
workload-side check is needed to detect complete suppression (when
`flush_inner` runs but `sync_all` is never called over a long window).

### Assertion 2 — Workload-side: monotone sync timestamp

The workload maintains a shadow `last_sync_wall_time` (from an
Antithesis-provided clock source, not Rust's `Instant`) and asserts that
`now - last_sync_wall_time <= K * flush_interval` periodically, even under
clock jitter. This requires a workload-observable hook (tracing event or
metric emitted when `sync_all` is called).

Candidate: emit a `tracing::info!` event at the `sync_all` callsite:

```rust
// writer.rs, after sync_all succeeds
info!(timestamp = ?std::time::SystemTime::now(), "sync_all completed");
```

The workload monitors this trace event and asserts bounded gaps.

---

## Relationship to Other Properties

+ `durable-unacked-events-survive-crash`: that property assumes a bounded
  loss window. Clock jitter extends it, potentially causing events that would
  survive under normal timing to be lost. Setting `flush_interval=0` in the
  harness is the recommended oracle setup for that property.
+ `writer-eventually-makes-progress`: independent — the deadlock path is
  arithmetic, not clock-driven.

---

## Open Questions

+ Does Antithesis virtual-time affect `Instant::now()` / `Instant::elapsed()`
  in Rust's standard library on the target runtime? The answer is yes for
  Antithesis's standard virtual-time instrumentation, but confirm that
  `crossbeam_utils::atomic::AtomicCell<Instant>` operations in the CAS path
  also see virtual time (they likely do since they use the underlying
  `Instant` type).

+ What is the maximum loss window under clock jitter before the FIRST file
  rotation if write volume is low (e.g., 1 event/second, `max_data_file_size`
  = 128MB)? At default sizes, rotation never fires; loss window is unbounded
  under indefinite clock jitter with no incoming writes to trigger rotation.
  Worth confirming against the GA doc's "500ms" claim.

+ Is there a watchdog / health check in Vector that would alert the operator
  to a suppressed fsync? Currently: no, the only observability is the
  `buffer_byte_size` gauge (masked by the #23561 fix) and tracing at TRACE
  level.

+ Antithesis clock-fault availability: confirm whether clock jitter is
  enabled in the target tenant or must be explicitly requested, since this
  property is meaningless without it.

+ The CAS-winner descheduling scenario is a second, distinct sub-property.
  Should it be split into its own slug, or is the combined framing sufficient
  for one evidence file?
