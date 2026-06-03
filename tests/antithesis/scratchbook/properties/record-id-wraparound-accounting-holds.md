---
slug: record-id-wraparound-accounting-holds
type: Safety / Always
status: LATENT BUG (now WATCHED) — the `- 1` at `ledger.rs:281` is not wrapping; equality case produces u64::MAX. Reported under the `antithesis` build by the committed detector at ledger.rs:271; the arithmetic itself is still unguarded in production.
sut_commit: 049eec79b737450c4669b7f8aa1dd814551ec466
---

# Property 17: record-id-wraparound-accounting-holds

## Catalog Entry

**Type:** Safety / Always

**Property:** Across u64 record-ID wraparound and the empty-buffer equality case,
`get_total_records` returns a correct count (0 when empty, N when N unacked). Never
~2^64, which would corrupt metrics, trigger false "buffer full" accounting, or emit
junk gauges.

**Invariant:** `get_total_records() <= actual_unacked_event_count + small_bounded_delta`.
Returns 0 when empty (`next_writer_record_id == last_reader_record_id`), never near
`u64::MAX`.

**The Bug — `get_total_records`:** `next_writer_id.wrapping_sub(last_reader_id) - 1`.
The `wrapping_sub` is the correct modular distance, but the OUTER `- 1` is plain
subtraction. When the IDs are equal — the empty/drained buffer, including fresh
start (both 0) and the `next=u64::MAX,last=u64::MAX` caught-up wrap point —
`wrapping_sub` is 0 and `0 - 1` underflows. Fix: `.wrapping_sub(1)`.

**Downstream impact:** `synchronize_buffer_usage` calls `get_total_records()`
unconditionally at init; on an empty previously-used buffer it gets `u64::MAX` and
feeds `increment_received_event_count_and_byte_size(u64::MAX, ...)`, pinning the
buffer-size gauge to ~1.844e19 for the process lifetime — the same symptom class as
issue #23995. Not used in any control path (`is_buffer_full`/`can_write_record`)
today, so it corrupts metrics only; a future control-path use would make it severe.

**Build-mode divergence (load-bearing):** debug PANICs on `0 - 1` at empty-buffer
startup before any metric — the LOUDER signal. Release silently wraps to `u64::MAX`,
needing a workload-side gauge assertion. Pick the profile that surfaces it.

**Antithesis Angle (G3):** drain fully, restart, scrape
`buffer_size_events`/`buffer_size_bytes`; assert ~0, not `u64::MAX`. The near-wrap
injection path needs the `#[cfg(test)]` `unsafe_set_*` id helpers — test-build-only.

SUT-side: COMMITTED detector inside `get_total_records`
(`assert_always_greater_than_or_equal_to!(next.wrapping_sub(last), 1, ...)`), fires
on the drained/equality case before the `- 1` wraps. A detector only — the
arithmetic still wraps in production (unguarded).

---

## Test plan (2026-06-02, expert-chorus decision)

**Home:** in-tree `proptest` property test in `lib/vector-buffers`, NOT an Antithesis
scenario (this arithmetic is deterministically reproducible in-process).

**Test A — ledger-level, pure sync (no async, no buffer).** The cheapest, most
direct test of `get_total_records` (`next.wrapping_sub(last) - 1`):

- Build a `Ledger` via `Ledger::load_or_create` backed by the `Vec<u8>` mmap
  (`impl WritableMemoryMap for Vec<u8>`). No bare-struct constructor; go through
  `load_or_create` as known_errors.rs does.
- Set `next_writer_record_id` / `last_reader_record_id` via the `#[cfg(test)]`
  `unsafe_set_writer_next_record_id` / `unsafe_set_reader_last_record_id` helpers
  (pattern in known_errors.rs).
- `proptest` over `(next, last)`, assert `get_total_records()` never wraps near
  `u64::MAX`. Failing case: drained `next == last` ⇒ `0 - 1` ⇒ `u64::MAX`. Safe:
  fresh `next=1,last=0 ⇒ 0`.

Reachable purely in-process (write/read/ack N to drain, or the setter approach) and
also fires on reopen via `synchronize_buffer_usage`. The setter is preferred —
smallest, fastest, deterministic, no async runtime.

**Discipline:** must FAIL on current code (TDD). Debug build ⇒ panic; release ⇒ silent
`u64::MAX` — pick the profile that surfaces it (debug is the louder signal). No
`sane`/`insane` in names. (Sibling test B for the `total_buffer_size` byte underflow lives
in `total-buffer-size-never-underflows.md`; it needs a reopen + torn tail.)
