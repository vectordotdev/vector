# RFC 6517 - 2021-03-36 - End-to-end Acknowledgement

This RFC proposes a framework for tracking the flow of events such that the
source can acknowledge them only after they are successfully delivered.

## Scope

This RFC will cover:

* The functional requirements to support end-to-end acknowledgement

* The data model changes and potential impacts to support the above


This RFC will not cover:

* Any details of changes to individual sources, transforms, or sinks to
  necessary to support this feature.

## Motivation

Users want assurance that their data pipeline successfully delivers all the
events that are presented to it. To accomplish this, Vector needs to be able to
maintain information on each event tying it back to the source that originated
the event, so that source can correctly acknowledge those incoming events at
the right time.

## Examples

Vector contains a number of features that can cause complications with a naïve
implementation. As such, we need to itemize a number of scenarios to ensure our
proposed solution covers all the edge cases. These will start with the simplest
scenario and then introduce complicating factors.

### Single source through linear transforms to a single sink

This is the simplest of cases. For each event the source produces, the event is
only modified on its transport through the system. The set of transforms
represented here are all those not listed explicitly below. When the
destination sink finishes handling the event, a notification needs to be
delivered to the originating source indicating the delivery status. Thus, the
event needs to contain a reference to the originating source and a unique
message identifier. Different sources will have varying requirements on the
message identifier.

### Multiple sources to a single sink

When multiple sources are configured to feed into a single sink, that sink may
end up producing batches containing events from multiple sources. This does not
really require any additional data tracking, but it is necessary to model the
acknowledgements as flowing back to the source independently for each event,
not at a batch level.

### Single source to multiple sinks

If an event is to be sent to multiple sinks, we want the acknowledgement to
only be issued once it has been acknowledged by all of the sinks. This will
require the tracking be shared across all the relevant sinks (as opposed to
being cloned with the event), as well as having a counter or some other
mechanism for determining when all of the deliveries are completed. The
topology will need to be involved in setting up this sink counter in order to
avoid a race condition between handing the event off to the last sink and an
earlier sink concluding that all deliveries are completed.

### Combining transform (ie `merge`)

If the `merge` transform is configured in a topology, it can produce
events that are a combination of multiple events merged together. As
such, their tracking information will also have to be merged, containing
a reference to all of the events that were combined to produce the
transformed event. As a complication, this transform may be fed from
more than one source, resulting in merged events coming from multiple
sources as well.

### Splitting transform (ie `route`)

This transform has the same kind of effect as does sending a single
event to multiple sinks, with the distinction that this may send the
event to one, or several, or even no output pipelines (see below).

### Dropped events (ie `dedupe`, `filter`, or `sample` transforms)

Some transforms optionally drop events depending on a variety of
conditions. The only realistic action to take at that point is to mark
the event as having been delivered. It may be useful to maintain
separate “delivered” vs “dropped” status flags, though this would not
make a difference to the source acknowledgement.

### User-space transforms (Lua or WASM)

User space transforms may end up doing any of the above behaviours, in
addition to the possibility of creating entirely new events with no
relation to the source event. We could potentially automatically tag
events emitted by a script with the same tracking information as the
source event, but this may not work correctly if the script is combining
events. Similar concerns hold for marking events as delivered for
scripts that do not emit a resulting event. As such, we will have to
leave this behaviour in the hands of script writers, with Vector
providing such scripts with convenience library functions to assist with
the common tasks outlined above.

### Non-acknowledging source

Some sources are unable to provide acknowledgements at a protocol level
(for example, `apache_metrics`, `prometheus_parser`, or `stdin`).
Further, users of Vector may wish to selectively enable this feature
only selectively on certain sources. To minimize the overhead of this
feature, sources that cannot or should not provide acknowledgements
should not be required to participate in delivery notifications, as they
would only be discarding the acknowledgement.

### Sinks using a disk buffer

In order to deal with longer delivery delays and backpressure, sinks may
be configured to store events temporarily in a disk buffer. Since the
events are not actually delivered when they are buffered, the final
delivery status will have to be confirmed after the event is loaded back
from the buffer. The originating source, to be used to determine where
to provide the delivery status indicator, must be converted into a form
that may be serialized (ie string or integer index). However, the
configuration may be reloaded while the event is buffered in such a way
that the source name provided in the configuration may change, or a
different component may be substituted for the same identifier. As such,
the configured component name is not sufficient to uniquely identify the
source. Further, Vector may be stopped while the event is buffered and
then restarted with an identical configuration. At this point, the old
source will be distinct from the current source, despite having the same
configured name. As such, the persisted identifier for the source must
stay the same across configuration reloads, but must be different for
each run of Vector.

## Internal Proposal

There are two components to the proposal: the communication between
event finalization and the originating sources, and the resulting event
metadata required to track the event status.

### Event finalization

When instantiated, the topology will provide each source with a channel…
This provides both a unique identifier of the source that needs to
receive the event finalization status, but also the mechanism for
delivering the event identifier and status.

### Event metadata

Given the channel above, the following metadata will be added to events:

1.  A reference (`Arc`) to a originating source, which is a dual-purpose structure containing:

    1.  The finalization channel, to be used when actually delivering the status.

    2.  The unique source identifier, to be used when serializing the metadata.

2.  A event identifier, contained in a new enum type.


### Data structures

Since a given event may actually be a composition of multiple source
events, this must be a set of the above pairs of data.

```rust
enum EventId {
    Number(u64),
    SenderNumber(u64, u64),
    // additional variants as required
}

enum EventStatus {
    Delivered,
    Dropped,
    Failed,
}

struct EventFinalization {
    id: EventId,
    status: EventStatus,
}

struct OriginatingSource {
    receiver: tokio::sync::mpsc::Sender<EventFinalization>,
    identifier: Arc<Box<str>>,
}

struct EventSource {
    source: OriginatingSource,
    id: EventId,
}

impl EventSource {
    fn acknowledge(&self, status: EventStatus);
}

struct EventMetadata {
    // existing fields
    sources: Box<[EventSource]>,
}
```

### Source configuration

When building a source, the topology system will provide it with an
originating source identifier. The source is responsible for creating
its own MPSC channel pair to provide to events. This allows sources that
receive events in batches to create a separate channel for each batch,
and then `await` on the receiver to collect the finalization
events. Since the size of each batch is known in advance, this may be a
bounded channel with the length fixed to the size of the source batch.
When the last event in the batch is dropped, the sender will be closed
and the receiver will be signalled that all events are completed.

Since this is making the parameter list for `trait SourceConfig::build`
increasingly unweidly, a new context structure will be introduced to
carry the data.

```rust
struct SourceContext<'a> {
    name: &'a str,
    identifier: Arc<Box<str>>,
    globals: &'a GlobalOptions,
    shutdown: ShutdownSignal,
    out: Pipeline,
}

trait SourceConfig {
    async fn build(&self, context: SourceContext<'_>) -> crate::Result<sources::Source>;
}
```

## Doc-level Proposal

The following additional source configuration will be added:

```toml
# Global enable option
acknowledgements = true

[sources.my_source_id]
  # Per-source enable option
  acknowledgements = true
```

We would also need to document when acknowledgement happens for each source.

## Rationale

The above structure provides for several considerations:

1.  This the minimum amount of data that can be added to the metadata to
    fully support this feature.

2.  Each source element is fixed size and requires no additional
    allocations.

3.  If a source does not need or is not configured to require
    finalization, it will not contribute to the list of sources and so
    has no additional event overhead, and no additional allocations when
    the event is created.

4.  Sending the finalization status to the source does not require any
    lookups or topology traversal.

5.  No additional work is required to handle dropped sources due to
    topology reconfiguration, other than the expected checking for a
    closed channel when sending.

## Drawbacks

1.  This adds a substantial base size overhead to each event (minimum of
    two words), even for configurations that do not support or require
    end-to-end acknowledgement.

## Alternatives

1.  The set of sources could be stored in a more customary
    `Vec<EventSource>`. This provides for merging multiple sources with
    a minimum of code. However, the data added to the event structure
    then grows by an additional word.

2.  The originating source could be stored as simply the unique
    identifier string. This requires that all reporting of delivery
    status proceed through a dictionary lookup instead of simply sending
    it through a channel, increasing the run-time overhead.

3.  Due to the variety of source event identifiers, two other
    possibilities for the `SourceId` type would be either a `Box<dyn
    SourceIdTrait>` or a `Box<str>`. Both of these are only two words
    long, which will likely be smaller than the final size of `enum
    SourceId` once all required variants are covered. However, since
    both of these are variably sized, they would require an additional
    allocation, negating any benefits they would provide in flexibility.
    Additionally, while a string is a simple and well understood data
    type, storing the required data in a string requires additional
    serializing and deserializing steps to use the data.

## Outstanding Questions

1. Is this feature intended to be opt-in to maximize performance in the
   default configuration, or opt-out to maximize reliability?

2. In most configurations, each event is likely to have a single source.
   Would it be better to store that single source inline (ie with either
   a `smallvec` of size 1, or an enum with variants of none, one, or an
   array of sources)? The would make each event larger but would
   eliminate the secondary allocation and indirection for all events
   with a single source. Some benchmarking may be instructive here, but
   it is difficult to measure the effects of data layout changes and
   other secondary effects.

## Plan Of Attack

  * [ ] Introduce `struct SourceContext`
  * [ ] Set up originating source identifiers
  * [ ] _TBD_
