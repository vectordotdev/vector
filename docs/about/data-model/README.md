---
description: 'A deeper look at Vector''s data model'
---

# Data Model

![][images.data-model]

## Event

As shown above, Vector generalizes all data flowing through Vector as "events":

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

You can view a complete definition in the [event proto\
definition][url.event_proto]. You'll notice that each event must be one of
2 types:

{% page-ref page="../data-model/log.md" %}

{% page-ref page="../data-model/metric.md" %}

## FAQ

### Isn't eveything just an events?

1. We like the "everything is an event" philosophy a lot.
2. We recognize that there's a large gap between that idea and a lot of
   existing tooling.
3. By starting "simple" (from an integration perspective, i.e. meeting people
   where they are) and evolving our data model as we encounter the specific
   needs of new sources/sinks/transforms, we avoid overdesigning yet another
   grand unified data format.
4. Starting with support for a little more "old school" model makes us a better
   tool for supporting incremental progress in existing infrastructures towards
   more event-based architectures.


[images.data-model]: ../../assets/data-model.svg
[url.event_proto]: https://github.com/timberio/vector/blob/master/proto/event.proto
