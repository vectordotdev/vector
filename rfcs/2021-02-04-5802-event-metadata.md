# RFC 5802 - 2021-02-04 - Event Metadata

This RFC introduces a plan to associate persistent metadata with every
Vector event, both logs and metrics.

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
    source_name: SmolStr,
    event_id: SmolStr,
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
message EventWrapper {
  oneof event {
    Log log = 1;
    Metric metric = 2;
  }
  # Reserve 3 for traces
  EventMetadata metadata = 4;
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

When reading an event from a buffer written by a previous version of
Vector, the `metadata` field of `EventWrapper` will not be present. The
buffer reader will fill in the metadata with stub values containing the
current time and a null source, indicating the event has no known
source.

### User Scripting

Vector supports three scripting languages which need special
consideration with regards to metadata.

VRL is the simple case here. Since it currently does not support
creating or destroying events, no metadata support is required. It may
become desirable to give users access to read the data, but that will be
handle as the need arises and is beyond the scope of this proposal.

Scripts written in Lua and WASM, on the other hand, are complete black
boxes to Vector. They allow users to merge, split, destroy, and create
events completely from scratch. As such, additional support will need to
be added to both script environments to handle the metadata.

#### Lua

Lua scripts, as of version 2, are passed an event structure that has log
data in a `log` field (ie `event.log["the_field"]`), with metric data
similar. This will make it simple to both add the metadata as a new
field to the exposed event, and to require it when passed to the `emit`
function parameter. Additional functions will be provided to the script
to clone and merge the metadata.

#### WASM

The WASM transform has several limitations that make it difficult to
support either exposing or copying metadata: it only supports log
events, and it exposes the log event data using a naïve JSON conversion
with no prefix. It will require the addition of a wrapper layer much
like the Lua transform. This will necessarily be a breaking change.

Current event data exposed to WASM transforms and required by the `emit`
function:

```json
{
  "message": "Something happened",
  "timestamp": "2021-02-08T11:11:11+00:00"
}
```

Proposed:

```json
{
  "log": {
    "message": "Something happened",
    "timestamp": "2021-02-08T11:11:11+00:00"
  },
  "metadata": {
    "first_timestamp": "2021-02-08T12:23:34+00:00"
  }
}
```

### Visibility

Other than as described above, this metadata will not initially be
visible to users, either through remap or JSON transforms, unless a use
case can demonstrate the need for it. Given the above structure of the
metadata, there are no user-modifiable parts.

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

## Performance Considerations

The addition of metadata will necessarily impact performance, since it
will grow the size of each event. This will in turn require extra
operations when creating and cloning events.

Since this is the key performance sensitive data structure, every step
of this implementation will be marked with specific benchmarks to ensure
any performance losses are minimized. For example, the initial stub
metadata should be entirely performance neutral. Careful testing will be
necessary to validate implementation choices against known alternatives.

The introduction of `SmallVec` is driven by the assumption that most
events will have a single source throughout their lifetime. `SmallVec`
is a data structure that can inline a (fixed) number of elements before
allocating memory. By using this feature, this common case can avoid an
extra allocation and improve data locality.

Similarly, the introduction of
[`SmolStr`](https://github.com/rust-analyzer/smol_str) is driven by the
fact that source name and event IDs are highly likely to be short, will
never be modified. Also, in the case of the source name, it will be
shared with the actual source and all other events originating from that
source, and `SmolStr` provides constant-time copy.

## Drawbacks

- The event data is a critical component of Vector. Any changes to this
  structure impacts virtually the entire system.
- All transforms that combine or duplicate events will have extra
  complications in handing the metadata.
- The user scripting transforms, Lua and WASM, will require extra work
  from script writers to handle the metadata.
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

## Plan Of Attack

Incremental steps that execute this change. Generally this is in the form of:

- [ ]  Create the new structures with an empty stub `EventMetadata`.
- [ ]  Add a simple timestamp to the metadata to ensure it is propagated
       from end to end.
- [ ]  Add instrumentation to full `Event` creation and cloning (gated
       by a feature flag) and benchmark the result to estimate the
       performance impact of clone vs reference counting the metadata.
- [ ]  Rework sources that use `Utc::now()` or equivalent to set a
       default timestamp to use this new metadata (optimization,
       optional).
- [ ]  Expand the timestamp into an array to support transforms like
       `reduce`.
- [ ]  Emit an internal event when events are dropped, which will create
       statistics about the time the event was in the system.
- [ ]  Add event source name/id info to the metadata.
