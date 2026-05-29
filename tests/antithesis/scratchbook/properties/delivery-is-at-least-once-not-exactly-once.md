# delivery-is-at-least-once-not-exactly-once — Duplicates Are Expected, Not a Bug

**Cluster:** I (semantic claim/reality divergences).
**Type:** Clarifying / anti-vacuity. **NOT a defect** — recorded so the oracle stays sound.

## Claim (as people state it)

"e2e acks give exactly-once delivery — no duplicates."

## Code reality

The contract is **at-least-once**, by design. Crash-replay (the buffer re-reads
records whose downstream ack wasn't durably recorded) and sink retries (a request
that succeeded but whose response was lost is retried) both re-deliver. tail's
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
2. **Use duplicates as an anti-vacuity signal.** This is now committed:
   `assert_sometimes_greater_than!(delivered_total, delivered, ...)` at
   eventually_conservation.rs:196 proves the at-least-once replay path actually
   ran in the run; if duplicates never occur, the retry/replay path was never
   exercised and a green is hollow.

## Evidence trail

2026-06-02 chorus; consistent with the catalog's existing note on
`every-written-event-eventually-delivered` ("duplicates expected — dedup and assert
≥1, not exactly-once").

## Open questions

The semantic fact (duplicates are in-contract) is fixed. Two soundness limits
touch this property's `acked ⊆ delivered` set oracle:

- **Quiescence-gated conservation.** The conservation
  `assert_always_less_than_or_equal_to!(missing_count, 0)` and spurious-id check
  fire only inside `if quiescent` (eventually_conservation.rs:165/174/181). A
  permanent writer wedge that prevents the counters from settling for 5 polls
  within the 240s deadline leaves these **skipped**, and a skipped `assert_always`
  is not a failure. Only the online integrity check (oracle.rs:118) is
  unconditional; conservation has no unconditional equivalent.

- **Best-effort ack relay.** The producer records each obligation by POSTing
  `/acked` with the HTTP result discarded (parallel_driver_produce.rs:71
  `let _ = client.post(.../acked)...`). A dropped relay over a fault-injected
  link erases the obligation, so a later genuine loss of that id is invisible to
  `missing = acked - delivered` — the id never entered `acked`, weakening the
  `acked ⊆ delivered` oracle.
