---
title: Guarantees
aliases: ["/docs/about/guarantees"]
---

Vector attempts to make it clear which guarantees you can expect from it. We categorize all
components by their targeted delivery guarantee and also by their general stability. This helps you
make the appropriate trade-offs for your use case.

Here you can find an overview of delivery guarantee types and their meaning as well as how we label
the stability of our components. Next, you can head over to the [components] page and use filters to
see which components support specific guarantees.

## Acknowledgement guarantees

Vector supports end-to-end acknowledgement for the majority of its
sources and sinks. This is a system which tracks the delivery status of
an event through the lifetime of that event as it travels from the
originating source to any number of destination sinks. Support for this
feature is indicated by the "acknowledgements" badge at the top of the
relevant sink or source configuration reference page.

For a source that supports end-to-end acknowledgements and is
connected to a sink that has the `acknowledgements` option enabled,
this will cause it to wait for _all_ connected sinks to either mark
the event as delivered, or to persist the events to a disk buffer, if
configured on the sink, before acknowledging receipt of the event. If
a sink signals the event was rejected and the source can provide an
error response (i.e. through an HTTP error code), then the source will
provide an appropriate error to the sending agent.

Sinks support end-to-end acknowledgements by providing an indicator of
the final status of each event after delivery has completed. This
includes waiting until all internal buffering and the retry process is
complete. Sinks which have a durable buffer configured will mark events
as delivered once they are persisted to that buffer, as an indicator
that Vector will continue to retry the event even after restarts. If the
sink does not support acknowledgements, events will be marked as having
been delivered when they are handed off to the sink component, before
any deliveries are attempted.

Some transforms will drop events as part of their normal
operation. For example, the `dedupe` transform will drop events that
duplicate another recent event in order to reduce data volume. When
this happens, the event will be considered as having been delivered
with no error.

## Delivery guarantees

### At-least-once

The **at-least-once** delivery guarantee ensures that an [event] received by a Vector component is
ultimately delivered at least once.

While rare, it is possible for an event to be delivered more than
once. See the [Does Vector support exactly-once
delivery?](#faq-at-least-once) FAQ below).

{{< warning >}}
In order to achieve at-least-once delivery between restarts,
your sink must be configured to use disk-based buffers:

```toml title="vector.toml"
[sinks.my_sink_id]
  [sinks.my_sink_id.buffer]
    type = "disk"
    when_full = "block"
    max_size = 104900000 # 100MiB
```

Refer to each [sink's][sinks] documentation for further guidance on its buffer options.

[sinks]: /docs/reference/configuration/sinks
{{< /warning >}}

### Best-effort

A **best-effort** delivery guarantee means that a Vector component makes a best effort to deliver
each event but it can't _guarantee_ delivery. This is usually due to limitations of the underlying
protocol, which is outside Vector's control.

Note that this is _not_ the same as at-most-once delivery, as it'is still possible for Vector to
introduce duplicates under extreme circumstances.

## Stability guarantees

### Stable

The `stable` status is a _subjective_ status defined by the Vector team. It's intended to give you a
general idea of a feature's suitability for production environments. A feature is considered stable
if it meets the following criteria:

1. A meaningful number of users (generally over 50) have been using the feature in a production
    environment for a sustained period of time without issue.
2. The feature has had sufficient time (generally more than 4 months) to be community tested.

3. The feature API is stable and unlikely to change.

4. There are no major [open bugs][bugs] for the feature.

### Beta

The `beta` status means that a feature has not met the criteria outlined in the [stable](#stable)
section and therefore should be used with caution in production environments.

### Deprecated

The `deprecated` status means that a feature will be removed in the next major version of Vector. We
will provide ample time to transition and, when possible, strive to retain backward compatibility.

## FAQs

### Do I need at-least-once delivery? {#faq-at-least-once}

One of the unique advantages of the metrics and logging use cases is that data is usually used for diagnostic purposes only. Therefore, losing the occasional event has little impact on your business. This affords you the opportunity to provision your pipeline towards performance, simplicity, and cost reduction. On the other hand, if you're using your data to perform business critical functions, then data loss is not acceptable and therefore requires "at-least-once" delivery.

To clarify, even though a source or sink is marked as "best effort" it doesn't mean Vector takes delivery lightly. In fact, once data is within the boundary of Vector it won't be lost if you've configured on-disk buffers. Data loss for "best-effort" sources and sinks is almost always due to the limitations of the underlying protocol.

### Does Vector support exactly-once delivery?

No, Vector does not support exactly once delivery. There are future plans to partially support this for sources and sinks that support it, for example Kafka, but it remains unclear if Vector will ever be able to achieve this. We recommend [subscribing to our mailing list](/community), which will keep you in the loop if this ever changes.

### How can I find components that meet these guarantees?

Head over to the [components page][components] and use the guarantee
filters.

[bugs]: https://github.com/vectordotdev/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22type%3A+bug%22
[components]: /components
[event]: /docs/about/under-the-hood/architecture/data-model
