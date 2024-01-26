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

Part of Vector's topology design involves propagating _backpressure_, which is a signal that events
cannot be processed as quickly as they are being received. When one component tries to send more
events to a component than that component can currently handle, the sending component is informed of
this indirectly. Backpressure can travel all the way from a [sink][sinks], up through any
[transforms][transforms], back to the [source][sources], and ultimately, even to clients such as
applications sending logs over HTTP.

Backpressure is a means of allowing a system to expose whether or not it can handle more work or if
it is too busy to do so. We rely on backpressure to be able to slow down the consumption or
acceptance of events, such as when pulling them from a source like Kafka, or accepting them over a
socket like HTTP.

In some cases, though, we don't always want to immediately propagate backpressure, as this could
lead to constantly slowing down upstream components and callers, potentially causing issues outside
of Vector. We want to avoid entirely slowing things down when a component just crosses over the
threshold of being fully saturated, as well being able to handle temporary slowdowns and outages in external
services that sinks send data to.

Buffering is the approach that Vector takes to solve these problems.

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

By default, sinks use an in-memory buffer like all other components do, but the default buffer size
is slightly increased, at 500 events. We've increased the buffer capacity for sinks specifically as,
again, they are typically the primary source of backpressure in any given Vector topology.

Beyond the default buffer capacity being larger, you can also fully control the buffer configuration
as well.  Vector exposes two main settings for controlling buffering: the type of buffer to use, and
the action to take when the buffer is full.

## Buffer types

### In-memory buffers

We've already talked about in-memory buffers. This buffer type, as you might be able to guess from
its name, will buffer events in memory. In-memory buffers are the fastest buffer type, but they come
with two main drawbacks: they can consume memory proportional to their size, and they're not
durable.

The fact that they consume memory is obvious, but it bears mentioning because it represents an
important factor in capacity planning. In-memory buffers are configured in terms of how many events
they can buffer, not the number of bytes they can hold.

For example, an in-memory buffer configured with a maximum event count of 100,000 could potentially
consume only a few megabytes if events were small, but could balloon to hundreds of megabytes if the
events were in the kilobytes size range. This means that the memory usage profile might change
substantially if the data being processed by Vector changes upstream and grows in size unexpectedly.
The size of events is fluid, and based off the internal representation used by Vector. As a rule of
thumb for capacity planning, you can estimate the size of an event by how large it would be when
encoded to JSON, without any compression.

Additionally, in-memory buffers are not durable. While Vector provides features like
[end-to-end acknowledgements][e2e_acks] to ensure that sources don't acknowledge events until they
have been processed, any events sitting in an in-memory buffer would be lost if Vector, or the host
running Vector, crashed. While pull-based sources like S3 or Kafka would handle this by simply
reattempting to process the events, push-based sources may not be able to retransmit their messages.

### Disk buffers

When the durability of data is more important than the overall performance of Vector, disk buffers
can be used to persist buffered events while stopping and starting Vector, including if Vector
crashes. Disk buffers allow Vector to essentially pick up from where it left off when it starts back
up again.

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
Disk buffers have some unique monitoring requirements compared to in-memory buffers,
specifically around free storage space.
{{< /warning >}}

I/O errors are notoriously hard to recover from, as it can be difficult to know what data made it to
disk or not. In order to provide the durability guarantees that an event written to a disk buffer is
safely on disk, Vector will **forcefully stop itself** when an I/O error occurs during flushing to
disk. An error message will be emitted before exiting that explains the underlying cause of the
error, such as "no storage space". Depending on the error, Vector can typically be safely restarted
and it will attempt to recover whatever events are in the disk buffer that are not corrupted, but we
cannot run that logic without reloading the buffers entirely, hence the forced process exit.

As an operator, the main resource you'll need to monitor is _free storage space_. If Vector cannot
write to a disk buffer because of a lack of free space, it must exit, as we can no longer be sure
what data has been written to disk or not. You **must** ensure that the data directory configured
for Vector (located within the global [`data_dir`][global_data_dir]) is on a storage volume with
enough free space based on the total maximum size of all configured disk buffers. You must also
ensure that other processes are not consuming that free space.

While Vector will exit at startup if it detects your disk buffers could grow to a size bigger than
the storage volume itself, it may not be able to detect that issue with exotic/unique storage
configurations, and it also cannot detect if other processes are writing files that are consuming
free space and stop itself from trying to continue to write to disk.

## "When full" behavior

As important as choosing which buffer type to use, choosing what to do when a buffer is full can
have a major impact on how Vector as a system performs, and this behavior often need to be matched
to the configuration and workload itself.

### Blocking (`block`)

When configured to block, Vector will wait indefinitely to write to a buffer that is currently full.
This is the default "when full" behavior.

This behavior is the default because it generally provides the intended behavior of reliably
processing observability data, in the order it was given to Vector. Additionally, blocking will
induce backpressure, which as we've talked about is an important signal to upstream components that
they may need to slow down or shed load.

Blocking may not be acceptable, however, if you're accepting data from clients and cannot afford to
have them also blocked on waiting for a response that the data was accepted by Vector. We'll cover
some common buffering scenarios (and configuration) further down.

### Drop the event (`drop_newest`)

{{< warning >}}
Using `drop_newest` with in-memory buffers is **not recommended** for bursty workloads, where events
arrive in large, periodic batches.

Doing so will typically result in the buffer being immediately filled and the remaining events being
dropped, even when Vector appears to have available processing capacity.
{{< /warning >}}

When configured to "drop newest", Vector will simply drop an event if the buffer is currently full.

This behavior can be useful when the data itself is idempotent (the same value is being sent
continually) or is generally not high-value, such as trace or debug logging. It allows Vector to
effectively shed load, by lowering the number of events in-flight for a topology, while
simultaneously avoiding the blocking of upstream components.

### Overflow to another buffer (`overflow`)

{{< danger >}}
Overflow buffers are **not yet suitable** for production workloads and may contain bugs that ultimately lead to **data
loss.**
{{< /danger >}}

Using the overflow behavior, operators can configure a **buffer topology**. This consists or two or
more buffers, arranged sequentially, where one buffer can overflow to the next one in the topology,
and so on, until either the last buffer is reached (which must either block or drop the event) or a
buffer is found with available capacity.

Instead of being forced to use only an in-memory buffer, which is limited by available memory, or
being forced to use only a disk buffer, which decreases throughput even if the sink is not
experiencing an issue, we can use the overflow mode to preferentially buffer events by first trying
to use an in-memory buffer, and only falling back to a disk buffer is necessary.

Here's a snippet of what it looks like to configure a buffer topology to use the overflow behavior:


```yaml title="vector.yaml"
sinks:
  overflow_test:
    type: blackhole
    buffer:
    - type: memory
      max_events: 1000
      when_full: overflow
    - type: disk
      max_size: 1073741824 # 1GiB.
      when_full: drop_newest
```

In this example, we have an in-memory channel with a maximum capacity of 1000 events overflowing to
a disk buffer that can grow up to 1GiB in size, after which point it will drop new events until free
space becomes available in the buffer.

An important thing to note is that if space becomes available in the in-memory buffer, new events
that Vector tries to buffer will go to in-memory buffer, even if there are still events in the disk
buffer. Additionally, those new events in the in-memory buffer may be returned _before_ older events
stored in the disk buffer. There are **no event ordering guarantees** when using the overflow behavior for
a buffer topology.

Additionally, the last buffer in a buffer topology cannot be set to the overflow mode. Naturally,
unless there is another buffer to overflow to, you must either block or drop an event when full.

## Recommended buffering configurations

Below are a few common scenarios that Vector users often deal with and the recommended buffering
configurations to use.

**I can't provide any storage to Vector.**

You'll have to use in-memory buffers then. Vector does not support buffering events to external
storage systems.

**Performance is the most important factor.**

You should use in-memory buffers. As noted above, the `drop_newest` mode will provide the highest
possible performance, but more events may be dropped than expected.

Generally, increasing `max_events` and leaving the default blocking behavior is sufficient to
handle higher event processing rates.

**Durability is the most important factor.**

You should use disk buffers.

Depending on your sources, you may be fine to keep the default blocking behavior, or you may wish to
also drop events when the buffer is full. As mentioned above, some sources are receiving data from
clients directly, rather than pulling it on demand, and it might be better to simply drop the event
rather than force the client to wait, which could cause issues further up the stack.

[sinks]: /docs/reference/configuration/sinks/
[transforms]: /docs/reference/configuration/transforms/
[sources]: /docs/reference/configuration/sources/
[e2e_acks]: /docs/about/under-the-hood/architecture/end-to-end-acknowledgements
[global_data_dir]: /docs/reference/configuration/global-options/#data_dir
