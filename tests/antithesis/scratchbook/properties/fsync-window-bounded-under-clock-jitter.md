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

### `force_full_flush` is clock-independent

File rotation calls `flush_inner(force_full_flush=true)` directly, bypassing
`should_flush()` — the ONLY clock-independent durability checkpoint. With
insufficient write volume to rotate, the only durability interval is the
`should_flush()` timer, suppressed under clock jitter. `DEFAULT_FLUSH_INTERVAL`
is 500ms.

### Mitigation: `flush_interval=0` removes clock dependence

With `flush_interval = ZERO`, `last_flush.elapsed() > ZERO` is true after any
elapsed time, making every `flush_inner()` a `sync_all` — removing the clock
attack surface (recommended oracle setup for `durable-unacked-events-survive-crash`),
though not the production default.

### CAS winner descheduling extension

`should_flush()` uses `AtomicCell::compare_exchange`: exactly one concurrent
caller wins the CAS and owns the `sync_all()`. If that caller is descheduled
between the CAS win and the `sync_all()`, all others see `should_flush()=false`
for the duration — a second, clock-independent suppression window Antithesis can
extend via CPU throttle. (A second distinct sub-scenario; combined here.)

---

## Fault Conditions

| Fault | Effect |
|---|---|
| Clock jitter (slow virtual clock) | `Instant::elapsed()` advances slowly; `should_flush()` rarely true; `sync_all` suppressed. |
| CPU throttle + slow clock | Writer descheduled after CAS win; `sync_all` delayed past the window. |
| Crash during suppressed window | Loss extends to all data since the last file rotation, not the last 500ms. |
| Low write volume (no rotation) | No `force_full_flush`; clock jitter suppresses sync indefinitely (unbounded loss). |

---

## SUT-Side Instrumentation and coverage

The committed underflow detectors do not cover the fsync window (see `_shelved.md`
header). Uncommitted candidates: an `assert_always!(elapsed_since_sync <= bound)`
at the end of `flush_inner` after `sync_all` (fires only when `sync_all` ran, so
it cannot see total suppression), plus a workload-side check that keeps a shadow
`last_sync` from an Antithesis clock source (not Rust `Instant`) and asserts a
bounded gap, fed by an emitted event/trace at the `sync_all` callsite — needed
because total suppression is invisible from inside the SUT.

Cross-link: `durable-unacked-events-survive-crash` assumes a bounded loss window
that clock jitter extends; `flush_interval=0` (every flush = fsync) removes this
attack surface and is the recommended oracle setup there.
`writer-eventually-makes-progress` is independent (its deadlock is arithmetic,
not clock-driven).

---

## Open Questions

+ **Confirm Antithesis virtual-time reaches `Instant::elapsed()`** on the target
  runtime (expected yes) AND the `AtomicCell<Instant>` CAS path (likely, same
  underlying `Instant`).
+ **Unbounded-before-first-rotation:** at default 128 MiB files and low write
  volume, rotation never fires, so indefinite clock jitter makes the loss window
  unbounded — directly contradicts the GA "500ms" claim. The shrunk 2 MiB file
  knob (experiment-spec) forces frequent rotation, capping the de-facto window.
+ **No fsync watchdog exists** — the only signal is the `buffer_byte_size` gauge
  (masked by the 23561 fix) and TRACE-level logging.
+ **Clock-fault availability** — this property is meaningless without clock
  jitter enabled in the tenant; confirm (grind-plan G8 marks it LATER, clock
  fault).
