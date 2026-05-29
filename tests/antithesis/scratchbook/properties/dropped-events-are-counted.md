---
slug: dropped-events-are-counted
type: Safety / Always
status: CURRENTLY VIOLATED (confirmed by #24606/#24144 and direct code inspection)
sut_commit: 049eec79b737450c4669b7f8aa1dd814551ec466
---

# Property 15: dropped-events-are-counted

## Catalog Entry

**Type:** Safety / Always

**Property:** When `when_full=drop_newest` intentionally drops an event because the buffer is
full, that drop is accounted at the component level — specifically `component_discarded_events_total`
— in addition to the buffer-level `buffer_discarded_events_total`. A user monitoring their
pipeline at the component level (the standard observability surface) must be able to observe
the drop.

**Invariant:** For every event dropped by the `drop_newest` policy:
`component_discarded_events_total{intentional="true"}` increments by the event count of the
dropped item. `buffer_discarded_events_total` incrementing without a corresponding
`component_discarded_events_total` increment is a violation.

**Current Status: VIOLATED.** Confirmed by GitHub issues #24606 and #24144, and verified by
direct code inspection. The call chain is:

1. `BufferSender::send` (`topology/channel/sender.rs:231-234`): when `WhenFull::DropNewest`
   and `try_send` returns `Some(item)` (item could not be sent), sets `was_dropped = true`.

2. `BufferSender::send` (`sender.rs:248-257`): if `was_dropped`, calls
   `instrumentation.increment_dropped_event_count_and_byte_size(count, size, true)`.

3. `BufferUsageHandle::increment_dropped_event_count_and_byte_size`
   (`buffer_usage_data.rs:193-206`): stores into `self.state.dropped_intentional` atomic
   counters. Does NOT call `ComponentEventsDropped::emit`.

4. On the next 2-second reporter tick, `BufferUsageData::report`
   (`buffer_usage_data.rs:316-327`): emits `BufferEventsDropped { intentional: true, reason:
   "drop_newest", ... }`.

5. `BufferEventsDropped::emit` (`internal_events.rs:177-243`): increments
   `buffer_discarded_events_total` and `buffer_discarded_bytes_total` (and updates buffer gauge).
   **It does NOT call `emit(ComponentEventsDropped::<INTENTIONAL> { ... })`.** There is no
   reference to `ComponentEventsDropped` anywhere in `lib/vector-buffers/` (confirmed by
   grep).

The result: `buffer_discarded_events_total` increments correctly, but
`component_discarded_events_total` stays 0. Any alert or dashboard based on the component-level
counter — which is the primary observability surface for Vector components — silently misses
all buffer-policy drops.

The `ComponentEventsDropped` type lives in `lib/vector-common/src/internal_event/component_events_dropped.rs`
and increments `CounterName::ComponentDiscardedEventsTotal`. It is used by sinks and
transforms, but not by the buffer layer. The fix would be to add a call to
`emit(ComponentEventsDropped::<INTENTIONAL> { count: ..., reason: "drop_newest" })` inside
`BufferEventsDropped::emit` or inside `BufferUsageData::report` at the point where
`dropped_intentional` is emitted.

**Antithesis Angle:**

Configure a pipeline with a disk buffer using `when_full: drop_newest` and a slow/paused
downstream (modeled as backpressure via Antithesis network/CPU fault injection):

1. Write events faster than the buffer can drain (by pausing the downstream sink or making it
   very slow).
2. Assert, via workload-side metric scraping, that when `buffer_discarded_events_total`
   increments (i.e. drops are occurring), `component_discarded_events_total` also increments
   by at least the same amount.
3. The invariant to assert in the Antithesis workload:
   `component_discarded_events_total >= buffer_discarded_events_total` at any stable point
   (allowing for the 2-second reporting lag of the buffer metrics reporter).

SUT-side: add an `assert_always!` inside `BufferUsageData::report` or `BufferEventsDropped::emit`
that fires after `buffer_discarded_events_total` is incremented to assert that
`component_discarded_events_total` has also been incremented. Alternatively, add the missing
`emit(ComponentEventsDropped...)` call and add an `assert_reachable!` to confirm the path is
exercised.

The 2-second reporter lag means the workload assertion must be written as: "eventually (within
a bounded window after drops stop), both counters match." Antithesis's time-control makes
this straightforward.

**Why It Matters:**

This is a blind spot in the primary observability surface. Vector operators monitor
`component_discarded_events_total` to detect data loss; they may not know about or monitor the
lower-level `buffer_discarded_events_total` counter. A pipeline silently dropping events under
backpressure shows 0 on the component dashboard while data is being lost. This is a known bug
(#24606, #24144) that appears to remain unaddressed as of the current commit
(`049eec79b`). The Antithesis property will both confirm the bug is present and provide a
regression test once fixed.

## Open Questions

1. **Is the fix intentionally deferred or just missed?** Issues #24606 and #24144 are open;
   there is no linked PR fixing `BufferEventsDropped::emit`. Confirm with the owning team
   before marking this as "known-unfixed" versus "recently fixed but not yet landed."

2. **Should `component_discarded_events_total` equal or only be bounded by
   `buffer_discarded_events_total`?** If the same event is counted at multiple buffer stages
   (e.g. overflow chain), the component counter might be expected to equal the total across all
   stages, not just the per-stage buffer counter. Clarify the intended semantics before writing
   the exact assertion.

3. **Does the 2-second reporting lag make the Antithesis assertion flaky?** The buffer metrics
   reporter ticks every 2 seconds (`buffer_usage_data.rs:405`), so `buffer_discarded_events_total`
   is not real-time — it is batched. If the workload checks immediately after injecting
   backpressure, both counters may be 0 and the test passes vacuously. The workload must
   either wait for a reporter tick or scrape after a delay. Antithesis's deterministic scheduling
   makes this tractable if the tick interval is modeled.

4. **Does `drop_newest` apply to disk buffers at all, or only to in-memory buffers?** The disk
   buffer writer's `try_write_record` is called from `SenderAdapter::try_send`
   (`sender.rs:69-83`), which is called from `BufferSender::send` for `DropNewest`. The disk
   buffer writer's `try_write_record` returns `Some(item)` when full (the item cannot be
   written). Confirm this code path is actually reachable for disk buffers, not only for
   `LimitedSender` in-memory variants. If disk-buffer `try_write_record` never returns the
   item (always blocks or errors), the `was_dropped` branch may be unreachable for disk buffers.

---

### Investigation Log

#### Is `drop_newest` reachable for the disk-buffer variant?

**Examined:** `lib/vector-buffers/src/variants/disk_v2/writer.rs:1166–1178` (`try_write_record` and `try_write_record_inner`).

**Found:** `try_write_record_inner` at writer.rs:1175–1178 checks `self.is_buffer_full()` at the top: if the buffer is full it immediately returns `Ok(Err(record))` — i.e., it returns the item back to the caller. `try_write_record` at writer.rs:1166–1168 maps this to `Ok(Some(record))`. This return value propagates up to `SenderAdapter::try_send` which sets `was_dropped = true`, triggering the `increment_dropped_event_count_and_byte_size` call. The `drop_newest` path is therefore reachable for disk buffers whenever `is_buffer_full()` returns true.

**Conclusion:** Confirmed reachable for the disk-buffer variant. The missing `ComponentEventsDropped` emission affects disk buffers, not only in-memory variants.

#### Is there a partial fix on another branch?

**Examined:** `lib/vector-buffers/src/variants/disk_v2/` (whole directory) via grep for `ComponentEventsDropped`.

**Not found:** No call to `ComponentEventsDropped` anywhere in `lib/vector-buffers/` (confirmed by grep returning no results). No evidence of a partial fix or in-progress work at commit 049eec79b. Issues #24606 and #24144 remain open and unlinked to any merged PR at this commit.

**Conclusion:** No partial fix is present at this commit. The missing `component_discarded_events_total` increment is an unaddressed gap as of 049eec79b.
