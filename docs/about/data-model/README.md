---
description: 'A deeper look at Vector''s data model'
---

# Data Model

![][images.data-model]

As shown above, Vector generalizes all data flowing through Vector as events:

{% code-tabs %}
{% code-tabs-item title="event.proto" %}
```coffeescript
message EventWrapper {
  oneof event {
    Log log = 1;
    Metric metric = 2;
  }
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

You can view a complete definition in the [event proto \
definition][url.event_proto]. You'll notice that each event must be one of
2 types:

{% page-ref page="../data-model/log.md" %}

{% page-ref page="../data-model/metric.md" %}

Each page above will provide a deeper dive into it's respective event type.

## Event

For clarification, Vector uses the term "event" to refer to both log and
metrics event. This is the generalized term that represents all units of data
flowing through Vector.


[images.data-model]: ../../assets/data-model.svg
[url.event_proto]: https://github.com/timberio/vector/blob/master/proto/event.proto
