---
title: Buffering model
weight: 2
tags: ["buffering", "buffers", "disk"]
---

Vector implements a buffering model that allows operators to choose whether to prioritize
performance or durability when handling an excess of events beyond what a sink can process.

## Backpressure and the need to buffer

While operators typically strive to ensure their Vector deployments are appropriately sized for the
expected load, sometimes problems can occur. Maybe an application starts generating more logs than
normal, or the downstream service where you're sending data starts to respond slower than normal.

Part of Vector's design is to propagate _backpressure_, where when a component is processing slowly,
the components that come before are notified of this slowdown and can potentially reduce their rate
of consumption, or temporarily turn away requests, in order to alleviate pressure on the component
(which may be isolated to Vector, or a downstream service) and hopefully allow processing to return
to normal.

In these cases, though, we don't always want to immediately propagate backpressure, as this could
lead to constantly slowing down upstream components and callers, potentially causing issues outside
of Vector. Vector is intended to be stable and resilient to temporary overload or outages, and this
is where buffering comes in.

## Buffering between components

All components in a Vector topology have a small in-memory buffer between them. The primary purpose
of this buffer is act as the channel that two components communicate over, but we take this a little
further by ensuring that there is a small amount of space -- typically 100 events -- that can be
used to send events even if the component on the receiving end is currently busy. This allows
maximizing throughput when workloads are not entirely uniform.

However, in order to provide protection against temporary overloads or outages, we need to provide a
more comprehensive buffering solution that can be tailored for the given workload.

## Buffering at the sink

When working with a Vector configuration, you'll be working with buffer configuration settings on
sinks. The main reason for this is that, in practice, sinks represent the primary source of
backpressure in a topology: talking to services over the network, where latency may be introduced,
or outages may temporarily occur.

By default, sinks use an in-memory buffer like all other components do, but the default buffer size is a
little bigger, around 500 events. This buffer is bigger because, again, sinks are typically the
primary source of backpressure and so we want to provide more cushion.

Beyond the default size being larger, you can also fully control the buffer configuration as well.
Vector exposes two main knobs for controlling buffering behavior: the type of buffer to use, and the
action to take when the buffer is full.

## Buffer types

### In-memory buffers

We've already talked about in-memory buffers. This buffer type, as you might be able to guess from
their name, will buffer events in memory. These are the fastest buffer type, but they come with two
main drawbacks: they can consume memory proportional to their size, and they're not durable.

The fact that they consume memory is obvious, but it bears mentioning because it represents an
important factor in capacity planning. In-memory buffers are configured in terms of how many events
they can buffer, not the number of bytes they can hold.

For example, an in-memory buffer configured with a maximum event count of 100,000 could potentially
consume only a few megabytes if events were small, but could balloon to hundreds of megabytes if the
events were in the kilobytes size range. This means that the memory usage profile might change
substantially if the data being processed by Vector changes upstream and grows in size unexpectedly.

Additionally, in-memory buffers are not durable. While Vector provides features like
[end-to-end acknowledgements][e2e_acks] to ensure that sources don't acknowledge events until they
have been processed, any events sitting in an in-memory buffer would be lost if Vector, or the host
running Vector, crashed. While pull-based sources like S3 or Kafka would handle this by simply
reattempting to process the events, push-based sources may not be able to retransmit their messages.

### Disk buffers

When the durability of data is more important than the overall performance of Vector, disk buffers
can be used to persist buffered events between stopping and starting Vector, including when
Vector crashes. Disk buffers allow Vector to essentially pick up from where it left off when it
starts back up again.

Disk buffers function like a write-ahead log, where every event is first sent through the buffer,
and written to the data files, before it is read back out. This may sound slow, but in practice,
modern operating systems allow reads to happen out of memory, so disk buffers generally maintain
high throughput on both the read and write path. By default, we do not synchronize data to disk for
every write, but instead synchronize on an interval (500 milliseconds) which allows for high
throughput with a reduced risk of data loss.

We've designed disk buffers to provide _consistent_ performance. While other projects may be able to
write data to disk faster than Vector, we've chosen to make sure that events can be read as fast as
they can be written, as well as reducing the tail latencies between an event being written and an
event being read on the other side.

Additionally, like in-memory buffers, disk buffers have a configurable maximum size so they can be
limited in terms of disk usage. This maximum size is adhered to rigidly, so you can depend on Vector
not exceeding it. There is a minimum size for all buffers, though -- currently ~256MiB -- which is a
requirement of the disk buffer implementation. On the filesystem, disk buffers will look like
append-only log files that grow to a maximum file size of 128MiB and are deleted once they have been
processed fully.

Storage errors are always a potential issue, whether due to hardware failures or data files being
mistakenly deleted while Vector is running. Disk buffers automatically checksum all events being
written to disk, and when corruption is detected during a read, they will automatically recover as
many events as can be correctly decoded. Disk buffers will also emit metrics when such corruption is
detected, to give as accurate of a view into the number of events that were lost as it possibly can.

#### Operator requirements

{{< warning >}}
Disk buffers have some unique monitoring requirements compared to in-memory buffers, specifically
around free storage space.
{{< /warning >}}

In order to provide the durability guarantees that an event written to a disk buffer is safely on
disk, many common error handling techniques cannot be used, such as retrying failed operations and
so on. This means that Vector will **forcefully panic** and exit the process when an I/O error
occurs during flushing to disk.

As an operator, the main resource you'll need to monitor is _free storage space_. If Vector cannot
write to a disk buffer because of a lack of free space, it will panic, as we can no longer be sure
what data has been written to disk or not. You **must** ensure that the data directory configured
for Vector ([`global.data_dir`][global_data_dir]) is on a storage volume with enough free space
based on the total maximum size of all configured disk buffers. While Vector will exit at startup if
it detects your disk buffers could grow to a size bigger than the storage volume itself, it may not
be able to detect that issue with exotic/unique storage configurations, and it also cannot detect if
other processes are writing files that are consuming free space.

## Buffer behavior

As important as choosing which buffer type to use, choosing what to do when a buffer is full can
ultimately have a major impact on how Vector performs, and will often need to be matched to the
topology and the goals of using Vector.

You'll recognize this as the `buffer.when_full` configuration option.

### Blocking

When configured to block, Vector will wait indefinitely to write to a buffer that is currently full.
This is the default "when full" behavior.

This behavior is the default because it generally provides the intended behavior of reliably
processing observability data, in the order it was given to Vector. Additionally, blocking will
induce backpressure, which as we've talked about is an important signal to upstream components that
they may need to slow down or shed load.

Blocking may not be acceptable, however, if you're accepting data from clients and cannot afford to
have them also blocked on waiting for a response that the data was accepted by Vector. We'll cover
some common buffering scenarios (and configuration) further down.

### Drop newest

When configured to drop newest, Vector will simply drop an event if the buffer is currently full.

This behavior can be useful when the data itself is idempotent (the same value is being sent
continually) or is generally not high-value, such as trace/debug logging. It allows Vector to
effectively shed load, by lowering the number of events in-flight for a topology, while
simultaneously avoiding the blocking of upstream components.

### Overflow (beta)

**Overflow buffers are a beta feature. Use with caution.**

Instead of blocking and causing backpressure, or losing events by dropping them, what if we could
combine the best parts of both in-memory buffers and disk buffers together? This is what overflow buffers
provide.

We're using the term "overflow buffers" because while this is technically a "when full" behavior, it
involves the configuration of multiple buffers. Essentially, a buffer can be configured to overflow
to another buffer instead of blocking or dropping.

This allows users to build more resource-efficient buffering topologies, such as using a fast,
in-memory channel of limited size (to put an upper bound on potential data loss) while overflow to a
disk buffer. Instead of being limited by the in-memory buffer, disk buffers could offer far more
buffering capacity in the case of extended slowdowns/outages. Additionally, it means that when load
is normal, events don't have to be written to the disk buffer: they can simply be sent through the
in-memory buffer. This reduces disk usage and generally increases performance, while still providing
the _ability_ to buffer to disk during overload scenarios.

However, the overflow behavior cannot be used on the last buffer in the buffer topology: you
inevitably have to either block or drop the event if there's no more capacity. There's a few other
constraints around how overflow buffer topologies that we don't cover here, but Vector will make
sure to warn you about if it detects an invalid configuration on startup.

## Recommended buffering configurations

Below are a few common scenarios that Vector users often deal with and the recommended buffering
configurations to use.

**I can't provide any storage to Vector.**

You'll have to use in-memory buffers then. Vector does not support buffering events to external
storage systems.

**Performance is the most important factor.**

You should use in-memory buffers, which are the default. Using `buffer.when_full = "drop_newest"`
would provide the highest performance, but it can sometimes drop more events than you may expect it
to: events are generally received in chunks, not as a steady stream, which can quickly exceed
`buffer.max_size`.

Generally, increasing `buffer.max_size` and leaving the default blocking behavior is sufficient to
handle higher event processing rates.

**Durability is the most important factor.**

You should use disk buffers.

Depending on your sources, you may be fine to keep the default blocking behavior, or you may wish to
also drop events when the buffer is full. As mentioned above, some sources are receiving data from
clients directly, rather than pulling it on demand, and it might be better to simply drop the event
rather than force the client to wait, which could cause issues further up the stack.

[e2e_acks]: /docs/about/under-the-hood/architecture/end-to-end-acknowledgements
[global_data_dir]: /docs/reference/configuration/global-options/#data_dir
