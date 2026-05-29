# Property: multi-hop-conservation-no-loss

## Catalog Entry

**Type:** Liveness / at-least-once across N hops —
workload-level `assert_always!(missing.is_empty())` after the quiet period
(primary falsification) + `assert_sometimes!(all_delivered)` (reachability of
full conservation).

**Property:** In an **N-node Vector chain** where each node is
`source -> disk_v2(when_full=block) + e2e acks -> vector sink -> next node's
source`, every event injected at the head source whose ack returned to the
injector must eventually appear **at least once** at the tail collector, across
arbitrary Antithesis faults. Duplicates are allowed (at-least-once);
silent loss of an acked id is the violation. This is "what goes in comes back
out" — the at-least-once contract *composed N times*.

**Invariant:** Let `ACKED` be the set of unique ids the head injector submitted
and received an application ack for. Let `DELIVERED` be the multiset of ids the
tail collector observed. After faults stop and the chain drains to quiescence,
`ACKED \ unique(DELIVERED)` must be empty. `|DELIVERED| >= |ACKED|` is expected
(crash+replay duplicates at any hop).

## Why a multi-node topology adds value over the single-hop property

This property is the cross-node generalization of
`every-written-event-eventually-delivered` (single buffer, single Vector). The
chain is not redundant — it reaches states the single-SUT topology cannot:

1. **Composition multiplies the loss surface.** A record crosses N independent
   disk buffers, each with its own fault timing. A rare silent-loss window
   (e.g. the `delete_completed_data_file` unlink-before-ledger-flush gap,
   `reader.rs:557-560`) that fires with low probability per hop has ~N
   independent chances to fire per record. The tail oracle catches a loss at
   *any* hop. More buffers crossed = more bug-finding per injected event.

2. **Network partitions become buffer-pressure events — a genuinely new fault
   lever.** Antithesis faults are **pod-level** (docs: `environment/fault_injection`
   — "placing two pieces of software in a single pod is equivalent to assuming
   they never see faults between them"). In the single-SUT topology the buffer
   is intra-process, so network faults never touch it. In the chain, each
   inter-node `vector` sink→source link **crosses a pod boundary**. Partitioning
   `node_i.sink` from `node_{i+1}.source` while `node_i`'s disk buffer fills
   under `when_full: block` drives sustained backpressure into exactly the
   full-buffer state where the #21683 `total_buffer_size` underflow / writer
   deadlock lives (Cluster A) — and propagates that backpressure upstream
   through the e2e-ack chain. This is the cheapest way to *organically* hold a
   buffer at capacity for a long time without a synthetic overfill workload.

3. **Ack composition across hops is itself under test.** With the `vector`
   source/sink native protocol + `acknowledgements: true`, the question of
   *when* the head ack returns relative to downstream durability is a real
   correctness surface (see OQ-1). A head ack that returns before the event is
   durable at hop 2 would make `ACKED` a lie — the conservation oracle would
   then (correctly) flag loss the product spec says shouldn't happen.

## Antithesis Angle

1. **Head injector** (`parallel_driver_`, N concurrent copies): POST/emit
   uniquely-IDed events into `node_0`'s source. Each driver mints globally
   unique ids (random 128-bit, or Antithesis structured random) so concurrent
   producers never collide without coordination. On ack, append the id to a
   shared-volume `ACKED` log.
2. **Tail collector** (long-lived `serve` process in the workload container):
   record every delivered id (with duplicates) to a shared-volume `DELIVERED`
   log.
3. Antithesis injects faults mid-run at arbitrary timing across **all N SUT
   pods independently**: node-kill/restart (boot-fresh-from-image included —
   tests durable-ack honesty per `durable-unacked-events-survive-crash`),
   partitions between adjacent nodes, CPU throttle, clock jitter.
4. **Oracle** (`eventually_`, faults stopped, retry/health loop until the chain
   reports drained + the collector count stabilizes): compute
   `ACKED \ unique(DELIVERED)`; `assert_always!` it is empty; `assert_sometimes!`
   the all-delivered state is reached at least once.

### Ring upgrade ("comes back out", later)

Close `node_{N-1}.sink -> node_0.source` and tag each event with a lap counter;
a `remap`+`route` at `node_0` exits a record to the tail collector once it has
completed its lap, else forwards it back into the ring. This keeps all N buffers
**full and churning** (maximal Cluster-A pressure) while staying countable. The
lap-drain is **load-bearing, not gold-plating**: a closed ring under
`when_full: block` with no drain self-deadlocks from legitimate backpressure
(every buffer fills, backpressure propagates around the loop, the whole ring
wedges) — a false positive indistinguishable from the real deadlock. Records
must drain after a bounded number of laps or the experiment is uninterpretable.

## SUT-side instrumentation

Reuse the **same** Cluster-A / Cluster-B SUT-side asserts already committed —
every node in the chain is the same instrumented Vector binary, so the three
committed `total_buffer_size`/record underflow detectors
(`ledger.rs:313` decrement, `reader.rs:529` delta, `ledger.rs:271`
get_total_records) fire on whichever node hits the bad state. Add one
chain-specific reachability assert so the search knows the new lever is working:

```rust
// after ensure_ready_for_write blocks on a full buffer (when_full=block):
antithesis_sdk::assert_sometimes!(
    blocked_on_full_buffer,
    "writer blocked on a full disk buffer (backpressure reached the buffer)",
    json!({ "total_buffer_size": self.ledger.get_total_buffer_size() })
);
```

NOTE: three SUT-side underflow detectors are committed in `lib/vector-buffers/src`
under the `antithesis` feature: `assert_always_greater_than_or_equal_to!` at
ledger.rs:313 (decrement), ledger.rs:271 (get_total_records `- 1`), and
reader.rs:529 (reader delta). The chain-reachability assert above is still
absent. The committed asserts are detectors, not guards — they report to
Antithesis but do not abort the underflowing subtraction.

## Fault Requirements

- **Node-termination (kill/restart)** — required (confirmed enabled in tenant
  per catalog, 2026-05-28). Each node killed independently.
- **Network partition between adjacent pods** — the new lever; required for the
  backpressure value above. Each node is its **own pod/container** (mandatory:
  same-pod processes never see faults between them).
- **Persistent volume per node** for each buffer `data_dir` (confirmed
  requirement) — otherwise a fresh-boot restart wipes that node's buffer and
  acked-but-undelivered ids vanish, which would be a *spurious* failure, not a
  real one. (A real fresh-boot loss of a *not-yet-durable* event is in-contract
  and excluded; see `durable-unacked-events-survive-crash`.)
- CPU throttle / clock jitter — secondary, widen fsync/flush windows.

---

## Open Questions

**OQ-1 (Critical): Does the `vector` source/sink native protocol propagate e2e
acks *transitively* end-to-end, or does each hop ack locally on buffer-accept?**
This decides what `ACKED` means. If a head ack returns only after the event is
durably accepted into `node_0`'s buffer (local ack), then "acked at head" means
"durable at hop 0", NOT "delivered to tail" — and the conservation oracle must
be keyed strictly off the **tail collector** with `ACKED` interpreted as "the
injector got a 200/ack so the event entered the chain". If acks are transitive
(tail delivery gates the head ack), the property is much stronger. Must verify
in `src/sources/vector` / `src/sinks/vector` + the buffer ack flow. Until
resolved, define `ACKED` operationally as "head injector received its ack" and
treat the tail collector as the sole source of truth for `DELIVERED`.
`(not yet investigated)`

**OQ-2: Does backpressure from a blocked intermediate buffer propagate all the
way to the head injector, or does an upstream source buffer absorb it?**
Determines whether a mid-chain partition actually stalls the injector (and thus
whether the injector should expect ack latency to spike). Affects how the
injector distinguishes "legitimately blocked on backpressure" from "stuck on a
bug". `(not yet investigated)`

**OQ-3: With `when_full: block` on every hop, what bounds the in-flight set so
the chain can quiesce for the `eventually_` oracle?**
In a pure chain (tail → collector, no loop) the chain drains naturally once the
injector stops. In the ring upgrade the lap-drain is what bounds it (OQ in the
ring section). Confirm the `eventually_` health loop waits on a concrete
drained signal (head buffer gauges ~0 on every node AND collector count stable
for K seconds), not a fixed sleep.

**OQ-4: How are duplicates distributed across hops, and can a duplicate at hop i
be mistaken for a fresh id downstream?**
Ids are globally unique and immutable as they traverse, so a replayed duplicate
keeps its id — dedup by id at the collector is sufficient. Confirm no hop
rewrites or namespaces the id field. `(not yet investigated)`

**OQ-5 (RESOLVED, 049eec79b): SUT-side underflow asserts exist at HEAD.** Three
`assert_always_greater_than_or_equal_to!` detectors are committed under the
`antithesis` feature — ledger.rs:271 (get_total_records), ledger.rs:313
(decrement), reader.rs:529 (reader delta). The chain harness can rely on these
SUT-side signals; they fire on whichever node underflows. (They are detectors,
not guards — they report but do not abort the subtraction.)

**OQ-6 (Soundness — quiescence-gated conservation):** The conservation
`assert_always_less_than_or_equal_to!(missing_count, 0)` and the spurious-id
check fire only inside `if quiescent` (eventually_conservation.rs:165/174/181).
The target bug is a permanent writer wedge. A wedge that keeps the counters from
settling for 5 polls within the 240s deadline leaves these checks **skipped** —
and a skipped `assert_always` is not a failure. Only the online integrity check
(oracle.rs:118) is unconditional; conservation has no unconditional equivalent.
A permanent deadlock can therefore evade the conservation oracle entirely.
`(open — needs an unconditional liveness/quiescence-timeout signal)`

**OQ-7 (Soundness — best-effort ack relay):** The producer records each ack
obligation by POSTing `/acked` with the HTTP result discarded
(parallel_driver_produce.rs:71 `let _ = client.post(.../acked)...`). A dropped
relay over a fault-injected link erases the obligation, so a later genuine loss
of that id is invisible to `missing = acked - delivered` — the id never entered
`acked`. This understates `ACKED` and can mask real loss.
`(open — relay must be durable/retried or the obligation logged before the POST)`
