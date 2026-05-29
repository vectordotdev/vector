# Antithesis Grind Plan — disk buffer v2 failure demonstrations

Working note (not a research artifact). Execution queue for the goal: stack
Antithesis tests, launch via `basic_test`, triage, demonstrate real disk-buffer
bugs. Each item = one gt branch + one (or more) launch/triage cycle. Ordered by
(reproducibility × value). "Phase 1" = workload-observable only; "Phase 2" =
needs SUT-side `antithesis_sdk` in `lib/vector-buffers` (rebuild Vector).

Faults available (user-confirmed): node-termination (kill/restart), persistent
buffer volume. Clock + custom faults: assume available, confirm at use.

## Pivot (2026-06-02): accounting-underflow cluster moves OUT of Antithesis

The #21683 accounting-underflow invariants (`total-buffer-size-never-underflows`,
`record-id-wraparound-accounting-holds`) are now slated as **in-tree `proptest`
property tests in `lib/vector-buffers`**, not Antithesis scenarios — they are
deterministically reproducible in-process, where Antithesis adds only
scheduler/coverage. See those two property files' "Test plan" sections for the
two tests (A: ledger-level `get_total_records` wrap, pure sync; B: reopen +
torn-tail `total_buffer_size` underflow, `current_thread` + `TestFilesystem`).
Antithesis retains the genuinely-distributed work: the `vector_to_vector_e2e_disk`
conservation experiment and the #24948 SIGHUP config-reload fault (both wired and
`snouty validate`-green at this commit).

## G0 — Bootstrap (setup task #1/#2/#3)

- Base harness green: vector healthy, workload `setup_complete` + reachable
  ("workload serve started", "event delivered end-to-end through disk buffer").
- Launch once via `basic_test`, triage: expect both reachables hit, "Software
  was instrumented" for the workload. Validates the whole pipeline.

## G1 — Durability / at-least-once under crash  [HIGH, Phase 1, node-kill]

Properties: durable-unacked-events-survive-crash, every-written-event-eventually-delivered.

- Workload: `produce` appends each sent id+timestamp to /shared/produced.log;
  collector appends delivered ids to /shared/delivered.log (shared tmpfs volume
  between... no — single workload container holds both; use one process or a
  shared file in the workload container).
- Vector config variant: `flush_interval: 0` (every flush = fsync) so the oracle
  is clean; e2e acks on.
- Fault: Antithesis node-kills vdbuf-vector repeatedly; persistent volume keeps
  the buffer.
- Check command (`eventually_` or quiet-period driver): every id produced
  >2×flush_interval ago must be in delivered (dups allowed). assert_always.
- Expected: should HOLD; a violation = real durability/recovery bug (strong find).

## G2 — Writer deadlock / no-progress (#21683)  [HIGHEST value, Phase 1 then Phase 2]

Properties: writer-eventually-makes-progress, total-buffer-size-never-underflows.

- Phase 1 (workload-observable compound stall detector): fill buffer; node-kill
  vector at rotation/partial-write moments; restart; resume writes. After a
  quiet period assert COMPOUND: produced-rate≈0 AND delivered-rate≈0 AND buffer
  >~90% AND duration>drain-bound ⇒ assert_unreachable("persistent_deadlock").
  Must use both rates (distinguish deadlock from healthy block backpressure).
- Phase 2 (precise signal): add antithesis_sdk to lib/vector-buffers; at
  ledger.rs:~292 decrement assert the value doesn't wrap (assert_always amount<=current);
  assert_unreachable on underflow. Rebuild Vector (release; no debug trace! panic).
- Needs many timelines + sustained writes; release build mandatory.

## G3 — record-id-wraparound empty-buffer 2^64 gauge  [MED-EASY, Phase 1]

Property: record-id-wraparound-accounting-holds (empty-buffer case).

- Workload: drain buffer fully (stop producing, let collector drain + ack);
  then trigger a vector restart (node-kill graceful or Antithesis restart);
  scrape buffer_size_events / buffer_size_bytes from :9598; assert ~0
  (assert_always small). Expected FAIL: gauge shows ~1.8e19 on drained restart.
- No node-kill strictly needed if a graceful restart can be driven; else use it.

## G4 — foreign .dat file stalls writer  [MED, Phase 1, no node-kill needed]

Property: foreign-data-file-no-writer-stall.

- Compose: also mount vdbuf-buffer into the workload container (ro? rw) so a
  test command can drop a large `foreign.dat` into /var/lib/vector/buffer/v2/<id>/.
- Test command places the file; vector restart picks it up (update_buffer_size
  sums all *.dat); assert writer still makes progress. Expected FAIL: stall.

## G5 — drop_newest not counted at component level  [MED, Phase 1]

Property: dropped-events-are-counted.

- Vector config variant: when_full: drop_newest; collector blocks/rejects so the
  256MB buffer fills (produce large events to fill faster).
- Scrape buffer_discarded_events_total vs component_discarded_events_total;
  assert equal. Expected FAIL: component stays 0 while buffer increments.

## G6 — sink-failure not silently acked  [MED, Phase 1]

Property: sink-failure-not-silently-acked.

- Collector returns 5xx for a window (workload-controlled). Assert events whose
  delivery errored are retained/retried, not dropped from the buffer.

## G7 — config-reload silent loss  [LATER, custom fault SIGHUP]

Property: config-reload-no-silent-loss. Needs SIGHUP-to-vector custom fault.

## G8 — fsync window under clock jitter  [LATER, clock fault]

Property: fsync-window-bounded-under-clock-jitter. Needs clock faults.

## Notes

- Each launch: `docker compose build` (only if images changed) → snouty validate
  → snouty launch --json --webhook basic_test --config antithesis/config
  --duration <mins>. Then triage by run id.
- Keep each test on its own gt branch stacked on antithesis-setup-harness.
- Do NOT fix Vector bugs — demonstrate + make reproducible.
- Multiple config variants (flush_interval, when_full) → either separate compose
  profiles or env-substituted vector.yaml. Decide per-test; simplest is a small
  set of vector-<variant>.yaml + compose overrides.
