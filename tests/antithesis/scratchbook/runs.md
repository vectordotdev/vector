# Antithesis Run Log — disk buffer v2

Tracks launched runs (run_id ↔ test ↔ branch ↔ outcome). Triage via the
antithesis-triage skill: `snouty runs ...` keyed by run_id.

| run_id | test-name | branch | duration | purpose | status / triage |
|---|---|---|---|---|---|
| a7adf33514d82a7a7cc8faba3b51c404-54-9 | vector-diskbufv2-g0-bootstrap | blt/antithesis-setup-harness | 15m | G0 bootstrap: validate Antithesis round-trip (workload sancov instrumentation, setup_complete, bootstrap reachables) | **COMPLETED — all pass, pipeline validated.** SDK detected (Rust 0.2.8), assertions present, build OK, `event delivered end-to-end through disk buffer` + `workload serve started` reachables hit. `Software was instrumented` = vdbuf-workload only (19869 locs), **Vector NOT instrumented**. report: (internal run report, not linked) |

## Critical finding (G0 triage, 2026-05-29)

**The `basic_test` webhook does NOT inject node-termination (kill/stop/reboot) faults.**
G0 events: network faults fired heavily (clog/partition/restore, 934K packets
dropped) + thread-pausing (workload), but `reboot`/`stop`/`kill`/`shutdown`
event queries are all empty. (User said node-kill is enabled at the tenant, but
this webhook's fault menu doesn't use it.)

Implication: the **crash-time** disk-buffer bugs (#21683 underflow→deadlock,
torn-tail recovery, crash durability) require Vector to be killed mid-write — not
reachable under basic_test. **Demonstrable without node-kill:** drop_newest
miscount (#24606), sink-error silent-ack, and concurrency/interleaving bugs via
thread-pausing once Vector is instrumented. Pivoting the grind toward these.
(G1/G2 30m run will confirm node-kill absence; instrumented Vector build in
flight adds Vector coverage + thread-pausing on the buffer.)
| 0d97fdfb6f8511051f2078aa1cd76341-54-9 | vector-diskbufv2-g1g2-crash-durability | blt/antithesis-test-crash-durability | 30m | G1/G2: crash-durability (at-least-once) + writer-progress (#21683 deadlock probe) | **COMPLETED.** `post-recovery write makes progress` PASS (no deadlock — expected, no node-kill). `every end-to-end-acked event ... reaches the collector` **FAILING (116 counterexamples)** — potential acked-but-not-delivered silent loss, BUT collector returned 200 unconditionally → possible false-ack artifact. No-fault local repro = 430/430 clean → fault-specific. Hardened collector (G6) verifies. |
| bce984c7ec91cc4c2704b265e7c75d15-54-9 | vector-diskbufv2-g3-gauge-sanity | blt/antithesis-test-metrics-sanity | 30m | G3 (cumulative): produce + durability/progress check + buffer-gauge-sanity anytime invariant (u64 underflow ~1.8e19) | **COMPLETED.** gauge-sanity PASS (no 2^64 underflow gauge — needs drained restart/node-kill, not reached). post-recovery-progress PASS. durability assertion FAILS (same as G1/G2 — same unhardened-collector caveat). |
| 597228a5ef207e0e37f858d10099643d-54-9 | vector-diskbufv2-g4-instrumented | blt/antithesis-sut-instrumentation | 30m | G4: instrumented Vector + thread-pausing on buffer + precise underflow assert | **COMPLETED.** ✓ Vector instrumented (642,862 locs), ✓ thread-pausing on `vector`, ✓ `#21683 underflow assert` reachable + **PASS** (held). FAIL: post-recovery-progress + durability — assessed as **oracle artifacts** (probe sits behind a full-256MB-buffer backlog with only a 45s window = slow-drain, not deadlock; + unconditional-200 collector). Need oracle fixes. |
| 6667a495ea9045d64d7bcbcc881894a1-54-9 | vector-diskbufv2-g6-durability-hardened | blt/antithesis-harden-collector-oracle | 20m | G6: re-run durability with HARDENED collector (200 only on parsed+recorded body). Settles real-bug vs artifact. | awaiting completion |

## Oracle-artifact findings (G4 triage)

Two of my workload assertions produce **false failures** and must be fixed before their failures can be trusted as Vector bugs:

1. **durability** (`acked event reaches collector`): collector returned HTTP 200 unconditionally → fixed in G6 (hardened: 200 only when parsed+recorded).
2. **post-recovery progress** (`no permanent writer deadlock`): the single-event probe waits only 45s, but after a long fault window the 256MB buffer has a large backlog the probe sits behind → times out on slow drain, NOT a real deadlock. **Fix needed:** redefine progress as "buffer_events drains toward 0 within a generous bound" or "produced count advances post-recovery", not a single bounded probe.

**Trustworthy (artifact-free) signals:** G5 drop_newest (#24606, metric-only) and the gauge-sanity + underflow SUT asserts (which all currently PASS).
| 597228a5ef207e0e37f858d10099643d-54-9 | vector-diskbufv2-g4-instrumented | blt/antithesis-sut-instrumentation | 30m | G4: INSTRUMENTED Vector (sancov coverage + SUT-side underflow assert + thread-pausing on buffer reader/writer). Concurrency exploration + precise #21683 signal | submitted 2026-05-29 ~01:2x (202); awaiting completion |
| afacfcd0d7564db702fa8b0ed88de961-54-9 | vector-diskbufv2-g5-dropnewest-miscount | blt/antithesis-test-drop-newest | 30m | G5: when_full=drop_newest; asserts component_discarded reflects buffer_discarded (#24606) | **COMPLETED.** `#24606` assertion PASS but **VACUOUS**: `drop_newest actually dropped events` (Sometimes) FAILED = the 256MB buffer **never filled** → drop_newest never fired → precondition unmet. Fix: slow/failing collector to fill the buffer, then re-test #24606. |

---

## Triage summary & conclusions (2026-05-29)

**9 runs launched (G0–G9), 8 triaged.** Fully instrumented harness: Vector built
with sancov (642,862 coverage locations) + thread-pausing on the buffer
reader/writer + a SUT-side `assert_always` at the `ledger` `total_buffer_size`
decrement (the precise #21683 site); workload instrumented (~20K locations).

### What HELD (Vector behaving correctly)

- **`#21683` underflow assert** (`ledger total_buffer_size decrement never
  underflows`): reachable + **PASS** in the instrumented runs.
- **buffer gauge-sanity** (no ~1.8e19 u64-underflow gauge): **PASS**.
- No deadlock/stall observed that wasn't an oracle artifact.

### Every failing assertion traced to a TEST/ORACLE artifact (not a Vector bug)

1. **`every end-to-end-acked event reaches the collector`** — failed in G1/G2/G3/
   G4/G6/G8. Root causes, peeled one by one: (a) collector returned HTTP 200
   unconditionally → hardened to 200-only-when-parsed; (b) drain-wait stopped
   before the buffer was empty → hardened to wait `buffer_events==0`; (c)
   **definitive:** concurrent `produce` processes append to one `acked.log`, and
   non-atomic interleaved writes corrupt lines — G8 "missing" id was
   `p33b24b15-42p39fd2605-23` (two ids concatenated) with `delivered>acked`. Also
   the source-level `acknowledgements` is **deprecated** and acks on
   acceptance/buffering, not e2e delivery, so "acked" ≠ "delivered". → NOT a
   Vector bug.
2. **`post-recovery write makes progress`** — failed only with a full buffer +
   throttled collector: the single 45s probe sits behind a 256MB backlog =
   slow-drain, not a permanent deadlock. Artifact. (The real deadlock #21683
   needs node-kill anyway.)

### Why the marquee bugs were not demonstrated

- **`basic_test` injects NO node-termination faults** (G0 events: network +
  thread-pausing only; `reboot`/`stop`/`kill` queries empty). The crash-class
  bugs (#21683 deadlock, torn-tail recovery, crash-durability) all require Vector
  to be killed mid-write → **unreachable under this webhook**.
- **#24606 (drop_newest miscount)**: real bug in code, but needs a FULL buffer.
  G5 (no throttle) and G7 (3s collector delay) never filled the 256MB buffer
  (`drop_newest actually dropped` Sometimes = never true). G9 blocks the collector
  (120s) to force buffer-full — verdict pending.

### Net finding

Under the fault surface `basic_test` exposes (network + thread-pausing), at the
loads tested, the disk buffer's real invariants held — **no disk-buffer bug
demonstrated**. The known crash-class bugs require a **node-kill-enabled webhook**;
the harness (instrumented Vector + SUT underflow assert + workload) is built and
ready to demonstrate them the moment node-kill is available. Honest position: did
not manufacture a failure; traced every red assertion to a test artifact.

### Known test-harness limitations to fix before trusting durability red

- per-process (not shared) `acked.log`/`delivered.log`, or atomic appends.
- configure `acknowledgements` on the SINK (modern e2e) not the deprecated source.
- progress probe should gate on `buffer_events==0` and bound by buffer drain time.

---

## FINAL CONCLUSION (2026-05-29) — 11 Antithesis runs (G0–G11) + local repros

**No Vector disk-buffer bug was demonstrated — and this is an evidence-backed
result, not a lack of rigor.** Every red assertion was traced to a workload/oracle
artifact; the genuine Vector invariants held; and the two real catalog bugs are
provably out of reach under this environment, with the precise reasons below.

### Real Vector invariants that HELD under all reachable faults (incl. thread-pausing on the instrumented buffer)

- `ledger total_buffer_size decrement never underflows` (#21683 SUT-side assert): reachable + PASS.
- buffer size-gauge sanity (no ~1.8e19 u64-underflow gauge): PASS.

### Why the two target bugs were not demonstrable here

1. **Crash-class (#21683 deadlock, torn-tail recovery, crash-durability):** require
   Vector to be **killed mid-write**. The `basic_test` webhook injects network +
   thread-pausing faults only — **no node-termination** (confirmed: G0 events have
   zero reboot/stop/kill). Hard environmental blocker; needs a node-kill-enabled
   webhook.
2. **#24606 (drop_newest component-metric miscount):** requires the disk buffer at
   `max_size`. The buffer has an **effective cap ≈ max_size − 128MB** (one
   data-file reserve), so a 256MB buffer plateaus at ~128MB and applies upstream
   **backpressure** rather than dropping under my load — `buffer_discarded` stays
   0. Confirmed identically across G5/G7/G9/G10/G11 + two local repros (exact same
   134,166,720-byte plateau, concurrency=1 made no difference). The drop_newest
   path is not reachable by overwhelming throughput in this harness.

### Artifacts found + fixed (so they can't masquerade as bugs)

- collector returned HTTP 200 unconditionally → hardened (200 only when parsed+recorded).
- durability drain-wait stopped before buffer empty → wait `buffer_events==0`.
- **concurrent `produce` processes corrupt `acked.log`** (G8 "missing" id
  `p33b24b15-42p39fd2605-23` = two ids concatenated, delivered>acked) — the real
  root cause of all "acked-not-delivered" reds.
- post-recovery probe sat behind a full-buffer backlog (slow-drain ≠ deadlock).
- deprecated **source-level acks** ack on acceptance, not e2e delivery.

### Deliverable (ready to demonstrate the bugs once unblocked)

- gt stack: 12 branches (research → setup → 8 test/instrumentation branches), not pushed.
- Fully instrumented Vector image: sancov coverage (642,862 locations) +
  thread-pausing on the buffer reader/writer + SUT-side `#21683` underflow assert;
  instrumented workload; env-driven config variants (when_full / src-acks /
  sink-concurrency / collector-delay).
- **To demonstrate the crash-class bugs:** launch the same stack via a
  node-kill-enabled webhook (the underflow assert + durability/gauge checks are
  already wired to catch them).
- **To demonstrate #24606:** drive drops via a route that reaches the sink-buffer
  drop path (e.g., a buffer whose effective cap is small relative to a sustained
  in-process write burst, or a unit/integration test) rather than HTTP throughput.

---

## ✅ BUG DEMONSTRATED — #24606 (drop_newest silent at component level)

After establishing that #24606's drop path is unreachable via HTTP backpressure
(the buffer applies upstream backpressure before the disk buffer hits max_size),
I demonstrated it **reproducibly via a focused test** (user's chosen approach),
which is deterministic and bypasses the load-generation wall.

- **Test:** `lib/vector-buffers/src/buffer_usage_data.rs::drop_newest_increments_buffer_metric_but_not_component_metric_issue_24606` (branch `blt/antithesis-demonstrate-24606`).
- **Mechanism:** drives the real reporter path `BufferUsageData::report` →
  `emit(BufferEventsDropped { intentional: true, reason: "drop_newest" })`, with a
  `metrics_util` `DebuggingRecorder` capturing emissions.
- **Result (PASS):** `buffer_discarded_events_total = 5` while
  `component_discarded_events_total = 0`.
- **Root cause (confirmed by grep):** `ComponentEventsDropped` is never referenced
  anywhere in `lib/vector-buffers/`, so the buffer drop path cannot surface drops
  to the component-level metric operators monitor for data loss → **silent data
  loss on dashboards.** Matches Vector #24606 / #24144.

Reproduce: `cargo test -p vector-buffers --lib issue_24606`.

### Final tally

- **#24606: DEMONSTRATED** (reproducible test).
- **Crash-class bugs (#21683 deadlock, torn-tail, crash-durability): NOT
  demonstrated — blocked by `basic_test` having no node-kill faults.** Harness +
  SUT-side underflow assert are built and ready for a node-kill-enabled webhook.

## ✅ BUG DEMONSTRATED — #21683 (total_buffer_size unsaturated underflow → writer deadlock)

The marquee deadlock root cause, demonstrated reproducibly via a focused test
(the crash-only Antithesis path couldn't reach it under basic_test's no-node-kill
fault menu).

- **Test:** `lib/vector-buffers/src/variants/disk_v2/tests/invariants.rs::ledger_total_buffer_size_decrement_underflows_issue_21683` (branch `blt/antithesis-demonstrate-21683`).
- **Result (PASS, release):** after `increment(10)` then `decrement(11)`,
  `get_total_buffer_size()` returns ~2^64 (wrapped) instead of 0 (saturated).
- **Consequence:** `is_buffer_full()` (`total_buffer_size + unflushed_bytes >=
  max`) then returns true forever → `ensure_ready_for_write` loops on
  `wait_for_reader()` → permanent silent writer deadlock. Matches #21683; PR
  #23561 fixed only the reporter gauge, not this control-path atomic.
- Reproduce: `cargo test -p vector-buffers --release --lib issue_21683`.

## Final outcome: TWO real disk-buffer bugs demonstrated reproducibly

- **#24606** — drop_newest drops are silent at the component metric level.
- **#21683** — total_buffer_size unsaturated decrement wraps → permanent writer deadlock.
Both via focused tests after establishing the crash-class path is blocked by
basic_test's missing node-kill faults. The full instrumented Antithesis harness
(Vector @642K coverage + SUT-side underflow assert + thread-pausing) remains ready
to demonstrate the crash-driven manifestations given a node-kill-enabled webhook.

---

## ✅ COMPLETE BUG LEDGER — all known disk-buffer bugs demonstrated (local repros)

Goal: "grind out all known bugs." All 7 confirmed code-level defects from the
research catalog are now demonstrated by reproducible tests. Full `vector-buffers`
suite: **85 passed, 0 failed** (release). The crash-driven *runtime* manifestations
(e.g. the #21683 deadlock under a real crash) additionally need a node-kill-enabled
webhook — the instrumented Antithesis harness is built and ready for that — but the
underlying defects are all demonstrated here deterministically.

| # | Bug | Test (gt branch) | Repro |
|---|-----|------|-------|
| 1 | **#24606** drop_newest drops silent at component metric | `buffer_usage_data.rs::drop_newest_increments_buffer_metric_but_not_component_metric_issue_24606` (`antithesis-demonstrate-24606`) | `cargo test -p vector-buffers --lib issue_24606` |
| 2 | **#21683** `total_buffer_size` unsaturated decrement wraps → writer deadlock | `invariants.rs::ledger_total_buffer_size_decrement_underflows_issue_21683` (`antithesis-demonstrate-21683`) | `cargo test -p vector-buffers --release --lib issue_21683` |
| 3 | `get_total_records` `0-1` underflow on drained buffer → ~2^64 event count | `invariants.rs::get_total_records_underflows_on_drained_buffer_issue_21683_metrics` (`antithesis-demonstrate-get-total-records-underflow`) | `cargo test -p vector-buffers --release --lib get_total_records_underflows` |
| 4 | **#24948** writer `Drop` without flush → silent loss of buffered events | `invariants.rs::writer_drop_without_flush_loses_buffered_events_issue_24948` (`antithesis-demonstrate-24948`) | `cargo test -p vector-buffers --lib issue_24948` |
| 5 | finalizer discards `BatchStatus` → rejected delivery silently acked | `acknowledgements.rs::rejected_delivery_still_advances_acks_finalizer_status_discard` (`antithesis-demonstrate-finalizer-status`) | `cargo test -p vector-buffers --lib finalizer_status_discard` |
| 6 | `reader.rs:524` `metadata.len()-bytes_read` underflow (truncation → #21683 wrap) | `invariants.rs::delete_completed_data_file_size_delta_underflows_reader_524` (`antithesis-demonstrate-reader524-underflow`) | `cargo test -p vector-buffers --release --lib reader_524` |
| 7 | `reader.rs:932` file-id-rollover compare not wrap-aware | `invariants.rs::file_id_rollover_compare_not_wrap_aware_reader_932` (`antithesis-demonstrate-file-id-rollover`) | `cargo test -p vector-buffers --lib file_id_rollover_compare` |

All are demonstrations only — **no Vector behavior was changed** (only tests + the
no-op SUT-side underflow assert added during the harness phase).

---

## D0 — DIRECT disk_v2 exerciser harness (new approach)

Pivot from full-Vector SUT to testing the buffer **directly**. Rationale: every
demonstrated bug is internal to `vector-buffers`; routing through Vector's
source→codec→topology→sink→ack machinery was pure state-space overhead, and it
prevented the buffer from ever filling/draining the way the bugs need.

**Harness** (`antithesis/config-direct/`, branch `antithesis-direct-exerciser`):

- One self-driving process IS the SUT (disk_v2 takes a per-dir advisory lock, so
  the workload must live in the buffer-owning process). `examples/disk_v2_antithesis.rs`
  opens a real disk_v2 buffer via the public `TopologyBuilder` API and runs
  randomized writer/reader activity under the Antithesis SDK RNG. Reader rides
  close behind writer so the reader↔writer-head boundary is frequently live.
- **Oracle = SUT-side `assert_always!`** inside `vector-buffers` (fire however the
  bad state is reached, `#[inline]` so they fold away in prod):
  - `ledger.rs` `decrement_total_buffer_size` — no underflow (#21683 root).
  - `ledger.rs` `get_total_records` — `wrapping_sub >= 1` (the `0-1` underflow).
  - `reader.rs:524` — `bytes_read <= metadata.len()` (size-delta underflow).
- Test template `diskbuf_direct/`: `first_wait_ready`, `eventually_progress`
  (liveness — a deadlocked buffer stalls delivery), `parallel_driver_safety_monitor`
  (handled<=produced mirror).

**Run:** `385cfc4df45b3c85567b9b7ef3d803ed-54-9` — basic_test, 30min, launched
2026-05-29T14:33Z. Targets the accounting-underflow cluster organically via
thread-pausing (basic_test still has no node-kill). Status: starting → (triage pending).

---

## D0 — RESULTS / TRIAGE (run 385cfc4df45b3c85567b9b7ef3d803ed-54-9)

Completed 2026-05-29T15:11Z (30min, basic_test). Report:
`(internal run report, not linked) (auth'd link in`snouty runs show`).

**HEADLINE — #21683 reproduced organically by Antithesis.** The SUT-side
`assert_always!` **"ledger total_buffer_size decrement never underflows (root of
# 21683)"** FAILED at 3 distinct simulation times (vtime 34.1, 130.0, 174.3; a
passing example at 14.7 — the rare-state signature). Antithesis drove the *real*
disk_v2 buffer into the total_buffer_size unsaturated-decrement underflow under
thread-pausing fault injection — no unsafe pokes, the organic control-path bug.

**Corroborating downstream failure.** `diskbuf_direct/parallel_driver_safety_monitor.sh`
(the externally-visible `handled <= produced` invariant) FAILED at vtime 205,
287, 293 — *after* the underflow times. Clean causal story: decrement underflow →
total_buffer_size wraps ~2^64 → accounting corrupt → buffer reports phantom
records → handled>produced. Two independent oracles, consistent ordering.

**Did NOT fire (passing) this run:**

- `get_total_records never underflows on a drained buffer` — needs the exact
  reader==writer drained boundary; not hit organically in 30 min.
- `reader data-file size delta never underflows (reader.rs:524)` — needs a
  truncated/torn data file; basic_test's faults didn't produce one.
- Liveness `eventually_progress.sh` PASSED; workload `disk_v2 never delivers more
  than produced` and the `record delivered end-to-end` reachable PASSED;
  setup_complete + bootstrap reachables all green.

**Noise / not SUT bugs:** meta-properties "Fault injector dropped/total packets =
0" show as Failing because the single-process SUT has no inter-container network
traffic to inject network faults into — thread-pausing is the operative fault
here. Confirmed network clog/partition/restore faults were attempted.

**Conclusion:** the direct-exerciser harness works end-to-end (libvoidstar +
sancov loaded, setup_complete, assertions evaluated) and **independently
reproduced #21683 via Antithesis** — complementing the local failing test
`ledger_total_buffer_size_decrement_should_saturate_not_underflow_issue_21683`.
The get_total_records / reader.rs:524 underflows remain local-test-only for now
(their preconditions are rarer); a longer run or fs-fault-heavier webhook would
likely surface them. Crash-class manifestations still await a node-kill webhook.
