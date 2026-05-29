# Evidence: corruption-skip-loss-bounded

**Slug:** corruption-skip-loss-bounded
**Type:** Safety / `Always` (workload-level)
**Status:** Expected VIOLATED by current design (conservative whole-file roll).

## Why this property exists (user concern)

Driving concern: *"if the checksum fails we'll skip records."* This property
quantifies and bounds that loss. The sibling property
`corruption-is-detected-and-recovered` only checks that the recovery path
*executes* (a `Sometimes` reachability check). It does not bound how much is
lost. This property does.

## The mechanism (reader.rs)

When `BufferReader::next` hits a bad read — `is_bad_read()` true for
`Checksum` / `Deserialization` / `PartialWrite` (reader.rs:132-139) — it calls
`roll_to_next_data_file()` (reader.rs:705-753) and returns the error. Rolling:

- adds a deletion marker covering only the records **actually read**
  (`data_file_record_count`, reader.rs:722/743),
- `self.reset()` + `increment_unacked_reader_file_id()` (reader.rs:749-750),
- **unconditionally abandons the entire remainder of the current data file.**

So for a data file `[A, B, CORRUPT, D, E]`, the reader delivers A, B, hits
CORRUPT, rolls, and **D and E are never read, delivered, acked, or counted** —
even though they are perfectly valid records. A single bit-flip near the start
of a 128 MB data file can abandon almost the whole file (the file holds up to
`max_data_file_size` / `min_record_size` records). The code comment
(reader.rs ~1028) calls this intentional ("not sure the rest of the file
is valid").

## The invariant we want to test

Workload-level `Always`: every record that (a) was durably written with a
**valid** checksum and (b) sits after a corrupt record in the same data file is
still eventually delivered. I.e. loss is bounded to the corrupt record itself
(plus any genuinely-unparseable contiguous tail), not the whole-file remainder.

`Always` is the right type: this is a safety/correctness bound that must hold on
every corruption event, not a reachability or liveness milestone.

## Antithesis angle

Write a multi-record data file with known IDs, inject a single bit-flip into an
**early** record's CRC-covered region (not the last), let the reader drain, then
compare delivered IDs against the valid (non-corrupted) IDs. The gap = records
lost purely to the conservative roll. Vary corruption position (first / middle)
and file fullness to measure the loss magnitude. fs-fault or workload-injected
bit-flip; needs the corruption in a *live* read, not a not-yet-opened file.

## Why it matters

The authoritative spec — internal doc *"internal buffer design notes"*
((internal doc id omitted)) — states the disk-buffer data-loss window is **500 ms of unsynced
writes**, and with e2e acks enabled, synced events are **not** lost. But a
corruption-triggered roll discards *synced, valid, not-yet-acked* records far
outside that 500 ms window — a silent contradiction of the stated guarantee for
an "at-least-once" buffer. Even if the conservative roll is accepted, the loss
must at minimum be bounded and counted (see `corruption-skip-loss-is-counted`).

## SUT-side instrumentation (not yet committed)

The Antithesis SDK is a committed dependency under the `antithesis` feature.
`existing-assertions.md`: only the three underflow
`assert_always_greater_than_or_equal_to!` detectors exist in `lib/vector-buffers`
today (ledger.rs:271/313, reader.rs:529); none covers corruption-skip loss.
Suggested: in `roll_to_next_data_file`
compute `abandoned = file_size_remaining_after(bytes_read)` and
`assert_always!(abandoned == 0 || tail_is_unparseable, ...)` — or, more
practically, a workload-level oracle (delivered ⊇ valid-records) since "tail is
genuinely unparseable" is hard to assert SUT-side.

## Open Questions

- Is the whole-file roll an accepted product tradeoff, or should the reader
  attempt to resync to the next record boundary within the same file and only
  abandon the unparseable span? `(needs human input)` — the code comment says
  intentional, but the internal spec implies synced events shouldn't be lost.
- Can records be re-found after a corrupt one given the length-delimited format
  (read `record_len`, skip, try next), or does a corrupt length delimiter make
  intra-file resync unsafe? `(partial: length delimiter is itself CRC-unprotected
  framing, so a corrupt delimiter can desync intra-file resync — supports the
  conservative roll; a CRC-valid record after a payload-corrupt record could in
  principle be recovered)`

### Investigation Log

#### Is the whole-file roll an accepted tradeoff or should the reader resync?

- Examined: `reader.rs` `roll_to_next_data_file` (705-753), `BufferReader::next` bad-read branch + comment (~1028, branch at 1046), `is_bad_read` (132-139); internal doc *internal buffer design notes* ((internal doc id omitted)).
- Found: the roll is unconditional and the code comment frames it as intentional ("not sure the rest of the file is valid"). The internal spec's 500ms/synced-not-lost guarantee says nothing about corruption, so the two are not formally reconciled.
- Not found: any product decision record stating whole-file abandonment is the accepted behavior for synced records. Conclusion: `(needs human input)` — owner must confirm intended vs. bug.

#### Can records be re-found after a corrupt one?

- Examined: length-delimited framing in `try_next_record` (reader.rs:306+), `read_length_delimiter` (reader.rs:226+), CRC coverage in `record.rs`.
- Found: the length delimiter is framing, not under the record CRC; a corrupt delimiter desyncs intra-file scanning, justifying the conservative roll. A payload-corrupt record with an intact delimiter could in principle be skipped past. Tagged `(partial)`.
