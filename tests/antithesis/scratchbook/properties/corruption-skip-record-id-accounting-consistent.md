# Evidence: corruption-skip-record-id-accounting-consistent

**Slug:** corruption-skip-record-id-accounting-consistent
**Type:** Safety / `Always` (SUT-side)
**Status:** Suspected-violable; cross-cuts the #21683 underflow cluster.

## Why this property exists

This is the cross-cutting link between the checksum-skip data-loss path and the
accounting-underflow / monotonicity bugs already in the catalog
(`total-buffer-size-never-underflows`, `record-id-monotonicity-holds`,
`get_total_records` underflow). When a corruption roll abandons records, the
ledger's record-ID and buffer-size accounting must stay self-consistent across
the gap, or a data-loss event silently mutates into an accounting-corruption
event (which can then deadlock the writer or report phantom counts).

## The mechanism (reader.rs)

`roll_to_next_data_file` (reader.rs:711-759):

- `data_file_event_count = last_reader_record_id.wrapping_sub(start_id) + 1`
  (reader.rs:724-727) — counts only events whose records were **read**.
- The deletion marker carries `(data_file_record_count_read, bytes_read)`
  (reader.rs:746-752); abandoned records contribute **nothing**.
- `increment_unacked_reader_file_id()` advances to the next file.

The abandoned records' IDs are never observed by the reader, so
`reader_last_record_id` is not advanced past them. The next file's first record
has an ID that is **> last_reader_record_id + (abandoned count)** — a gap.

Two downstream hazards:

1. **Buffer-size desync → #21683.** When the rolled file is later deleted,
   `delete_completed_data_file` decrements `total_buffer_size` by a `size_delta`
   derived from `metadata.len() - bytes_read` (reader.rs:521-535). If the file
   was truncated, or `bytes_read` disagrees with on-disk length, this is the
   exact **reader.rs:524 underflow** — the abandoned-tail bytes make the delta
   computation the most likely real trigger for the #21683 wrap.

2. **Record-ID monotonicity.** On the next read / next restart,
   `seek_to_next_record` / `validate_last_write` and the monotonicity guard
   (`reader.rs:~480`, "record ID monotonicity violation … serious bug" panic)
   expect IDs to advance by exactly the consumed count. An unaccounted gap from
   the abandoned span risks tripping the guard (→ process panic → restart loop)
   or silently mis-setting `get_total_records`.

## The invariant we want to test

`Always` (SUT-side): after a corruption roll, the ledger satisfies
`next_writer_record_id - reader_last_record_id == on-disk unread records`, the
`total_buffer_size` decrement for the rolled file equals the true remaining
bytes (no underflow), and the monotonicity guard never trips. Stated negatively:
a corruption roll never converts bounded data loss into accounting corruption.

## Antithesis angle

Inject corruption mid-file (to force a roll with a non-empty abandoned tail),
then continue reading across the file boundary and across a crash+restart.
Watch the three SUT-side underflow asserts already wired (decrement,
get_total_records, reader.rs:524) plus the monotonicity guard. This is where the
corruption-skip path and the organically-reproduced #21683 (run D0) most
plausibly meet.

## Relationship to existing properties

- **Strengthens** `total-buffer-size-never-underflows`: identifies the
  corruption-roll abandoned-tail as a concrete real trigger for the reader.rs:524
  underflow (not only external truncation).
- **Strengthens** `record-id-monotonicity-holds`: corruption-roll gaps as a
  trigger for the monotonicity panic.
- **Depends-on** the loss being real (`corruption-skip-loss-bounded`).

## SUT-side instrumentation

Largely PRESENT: the three underflow `assert_always!` guards added this effort
(`decrement_total_buffer_size`, `get_total_records`, reader.rs:524) already
cover hazard (1). MISSING: an assertion tying the abandoned-record-ID gap to
`reader_last_record_id` advancement in `roll_to_next_data_file` (hazard 2 — the
record-ID gap is currently uninstrumented).

## Open Questions

- Does any path advance `reader_last_record_id` to cover abandoned record IDs,
  or is the gap permanent until the next file's first read re-anchors it?
  `(partial: roll_to_next_data_file does not advance it past abandoned records;
  whether the next file's read re-anchors cleanly or trips monotonicity needs a
  cross-file read trace)`
- Is the reader.rs:524 underflow reachable purely via corruption-roll
  (abandoned tail) without external truncation? If so, #21683 is reachable on
  the pure-corruption path, not only the crash/fs-fault path. `(needs human input
  / Antithesis run with mid-file corruption)`

### Investigation Log

#### Does any path advance reader_last_record_id over abandoned IDs?

- Examined: `roll_to_next_data_file` (711-759), `reset` (458), `increment_unacked_reader_file_id`, monotonicity guard (~480), `seek_to_next_record` (827+).
- Found: the roll advances the reader *file* id but does not advance `reader_last_record_id` past abandoned records; re-anchoring depends on the next file's first read. Whether that re-anchor is clean or trips the monotonicity guard needs a cross-file read trace under corruption. Tagged `(partial)`.

#### Is reader.rs:524 underflow reachable purely via corruption-roll?

- Examined: `delete_completed_data_file` size_delta (521-535, the reader.rs:524 site) and how `bytes_read` vs `metadata.len()` are sourced after a roll.
- Found: the abandoned tail makes `bytes_read < metadata.len()` (the normal partial-read case, which currently does NOT underflow) — underflow needs `bytes_read > metadata.len()` (truncation). So pure corruption-roll alone likely does not underflow; it needs a *concurrent* truncation/fault. Conclusion: `(needs human input / Antithesis run with mid-file corruption + fs fault)` to confirm reachability.
