---
title: Data Model
sidebar_label: hidden
description: Vector's internal data model -- event and it's subtypes.
---

<SVG src="/optimized_svg/data-model-event_808_359.svg" />

The individual units of data flowing through Vector are known as **events**.
Events must fall into one of Vector's defined observability types.

## Event Types

Vector defines subtypes for events. This is necessary to establish domain
specific requirements enabling interoperability with existing monitoring and
observability systems.

<Jump to="/docs/about/under-the-hood/architecture/data-model/log/" leftIcon="book">Log</Jump>
<Jump to="/docs/about/under-the-hood/architecture/data-model/metric/" leftIcon="book">Metric</Jump>

## FAQ

### Why Not _Just_ Events?

We, _very much_, like the idea of an event-only world, one where every service
is perfectly instrumented with events that contain rich data and context.
Unfortunately, that is not the case; existing services often emit metrics,
traces, and logs of varying quality. By designing Vector to meet services where
they are in their current state, we serve as a bridge to newer standards. This is why
we place "events" at the top of our data model, where logs and metrics are
derived.

Finally, a sophisticated data model that accounts for the various data types
allows for _correct_ interoperability between observability systems. For
example, a pipeline with a `statsd` source and a `prometheus` sink would not
be possible without the correct internal metrics data types.
