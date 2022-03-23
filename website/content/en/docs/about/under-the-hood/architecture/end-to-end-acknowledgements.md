---
title: End-to-end Acknowledgements
weight: 1
tags: ["acknowledgements", "configuration"]
---

Vector has the capability of allowing clients to verify that data has been delivered to destination sinks. This is called end-to-end acknowledgement.

The structure of this capability is conceptually simple. When a participating [source][sources] receives a batch of data, it also optionally creates a batch notifier from which it can receive the combined delivery status of all the events in the batch. Each event derived from the data contains a reference to this batch structure. When those events reach their [destination sink][sinks], they update that batch with the response from the sink. When the last event has been delivered, the batch notifies the source component, and that source notifies the sender of the overall status of the batch.

This simplicity hides a number of complications in the process. Events may end up being sent to multiple sinks. In this case, we have to track the delivery status across all the destinations. To do so, the status of an event lives in a piece of data that is shared across all the copies of the event. This provides the ability to notify the source component when the last copy of the original event has been delivered.

Similarly, multiple events may end up being merged into a single event, through the [`aggregate`][aggregate_transform] or [`reduce`][reduce_transform] transforms. For these events, a single delivery might end up having come from multiple source batches. To handle this, each event has not just a single batch reference, but a list of all the batches from which the source events originated. When the event is delivered, all of the source batches are updated at once.

[sources]: /docs/reference/configuration/sources
[sinks]: /docs/reference/configuration/sinks
[aggregate_transform]: /docs/reference/configuration/transforms/aggregate
[reduce_transform]: /docs/reference/configuration/transforms/reduce
