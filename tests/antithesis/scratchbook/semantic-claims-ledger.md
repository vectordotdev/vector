---
sut_path: /home/ssm-user/src/vector
commit: 049eec79b737450c4669b7f8aa1dd814551ec466
updated: 2026-06-02
external_references: []
---

# Semantic claims ledger — what people believe vs. what the code does

Doctrine (2026-06-02): discuss the program's **claimed/understood semantics first**,
then show where code reality diverges — demonstrating that divergence is the goal.
This ledger is the semantics-first index over the property catalog: each row is a
claim as people *state* it about the `vector_to_vector_e2e_disk` topology (head
`http_server`+acks → `disk_v2`(block) → `vector` sink+acks → tail `vector`+acks →
`disk_v2`(block) → `http` sink+acks → collector; both nodes "durable"), the code
reality, and how to exhibit the gap in Antithesis.

Assertion doctrine for exhibition: pair `assert_always(invariant)` (carries the
counterexample) with an `assert_unreachable("<the bad state>")` **loss magnet** on
the confirmed-violation branch so the search engine actively *hunts* the divergence
instead of passively checking for it.

| # | Claim (as stated) | Code reality | Divergence | Catalog property / how to exhibit |
|---|---|---|---|---|
| C1 | "If Vector acks (200), the event is **durably persisted** — it'll survive a crash." | The 200 fires when the event is *encoded into the in-memory write buffer*, **before fsync** (`writer.rs:472` drops the source finalizer at encode → `Delivered`; `prelude.rs:303-310`; no flush on the send path, `topology/channel/sender.rs:46-59`). | Acked ≠ durable. A crash/reload before the next fsync loses acked data. | **NEW `ack-does-not-imply-durability`**. Exhibit: reload (#24948) or kill head before fsync; `assert_unreachable("an acked event was permanently lost")`. **(launched run exhibits this)** |
| C2 | "Chain e2e-ack nodes and the head's ack means it reached the tail (end-to-end)." | The disk buffer **short-circuits** the ack: it mints a *fresh* `BatchNotifier` for the downstream hop (`reader.rs:1128-1129`); the source finalizer is consumed at encode. So head's 200 = "durable in head's buffer," NOT "tail got it." | The ack is **per-hop**, not transitive. "Acked at head" says nothing about the tail. | **NEW `ack-is-per-hop-not-transitive`** (resolves `multi-hop-conservation-no-loss` OQ#1). Exhibit: head 200, then partition head↔tail + drop head's buffer → head-acked id never reaches tail. |
| C3 | "Disk buffer = durable across restart." | True only for data already fsync'd. The unflushed write-buffer tail + a torn/partial tail are lost or skipped on reopen (`reader.rs:111-115` "acknowledged but the data/file was corrupted"). | "Durable across restart" holds only past the fsync window, not for everything acked. | `durable-unacked-events-survive-crash`, `partial-write-at-rotation-recovers`, `corruption-skip-loss-bounded`. |
| C4 | "e2e acks ⇒ at-least-once, no loss." | Finalizer **discards** non-`Delivered` status (`ledger.rs:717`) — an errored/rejected delivery is treated as acked and freed within a process lifetime. | At-least-once breaks for in-process sink errors (only a full crash+replay restores it). | `sink-failure-not-silently-acked` (currently VIOLATED). |
| C5 | "`when_full: block` backpressure never drops data." | Mostly true for `block`, but the #21683 underflow wedges the writer so new writes block forever (looks like backpressure, is a deadlock); and `drop_newest` mode drops without a component-visible count. | "Never drops" hides a permanent stall (no data moves) and an uncounted-drop mode. | `writer-eventually-makes-progress`, `buffer-size-within-max` (vacuity), `dropped-events-are-counted`. |
| C6 | "Chaining durable nodes ⇒ no end-to-end loss." | Composition of C1–C4 across N hops + C2 (per-hop ack). Each hop independently loses acked-unflushed data on reload/crash; the chain does not repair it. | End-to-end no-loss is false the moment any hop loses acked-unflushed data. | `multi-hop-conservation-no-loss`; the launched exhibition run (both nodes durable). |
| C7 | "The buffer drains after faults stop." | The #21683 byte-counter underflow makes `is_buffer_full()` permanently true; the *events* gauge still reads 0 (PR #23561 saturated the reporter), so the buffer looks drained while the writer is wedged. | "Drains after faults" is observably true on the gauge yet the writer is dead. | `writer-eventually-makes-progress`, `reader-drains-and-terminates-cleanly`. Detect via post-recovery probe + SUT-side `total-buffer-size-never-underflows`, NOT the events gauge. |
| C8 | "Metrics reflect the real buffer state." | `total_buffer_size` underflows to ~2^64 but the gauge is `saturating_sub`'d to look clean (#23561); `get_total_records` `0-1` wraps to ~2^64 on a drained restart (`ledger.rs:281`); corruption-roll loss hits *neither* discard counter (`reader.rs` roll). | The gauge actively *hides* the deadlock and miscounts; loss is uncounted. | `record-id-wraparound-accounting-holds`, `corruption-skip-loss-is-counted`, `dropped-events-are-counted`, `buffer-size-within-max`. |
| C9 | "Graceful shutdown flushes everything (lossless)." | `BufferWriter::Drop` calls `close()` but **not** `flush()`/`sync_all()` (`writer.rs:1366-1374`); losslessness depends on the topology flushing before drop — unverified, racy. | Graceful-shutdown losslessness is conditional on an external flush that may not happen. | `graceful-shutdown-flushes-all`. |
| C10 | "Config reload is safe — no data dropped." | Reload rebuilds the sink, dropping the writer without flushing (#24948) → unflushed acked events lost; old+new topologies may contend the per-process advisory lock. | A routine reload silently drops buffered acked events. | `config-reload-no-silent-loss` (#24948). The cleanest C1 exhibit — no kernel fault needed. |
| C11 | "e2e acks ⇒ exactly-once (no duplicates)." | At-least-once by design: crash-replay and sink retries re-deliver (tail source has no dedup). | NOT a bug — but people conflate at-least-once with exactly-once. Duplicates are expected. | **NEW `delivery-is-at-least-once-not-exactly-once`** (clarifying, marked not-a-defect). Exhibit: `assert_sometimes(duplicate observed)` — also our anti-vacuity guard. |
| C12 | "Events come out in the order they went in." | disk_v2 is FIFO by *ingestion* order, but concurrent ingest races the writer `Mutex` and egress reorders (`FuturesUnordered`, adaptive concurrency). | NOT a strong product promise — order is not preserved end-to-end. Documented here so the oracle never assumes it (we use sets, not order). | No assertion (would be a false red). Recorded so harness authors don't build order-based checks. |

The mission's headline is **C1 + C2 + C6 + C10**: build the advertised "durable
chain" exactly as people describe and force it to drop data Vector said "ack" to.
C3–C5, C7–C9 are the supporting divergences already in the catalog. C11–C12 are
understanding-gaps, not defects — recorded so the oracle stays sound.
