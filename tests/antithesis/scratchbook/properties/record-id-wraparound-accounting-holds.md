---
slug: record-id-wraparound-accounting-holds
type: Safety / Always
status: LATENT BUG — the `- 1` at `ledger.rs:266` is not wrapping; equality case produces u64::MAX
sut_commit: b7aae737cef5dd37d1445915443a1eb97b584f85
---

# Property 17: record-id-wraparound-accounting-holds

## Catalog Entry

**Type:** Safety / Always

**Property:** Across u64 record-ID wraparound and at the empty-buffer equality case,
`get_total_records` returns a semantically correct count (0 when empty, N when N events are
unacknowledged). It never returns an astronomically wrong value (~2^64) that would corrupt
metrics, trigger false "buffer full" accounting, or cause reporting loops to emit junk gauges.

**Invariant:** `get_total_records() <= actual_unacked_event_count + some_small_bounded_delta`.
Specifically, `get_total_records()` must return 0 when the buffer is empty (all events acked,
`next_writer_record_id == last_reader_record_id`), and must never return a value close to
`u64::MAX`.

**The Bug — `ledger.rs:262-267`:**

```rust
pub fn get_total_records(&self) -> u64 {
    let next_writer_id = self.state().get_next_writer_record_id();
    let last_reader_id = self.state().get_last_reader_record_id();

    next_writer_id.wrapping_sub(last_reader_id) - 1  // <-- outer `-1` is NOT wrapping
}
```

The function computes `(next_writer_id wrapping_sub last_reader_id) - 1`. The `wrapping_sub`
is correct for the modular distance between the writer ID and the reader ID — it handles u64
wraparound. But the outer `- 1` is plain Rust integer subtraction, which panics in debug mode
on underflow and wraps to `u64::MAX` in release mode when the intermediate result is 0.

**When does the intermediate result equal 0?**

`wrapping_sub` returns 0 when `next_writer_id == last_reader_id`. This happens at initialization:
both start at 0 (or at the same persisted value on restart when no unacked events remain). The
empty-buffer state is exactly the case where these two IDs are equal.

The doc-comment for `get_last_reader_record_id` clarifies: the reader ID is the ID of the last
record acknowledged. The writer ID is the ID of the next record to be written. When the buffer
is empty and fully caught up, the writer's "next" ID and the reader's "last acked" ID are
equal (both pointing at the same boundary). This makes `wrapping_sub(...) = 0`, and then
`0 - 1` in u64 = `u64::MAX = 18446744073709551615`.

**Downstream impact:**

`get_total_records` is called at two sites:

1. **`synchronize_buffer_usage` (`ledger.rs:517`):** Called during initialization after
   `seek_to_last_record` and `validate_last_write`. If the buffer is empty on startup (all
   events acked before previous shutdown), `get_total_records()` returns `u64::MAX`. This is
   passed to `increment_received_event_count_and_byte_size(u64::MAX, ...)`, which adds `u64::MAX`
   to the received-events atomic counter. The buffer usage reporter then emits a
   `BufferEventsReceived` event with `count = u64::MAX`, setting the buffer-size gauge to
   `u64::MAX as f64 = 1.844e19`. The gauge is permanently stuck at an astronomical value for
   the lifetime of the process (or until enough sent/dropped events saturate the subtraction
   back down, which at normal throughput would take millions of years). This is visible as a
   stuck/wrong buffer-size metric on dashboards — the same symptom class as issue #23995.

2. **`tests/` (model invariant checks):** `get_total_records` is used in model tests
   (`tests/model/mod.rs:1005`, `tests/invariants.rs:115, 708-719`) to assert that the event
   count matches expectations. If the model exercises an empty-buffer state and calls
   `get_total_records`, the test would panic (debug build) or return `u64::MAX` (release
   build, causing the assert to fail). The test suite likely avoids the exact initial state
   where both IDs are equal by construction, but this is fragile.

**The record-ID-wraparound case:**

The outer `- 1` being non-wrapping also affects the record-ID near-wrap case. If
`next_writer_id = 0` (just wrapped from `u64::MAX`) and `last_reader_id = u64::MAX` (not yet
caught up from before the wrap), then `wrapping_sub(0, u64::MAX) = 1`, and `1 - 1 = 0` —
which is actually correct (the buffer has 1 unit of unacknowledged distance). But if
`next_writer_id = u64::MAX` (about to wrap) and `last_reader_id = u64::MAX` (caught up),
both equal → wraparound_sub = 0 → `0 - 1 = u64::MAX`. Same bug, different trigger point.

The fix is: `next_writer_id.wrapping_sub(last_reader_id).wrapping_sub(1)`.

**Antithesis Angle:**

1. **The equality/empty case (most reachable):** Start Vector with a disk buffer. Write N
   events. Read and acknowledge all N events (drain the buffer completely). Assert immediately
   after drain that `get_total_records() == 0`. In an Antithesis run, a workload can drain the
   buffer and then query the metric. The metric value `18446744073709551615` (or any value >
   N+small_delta) should trigger a workload-side assertion failure.

2. **The near-wraparound case (requires injection):** Use the test-only
   `unsafe_set_writer_next_record_id` and `unsafe_set_reader_last_record_id` helpers
   (`ledger.rs:174-196`) to place both IDs near `u64::MAX`, then drain the buffer to equality.
   Assert `get_total_records() == 0`. This is available only in test builds.

3. **The `synchronize_buffer_usage` path:** Start Vector with an empty, previously-used buffer
   (all events acked on last run). After startup, scrape `buffer_size_events` (or equivalent
   gauge). Assert the value is close to 0, not `u64::MAX`. This requires only a workload +
   metric scrape and is exercisable without any special test hooks.

SUT-side assertion: add `assert_always!(result <= reasonable_upper_bound, "get_total_records returned impossible value", { "result": result, "writer_id": next_writer_id, "reader_id": last_reader_id })` inside `get_total_records`, where `reasonable_upper_bound` is something like `self.get_total_buffer_size() / min_record_size` or simply `u64::MAX / 2`.

**Why It Matters:**

The empty-buffer case is the most common and most benign path through the system — a healthy
drain-and-restart cycle. Yet this is precisely the path that triggers `u64::MAX` in
`get_total_records`, immediately poisoning the buffer's usage metrics on every clean restart
with an empty buffer. The resulting metric spike (buffer size gauge = 1.844e19) is visible on
dashboards and may trigger false alerts. It also adds `u64::MAX` to the received-events counter
atomically in `synchronize_buffer_usage`, which can interfere with other metric accounting
(e.g. the `current()` computation in `ReporterCurrentMetrics` uses `saturating_sub`, so the
gauge would clamp rather than go negative, but would still be stuck at the saturated value).

This is a distinct bug from the `total_buffer_size` underflow (INV-7), but shares the same
symptom class: lying buffer metrics with no error log and no indication of the root cause. The
Antithesis property provides an automatic regression test for the fix.

## Open Questions

1. **Does the current test suite actually exercise the empty-buffer equality case?** The model
   tests call `get_total_records` (`tests/model/mod.rs:1005`) but the model initializes with
   specific non-empty state. The unit test at `tests/invariants.rs:115` asserts
   `get_total_records() == 1` after writing one record, not 0 after draining. If no test
   exercises the zero case, the bug is undetected by the test suite. Verifying this would
   require scanning the test suite for sequences that drain to empty and then call
   `get_total_records`.

2. **Is the initial state of `last_reader_record_id` actually equal to `writer_next_record_id`
   on a fresh buffer start?** Looking at `BufferReader::new` (`reader.rs:423-424`):
   `ledger_last_reader_record_id = ledger.state().get_last_reader_record_id()` and
   `next_expected_record_id = ledger_last_reader_record_id.wrapping_add(1)`. On a fresh start
   both IDs start at 0 (the ledger's default value). `wrapping_sub(0, 0) = 0`, then `0 - 1 =
   u64::MAX`. This confirms the bug triggers on every fresh-start with an empty buffer.

3. **Does the bug only affect metrics, or does the corrupted `get_total_records` value flow
   into any control-path decision?** Currently `get_total_records` is used only in
   `synchronize_buffer_usage` (metrics) and tests. It is not used in `is_buffer_full()`,
   `can_write_record()`, or any reader/writer gating logic. So the bug corrupts metrics but
   does not directly cause a pipeline stall or data loss. However, if a future change uses
   `get_total_records` in a control path (e.g. to decide when the buffer is empty for clean
   shutdown), the impact would become severe. Flag this as a correctness issue regardless.

4. **Would a debug build of Vector panic on the `0 - 1` subtraction?** In Rust, integer
   overflow in debug mode panics (`overflow` behavior). `0u64 - 1` would panic with "attempt
   to subtract with overflow." If Antithesis runs a debug build, the test would surface this
   bug as a panic rather than a silent `u64::MAX` value, which is actually a stronger signal.
   Production builds use `--release` (no overflow checks), so they would wrap silently. The
   Antithesis harness should clarify which build profile is used.

---

### Investigation Log

#### Does the debug-build `synchronize_buffer_usage`/`get_total_records` `0-1` panic occur before release semantics are observable?

**Examined:** `ledger.rs:262–267` (`get_total_records`), `ledger.rs:516–524` (`synchronize_buffer_usage`), `ledger.rs:173–202` (`unsafe_set_writer_next_record_id`, `unsafe_set_reader_last_record_id`).

**Found:** The arithmetic at ledger.rs:266 is `next_writer_id.wrapping_sub(last_reader_id) - 1`. The `wrapping_sub` is wrapping-safe; the outer `- 1` is plain Rust integer subtraction on `u64`. In a debug build this will panic with "attempt to subtract with overflow" when `wrapping_sub` returns 0 (empty-buffer equality case). `synchronize_buffer_usage` at ledger.rs:517 calls `get_total_records()` unconditionally during initialization; if the buffer is empty, the panic fires before any metric is emitted. In a release build the same operation silently wraps to `u64::MAX` and proceeds to `increment_received_event_count_and_byte_size(u64::MAX, ...)` at ledger.rs:520–523. The debug-vs-release divergence is therefore: **debug → panic at startup on empty buffer; release → silent u64::MAX metric poisoning**. This affects harness build-mode selection: a debug build surfaces the bug as a crash (easier to detect), while a release build surfaces it as a metric anomaly (harder to detect without workload-side assertions).

**Found — cfg(test) gating of the u64-wrap helpers:** `unsafe_set_writer_next_record_id` (ledger.rs:173–187) and `unsafe_set_reader_last_record_id` (ledger.rs:189–202) are both annotated `#[cfg(test)]`. They are unavailable in production or Antithesis-production binaries; the near-wraparound test path (injecting IDs near `u64::MAX`) is only exercisable in test builds.

**Not found:** No evidence of a build-profile guard inside `get_total_records` or `synchronize_buffer_usage` that would suppress the panic in a specific configuration.

**Conclusion:** The debug-build panic is real and fires on empty-buffer startup before release semantics (silent metric corruption) are observable. An Antithesis run using a debug build will see a crash on first clean restart with an empty buffer; a release build will see astronomical metric values. Both are confirmations of the same bug via different signals.
