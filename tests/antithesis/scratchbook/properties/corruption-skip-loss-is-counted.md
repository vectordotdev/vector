# Evidence: corruption-skip-loss-is-counted

**Slug:** corruption-skip-loss-is-counted
**Type:** Safety / `Always`
**Status:** Expected VIOLATED (corruption-skip loss is silent on standard metrics).

## Why this property exists (user concern)

The user is "seriously concerned about data loss," specifically the
checksum-skip path. Beyond *how much* is lost (`corruption-skip-loss-bounded`),
the second-order danger is that the loss is **silent** — invisible on the
metrics operators actually watch. The internal doc *"[technical report] Telemetry
correctness"* ((internal doc id omitted)) names this directly: *"silent data loss going
undetected because `component_discarded_events_total`…"* and lists
"A1. `component_discarded_events_total` blind to buffer drops [HIGH]" (#24606).
This property extends that concern from `drop_newest` to the corruption path.

## Distinct from `dropped-events-are-counted`

`dropped-events-are-counted` (#24606) is about `when_full = drop_newest`: the
`BufferSender` drop path increments `buffer_discarded_events_total` but never
`component_discarded_events_total`. That is the **write-side** drop.

This property is about the **read-side** corruption skip, a *different* code
path with *no* counting at all:

- `roll_to_next_data_file` (reader.rs:705-753) adds a deletion marker for the
  records **read** and abandons the rest. It never calls `track_dropped_events`.
- `track_dropped_events(events_skipped)` (reader.rs:650) is invoked only for
  **writer-side gap markers** in the ack-processing loop (reader.rs:596-650),
  i.e. data files the *writer* explicitly marked to skip — not reader-side
  corruption rolls.
- The abandoned records are therefore charged only to `decrement_total_buffer_size`
  via `delete_completed_data_file`'s `size_delta` (reader.rs:535 computes it; the
  reader.rs:529 underflow site asserts on it) — a *byte-accounting* adjustment,
  never an event-loss metric.

Net: corruption-skipped records increment **neither**
`buffer_discarded_events_total` **nor** `component_discarded_events_total`.
Strictly more silent than #24606.

## The invariant we want to test

`Always`: for every record abandoned by a corruption-triggered roll,
`component_discarded_events_total` (and/or `buffer_discarded_events_total`)
increases by the abandoned event count. Equivalently: after a corruption event
that abandons N valid records, `produced - delivered - counted_dropped == 0`.

## Antithesis angle

Same fault as `corruption-skip-loss-bounded` (early-record bit-flip in a
multi-record file). Oracle scrapes the metrics: assert the discarded-events
counter rose by the number of abandoned records once the roll completes. With
e2e acks the workload knows exactly which IDs were produced+synced and which
were delivered; the difference must equal the counted drops.

## Why it matters

A buffer marketed "at-least-once" silently discarding a whole data-file tail on
a single bit-flip — with **zero** signal on the standard component dashboard —
is the worst class of data loss: undetectable. Operators cannot alert on what
isn't counted. This is the read-side companion to the HIGH-severity #24606
finding in the Telemetry-correctness report.

## SUT-side instrumentation (MISSING)

In `roll_to_next_data_file`, after computing the abandoned span, emit a
`ComponentEventsDropped` (reason `"corruption_skip"`) for the abandoned events
and `assert_always!(component_discarded_increased)`. The Antithesis SDK is a
committed dependency under the `antithesis` feature and three underflow asserts
exist (ledger.rs:271/313, reader.rs:529; see existing-assertions.md), but
nothing in `lib/vector-buffers` references `ComponentEventsDropped` and no
corruption-skip event-loss counting exists today — that specific emit + assert
is genuinely still missing.

## Open Questions

- Are the abandoned records even *counted* internally anywhere (e.g. a debug
  log) that could be promoted to a metric, or is the count never computed?
  `(partial: roll_to_next_data_file computes data_file_record_count for READ
  records only; the abandoned count is not computed at all — the marker uses
  bytes_read, so the abandoned event count is never materialized)`
- Should corruption loss count as `component_discarded_events_total`
  (intentional vs unintentional flavor) or a new dedicated counter? Intentional
  vs error semantics affect which alert fires. `(needs human input)`

### Investigation Log

#### Is the abandoned-record count computed internally?

- Examined: `roll_to_next_data_file` (reader.rs:705-753), `track_dropped_events` callsite (reader.rs:650) and its ack-loop context (596-650), `delete_completed_data_file` size_delta (reader.rs:535, underflow assert 529).
- Found: the roll computes `data_file_record_count`/`data_file_event_count` for records **read**, and the deletion marker carries `(records_read, bytes_read)`. The abandoned (post-corruption) records are never enumerated; `track_dropped_events` fires only for writer-side gap markers in the ack loop, not here. So the abandoned event count is never materialized and no discarded-events metric is emitted. Tagged `(partial)` — confirmed not computed; only byte-accounting via decrement.

#### Intentional vs error counter semantics?

- Examined: `BufferEventsDropped` emit path (buffer_usage_data.rs / internal_events.rs), `ComponentEventsDropped` usage (absent in lib/vector-buffers).
- Not found: any existing classification for corruption loss. Conclusion: `(needs human input)` — which counter/flavor is a product/observability decision.
