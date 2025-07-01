---
title: End-to-end Acknowledgements
weight: 1
tags: ["acknowledgements", "configuration"]
---

Vector has the capability of allowing clients to verify that data has been delivered to destination
sinks. This is called end-to-end acknowledgement.

## Design

When a participating [source][sources] receives an event, or batch of events, it can optionally
create a **batch notifier** for those events. The batch notifier has two parts: one part stays with
the source, and the other part is attached to the events. When the events reach their
[destination sink][sinks] and are processed by the sink, Vector captures the status of the response
from the downstream service and uses it to update the batch notifier. By doing so, we can indicate
whether an event was successfully processed or not.

Additionally, Vector ensures that the batch notifier for an event is always updated, whether or not
the event made it to a sink. This ensures that if an event is intentionally dropped (for example, by
using a [`filter`][filter_transform] transform) or even unintentionally dropped (maybe Vector had
a bug, uh oh!), we still update the batch notifier to indicate the processing status of the event.

Meanwhile, the source will hold on to the other half of the batch notifiers that it has created, and
is notified when a batch notifier is updated. Once notified, a source will propagate that batch
notifier status back upstream: maybe this means responding with an appropriate HTTP status code (200
vs 500, etc) if the events came from an [HTTP request][http_source], or acknowledging the event
directly, such as when using the [`kafka`][kafka_source] or [`aws_sqs`][aws_sqs_source] sources,
which have native support for acknowledging messages.

## Ensuring acknowledgement of events even through fanout, batching, aggregation, etc

The high-level description of how end-to-end acknowledgements work leaves out some of the corner
cases and complications in providing this capability.

For example, events may end up being sent to multiple sinks. In this case, we have to track the
delivery status across all the destinations. To do so, the status of an event lives in a piece of
data that is shared across all the copies of the event. This ensures that Vector only notifies the
source once all copies of an event have been processed of a sink, and that the "worst" status is the
status reported to the source. If an event is sent to three sinks, and is only processed
successfully by two of them, we mark that event as having failed which ensures it can be sent again,
giving all three sinks a chance to process it successfully.

Similarly, multiple events may end up being merged into a single event, through the
[`aggregate`][aggregate_transform] or [`reduce`][reduce_transform] transforms. For these events, a
single delivery might end up having come from multiple source batches. To handle this, each event
has not just a single batch reference, but a list of all the batches from which the source events
originated. When the event is delivered, all of the source batches are updated at once.

## End-to-end acknowledgement support between sources and sinks

So far, we've talked about how end-to-end acknowledgements work between a source and sink, but we
originally mentioned the term "participating" source, which is an important point: a source must be
_capable_ of acknowledging events in order for end-to-end acknowledgements to provide any durability
guarantees.

Not all sources can acknowledge events. For example, the [`socket`][socket_source] source cannot
acknowledge events because it simply decodes bytes it receives over a socket, and has no way to send
back a message to say "Hey, that event you just sent me wasn't processed correctly. Can you please
resend it?". When Vector starts up and loads its configuration, it checks to ensure that for any
sinks with end-to-end acknowledgements enabled, the events it consumes come from a source that
supports acknowledgements.  If the source _doesn't_ have acknowledgement support, a warning message
is emitted to let you know that end-to-end acknowledgements cannot provide its typical promise of
durable processing, and that silent data loss may occur.

[sources]: /docs/reference/configuration/sources
[sinks]: /docs/reference/configuration/sinks
[filter_transform]: /docs/reference/configuration/transforms/filter/
[http_source]: /docs/reference/configuration/sources/http/
[kafka_source]: /docs/reference/configuration/sources/kafka/
[aws_sqs_source]: /docs/reference/configuration/sources/aws_sqs/
[aggregate_transform]: /docs/reference/configuration/transforms/aggregate
[reduce_transform]: /docs/reference/configuration/transforms/reduce
[socket_source]: /docs/reference/configuration/sources/socket/
