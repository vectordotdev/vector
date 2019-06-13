---
description: Send log and metric events to a downstream service
---

# Sinks

![](../../../.gitbook/assets/sinks.svg)

Sinks are last in the [pipeline](../../../about/concepts.md#pipelines), responsible for sending [events](../../../about/data-model.md#event) downstream. These can be service specific sinks, such as [`vector`](vector.md), [`elasticsearch`](elasticsearch.md), and [`s3`](aws_s3.md), or generic protocol sinks like [`http`](http.md), [`tcp`](tcp.md), or [`udp`](udp.md).

## How It Works

Sinks are responsible for forwarding [events](../../../about/data-model.md#event) downstream. They generally overlap in behavior falling into 2 categories: streaming or batching. To provide high-level structure we'll cover the common behavioral traits here to establish an understanding of shared behavior. For explicitness, each sink will document this behavior as well.

### Buffers vs. Batches

For sinks that batch and flush it's helpful to understand the difference between buffers and batches within Vector. Batches represent the batched payload being sent to the downstream service while [buffers](buffer.md) represent the internal data buffer Vector uses for each sink. More detailed descriptions are as follows.

#### Buffers

Vector couples [buffers](buffer.md) with each sink, this offers [a number of advantages](buffer.md#coupled-with-sinks) over a single shared global buffer. In general, you should [configure your sink's buffer](buffer.md) to exceed the batch size \(if your sink has a batch\). This is especially true when using [on-disk](buffer.md#in-memory-or-on-disk) buffers, as it ensures data is not lost in the event of restarts.

#### Batches

Batches represent the batched payload being sent to the downstream service. Sinks will provide 2 options to control the size and age before being sent, the `batch_size` and `batch_timeout` options. They will be documented in a "Batching" section within any sink that supports them.

### Event Types

#### Input

Every sink will include an "Input" section that specifies the [event type](../../../about/data-model.md#event) it can accept.

#### Connectivity

A sink must be connected to a [source](../sources/) or [transform](../transforms/) that outputs the same event type.

### Healthchecks

All sinks are required to implement a healthcheck behavior. This is intended to be a light weight check to ensure downstream availability and avoid subsequent failures if the service is not available. Additionally, you can require all health checks to pass via the [`--require-healthy` flag](../../administration/starting.md#options) when [starting](../../administration/starting.md) Vector.

### Output

Each sink will document it's output, and if possible, include examples so you can more clearly understand it's behavior.

### Rate Limiting

Any sink that batches will include options to rate limit requests. These options include the `request_in_flight_limit`, `request_timeout_secs`, and `request_rate_limit_duration_secs`, `request_rate_limit_num`. For explicitness, these options will be documented directly on the sinks that support them.

### Retries

Any sink that batches will include options to retry failed requests. These options include the `request_retry_attempts` , and `request_retry_backoff_secs`. For explicitness, these options will be documented directly on the sinks that support them.

### Timeouts

All sinks will support a `request_timeout_secs` option. This will kill long running requests. It's highly recommended that you configure timeouts downstream to be less than the value here. This will ensure Vector does not pile on requests.

### Vector to Vector Communication

If you're sending data to another downstream [Vector service](../../../setup/deployment/roles/service.md) then you should use the [`vector` sink](vector.md), with the downstream service using the [`vector` source](../sources/vector.md).

{% page-ref page="../../guides/vector-to-vector-guide.md" %}

