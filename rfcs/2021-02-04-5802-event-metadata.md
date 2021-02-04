# RFC 5802 - 2021-02-04 - Event Metadata

This RFC introduces a plan to associate persistent metadata with every Vector event, both logs and metrics.

## Scope

This RFC will cover the placement of the metadata, its handling, and
some potential contents. It will not comprehensively cover the specific
contents beyond what is required for the initial task, as that will
depend on implement requirements of the relevant features.

## Motivation

There are two upcoming features planned for Vector that require events
to be associated with additional data that must persist throughout the
life of an event:

1. End-to-end latency measurement
2. End-to-end acknowledgements

Both of these features require each event to be tagged with several
pieces of data. These datum are not part of the data read from the
source, nor part of what is sent to the sinks, but exist outside of the
normal stream.

## Internal Proposal

### Data Structures

The event metadata will be introduced as a new structure named
`EventMetadata`. This will contain fixed data elements, starting with
just the ingestion timestamp (which is distinct from the timestamp in
the event data) but will eventually be extended to contain the source
name, record identifiers, and possibly acknowledgement status:

```rust
struct EventMetadata {
    first_timestamp: DateTime,
    source_events: SmallVec<SourceEvent>,
    status: EventDeliveryStatus,
}

struct SourceEvent {
    timestamp: DateTime,
    source_name: String,
    event_id: String,
}

enum EventDeliveryStatus {
    …TBD…
}
```

To accommodate the metadata, the current `Event` enum will be renamed to
`EventData`, and a wrapper structure will replace that name:

```rust
struct Event {
    data: EventData,
    metadata: EventMetadata,
}

enum EventData {
    Log(LogEvent),
    Metric(Metric),
}
```

All of the methods on `EventData` will remain in place, but this will
require additional handling for both the creation of new events (since
the metadata will need to be filled in) and the "into" methods (`fn
into_log` and `fn into_metric`). Implementations for `Deref` and
`DerefMut` will simplify porting existing code to the new structure.

### Vector Protocol

The event protocol is used for both the transmission of events between
Vector instances and the storage of events in disk buffers. This
metadata is only relevant within a single instance of Vector, so should
not be transmitted across the wire. However, for the latter, the
protobuf definition will need to be expanded to include the metadata by
adding the appropriate structures.

```protobuf
message EventWithMetadata {
  EventWrapper data = 1;
  EventMetadata metadata = 2;
}

message EventMetadata {
  google.protobuf.Timestamp first_timestamp = 1;
  repeated SourceEvent source_events = 2;
  EventDeliveryStatus status = 3;
}

message SourceEvent {
  google.protobuf.Timestamp timestamp = 1;
  string source_name = 2;
  string event_id = 3;
}

enum EventDeliveryStatus {
  …TBD…
}
```

### Visibility

This metadata will not be visible to users, either through remap or JSON
transforms, unless a use case can demonstrate the need for it. Given the
above structure of the metadata, there are no user-modifiable parts.

## Doc-level Proposal

N/A

## Rationale

This addition is required to support the two features mentioned in the
motivation. These provide users with:

1. metrics about how long events are taking to reach their destination,
   which can be used in meta-analysis about the observation framework
   itself, and
2. assurance that events are fully retained between when they are
   generated and when they are stored and processed, but reducing the
   number of ways they may be lost in transit.

Once in place, this metadata may also enable additional features not yet
envisioned. For example, the VRL compiler collects type information on
fields, but this can be expensive for arrays. Transforms such as
`add_fields` could build a field type cache to improve the performance
of this feature.

The introduction of `SmallVec` in the metadata structure is driven by
the assumption that most events will have a single source throughout
their lifetime. `SmallVec` is a data structure that can inline a (fixed)
number of elements before allocating memory. By using this feature, this
common case can avoid an extra allocation and improve data locality.

## Prior Art

N/A

## Drawbacks

- The event data is a critical component of Vector. Any changes to this
  structure impacts virtually the entire system.
- All transforms that combine or duplicate events will have extra
  complications in handing the metadata. This particularly applies to
  the Lua and WASM scripting transforms.
- This metadata will increase the memory required by Vector for any
  given event flow.
- Recording, copying, and finalizing the metadata will increase the CPU
  usage.

## Alternatives

The following alternative components have been considered and discarded:

1. Reduce the event metadata to a UUID, and track the actual metadata in
   a separate map (or set of maps, as appropriate). This has the
   downside of requiring most of the work necessary to just attach the
   metadata itself to the event, as well as introducing both the
   overhead of map lookups and the possibility of causing memory leaks
   due to map entries not being purged, without any real benefit.
2. Use a dictionary (ie `HashMap`) of arbitrary values to hold the
   metadata. While some of the metadata may end up being optional, this
   adds overhead to all accesses, due to the hashing and conversion of
   the resulting data, as well as more work for the allocator.
3. Store each event source in an `Arc`. Since the event source is
   read-only, there is no need for an additional `Mutex` layer, and this
   can provide an easy performance boost when events are duplicated
   between multiple transforms or sinks. However, this adds the overhead
   of an extra allocation and reference counting for the `Arc`
   box. Since common usage patterns do not duplicate events, the costs
   outweigh the benefits on this one.

## Outstanding Questions

- What is the best way of uniquely identifying the source? Is a simple
  name adequate (since no two components may have the same name) or is a
  new unique identifier or serial number required to prevent confusion
  when configuration is reloaded?
- What data is needed to uniquely identify source messages in order to
  acknowledge them? The above provides for a `String`, into which any
  other data may be encoded, but this is likely less than ideal.

## Plan Of Attack

Incremental steps that execute this change. Generally this is in the form of:

- [ ]  Create the new structures with an empty stub `EventMetadata`.
- [ ]  Add optional instrumentation to full `Event` creation and cloning
       and benchmark the result to determine the impact of clone vs
       reference counting the metadata.
- [ ]  Add a simple timestamp to the metadata to ensure it is propagated
       from end to end.
- [ ]  Rework sources that use `Utc::now()` or equivalent to set a
       default timestamp to use this new metadata (optimization,
       optional).
- [ ]  Expand the timestamp into an array to support transforms like
       `reduce`.
- [ ]  Emit an internal event when events are dropped, which will create
       statistics about the time the event was in the system.
- [ ]  Add event source name/id info to the metadata.
