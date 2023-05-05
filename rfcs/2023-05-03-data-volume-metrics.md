# RFC <issue#> - 2023-05-02 - Data Volume Insights metrics

Vector needs to be able to emit accurate metrics that can be usefully queried
to give users insights into the volume of data moving through the system.

## Scope

### In scope

- All event metrics within Vector need to emit the estimated JSON size of the
  event. With a consistent method for determining the size it will be easier
  to accurately compare data in vs data out.
- The metrics sent by each sink needs to be tagged with the source id of the
  event so the route an event takes through Vector can be queried.
- Each event needs to be labelled with a `service`. This is a new concept
  within Vector and represents the application that generated the log or
  metric.
- The service tag and source tag in the metrics needs to be opt in so customers
  that don't need the increased cardinality are unaffected.

### Out of scope

- A separete metric, `component_sent_bytes_total` that indicates network bytes
  sent by Vector is not considered here.

## Pain

Currently it is difficult to accurately gauge the volume of data that is moving
through Vector. It is difficult to query where data being sent out has come
from.

## Proposal

### User Experience

Global config optionns will be provided allowing the `service` tag and the
`source` tag to be specified. For example:

```yaml
metrics:
  tags:
    service: service
    source: input
```

The default will be to not emit these tags.

### Implementation

#### Metric tags

*service* - to attach the service, we need to add a new meaning to Vector - *service*. Any sources that
            receive data that could potentially be considered a service will need to indicate which field
            means `service`. 
            This work has largely already been done with the LogNamespacing work, so it will be trivial to add
            this new field.

*source* - A new field will be added to the [Event metadata][event_metadata] - `Arc<ComponentId>` that will indicate the source
           of the event.

We will need to do an audit of all components to ensure the bytes emitted for the `component_received_event_bytes_total`
and `component_sent_event_bytes_total` metrics are the estimated JSON size of the event.

#### `component_received_event_bytes_total`

This metric is emitted by the framework [here][source_sender], so it looks like the only change needed is
to add the service tag. 

#### `component_sent_event_bytes_total`

For stream based sinks this will typically be the byte value returned by `DriverResponse::events_sent`. 

Despite being in the [Component Spec][component_spec], not all sinks currently conform to this.

As an example, from a cursory glance over a couple of sinks:

The Amqp sink currently emits this value as the length of the binary data that is sent. By the time the data has 
reached the code where the `component_sent_event_bytes_total` event is emitted, that event has been encoded
and the actual estimated JSON size has been lost. The sink will need to be updated so that when the event is
encoded, the encoded event together with the pre-encoded JSON bytesize will be sent to the service where the
event is emitted.

The Kafka sink also currently sends the binary size, but it looks like the estimated JSON bytesize is easily
accessible at the point of emitting, so would not need too much of a change.

To ensure that the correct metric is sent in a typesafe manner, we will wrap the estimated JSON size in a 
newtype:

```rust
pub struct JsonSize(usize);
```

The `EventsSent` metric will only accept this type.

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
  less accurate compared to the data being sent and recieved which is often JSON.

## Outstanding Questions

- Should the default be to not emit tags? Could the default be different for OP users and standard Vector 
  users?
- What should the source be tagged as for events that go through a transform that could potentially 
  combine one event from multiple sources - eg. `reduce`?
- What should the source be for events that come from no source (emitted by the `lua` transform)?
- Not all sources will have a `service`. Should we default the service to a value, emit a null service tag
  or not include the tag at all if there is no service?

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Submit a PR with spike-level code _roughly_ demonstrating the change.
- [ ] Incremental change #1
- [ ] Incremental change #2
- [ ] ...

Note: This can be filled out during the review process.

## Future Improvements


[component_spec]: https://github.com/vectordotdev/vector/blob/master/docs/specs/component.md#componenteventssent
[source_sender]: https://github.com/vectordotdev/vector/blob/master/src/source_sender/mod.rs#L265-L268
[event_metadata]: https://github.com/vectordotdev/vector/blob/master/lib/vector-core/src/event/metadata.rs#L20-L38