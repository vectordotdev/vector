# delivery-is-at-least-once-not-exactly-once — Duplicates Are Expected, Not a Bug

**Cluster:** I (semantic claim/reality divergences).
**Type:** Clarifying / anti-vacuity. **NOT a defect** — recorded so the oracle stays sound.

## Claim (as people state it)

"e2e acks give exactly-once delivery — no duplicates."

## Code reality

The contract is **at-least-once**, by design. Crash-replay (the buffer re-reads
records whose downstream ack wasn't durably recorded) and sink retries (a request
that succeeded but whose response was lost is retried) both re-deliver. node1's
`vector` source does **no deduplication**. So the same payload id can arrive at the
collector more than once.

## Divergence

This is a *understanding* gap, not a bug: people conflate at-least-once with
exactly-once. Duplicates are the correct, documented behavior of an at-least-once
pipeline.

## Why it matters for the harness

Two consequences for the oracle (both already honored):

1. **Never assert against duplicates.** The loss oracle must use **set membership**
   (`acked ⊆ delivered`), not equality or order — a duplicate must not register as a
   fault. Asserting "no duplicates" would be a false red.
2. **Use duplicates as an anti-vacuity signal.** `assert_sometimes(delivered_total >
   delivered_distinct)` proves the at-least-once replay path actually ran in the
   run; if duplicates never occur, the retry/replay path was never exercised and a
   green is hollow.

## Evidence trail

2026-06-02 chorus; consistent with the catalog's existing note on
`every-written-event-eventually-delivered` ("duplicates expected — dedup and assert
≥1, not exactly-once").

## Open questions

- None. This is a fixed semantic fact about the topology.
