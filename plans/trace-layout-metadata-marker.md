# Plan: Record the originating trace layout in event metadata

## Problem

Vector has two trace-producing sources, `datadog_agent` and `opentelemetry`. Both emit
`TraceEvent`, which is a newtype over `LogEvent`, and each uses its own undocumented key layout.
The two layouts share some spellings (both expose `spans`) while meaning different things, and
neither is self-describing.

A downstream transform or sink receiving trace events from a fan-in of both sources has no reliable
way to tell which layout a given event carries. `EventMetadata.source_type` cannot answer it: the
source pump at `src/topology/builder.rs:1040` sets `source_type` on every emission to the type of
the component doing the emitting, so a trace that crosses a `vector` sink-to-source hop arrives
downstream reading `vector`, with the originating trace source unrecoverable.

## Proposal

Have each trace-producing source write a static marker identifying its layout into
`EventMetadata.value`, under the read-only `vector` namespace — for example
`vector.trace_layout` with values `datadog_agent` and `opentelemetry`.

That location has the properties the marker needs, and they are properties no other existing field
has together:

- It is serialized with the event, so it survives fan-in, disk buffers, and `vector` source/sink
  hops unchanged.
- It is read-only to VRL. `compile_vrl` in `lib/vector-core/src/vrl.rs:23` calls
  `set_read_only_path` on `metadata."vector"`, so a transform between source and sink cannot delete
  or overwrite it. The comment on that call already describes the intended use: the namespace
  "contains metadata that transforms / sinks may rely on".
- It is not rewritten by the topology, unlike `source_type`.

## Why this is worth doing on its own

The marker serves consumers inside the Vector codebase: a transform or sink that needs to dispatch
on trace layout can read it rather than inferring the layout from key shapes. Because it is not
published to users (see "Stability" below), it offers no configuration-level benefit today, so the
case for shipping it rests on the timing argument that follows.

A durable origin marker is only useful once it is present in events written by a released version.
Anything that later wants to identify a trace event's layout can only rely on the marker for events
produced by versions that already emit it. Shipping it early is what makes it available later;
shipping it late means a correspondingly late date before it can be depended upon. The cost of
shipping it now is small and the cost of delay accrues in calendar time regardless of what else
happens.

## Stability

The marker is for internal consumers only. It is not documented for users, its presence and value
set are not a compatibility contract, and it may change or be removed without a deprecation cycle.
Record that intent in a doc comment on the path constant so a later reader does not mistake it for
a published field.

This does not make the marker invisible. The `vector` metadata namespace is read-only to VRL but it
is *readable*, so a user who goes looking will find it and may come to depend on it. Declaring it
unstable states the intent; it does not enforce it.

The name is durable regardless, because events carrying it outlive any rename: a value written by
one version is read by a later one out of a disk buffer or across a `vector` hop. Renaming later
means accepting both spellings for as long as older events can still arrive, so the name warrants a
deliberate choice at implementation time even though it is unpublished.

## Scope

In scope:

- Set the marker in `src/sources/datadog_agent/traces.rs` and the trace paths of
  `src/sources/opentelemetry/`.
- A shared constant for the metadata path and one for each source's value, so consumers and
  producers cannot disagree by typo.
- A doc comment on the path constant recording that the marker is internal and unstable.

Out of scope:

- Any consumer of the marker. This plan only produces it.
- User-facing documentation, and any published guarantee about the marker's presence or values.
- Any change to either source's trace key layout.
- Extending the marker to log or metric events.

## Approach

1. Define the path constant and the per-source value constants in `vector-core` alongside the
   existing metadata helpers, not in the sources.
2. Set the marker at the point each source constructs a `TraceEvent`, before it reaches the source
   sender. In `datadog_agent` that is the conversion in `traces.rs`; in `opentelemetry` it is the
   equivalent trace construction path. Both sources have more than one construction site, so verify
   coverage rather than assuming a single choke point.
3. Confirm the marker survives a `vector` sink-to-source round trip and a disk buffer round trip.
   This is the property the whole change exists for and it is not obvious from inspection.

## Testing

- Unit: each source's emitted trace events carry the expected marker, at every construction site.
- Integration: a `datadog_agent` trace relayed through a `vector` sink into a `vector` source
  arrives with the marker intact and with `source_type` reading `vector`, demonstrating the
  difference between the two fields.
- Disk buffer round trip preserves the marker.
- A `remap` transform cannot delete or overwrite it, confirming the read-only guarantee holds for
  this path in practice and not just by construction.

## Risks

- **De facto dependence despite the unstable declaration.** The marker is readable from VRL, so
  users may discover and rely on it whatever its documented status. A later rename or removal may
  therefore break someone even though nothing was promised.
- **Incomplete coverage is worse than absence.** A marker that is present on most trace events and
  missing on some is harder to consume correctly than one that is never present, because consumers
  will assume presence. Coverage of every construction site in both sources is the correctness
  requirement here.

## Deliverables

- One PR adding the constants and setting the marker in both sources, with tests.
- No user-facing documentation and no published guarantee. Whether an internal-only metadata
  addition warrants a changelog fragment is a judgment call; if one is added, it should describe
  the field as internal and unstable.
