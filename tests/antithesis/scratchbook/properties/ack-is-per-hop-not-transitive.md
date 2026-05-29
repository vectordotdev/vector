# ack-is-per-hop-not-transitive — The Head's Ack Does Not Mean It Reached the Tail

**Cluster:** I (semantic claim/reality divergences) · bridges B (`multi-hop-conservation-no-loss`).
**Type:** Safety (semantic divergence; the consequence — loss — is exhibited via `ack-does-not-imply-durability`).

## Claim (as people state it)

"Chain e2e-ack nodes together and the acks chain transitively: when the head node
acks the client (200), the event has made it end-to-end through the downstream
node(s) — that's why chaining durable nodes loses nothing."

## Code reality

A disk buffer **terminates the upstream ack chain locally and starts a new one**:

- The source finalizer is consumed at encode time into node0's disk buffer (see
  `ack-does-not-imply-durability`), so node0's 200 fires on **local durable-write**,
  independent of node1.
- The disk **reader mints a fresh `BatchNotifier`** for each record as it reads it
  out toward the downstream hop (`reader.rs:1117-1119`); that fresh notifier's acks
  drive only node0's own record/file deletion — they are a *separate* ack chain from
  node0→node1, which is itself separate from node1→collector.

So "acked at the head" means "durable in the head's buffer," never "delivered to the
next node," let alone "delivered to the tail."

## Divergence

The ack is **per-hop, not transitive**. The advertised "chain of durable nodes ⇒
end-to-end no loss" reasoning rests on a transitivity that does not exist: each hop
only promises *its own local* durability (and per C1, only encode-time, not fsync).

## How to exhibit

- Client POSTs to node0, gets 200 (head ack recorded). **Partition node0↔node1** so
  node0's buffer cannot drain, keep the head-acked id in node0's buffer, then **drop
  node0's buffer** (reload #24948 or crash-before-fsync). The id was acked at the
  head and never reaches the tail collector.
- Oracle: the collector records the head ack (producer relay) and is the tail sink;
  `assert_unreachable("an end-to-end-acked event was permanently lost")` fires.

## Resolves a standing open question

`multi-hop-conservation-no-loss` OQ#1 ("Does the `vector` source/sink protocol
propagate e2e acks transitively end-to-end, or ack locally on buffer-accept?") is
**RESOLVED: it acks locally on buffer-accept.** The disk buffer mints a fresh ack
chain (`reader.rs:1117-1119`); there is no transitive end-to-end ack. The tail
collector is therefore the *only* truth for end-to-end delivery — exactly how the
harness treats it.

## Evidence trail

2026-06-02 Vector-durability chorus agent, current tree: `reader.rs:1117-1119`
(fresh `BatchNotifier::new_with_receiver` per record), `writer.rs:472` (source
finalizer consumed at encode), `prelude.rs:303-310` (200 on local `Delivered`).

## Open questions

- Does the `vector` *source→sink* path (no disk buffer) propagate acks transitively?
  Likely yes (no buffer to short-circuit), which is why the *disk buffer* is the
  thing that breaks transitivity. `(only the disk-buffered path is in scope here)`
