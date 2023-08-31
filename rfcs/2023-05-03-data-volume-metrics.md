# RFC 2023-05-02 - Data Volume Insights metrics

Vector needs to be able to emit accurate metrics that can be usefully queried
to give users insights into the volume of data moving through the system.

## Scope

### In scope

- All volume event metrics within Vector need to emit the estimated JSON size of the
  event. With a consistent method for determining the size it will be easier to accurately
  compare data in vs data out.
  - `component_received_event_bytes_total`
  - `component_sent_event_bytes_total`
  - `component_received_event_total`
  - `component_sent_event_total`
- The metrics sent by each sink needs to be tagged with the source id of the
  event so the route an event takes through Vector can be queried.
- Each event needs to be labelled with a `service`. This is a new concept
  within Vector and represents the application that generated the log,
  metric or trace.
- The service tag and source tag in the metrics needs to be opt in so customers
  that don't need the increased cardinality are unaffected.

### Out of scope

- Separate metrics, `component_sent_bytes_total`  and `component_received_bytes_total`
  that indicate network bytes sent by Vector are not considered here.

## Pain

Currently it is difficult to accurately gauge the volume of data that is moving
through Vector. It is difficult to query where data being sent out has come
from.

## Proposal

### User Experience

Global config options will be provided to indicate that the `service` tag and the
`source` tag should be sent. For example:

```yaml
telemetry:
  tags:
    service: true
    source_id: true
```

This will cause Vector to emit a metric like (note the last two tags):

```prometheus
vector_component_sent_event_bytes_total{component_id="out",component_kind="sink",component_name="out",component_type="console"
                                        ,host="machine",service="potato",source_id="stdin"} 123
```

The default will be to not emit these tags.

### Implementation

#### Metric tags

**service** - to attach the service, we need to add a new meaning to Vector -
              `service`. Any sources that receive data that could potentially
              be considered a service will need to indicate which field means
              `service`. This work has largely already been done with the
              LogNamespacing work, so it will be trivial to add this new field.
              Not all sources will be able to specify a specific field to
              indicate the `service`. In time it will be possible for this to
              be accomplished through `VRL`.

**source_id** - A new field will be added to the [Event metadata][event_metadata] -
                `Arc<OutputId>` that will indicate the source of the event.
                `OutputId` will need to be serializable so it can be stored in
                the disk buffer. Since this field is just an identifier, it can
                still be used even if the source no longer exists when the event
                is consumed by a sink.

We will need to do an audit of all components to ensure the
bytes emitted for the `component_received_event_bytes_total` and
`component_sent_event_bytes_total` metrics are the estimated JSON size of the
event.

These tags will be given the name that was configured in [User Experience]
(#user-experience).

Transforms `reduce` and `aggregate` combine multiple events together. In this
case the `source` and `service` of the first event will be taken.

If there is no `source` a source of `-` will be emitted. The only way this can
happen is if the event was created by the `lua` transform.

If there is no `service` available, a service of `-` will be emitted.

Emitting a `-` rather than not emitting anything at all makes it clear that
there was no value rather than it just having been forgotten and ensures it
is clear that the metric represents no `service` or `source` rather than the
aggregate value across all  services.

The [Component Spec][component_spec] will need updating to indicate these tags
will need including.

**Performance** - There is going to be a performance hit when emitting these metrics.
Currently for each batch a simple event is emitted containing the count and size
of the entire batch. With this change it will be necessary to scan the entire
batch to obtain the count of source, service combinations of events before emitting
the counts. This will involve additional allocations to maintain the counts as well
as the O(1) scan.

#### `component_received_event_bytes_total`

This metric is emitted by the framework [here][source_sender], so it looks like
the only change needed is to add the service tag.

#### `component_sent_event_bytes_total`

For stream based sinks this will typically be the byte value returned by
`DriverResponse::events_sent`.

Despite being in the [Component Spec][component_spec], not all sinks currently
conform to this.

As an example, from a cursory glance over a couple of sinks:

The Amqp sink currently emits this value as the length of the binary
data that is sent. By the time the data has reached the code where the
`component_sent_event_bytes_total` event is emitted, that event has been
encoded and the actual estimated JSON size has been lost. The sink will need
to be updated so that when the event is encoded, the encoded event together
with the pre-encoded JSON bytesize will be sent to the service where the event
is emitted.

The Kafka sink also currently sends the binary size, but it looks like the
estimated JSON bytesize is easily accessible at the point of emitting, so would
not need too much of a change.

To ensure that the correct metric is sent in a type-safe manner, we will wrap
the estimated JSON size in a newtype:

```rust
pub struct JsonSize(usize);
```

The `EventsSent` metric will only accept this type.

### Registered metrics

It is currently not possible to have dynamic tags with preregistered metrics.

Preregistering these metrics are essential to ensure that they don't expire.

The current mechanism to expire metrics is to check if a handle to the given
metric is being held. If it isn't, and nothing has updated that metric in
the last cycle - the metric is dropped. If a metric is dropped, the next time
that event is emitted with those tags, the count starts at zero again.

We will need to introduce a registered event caching layer that will register
and cache new events keyed on the tags that are sent to it.

Currently a registered metrics is stored in a `Registered<EventSent>`.

We will need a new struct that can wrap this that will be generic over a tuple of
the tags for each event and the event - eg. `Cached<(String, String), EventSent>`.
This struct will maintain a BTreeMap of tags -> `Registered`. Since this will
need to be shared across threads, the cache will need to be stored in an `RwLock`.

In pseudo rust:

```rust
struct Cached<Tags, Event> {
  cache: Arc<RwLock<BTreemap<Tags, Registered<Event>>>,
  register: Fn(Tags) -> Registered<Event>,
}

impl<Tags, Event> Cached<Tags, Event> {
  fn emit(&mut self, tags: Tags, value: Event) -> {
    if Some(event) = self.cache.get(tags) {
      event.emit(value);
    } else {
      let event = self.register(tags);
      event.emit(value);
      self.cache.insert(tags, event);
    }
  }
}
```

## Rationale

The ability to visualize data flowing through Vector will allow users to ascertain
the effectiveness of the current use of Vector. This will enable users to
optimise their configurations to make the best use of Vector's features.

## Drawbacks

The additional tags being added to the metrics will increase the cardinality of
those metrics if they are enabled.

## Prior Art


## Alternatives

We could use an alternative metric instead of estimated JSON size.

- *Network bytes* This provides a more accurate picture of the actual data being received
  and sent by Vector, but will regularly produce different sizes for an incoming event
  to an outgoing event.
- *In memory size* The size of the event as held in memory. This may be more accurate in
  determining the amount of memory Vector will be utilizing at any time, will often be
  less accurate compared to the data being sent and received which is often JSON.

## Outstanding Questions

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Add the `source` field to the Event metadata to indicate the source the event has come from.
- [ ] Update the Volume event metrics to take a `JsonSize` value. Use the compiler to ensure all metrics
      emitted use this. The `EstimatedJsonEncodedSizeOf` trait will be updated return a `JsonSize`.
- [ ] Add the Service meaning. Update any sources that potentially create a service to point the meaning
      to the relevant field.
- [ ] Introduce an event caching layer that caches registered events based on the tags sent to it.
- [ ] Update the emitted events to accept the new tags - taking the `telemetry` configuration options
      into account.
- [ ] There is going to be a hit on performance with these changes. Add benchmarking to help us understand
      how much the impact will be.

## Future Improvements

- Logs emitted by Vector should also be tagged with `source_id` and `service`.
- This rfc proposes storing the source and service as strings. This incurs a cost since scanning each
  event to get the counts of events by source and service will involve multiple string comparisons. A
  future optimization could be to hash the combination of these values at the source into a single
  integer.

[component_spec]: https://github.com/vectordotdev/vector/blob/master/docs/specs/component.md#componenteventssent
[source_sender]: https://github.com/vectordotdev/vector/blob/master/src/source_sender/mod.rs#L265-L268
[event_metadata]: https://github.com/vectordotdev/vector/blob/master/lib/vector-core/src/event/metadata.rs#L20-L38
