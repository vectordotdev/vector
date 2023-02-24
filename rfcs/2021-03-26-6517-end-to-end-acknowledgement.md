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

## Discussion

Vector contains a number of features that can cause complications with a naïve
implementation. As such, we need to itemize a number of scenarios to ensure our
proposed solution covers all the edge cases. These will start with the simplest
scenario and then introduce complicating factors.

In the discussion, the term "finalization" is used to describe the point
at which an event completes its trip through the pipeline, which may be
caused by its acknowledged delivery, a permanent failure, or a situation
causing the event to be dropped in processing.

### Single source through linear transforms to a single sink

This is the simplest of cases. For each event the source produces, the
event is only modified on its transport through the system. The set of
transforms represented here are all those not listed explicitly
below. When the destination sink finishes handling the event, a
notification needs to be delivered to the source indicating the
finalization status. Thus, the event needs to contain a reference to the
source and a unique message identifier. Different sources will have
varying requirements on the message identifier.

### Multiple sources to a single sink

When multiple sources are configured to feed into a single sink, that sink may
end up producing batches containing events from multiple sources. This does not
really require any additional data tracking, but it is necessary to model the
statuses as flowing back to the source independently for each event,
not at a batch level.

### Single source to multiple sinks

If an event is to be sent to multiple sinks, we want the finalization status to
only be issued once it has been finalized by all of the sinks. This will
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
a reference to all the sources of the events that were combined to produce the
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
should not be required to participate in finalization handling, as they
would only be discarding the status.

### Sinks using a disk buffer

In order to deal with longer delivery delays and backpressure, sinks may
be configured to store events temporarily in a disk buffer. This may
cause acknowledgements to follow one of two paths, depending on the
configuration:

1. Since the events will no longer be lost in the event of a crash, they
   may be finalized as soon as they are persisted to the buffer. In this
   case, buffer will handle the finalization, and the finalization data
   will be stripped before the event is serialized. However, the buffer
   will need to be modified to handle acknowledgement, and only purge
   events from the buffer once they are delivered. As such, new
   finalization tracking will be added to the events on reload so the
   buffer can track their status.

2. Since the events are not actually delivered when they are buffered,
   the final delivery status will have to be confirmed after the event
   is loaded back from the buffer. The source, to be used to determine
   where to provide the finalization status indicator, must be converted
   into a form that may be serialized (ie string or integer
   index). However, the configuration may be reloaded while the event is
   buffered in such a way that the source name provided in the
   configuration may change, or a different component may be substituted
   for the same identifier. As such, the configured component id is
   not sufficient to uniquely identify the source. Further, Vector may
   be stopped while the event is buffered and then restarted with an
   identical configuration. At this point, the old source will be
   distinct from the current source, despite having the same configured
   name. As such, the persisted identifier for the source must stay the
   same across configuration reloads, but must be different for each run
   of Vector.

## Internal Proposal

There are two components to the proposal: the communication between
event finalization and the sources, and the resulting event
metadata required to track the event status.

### Event finalization

When instantiated, the topology will provide each source with a unique
identifier token. The source may use this identifier to create
one or more notification channels, each with their own unique
identifier. This provides both a unique identifier of the source that
needs to receive the event finalization status as well as the mechanism
for delivering the event identifier and status.

Event finalization is a three step process:

1. When a sink completes delivery of an event, the delivery status is
   recorded in the finalizer status that is shared across all clones of
   the event. This may change that status from `Dropped` (the
   initialization state) to either `Delivered`, `Errored`, or `Failed`.
   The `Recorded` state is never changed.
2. If one of those sinks is configured to be authoritative, it will
   immediately update the status of all its source batches and update
   the event status to `Recorded` that no extraneous updates happen.
   Otherwise, the last copy of the event does this status update when
   the shared finalizer is dropped.
3. When the last event if a batch is finalized, the status of that batch
   is sent *once* to the source via a one-shot channel. This signals the
   source to acknowledge the batch.

### Event metadata

The structure added to the event metadata is as follows:

1. Each event has an optional finalizer. When an event is fanned out to
   multiple transforms or sinks, the event is cloned to each
   destination, but each destination will share the same
   finalizer. Sources that do not handle acknowledgements will not
   initialize this finalizer, but it may be set subsequently if the
   event is merged with another that does require acknowledgements.
2. Each finalizer contains a status marker for the event and one or more
   shared references to source batch notifiers. When an event is merged
   with another, their source batches will be combined here. All events
   in the batch will be acknowledged when the last event in the batch is
   finalized. Sources that receive events individually will need to
   create a "batch" for each event.
3. The batch notifier contains a one-shot channel to the source that
   originated the batch, as well as the current status for the batch and
   a unique identifier. The identifier is used to reinstantiate the
   channel after an event is serialized.

### Data structures

```rust
struct EventMetadata {
    // … existing fields …
    finalizers: Box<[Arc<EventFinalizer>]>,
}

struct EventFinalizer {
    status: EventStatus,
    source: Arc<BatchNotifier>,
    identifier: Uuid,
}

struct BatchNotifier {
    status: Mutex<BatchStatus>,
    notifier: tokio::sync::oneshot::Sender<BatchStatus>,
    identifier: Uuid,
}

enum BatchStatus {
    Delivered,
    Errored,
    Failed,
}

enum EventStatus {
    Dropped, // default status
    Delivered,
    Errored,
    Failed,
    Recorded,
}
```

### Source configuration

When building a source, the topology system will provide it with an
unique source identifier. The source is responsible for creating its own
MPSC channel pair to provide to events. This allows sources to create a
separate channel for each sender or batch, as convenient for the source
protocol, and then `await` on the receiver to collect the finalization
events. Since the size of each batch is known in advance, this may be a
bounded channel with the length fixed to the size of the source batch.
When the last event in the batch is dropped, the sender will be closed
and the receiver will be signalled that all events are completed.

Since this is making the parameter list for `trait SourceConfig::build`
increasingly unwieldy, a new context structure will be introduced to
carry the data.

```rust
struct SourceContext<'a> {
    name: &'a str,
    identifier: Box<str>,
    globals: &'a GlobalOptions,
    shutdown: ShutdownSignal,
    out: Pipeline,
}

trait SourceConfig {
    async fn build(&self, context: SourceContext<'_>) -> crate::Result<sources::Source>;
}
```

### Sink configuration

A new global configuration setting will be added to all sources,
flagging one or more sinks as "authoritative". This setting will change
the behavior of finalization as described above. When an event is
delivered to an authoritative sink, its finalization status is
immediately delivered to all sources of the event. In order to prevent
another status from being delivered at a later point, the status of the
event is changed to "no-op", the same as if it was merged into another
event.

```rust
struct SinkOuter {
    // … existing fields …

    #[serde(default)]
    authoritative: bool,
}
```

## Doc-level Proposal

A new `acknowledgements` setting will be added to the configuration, at
both the global level and for each source, to control how
acknowledgements are done for sources. It is a boolean defaulting to
`true` indicating that the source will participate in end-to-end
acknowledgements.

Additionally, a new `authoritative` setting will be added to the
configuration at the sink level to control which sink is authoritative
for acknowledgements. It is a boolean defaulting to `false`. If no sink
indicates it is authoritative, all sinks must finalize the event before
an acknowledgement may be sent, as described above.

Finally, a new `acknowledgements` option will be added to the buffer
configuration to control if events will be acknowledged when they are
persisted to the buffer. It is a boolean defaulting to `false`.

```toml
# Global enable option
# Defaults to `true`
acknowledgements = true

[sources.my_source_id]
  # Enable or disable waiting for acknowledgements for this sink.
  # Defaults to the global value of `acknowledgements`
  acknowledgements = true

[sinks.my_sink_id]
  # Treat this sinks' acknowledgements as authoritative for the event.
  # Defaults to `false`
  authoritative = true

  # Enable or disable acknowledging events when they are buffered.
  # Defaults to `false`
  buffer.acknowledgements = true
```

A new class named `acknowledgements` will be added to the source
reference documentation which will be used to describe if and how the
source handles acknowledgements.

## Rationale

The above structure provides for several considerations:

1.  This the minimum amount of data that can be added to the metadata to
    fully support this feature, amounting to a single shared reference
    as the `Option` is optimized into the `Arc`.

2.  The use of `Arc` reference counting for finalization prevents events
    from "escaping" without providing a status indication.

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

1.  This adds a base size overhead to each event, even for
    configurations that do not support or require end-to-end
    acknowledgement.

## Alternatives

1.  The set of sources could be stored in a more customary `Vec`. This
    provides for merging multiple sources with a minimum of
    code. However, merged events already have other overhead, and it
    increases the data required for this array by an additional word.

2.  The source could be stored as simply the unique identifier
    string. This requires that all reporting of finalization status
    proceed through a dictionary lookup instead of simply sending it
    through a channel, increasing the run-time overhead.

## Outstanding Questions

## Plan Of Attack

* [ ] Introduce `struct SourceContext`
* [ ] Set up unique source identifiers
* [ ] Set up source metadata and event drop handling
* [ ] Modify sinks to provide finalization notification
* [ ] Modify sources to provide finalization handling
