# Plan: Replace the vendored Datadog trace protobuf with the upstream definitions

## Problem

`proto/vector/dd_trace.proto` is a hand-maintained partial copy of the Datadog Agent's
agent-to-backend trace protobuf. It has drifted from upstream, and because protobuf silently
discards fields the local schema does not declare, the drift is invisible at runtime and shows up
as missing data.

Fields the Agent emits today that Vector's `datadog_agent` source cannot see:

| Message | Missing field | Consequence |
| ------- | ------------- | ----------- |
| `Span` | `repeated SpanLink spanLinks = 14` | span links dropped on ingest |
| `Span` | `repeated SpanEvent spanEvents = 15` | span events dropped on ingest |
| `TracerPayload` | `ContainerDebug containerDebug = 11` | container debug context dropped |
| `AgentPayload` | `bool rareSamplerEnabled = 10` | rare-sampler state dropped |

The local `Span` message stops at `meta_struct = 13`. Upstream also carries the
`AttributeAnyValue` / `AttributeArray` value types that `SpanEvent` depends on, which have no local
equivalent at all.

The same file also carries a dead decode path. `TracePayload` declares
`repeated APITrace traces = 3` and `repeated Span transactions = 4`, the pre-`tracerPayloads`
payload shape. `handle_dd_trace_payload` in `src/sources/datadog_agent/traces.rs:92` dispatches on
`tracer_payloads.is_empty()` into `handle_dd_trace_payload_v0` at line 208, which converts those
fields. Upstream's `AgentPayload` does not declare fields 3 and 4 at all.

## Why this is worth doing on its own

Dropping span links and span events is a data-loss bug in the `datadog_agent` source, reportable
and fixable without reference to anything else. Vendoring upstream fixes it and removes the
recurring obligation to notice upstream schema changes by hand — which is the mechanism that
produced the current gap.

The vendored upstream files are already present in the working tree at
`proto/datadog/trace/{agent_payload,tracer_payload,span,stats}.proto`, untracked.

## Sequencing constraint

These cannot be done in either order. Upstream `AgentPayload` has no `traces` or `transactions`
fields, so swapping the schema first breaks `handle_dd_trace_payload_v0` at compile time. Two
viable orderings:

**Remove the legacy path first, then swap** (recommended). The removal is a self-contained,
reviewable change against the existing schema; the swap afterwards is a pure substitution with no
dead code to carry across it. The cost is that the data-loss fix waits behind whatever deprecation
window the removal needs.

**Swap first, retaining a minimal legacy shim.** Keep a small local proto declaring only the
pre-`tracerPayloads` shape and attempt it as a fallback when `tracerPayloads` is empty. This
decouples the two completely and lets the data-loss fix ship immediately, at the cost of a
deliberately retained vestigial file and a second decode attempt on the empty-payload path.

Pick based on whether the legacy removal needs a deprecation window. If it does, the second
ordering is worth its cost, because the data-loss fix should not be gated behind a deprecation
cycle for unrelated functionality.

## Step 1: Remove the pre-`tracerPayloads` decode path

Delete `handle_dd_trace_payload_v0` (`traces.rs:208`) and the `transactions` fan-out at
`traces.rs:242`, and collapse the dispatch at `traces.rs:92-106` to the single current path.

The behavior change: a payload with empty `tracerPayloads` currently produces events via the
legacy conversion and would afterwards produce none. It must not do so silently — report it with
component error telemetry naming the unsupported payload shape, so an operator running an Agent old
enough to emit it gets a diagnosable signal rather than missing traces.

Determine the Agent version boundary before deciding on the window. `tracerPayloads` has been the
Agent's emitted shape for long enough that the legacy path may be unreachable in any supported
deployment, in which case a changelog entry suffices. If it is reachable, add a `deprecation.d/`
entry first and remove in a later release, following the existing entries in that directory as the
template.

## Step 2: Swap to the upstream schema

1. Track the four vendored files under `proto/datadog/trace/`. Record the upstream commit they were
   taken from, in a comment or a sibling note, so the next refresh is a diff rather than an
   archaeology exercise.

2. Resolve the `idx` dependency. Upstream `agent_payload.proto` declares
   `repeated idx.TracerPayload idxTracerPayloads = 11`, which references a package not among the
   vendored files. Either vendor that definition too or drop the field from the vendored copy. If
   it is dropped, note it explicitly in the file, since an undocumented local deletion is exactly
   the drift this change exists to eliminate. If it is vendored, decide what the source does with
   an indexed-only payload — most likely report it as an unsupported payload version and produce
   no events, while continuing to process any standard entries in the same payload.

3. Update `build.rs`: replace the `proto/vector/dd_trace.proto` entries at lines 137 and 165 with
   the vendored paths, and add `proto/datadog` to the include paths alongside the existing
   `proto/third-party` and `proto/vector` entries at lines 172-174.

4. Update the module in `src/sources/datadog_agent/mod.rs:17`. The generated module path changes
   from `dd_trace.rs` to the `datadog.trace` package output, and the top-level message renames from
   `TracePayload` to `AgentPayload`. Both are mechanical renames through `traces.rs`.

5. Delete `proto/vector/dd_trace.proto`.

## Step 3: Surface the newly available fields

Swapping the schema makes `spanLinks`, `spanEvents`, `containerDebug`, and `rareSamplerEnabled`
decodable but does not put them into the emitted event. Decide, per field, whether it is carried
into the trace event now or left decoded-and-discarded, and say which in the changelog. Adding
`spanLinks` and `spanEvents` to the event is the actual fix for the reported data loss; the other
two are lower value and can wait.

This step changes the shape of events the source produces, so it needs a changelog entry and is the
part most likely to warrant its own PR.

## Testing

- Fixture-based decode tests for a payload carrying span links and span events, asserting they
  survive to the emitted event.
- A payload with empty `tracerPayloads`, asserting zero events plus the expected error telemetry.
- Confirm the existing `datadog_agent` trace tests in `src/sources/datadog_agent/` pass unchanged
  across the schema swap; step 2 must be behavior-preserving for every currently supported payload.
- Regenerate or verify any committed protobuf fixtures affected by the package rename.

## Risks

- **The `idx` package dependency** is the only unknown in step 2 and should be resolved before
  committing to a sequencing choice.
- **Upstream comment volume.** The vendored files carry extensive `@gotags` annotations and doc
  comments. They are harmless but make the diff large; note in the PR description that the files
  are verbatim upstream so review can focus on the Rust changes.
- **Removing the legacy path is user-visible** for anyone still running an Agent old enough to hit
  it. Establish whether that population is empty before choosing the window.

## Deliverables

- PR: remove the pre-`tracerPayloads` decode path (plus a `deprecation.d/` entry if the window is
  needed).
- PR: vendor upstream protos, update `build.rs` and the generated module path, delete the local
  proto. Behavior-preserving.
- PR: carry span links and span events into the emitted trace event. Fix changelog fragment.
