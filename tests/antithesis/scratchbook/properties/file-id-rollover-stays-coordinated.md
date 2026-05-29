---
slug: file-id-rollover-stays-coordinated
type: Safety / Always
status: LATENT BUG — reachable in tests (MAX_FILE_ID=6), latent in production (MAX_FILE_ID=65536)
sut_commit: 049eec79b737450c4669b7f8aa1dd814551ec466
---

# Property 16: file-id-rollover-stays-coordinated

## Catalog Entry

**Type:** Safety / Always

**Property:** Across a u16 file-ID rollover (writer wraps from `MAX_FILE_ID - 1` back to 0),
the reader and writer remain correctly coordinated: the reader can still determine its position
relative to the writer, and `seek_to_next_record` does not misclassify the synchronization
state, deadlock, or skip events.

**Invariant:** At all times, including across file-ID rollover, the condition used by
`seek_to_next_record` to determine "reader is now synchronized with writer" (`reader_file_id >
writer_file_id`) is semantically correct. After rollover, this raw `u16 >` comparison produces
false answers for any configuration where the reader's file ID has wrapped past 0 while the
writer's has not (or vice versa), causing either premature "synchronized" claims or failure to
terminate the seek loop.

**The Bug — `reader.rs:941-945`:**

```rust
// reader.rs:941-945 — raw u16 comparison, not wrap-aware
let (reader_file_id, writer_file_id) =
    self.ledger.get_current_reader_writer_file_id();
if reader_file_id > writer_file_id {
    break;
}
```

This comparison is the synchronization gate inside `seek_to_next_record`'s bad-read handling
loop. It is intended to detect the case where the reader has advanced to the file the writer
hasn't yet created (meaning they are synchronized). The logic is correct in the non-rollover
case: reader on file 4, writer on file 3 → reader_file_id (4) > writer_file_id (3) → break.

After rollover, the semantics invert. Example with `MAX_FILE_ID = 6`:

- Writer has written to files 0, 1, 2, 3, 4, 5, 0 (wrapped), 1. Writer is now on file ID 1.
- Reader has advanced to file ID 2 (wrapped past 5→0→1→2), ahead of the writer.
- `reader_file_id` = 2, `writer_file_id` = 1. Condition: `2 > 1` → `true` → `break`.
  This is correct in this case (reader is ahead).

But consider the inverse: reader on file 5 (about to wrap), writer just wrapped to file 0:

- `reader_file_id` = 5, `writer_file_id` = 0. Condition: `5 > 0` → `true` → `break`.
  The reader incorrectly claims it is synchronized (ahead of the writer) when in fact it is
  behind (the writer has lapped the reader).

The wrap-safe comparison requires tracking how many times each side has wrapped, or using a
distance function that is rollover-aware (e.g. treating file IDs as a modular ring). The writer
and reader file IDs both live in the mmap'd ledger as `AtomicU16`; there is no wrap counter.
The arithmetic used to compute "next" file IDs (`(current + 1) % MAX_FILE_ID`,
`ledger.rs:128,150`) is wrap-correct, but the comparisons used for ordering are not.

**Additional file-ID arithmetic context:**

- `get_next_writer_file_id` (`ledger.rs:128`): `(writer + 1) % MAX_FILE_ID` — wrap-correct.
- `get_next_reader_file_id` (`ledger.rs:150`): `(reader + 1) % MAX_FILE_ID` — wrap-correct.
- `get_offset_reader_file_id` (`ledger.rs:154`): `reader.wrapping_add(offset) % MAX_FILE_ID`
  — wrap-correct for the addition.
- `reader_file_id > writer_file_id` (`reader.rs:943`): raw `u16` comparison — NOT wrap-aware.

**Reachability:**

In test builds, `MAX_FILE_ID = 6` (`common.rs:45`). With a small enough buffer configuration,
a test can cycle through all 6 file IDs in a short run. The in-repo proptest model suite uses
this constant. In production, `MAX_FILE_ID = u16::MAX = 65535` (`common.rs:43`), requiring
65535 data-file rotations (each up to 128MB = ~8TB of data) to hit rollover — not reachable
in production without months of sustained high-volume writes, but reachable in Antithesis with
`MAX_FILE_ID = 6` (the test build constant).

**Antithesis Angle:**

1. Run Vector with a test build (`MAX_FILE_ID = 6`, which is the default under `#[cfg(test)]`)
   or with a small configured `max_data_file_size` and high write volume to force rapid rotation.
2. Write enough data to cycle through all 6 file IDs multiple times (triggering rollover).
3. Inject node-kill faults at the rollover boundary (when writer_file_id is near 5 and
   reader_file_id is near 0, or vice versa) to force `seek_to_next_record` to run across the
   rollover point.
4. After restart, assert:
   - Vector does not deadlock (the seek loop terminates).
   - No events are skipped (event count before crash = events delivered after restart).
   - `buffer_discarded_events_total` does not increment unexpectedly.
   - Vector's buffer metrics settle to a consistent state.

The "deadlock / no-progress" variant is most likely to manifest: if the comparison misfires and
the seek loop never breaks, `seek_to_next_record` hangs forever (no timeout), and Vector's
read path never marks itself `ready_to_read`. The pipeline stalls silently, similar to the
`total_buffer_size` underflow deadlock (INV-7 / L1 / L8).

SUT-side `assert_always!` candidates:

- Inside the `seek_to_next_record` bad-read loop: assert that the loop terminates within a
  bounded number of iterations (e.g. `MAX_FILE_ID` iterations).
- After `seek_to_next_record` completes: assert `self.ready_to_read == true` within a
  bounded time after startup.

**Why It Matters:**

A file-ID rollover bug causes a silent pipeline stall on restart after the buffer has cycled
through its full file namespace. In production this is an extreme edge case (requires ~8TB
written through a single buffer), but it is exactly reachable in test environments with
`MAX_FILE_ID = 6`. Antithesis, running the test binary, will hit rollover routinely. The bug
provides a concrete, Antithesis-reachable test of whether the recovery path is robust to
rollover — and whether the two disabled tests
(`reader_exits_cleanly_when_writer_done_and_in_flight_acks`, `writer_waits_when_buffer_is_full`)
are related to this class of issue.

## Open Questions

1. **Is the `reader_file_id > writer_file_id` comparison actually the problematic gate, or is
   there additional context (e.g. the `unacked_reader_file_id_offset` accounting) that makes
   it correct in the rollover case?** `get_current_reader_file_id` (`ledger.rs:332-335`) adds
   the `unacked_reader_file_id_offset` to the persisted reader file ID. This offset represents
   how many files the reader has consumed but not yet acked. If this offset is bounded and
   resets correctly at rollover, the comparison may be more correct than the raw IDs suggest.
   Needs deeper analysis.

2. **Does the `unacked_reader_file_id_offset` also suffer from rollover? The `get_offset_reader_file_id`
   function uses `wrapping_add(offset) % MAX_FILE_ID` (ledger.rs:154). If `offset` grows large
   enough (multiple unacked files), could the offset itself wrap and produce a spurious file ID?
   The offset is bounded by the number of concurrently unacked files, which is bounded by the
   buffer size / data file size, so it should be small in practice. But this should be verified.

3. **Does Antithesis run test binaries (with `MAX_FILE_ID=6`) or production binaries (with
   `MAX_FILE_ID=65535`)?** If production binaries are used, the rollover scenario requires
   injecting synthetic file-ID state (using the `unsafe_set_writer_next_record_id` test helpers,
   which are cfg(test)-gated and unavailable in production binaries). A production-binary
   Antithesis run would need to use a very small `max_data_file_size` and very high write
   throughput to approach rollover, which may not be achievable in the test window.

4. **What is the `seek_to_next_record` loop bound?** The loop has no explicit iteration limit.
   If the rollover bug causes the loop to not terminate (e.g. the condition never fires), it
   will spin indefinitely using CPU but blocking the read path. Whether this manifests as a CPU
   spike (spinning loop) or a true deadlock (blocked await) depends on whether any `await`
   points are hit inside the loop — which they are (`self.next().await`). So the actual
   behavior is likely a livelock: calling `next()` repeatedly, hitting bad reads, never breaking,
   consuming resources without progress. Antithesis CPU throttle faults could help expose this.

---

### Investigation Log

#### Build requirement: `MAX_FILE_ID` is 65535 in production, 6 only in `cfg(test)`

**Examined:** `common.rs:42–45`.

**Found:** The cfg-gated constants are confirmed at common.rs:42–45:

```rust
#[cfg(not(test))]
pub const MAX_FILE_ID: u16 = u16::MAX;  // 65535
#[cfg(test)]
pub const MAX_FILE_ID: u16 = 6;
```

The test-only value of 6 means rollover is exercisable in a handful of file rotations under test; the production value of 65535 requires ~8TB of data through a single buffer. An Antithesis run using the production binary would not trigger rollover organically; triggering it would require either a test binary or injected file-ID state via `unsafe_set_writer_next_record_id` / `unsafe_set_reader_last_record_id`, which are themselves `#[cfg(test)]`-gated (ledger.rs:174, 190).

**Conclusion:** Confirmed cfg-gated at common.rs:43–45. Rollover testing in Antithesis requires the test binary (MAX_FILE_ID=6) or a specially instrumented production build.

#### Does the `unacked_reader_file_id_offset` indirection make the raw `>` comparison at `reader.rs:943` more correct than it looks?

**Examined:** `ledger.rs:229` (`unacked_reader_file_id_offset: AtomicU16`), `ledger.rs:332–335` (`get_current_reader_file_id`), `ledger.rs:354–359` (`get_current_reader_writer_file_id`), `reader.rs:941–945`.

**Found:** `get_current_reader_file_id` at ledger.rs:332–335 returns `self.state().get_offset_reader_file_id(unacked_offset)` where `unacked_offset = self.unacked_reader_file_id_offset.load(Ordering::Acquire)`. The `get_offset_reader_file_id` helper at ledger.rs:154–156 computes `get_current_reader_file_id().wrapping_add(offset) % MAX_FILE_ID` — the `%` is wrap-correct for the arithmetic. `get_current_reader_writer_file_id` at ledger.rs:354–359 calls both accessors and returns the pair. The offset means `reader_file_id` at reader.rs:943 is the *adjusted* reader file ID (accounting for files consumed but not yet acked), not the raw persisted value. This makes the raw `u16 >` comparison slightly more correct than a comparison of bare persisted IDs, because the adjusted ID represents where the reader is actually reading, not just where it last checkpointed.

**Not found:** No wrap-aware modular distance function is used for the comparison — it is still `reader_file_id > writer_file_id` with raw `u16` values. The `unacked_reader_file_id_offset` does not introduce a generation counter or wrap-epoch tracking. If both the reader and writer have wrapped through 0 at least once, the adjusted reader ID can still be numerically less than the writer ID even though the reader is ahead in the modular ring, making the `>` comparison produce a false negative (reader incorrectly appears to be behind). The offset indirection reduces — but does not eliminate — the incorrectness of the raw comparison.

**Conclusion:** The `unacked_reader_file_id_offset` makes the comparison marginally more correct (it reflects actual read position rather than acked position) but does not fix the rollover-incorrectness of the raw `u16 >` gate. The bug is real and the comparison at reader.rs:943 is not wrap-safe.
