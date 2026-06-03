---
sut_path: /home/ssm-user/src/vector
commit: 2dae1f421
updated: 2026-06-03
external_references:
  - path: lib/vector-buffers/src/variants/disk_v2/
    why: Committed SUT-side assertions, behind the `antithesis` feature
  - path: tests/antithesis/scenarios/vector_to_vector_e2e_disk/
    why: Committed harness carrying workload-side and oracle-side assertions
  - path: https://github.com/vectordotdev/vector/issues/21683
    why: The total_buffer_size underflow / writer-deadlock the SUT assertions target
---

# Existing Antithesis SDK Assertions

Every assertion below is an Antithesis SDK macro: they **report, not panic** (a
detector, not a guard — the watched arithmetic still runs on the next line). The
SUT-side ones compile only under `--features antithesis`; production is unchanged.

## SUT-side (`lib/vector-buffers/src/variants/disk_v2/`, `#[cfg(feature = "antithesis")]`)

Listed by symbol/message — grep the message. 11 sites, not the 3 an earlier pass
recorded. The three `assert_always_greater_than_or_equal_to!` underflow witnesses
are the #21683 family:

- `ledger.rs` `get_total_records` — "ledger get_total_records never underflows on
  a drained buffer" (`next_writer_id.wrapping_sub(last_reader_id) >= 1`).
- `ledger.rs` `decrement_total_buffer_size` — "ledger total_buffer_size decrement
  never underflows" (`total_buffer_size >= amount`, before the `fetch_sub`).
- `reader.rs` `delete_completed_data_file` — "reader data-file size delta never
  underflows" (`metadata.len() >= bytes_read`).

The remaining sites mark reachability and rare-state coverage:

- `ledger.rs` reopen path — `assert_sometimes!` "the buffer reopens with
  pre-existing on-disk records".
- `reader.rs` monotonicity check — `assert_unreachable!` "reader never sees a
  record id that breaks monotonicity".
- `reader.rs` bad-read branch — `assert_sometimes!` "the reader skips a torn or
  corrupted record and rolls the file".
- `reader.rs` emission point — `assert_always_or_unreachable!` "no corrupted or
  empty record is delivered to the reader" (`record_bytes > 8`).
- `writer.rs` rotation gate — `assert_always_or_unreachable!` "a record never spans
  two data files".
- `writer.rs` full-buffer wait — `assert_sometimes!` "the writer blocks on a full
  buffer".
- `writer.rs` rotation — `assert_sometimes!` "the writer rolls to a new data file".
- `writer.rs` flush — `assert_sometimes!` "a record at or over the write-buffer
  size is written".

TOCTOU note: the `decrement_total_buffer_size` check loads `total_buffer_size`
separately from the `fetch_sub`. Under disk_v2's single-writer model (writer only
increments) a concurrent increment only makes the subtraction safer, so this cannot
mask a real underflow.

## Harness-side (`vector_to_vector_e2e_disk/src/bin`, always compiled with SDK `full`)

- **oracle.rs** — `assert_always!(was_issued)` online integrity / no-spurious on
  every `/ingest`; `assert_reachable!` first end-to-end delivery; `assert_reachable!`
  oracle up after SUT readiness. Calls `lifecycle::setup_complete(...)` once the SUT
  metrics endpoint answers.
- **parallel_driver_produce.rs** — `assert_reachable!` the e2e-ack path is exercised.
- **eventually_conservation.rs** — `assert_unreachable!` oracle reachable at
  judgment; `assert_always_less_than_or_equal_to!(missing_count, 0)` conservation
  (every acked id came back); `assert_unreachable!` redundant loss flag;
  `assert_always_less_than_or_equal_to!(spurious_count, 0)` no invented/corrupted
  ids; `assert_sometimes_greater_than!(acked, 100)` coverage;
  `assert_sometimes_greater_than!(delivered_total, delivered)` at-least-once replay
  ran; `assert_always!(progressed)` liveness (a fresh event still round-trips).

## Soundness notes (what a green run means)

- **Quiescence-gated conservation.** The `missing_count`/`spurious_count` checks
  fire only inside `if quiescent`. The target bug is a permanent writer wedge; a
  wedge preventing the counters from settling within the deadline leaves these
  checks *skipped* (a skipped `assert_always` is not a failure). Only the online
  `oracle.rs` spurious check is unconditional — conservation has no unconditional
  equivalent.
- **Best-effort ack relay.** `parallel_driver_produce.rs` records the obligation by
  POSTing `/acked` with the HTTP result discarded (`let _ =`). A dropped relay over
  a fault-injected link erases the obligation, so a later genuine loss of that id is
  invisible to `missing = acked - delivered`.
