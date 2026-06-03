---
slug: dropped-events-are-counted
type: Safety / Always
status: CURRENTLY VIOLATED (confirmed by #24606/#24144 and direct code inspection)
sut_commit: 049eec79b737450c4669b7f8aa1dd814551ec466
---

# Property 15: dropped-events-are-counted

## Catalog Entry

**Type:** Safety / Always

**Property:** When `when_full=drop_newest` drops an event because the buffer is
full, the drop is accounted at the component level (`component_discarded_events_total`),
not only the buffer level (`buffer_discarded_events_total`). The component level is
the standard observability surface a user monitors.

**Invariant:** For every `drop_newest` drop,
`component_discarded_events_total{intentional="true"}` increments by the dropped
event count. `buffer_discarded_events_total` incrementing without a corresponding
`component_discarded_events_total` increment is a violation.

**Current Status: VIOLATED** (confirmed by #24606 and #24144 and code inspection).
Call chain:

1. `BufferSender::send` (`topology/channel/sender.rs`): `WhenFull::DropNewest` +
   `try_send` returning `Some(item)` sets `was_dropped = true`.
2. `BufferSender::send`: if `was_dropped`, calls
   `instrumentation.increment_dropped_event_count_and_byte_size(count, size, true)`.
3. `BufferUsageHandle::increment_dropped_event_count_and_byte_size`: stores into
   `self.state.dropped_intentional`. Does NOT call `ComponentEventsDropped::emit`.
4. Next 2-second reporter tick, `BufferUsageData::report`: emits
   `BufferEventsDropped { intentional: true, reason: "drop_newest", ... }`.
5. `BufferEventsDropped::emit`: increments `buffer_discarded_events_total` and
   `buffer_discarded_bytes_total` (and the gauge). **Does NOT call
   `emit(ComponentEventsDropped::<INTENTIONAL> { ... })`** — no reference to
   `ComponentEventsDropped` anywhere in `lib/vector-buffers/` (grep-confirmed).

Result: `buffer_discarded_events_total` increments, `component_discarded_events_total`
stays 0. Any alert/dashboard on the component counter — the primary surface — misses
all buffer-policy drops.

`ComponentEventsDropped` lives in
`lib/vector-common/src/internal_event/component_events_dropped.rs` and increments
`CounterName::ComponentDiscardedEventsTotal`. Used by sinks/transforms, not the
buffer layer. Fix: add `emit(ComponentEventsDropped::<INTENTIONAL> { count: ...,
reason: "drop_newest" })` in `BufferEventsDropped::emit` or `BufferUsageData::report`
where `dropped_intentional` is emitted.

**Antithesis Angle:**

Configure a disk buffer with `when_full: drop_newest` and a slow/paused downstream
(backpressure via Antithesis network/CPU faults):

1. Write faster than the buffer drains (pause or slow the sink).
2. Workload-scrape: when `buffer_discarded_events_total` increments,
   `component_discarded_events_total` increments by at least as much.
3. Invariant: `component_discarded_events_total >= buffer_discarded_events_total`
   at any stable point (allowing the 2-second reporter lag).

SUT-side: add an `assert_always!` in `BufferUsageData::report` or
`BufferEventsDropped::emit`, firing after `buffer_discarded_events_total` increments,
asserting `component_discarded_events_total` also incremented. Or add the missing
`emit(ComponentEventsDropped...)` plus an `assert_reachable!`.

The 2-second lag means the workload assertion is "eventually (within a bounded
window after drops stop), both counters match."

**Why It Matters:**

A blind spot in the primary observability surface. Operators monitor
`component_discarded_events_total`; the lower-level `buffer_discarded_events_total`
may go unwatched. A pipeline dropping under backpressure shows 0 on the component
dashboard while losing data. Known bug (#24606, #24144), unaddressed at `049eec79b`.
The property confirms the bug and serves as a regression test once fixed.

## Open Questions

1. **Fix deferred or missed?** #24606/#24144 are open with no linked PR fixing
   `BufferEventsDropped::emit`. Confirm with the owning team.
2. **Should the component counter equal or only bound the buffer counter?** With
   multi-stage buffering (overflow chain) the component counter might be expected to
   equal the total across stages. Clarify semantics before writing the assertion.
3. **Does the 2-second lag make the assertion flaky?** The reporter ticks every 2s
   (`buffer_usage_data.rs`), so the counter is batched. Checking immediately after
   injecting backpressure can see both at 0 and pass vacuously. The workload must
   wait for a tick or scrape after a delay.

---

### Investigation Log

#### Is `drop_newest` reachable for the disk-buffer variant?

`try_write_record_inner` checks `self.is_buffer_full()` at the top: if full it
returns `Ok(Err(record))`; `try_write_record` maps this to `Ok(Some(record))`,
which propagates to `SenderAdapter::try_send` setting `was_dropped = true` and
triggering `increment_dropped_event_count_and_byte_size`. CONFIRMED reachable for
disk buffers whenever `is_buffer_full()` is true — the missing emission is not
in-memory-only.

#### Is there a partial fix on another branch?

Grep for `ComponentEventsDropped` across `lib/vector-buffers/` returns nothing. No
partial fix or in-progress work at `049eec79b`; #24606/#24144 remain open and
unlinked to any merged PR.
