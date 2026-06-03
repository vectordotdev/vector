# Antithesis Run Log — disk buffer v2

Run-by-run chronology (G0–G11, D0) lived here for the superseded
`basic_test`/full-Vector launch path. Distilled verdict: under the faults
`basic_test` exposes (network + thread-pausing, **no node-kill**) the buffer's real
invariants held, and every red workload assertion traced to a test artifact
(unconditional-200 collector, drain-wait stopping before empty, non-atomic
concurrent appends corrupting a shared `acked.log`, deprecated source-level acks).
The one organic win was direct-exerciser run
`385cfc4df45b3c85567b9b7ef3d803ed-54-9`: the SUT-side
`total_buffer_size`-decrement assert FAILED at three sim-times — Antithesis drove
the real disk_v2 buffer into the #21683 underflow via thread-pausing alone.

## Net finding

- **Crash-class bugs** (#21683 deadlock, torn-tail recovery, crash-durability) need
  Vector killed mid-write → a **node-kill-enabled webhook**. Unreachable under
  `basic_test`. The committed `vector_to_vector_e2e_disk` harness + SUT detectors
  are wired to catch them the moment node-kill is available.
- **Accounting underflows** (#21683, `get_total_records`) reproduced organically by
  thread-pausing in the direct exerciser, and deterministically by the focused tests
  below.
- **Metric/loss bugs** (#24606, #24948, finalizer status-discard) are
  precondition-gated (need a full buffer or a Drop-without-flush), demonstrated by
  focused tests, not load.

## Demonstrated-bug ledger (the deliverable)

All 7 catalog defects demonstrated by reproducible tests. Full `vector-buffers`
suite: 85 passed, 0 failed (release). No Vector behavior changed — tests plus the
no-op SUT detectors only.

| # | Bug | Test | Repro |
|---|-----|------|-------|
| 1 | **#24606** drop_newest drops silent at component metric | `buffer_usage_data.rs::drop_newest_increments_buffer_metric_but_not_component_metric_issue_24606` | `cargo test -p vector-buffers --lib issue_24606` |
| 2 | **#21683** `total_buffer_size` unsaturated decrement wraps → writer deadlock | `disk_v2/tests/invariants.rs::ledger_total_buffer_size_decrement_underflows_issue_21683` | `cargo test -p vector-buffers --release --lib issue_21683` |
| 3 | `get_total_records` `0-1` underflow on drained buffer → ~2^64 count | `invariants.rs::get_total_records_underflows_on_drained_buffer_issue_21683_metrics` | `cargo test -p vector-buffers --release --lib get_total_records_underflows` |
| 4 | **#24948** writer `Drop` without flush → silent loss of buffered events | `invariants.rs::writer_drop_without_flush_loses_buffered_events_issue_24948` | `cargo test -p vector-buffers --lib issue_24948` |
| 5 | finalizer discards `BatchStatus` → rejected delivery silently acked | `acknowledgements.rs::rejected_delivery_still_advances_acks_finalizer_status_discard` | `cargo test -p vector-buffers --lib finalizer_status_discard` |
| 6 | reader `metadata.len()-bytes_read` size-delta underflow (truncation → #21683 wrap) | `invariants.rs::delete_completed_data_file_size_delta_underflows_reader_524` | `cargo test -p vector-buffers --release --lib reader_524` |
| 7 | reader file-id-rollover compare not wrap-aware | `invariants.rs::file_id_rollover_compare_not_wrap_aware_reader_932` | `cargo test -p vector-buffers --lib file_id_rollover_compare` |
