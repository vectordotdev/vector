---
sut_path: lib/vector-buffers/src/variants/disk_v2
commit: 049eec79b737450c4669b7f8aa1dd814551ec466
updated: 2026-06-02
external_references: []
---

# Experiment spec: `vector_to_vector_e2e_disk`

## Mission: falsify the durability claim

An **exhibition** experiment, not a certification. The claim, as stated: *"turn on
e2e acks + a disk buffer and Vector is durable — if it says ack, it comes out the
other side; chain such durable nodes and you lose nothing end to end."* The goal:
**build that exact advertised chain and force it to lose acked data.** **Green is a
failed hunt.**

The claim is **false in the code, confirmed**: head's 200 (the "ack") fires when the
event is encoded into the **in-memory write buffer, before any fsync**
(`writer.rs:472`, `sender.rs:46-59`), and the disk buffer **short-circuits the
cross-node ack** so the 200 does not mean tail received it (`reader.rs:1117-1119`).
Acked data is therefore lost by:

- **#24948 config reload:** `BufferWriter::drop` calls `close()` but never
  `flush()`/`sync_all()` (`writer.rs:1366-1374`) → unflushed *acked* events dropped.
  A routine reload exhibits it, no exotic fault needed.
- **head crash before fsync** (acked events sit in page cache).
- **corruption / torn tail** of acked-unflushed data, skipped on reopen — the code
  comment says it: *"acknowledged but the data/file was corrupted"* (`reader.rs:111-115`).
- **#21683** `total_buffer_size` underflow wedges the writer (separate failure:
  blocks new writes, caught by the probe + SUT-side assert).

Both nodes are "durable nodes" (e2e acks + disk_v2 + `when_full: block`), exactly as
the chain claim describes — tail too (it previously had no disk buffer).

## How we make Antithesis HUNT the loss (not just check)

`assert_always(missing == 0)` gives the verdict + counterexample. To drive the
search engine *toward* a loss timeline we also emit, on the confirmed-loss branch at
quiescence, `assert_unreachable("an end-to-end-acked event was permanently lost")`.
Antithesis searches for a fault schedule that reaches it — reaching it IS the
demonstration. The obligation model stays producer-witnessed (one id per invocation).
The ack-window seam is irrelevant to exhibition: batch/systematic loss is caught in
nearly every timeline, the seam can only hide a single event. SUT-side
durable-acceptance reporting would be needed only to *certify* zero loss, not this
experiment's goal.

## Forcing the loss-prone state

Partition head↔tail so the buffer backs up with acked-but-undelivered records, keep
producing hot, then fire the reload / kill a node at a rotation boundary on the
persistent volume. The shrunk data-file knob (2 MiB, shipped) keeps both buffers
rotating constantly so reopen/torn-tail boundaries are crossed often.

---

(The #21683 writer-deadlock remains a target, caught by the probe + SUT-side
underflow assert.)

## Topology

```
producers ──POST {id}──▶ head (http_server src, e2e acks ON)
                            │
                         disk_v2 buffer  ◀── the system under test
                            │
                         vector sink ──▶ tail (vector src ──▶ http sink) ──▶ collector /ingest
```

`head` and `tail` are each their own Vector container (the SUT). The producers
(`parallel_driver_produce`), collector, and drain-and-check oracle
(`eventually_conservation`) are test commands in the `oracle` container. Ports: 8080
head HTTP source, 6000 head→tail inter-node, 9598 metrics on both nodes, 8686 oracle.

## The contract under test

head's `http_server` source with e2e acks returns **200 only after the event is
durably written to the disk_v2 buffer** (`prelude.rs:303`), Vector's durability
promise: "I own this, I will deliver it, you may discard your copy." So:

- `N ∈ acked` ≡ Vector gave its durability promise for `N`.
- `N ∈ delivered` ≡ `N` emerged at the terminal collector.

## The oracle — plain integer-id sets, no watermark, no order

Order carries **no** end-to-end information: ingest order ≠ id-assignment order
(concurrent POSTs race for the single writer `Mutex`, `prelude.rs:193/286`), egress
reorders + duplicates (sink `FuturesUnordered`, adaptive concurrency, retries,
`driver.rs:87`). Transport is **at-least-once**: duplicates are legal. The only sound
oracle is **set membership**.

Collector state:

```
issued    : HashSet<u64>   // minted at /claim (free — collector owns the counter)
acked     : HashSet<u64>   // SUT returned 200; producer relayed it ("owed")
delivered : HashSet<u64>   // id arrived at the terminal collector
delivered_total : u64      // raw /ingest count, for the duplicate guard
```

## Proof: how the assertions detect data loss

**Definition of loss:** Vector accepted durable responsibility and the event never
emerged — exactly `∃N. N ∈ acked ∧ N ∉ delivered`.

**Assertion 2** is `acked ⊆ delivered`, logically identical to:

```
∀N. (N ∈ acked → N ∈ delivered)  ≡  ¬∃N.(N ∈ acked ∧ N ∉ delivered)  ≡  "no lost event exists"
```

The subset **failing IS a lost event existing**, and the failing element names the
dropped id. Evaluate **at quiescence** (producers stopped, in-flight stable, no active
fault): before quiescence an acked-not-yet-delivered id is legal in-flight, at rest
undelivered can only mean lost.

**Subset, not equality**: `delivered` can be a strict superset of `acked` (an event
whose 200 issued but whose ack-back was lost still gets delivered, duplicates collapse
into the set). Equality would be a false red.

**Why assertion 2 alone is insufficient — suppression vacuity.** #21683 wedges the
writer, head stops returning 200s, those events never enter `acked`, and over the
truncated set `acked ⊆ delivered` still holds → green on a silently dead buffer.
Set-conservation cannot see data lost *before it was acked*. Hence the proof is a
**conjunction**:

| # | Assertion | Type | Gate | Catches |
|---|---|---|---|---|
| 1 | `delivered ⊆ issued` | `assert_always` | continuous, ungated | fabricated/corrupt id; keeps `delivered` honest so 2 can't pass falsely |
| 2 | `acked ⊆ delivered` | `assert_always` evaluated at quiescence (the `eventually_` command) | event-driven quiescence | direct loss of an acked event |
| 3 | buffer drained to empty | `assert_always` | after faults-stopped quiet window, **ungated** | the wedge that suppresses acks (loss 2 is blind to). #21683 corrupts the gauge so it never reads 0 → can never satisfy → RED |
| 4 | post-recovery probe round-trips within a bound | `assert_always` | after recovery | causal proof the writer still makes progress (no deadlock) |
| 5 | `acked > floor`; a duplicate was observed; recovered-from-crash; a rotation happened; buffer near-full | `assert_sometimes` | — | anti-vacuity: green only if we drove real load + faults |

A green run therefore means: every promised event was delivered (2), the buffer
empties and a fresh write still flows (3,4), `delivered` is uncorrupted (1), and we
pushed enough load + faults for those statements to be non-trivial (5).

## Mandatory guards (without these a red is not trustworthy)

- **Quiescence is event-driven** — producers confirmed stopped + buffer gauges
  stable + no active fault. Never wall-clock: `clock_jitter` would fire a timer
  mid-flight → false red.
- **`issued` recorded at mint** (superset of `acked`) so in-flight ids never trip
  integrity (assertion 1).
- **Collector is the oracle, not the SUT — exempt it from state-wiping faults** (omit
  from node-termination/hang) **and persist its sets** to the shared volume. A wiped
  collector is both a false miss and a false red.

## The residual seam (stated, not hidden)

If a producer gets head's 200 but dies before relaying the ack-back, `N` never enters
`acked`, and if `N` is then lost assertion 2 can't see it. One-event-per-invocation
shrinks the window to ≤1 id per producer death. Closing it fully needs head itself to
report durable acceptance (SUT-side instrumentation). Honest scope: **this proves
loss-detection for every acked-and-recorded event plus the ack-suppressing wedge — not
a zero-loss proof across the un-recorded-ack instant.**

**Quiescence-skip soundness limit.** Assertion 2 and the integrity check are
quiescence-gated: in the committed `eventually_conservation` the loss and spurious
`assert_always_less_than_or_equal_to!` checks (`eventually_conservation.rs:165`,
`:181`, plus the post-recovery `assert_always!` at `:218`) run only inside
`if quiescent`, polled against a 240s deadline (`:130`). If a writer wedge prevents
settling within that deadline, the loop returns without entering the gated branch and
the conservation asserts are **silently SKIPPED** — a skipped `assert_always` is not a
failure to Antithesis. The wedge is then caught only by the SUT-side underflow
detectors and the post-recovery probe, not the conservation oracle. A real seam: the
loss verdict is sound only when quiescence is reached.

## Producer (`parallel_driver_produce`) — pure Antithesis shape

Each invocation: claim **one** id, POST it to head, **retry the SAME id** on
timeout/non-2xx (stable idempotency key — retry ≠ re-mint), and relay the ack-back on
2xx. Antithesis owns the parallelism (many concurrent invocations) and crash timing.
No internal block loop. Payload size is **boundary-biased** via a real RNG seeded
from Antithesis entropy (`random_range`/`choose`, never `get_random() % N`), kept
**below `max_record_size`** and clustered at buffer boundaries (`0, 1, k·data_file_size ± 1`).

## SUT knob: shrink the data file to force the rare bugs

Default `max_data_file_size` is 128 MiB (`common.rs:15`); at that size a 30-min run
may never rotate, so rotation / file-id rollover / torn-tail-on-reopen are unreachable
and any green is vacuous on those bugs. The public YAML never exposes it. A
**feature-gated env override** is COMMITTED at `mod.rs:364-373`, compiled only under
the `antithesis` feature (no production change):

```rust
#[cfg(feature = "antithesis")]
let builder = match std::env::var("VECTOR_DISK_V2_MAX_DATA_FILE_SIZE")
    .ok()
    .and_then(|v| v.parse::<u64>().ok())
{
    Some(bytes) => builder
        .max_data_file_size(bytes)
        .max_record_size(usize::try_from(bytes).unwrap_or(usize::MAX)),
    None => builder,
};
```

`max_record_size` takes a `usize`, hence the `try_from`.
`VECTOR_DISK_V2_MAX_DATA_FILE_SIZE=2097152` (2 MiB) is set in the shipped compose on
**both** head and tail. Constraints (`common.rs`): `max_record_size <= max_data_file_size`
(`:353`) — set both; `max_buffer_size >= 2 × max_data_file_size` — each node `max_size`
≥ 4 MiB (shipped 8 MiB). Shrunk files → many rotations/min → frequent crash boundaries
(#21683 reopen torn tail), fast file-id churn (u16 rollover reachable), and the
anti-vacuity rotation guards (assertion 5) fire.

## Launch: `persistent_storage` endpoint + targeted faults

Launch through the scenario's `launch.sh`, never a hand-typed `snouty launch` — the
script pins the webhook, config directory, and fault profile so every shot is
identical (see `tests/antithesis/AGENTS.md`). The pinned profile:

```
snouty launch --webhook persistent_storage --config <dir> --duration 30 \
  --param custom.include_for_node_termination="head tail" \
  --param custom.include_for_node_hang="head tail" \
  --param custom.include_for_node_throttle="head tail" \
  --param custom.cpu_mod=true \
  --param custom.clock_jitter=true
```

`head`/`tail` get terminated/hung/throttled (buffer crash + reopen on the persistent
volume = #21683 path). The oracle is **omitted from termination and hang only** so its
in-memory obligation ledger survives, but stays subject to network faults to exercise
the `tail` → `oracle` path. `cpu_mod` perturbs the writer-`Mutex` races,
`clock_jitter` stresses the fsync window. The conservation check tolerates all of this
because Antithesis stops faults in the `eventually_` window, where the harness drains
then judges.

## Review corrections (2026-06-02) — supersede the table above

The adversarial review found the original assertion 3 unsound and several guards
unimplemented. Corrected design:

1. **The events gauge does NOT detect #21683 (critical).** The bug underflows the
   *bytes* counter `total_buffer_size` (`fetch_sub` at `ledger.rs:319`) and wedges the
   writer. The reader keeps draining events, so `vector_buffer_events → 0` and the old
   "buffer drained" assert passes on a fully wedged buffer (PR #23561 even saturates
   the byte gauge reporter to 0). **Drop the events-gauge "drained" deadlock
   detector.** The deadlock is caught two ways instead:
   - **Assertion 4 (post-recovery probe) is THE end-to-end deadlock detector** — but
     it must POST **representative** payloads from the boundary menu (incl. a
     near-data-file-size record), not a 1-byte event, or a partial wedge slips
     through. A wedged writer blocks the probe → never round-trips → RED.
   - **SUT-side assertion (root-cause detector, grind-plan G2-Phase-2):** COMMITTED —
     `assert_always_greater_than_or_equal_to!(total_buffer_size, amount)` at
     `decrement_total_buffer_size` (`ledger.rs:313`), under the `antithesis` feature,
     just before the `fetch_sub` (`:319`). Fires at the instruction that would corrupt
     the counter — diagnostic and robust to the partial wedge. A detector not a guard:
     it reports the wrap, the subtraction still runs. Two sibling detectors back the
     same bug at `ledger.rs:271` (get_total_records) and `reader.rs:544` (reader
     size-delta). Feature-gated, absent from production builds.
2. **Ungate loss + integrity (critical).** The old oracle gated both behind
   `if drained`, so the bug suppressed its own detection. Integrity
   (`delivered ⊆ issued`) is checked **continuously in the collector** at `/ingest`
   (immediate `assert_always` on an un-issued id). Loss (`acked ⊆ delivered`, i.e.
   `missing_count == 0`) is asserted **unconditionally at quiescence**, never gated on
   a buffer gauge.
3. **Quiescence is counter-driven, not wall-clock.** Quiescent ⇔ `acked` stable AND
   `delivered` stable across K consecutive `/report` polls AND **all nodes healthy**
   (200 on `/metrics`). `acked`-stable is the producers-stopped signal. `clock_jitter`
   only changes poll spacing, never the verdict. Do not use `vector_buffer_events` in
   the gate (corruptible + tail's in-memory buffer contaminates it).
4. **Node-down ≠ not-drained.** If any node is unreachable, stay neutral and keep
   waiting (do not assert loss over an unobservable system, do not red a down node as
   a wedge). Assert only once all nodes are healthy and counters are stable.
5. **Collector persistence (mandatory guard, was unimplemented).** Add a dedicated
   volume `v2v-collector-state` to `oracle`. The collector durably appends
   `issued`/`acked`/`delivered` + `next_id` and reloads on startup. A wiped collector
   is both a false miss and a false red.
6. **Producer: one id per invocation** (was a 256-block, which widened the
   un-recorded-ack seam to 256 and contradicted the stated ≤1 bound). Claim 1, retry
   the same id, ack-back on 2xx. Boundary sizes via `random_choice` (no `% N`), capped
   below `max_record_size`.
7. **Anti-vacuity duplicate guard:** `assert_sometimes(delivered_total >
   delivered.len())` — a duplicate was observed (proves the at-least-once replay path
   ran). Duplicates come from head crash-replay, the collector stays off the
   network-fault list so its bookkeeping is trustworthy.
8. **Dockerfile:** add `/symbols` symlinks for `parallel_driver_produce` and
   `eventually_conservation` (symbolization gap from prior triage).
